//! 网络环境监测与防火墙归类调整（spec 002 US2：用户决策的归类调整入口）。
//!
//! - 检测：PowerShell 隐藏执行一次调用，输出活动网络归类的压缩 JSON（纯
//!   `Get-NetConnectionProfile`；spec 010 起 443 规则 Profile 探测段随归类告警
//!   整体退役——告警判定对象消亡，plan §3.6）；读取为普通用户权限
//! - 修改：ShellExecuteW runas 提权 `Set-NetConnectionProfile`（fire-and-forget），
//!   结果以复测为准；UAC 拒绝 → Err 交上层提示（AC7，`shell_error_text` 消费）
//! - 轮询：NetMonitor 60s 周期（spec 010 T5 降频供数：networks 仅供归类卡与
//!   lan_guard 的 `public_blocks_exception` 判定消费），状态变化才发
//!   `net://changed`；探测失败静默保持上次状态（AC4 降级不劣化）
//!
//! 探测出口经 `NetProbe` trait 注入，单测以脚本化输出驱动状态机，零真实进程。

use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// 轮询间隔（spec 010：告警链退役后降频供数，plan §3.6 取 60s 摊薄 PS 拉起
/// 成本；原 15s 为 002 告警 ≤30s 收敛服务，随对象退役）
pub const NET_POLL_INTERVAL: Duration = Duration::from_secs(60);
/// 单次探测超时（超时视为本次探测失败，静默降级）
const DETECT_TIMEOUT: Duration = Duration::from_secs(10);
/// 状态事件名（载荷 = NetStatus）
pub const EVENT_NET_CHANGED: &str = "net://changed";

// ── 数据模型 ───────────────────────────────────────────────────────────────

/// 网络归类（Windows NetworkCategory：0=Public / 1=Private / 2=Domain）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NetCategory {
    Public,
    Private,
    Domain,
    /// 枚举外取值（前瞻兼容），不参与归类切换
    Unknown,
}

/// 一条活动网络（Get-NetConnectionProfile 行）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkEntry {
    pub name: String,
    pub if_index: u32,
    pub category: NetCategory,
}

/// 网络环境快照（前端渲染 + `net://changed` 载荷）。
/// spec 010 T5：002 的规则存在性/私有判定/告警位字段随归类告警链整体退役，
/// 快照仅存活动网络行（plan §3.6）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetStatus {
    pub networks: Vec<NetworkEntry>,
}

// ── 探测（spec 生成 + 解析）────────────────────────────────────────────────

/// 探测命令参数（powershell.exe args）。
/// 单次调用输出活动网络归类的压缩 JSON（spec 010 T5 契约：探测收缩为纯
/// `Get-NetConnectionProfile`，防火墙规则 Profile 查询段已随告警链退役，
/// plan §3.6）；UTF8 前缀保证中文网络名经管道不乱码。
pub fn detect_args() -> Vec<String> {
    vec![
        "-NoProfile".into(),
        "-NonInteractive".into(),
        "-ExecutionPolicy".into(),
        "Bypass".into(),
        "-Command".into(),
        format!(
            "[Console]::OutputEncoding=[Text.Encoding]::UTF8; \
             $ns = @(Get-NetConnectionProfile -ErrorAction SilentlyContinue | \
             ForEach-Object {{ @{{ name = $_.Name; ifIndex = $_.InterfaceIndex; category = [int]$_.NetworkCategory }} }}); \
             [ordered]@{{ networks = $ns }} | ConvertTo-Json -Compress"
        ),
    ]
}

/// PS 输出 → NetStatus（契约解析；category 数值映射见 NetCategory；
/// 单/多网络由 PS 侧 @() 保证数组，解析仍容错缺字段）。
pub fn parse_net_status(raw: &str) -> Result<NetStatus, String> {
    let v: serde_json::Value =
        serde_json::from_str(raw.trim()).map_err(|e| format!("探测输出非 JSON：{e}"))?;
    let mut networks = Vec::new();
    if let Some(arr) = v["networks"].as_array() {
        for n in arr {
            networks.push(NetworkEntry {
                name: n["name"].as_str().unwrap_or_default().to_string(),
                if_index: n["ifIndex"].as_u64().unwrap_or(0) as u32,
                category: match n["category"].as_i64() {
                    Some(0) => NetCategory::Public,
                    Some(1) => NetCategory::Private,
                    Some(2) => NetCategory::Domain,
                    _ => NetCategory::Unknown,
                },
            });
        }
    }
    Ok(NetStatus { networks })
}

// ── 调整（提权参数构造）────────────────────────────────────────────────────

/// 归类切换的提权参数串（ShellExecuteW lpParameters；AC5/AC6）。
/// 定位以**网络名优先、接口序号兜底**：InterfaceIndex 会随适配器重枚举漂移
/// （真机复现：热点重连后旧序号查无对象、切换静默失败），网络名在活动连接
/// 表内相对稳定；同名多接口视为同一网络一并设置。非法目标归类 → Err
/// （AC7 的命令校验分支）。结果不回流，以复测为准（plan §2）。
pub fn set_category_params(name: &str, if_index: u32, category: &str) -> Result<String, String> {
    let kw = match category {
        "private" => "Private",
        "public" => "Public",
        other => return Err(format!("非法目标归类：{other}（合法：private/public）")),
    };
    if name.is_empty() && if_index == 0 {
        return Err("缺少网络定位信息（网络名与接口序号均为空）".into());
    }
    // PS 单引号字面量：内部单引号按规则翻倍
    let name_lit = name.replace('\'', "''");
    Ok(format!(
        "-NoProfile -NonInteractive -WindowStyle Hidden -Command \"\
         $p = Get-NetConnectionProfile | Where-Object {{ $_.Name -eq '{name_lit}' }}; \
         if (-not $p) {{ $p = Get-NetConnectionProfile | Where-Object {{ $_.InterfaceIndex -eq {if_index} }} }}; \
         if ($p) {{ $p | Set-NetConnectionProfile -NetworkCategory {kw} }}\""
    ))
}

// ── 监测器（探测出口注入；变化才发声）──────────────────────────────────────

/// 探测出口（真实实现拉起 PowerShell；单测脚本化输出）
pub trait NetProbe: Send + Sync {
    /// 返回探测原始 stdout；Err = 探测失败（AC4 静默降级）
    fn detect(&self) -> Result<String, String>;
}

/// 事件出口（真实实现 Tauri emit；单测断言事件序列）
pub trait NetEventSink: Send + Sync {
    fn emit_net_status(&self, st: &NetStatus);
}

/// Tauri 事件实现（装配层注入 AppHandle）
pub struct TauriNetEmitter {
    app: tauri::AppHandle,
}

impl TauriNetEmitter {
    pub fn new(app: tauri::AppHandle) -> Self {
        Self { app }
    }
}

impl NetEventSink for TauriNetEmitter {
    fn emit_net_status(&self, st: &NetStatus) {
        use tauri::Emitter;
        if let Err(e) = self.app.emit(EVENT_NET_CHANGED, st) {
            log::error!("发送 {EVENT_NET_CHANGED} 失败：{e}");
        }
    }
}

/// 网络环境监测器：refresh 即时探测 + 缓存 + 变化去重发声；spawn_poller 周期驱动。
#[derive(Clone)]
pub struct NetMonitor {
    probe: Arc<dyn NetProbe>,
    sink: Arc<dyn NetEventSink>,
    last: Arc<Mutex<Option<NetStatus>>>,
}

impl NetMonitor {
    pub fn new(probe: Arc<dyn NetProbe>, sink: Arc<dyn NetEventSink>) -> Self {
        Self { probe, sink, last: Arc::new(Mutex::new(None)) }
    }

    /// 探测一次：成功且与上次不同 → 更新缓存并发事件；失败静默保持上次（AC4）。
    /// 返回本次可见状态（失败且有缓存 → 上次；失败且无缓存 → None）。
    /// 锁纪律：std Mutex 不可重入——所有路径单次加锁，事件在锁释放后发。
    pub fn refresh(&self) -> Option<NetStatus> {
        let parsed = self.probe.detect().ok().as_deref().map(parse_net_status);
        match parsed {
            Some(Ok(st)) => {
                let mut guard = self.last.lock().expect("网络状态锁中毒");
                let changed = guard.as_ref() != Some(&st);
                let visible = st.clone();
                if changed {
                    *guard = Some(st);
                }
                drop(guard);
                if changed {
                    self.sink.emit_net_status(&visible);
                }
                Some(visible)
            }
            _ => {
                log::debug!("网络探测失败：保持上次状态（AC4 静默降级）");
                self.last.lock().expect("网络状态锁中毒").clone()
            }
        }
    }

    /// 启动轮询线程（随进程生命周期；句柄语义与组件轮询器一致）
    pub fn spawn_poller(&self) -> std::thread::JoinHandle<()> {
        let me = self.clone();
        std::thread::Builder::new()
            .name("wb-net-poller".into())
            .spawn(move || loop {
                me.refresh();
                std::thread::sleep(NET_POLL_INTERVAL);
            })
            .expect("网络轮询线程创建失败")
    }

    /// 上次成功探测的缓存快照（探测失败不更新；None = 尚无成功探测）。
    /// spec 010：活动网络归类喂给 LanGuardMonitor（public_blocks_exception
    /// 判定数据源，plan §3.5「活动网络数据由调用方注入」）。
    pub fn last(&self) -> Option<NetStatus> {
        self.last.lock().expect("网络状态锁中毒").clone()
    }
}

// ── 真实探测实现（Windows；std::process 直采 stdout，不经日志文件）─────────

/// PowerShell 隐藏执行探测（CREATE_NO_WINDOW；超时杀进程判失败）
#[cfg(windows)]
pub struct PsNetProbe;

#[cfg(windows)]
impl Default for PsNetProbe {
    fn default() -> Self {
        Self
    }
}

#[cfg(windows)]
impl NetProbe for PsNetProbe {
    fn detect(&self) -> Result<String, String> {
        use std::io::Read;
        use std::os::windows::process::CommandExt;
        use std::process::{Command, Stdio};

        let mut child = Command::new("powershell.exe")
            .args(detect_args())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .creation_flags(crate::scripts::CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| format!("powershell 拉起失败：{e}"))?;

        // 就绪等待（超时即杀）：探测为只读操作，超时静默降级即可
        let deadline = Instant::now() + DETECT_TIMEOUT;
        let status = loop {
            match child.try_wait() {
                Ok(Some(code)) => break Ok(code),
                Ok(None) if Instant::now() >= deadline => {
                    let _ = child.kill();
                    break Err("探测超时".to_string());
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(100)),
                Err(e) => break Err(format!("等待探测进程失败：{e}")),
            }
        };
        let code = status.map_err(|e| e)?;
        if !code.success() {
            return Err(format!("探测进程非零退出（code={:?}）", code.code()));
        }
        let mut stdout = String::new();
        if let Some(mut pipe) = child.stdout.take() {
            pipe.read_to_string(&mut stdout)
                .map_err(|e| format!("读取探测输出失败：{e}"))?;
        }
        Ok(stdout)
    }
}

// ── 单元测试（纯逻辑：退役断言 / 解析 / 契约 / 参数构造 / 监测器状态机）──

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::Mutex as StdMutex;

    /// 退役断言（spec 010 T5/AC4）：002 的 443 归类告警链整体退役——本源码不得
    /// 再含告警判定函数、443 规则名常量与规则 Profile 探测段（判定对象随
    /// Private 语义消亡，plan §3.6）。针串以 concat 拼接：断言代码同在本源码内，
    /// 直书针串会自证命中。
    #[test]
    fn alert_chain_retired_from_source() {
        let src = include_str!("network.rs");
        let needle_judge = concat!("needs", "_alert");
        let needle_const = concat!("FIREWALL", "_RULE_NAME");
        let needle_cmdlet = concat!("Get-Net", "FirewallRule");
        let needle_field = concat!("private", "Only");
        assert!(!src.contains(needle_judge), "告警判定必须退役（plan §3.6）");
        assert!(!src.contains(needle_const), "443 规则名常量必须退役");
        assert!(!src.contains(needle_cmdlet), "规则 Profile 探测段必须退役");
        assert!(!src.contains(needle_field), "旧探测契约字段必须退役");
        assert!(
            src.contains(concat!("Get-Net", "ConnectionProfile")),
            "归类探测本体保留（002 US2 调整入口供数）"
        );
    }

    /// 轮询降频契约：告警退役后 networks 仅供归类卡与 public_blocks_exception
    /// 消费，60s 摊薄 PS 拉起成本（plan §3.6；原 15s 为 002 告警收敛服务）
    #[test]
    fn poll_interval_sixty_seconds_after_retirement() {
        assert_eq!(NET_POLL_INTERVAL, Duration::from_secs(60));
    }

    const JSON_ONE_PUBLIC: &str =
        r#"{"networks":[{"name":"LiuGong-Guest-t","ifIndex":7,"category":0}]}"#;
    const JSON_ONE_PRIVATE: &str = r#"{"networks":[{"name":"home","ifIndex":7,"category":1}]}"#;

    #[test]
    fn parse_maps_categories() {
        let st = parse_net_status(JSON_ONE_PUBLIC).unwrap();
        assert_eq!(st.networks.len(), 1);
        assert_eq!(st.networks[0].name, "LiuGong-Guest-t");
        assert_eq!(st.networks[0].category, NetCategory::Public);
        assert_eq!(st.networks[0].if_index, 7);

        let st2 = parse_net_status(JSON_ONE_PRIVATE).unwrap();
        assert_eq!(st2.networks[0].category, NetCategory::Private);
    }

    #[test]
    fn parse_tolerates_missing_fields_and_unknown_category() {
        let st = parse_net_status(r#"{"networks":[]}"#).unwrap();
        assert!(st.networks.is_empty(), "空网络表 → 快照为空（不 Err）");

        let st2 = parse_net_status(
            r#"{"networks":[{"name":"x","ifIndex":1,"category":2},{"name":"y","category":9}]}"#,
        )
        .unwrap();
        assert_eq!(st2.networks[0].category, NetCategory::Domain);
        assert_eq!(st2.networks[1].category, NetCategory::Unknown);

        // 顶层缺 networks 字段 → 空表容错（不 Err）
        assert!(parse_net_status("{}").unwrap().networks.is_empty());
    }

    #[test]
    fn parse_rejects_non_json() {
        assert!(parse_net_status("not json").is_err());
        assert!(parse_net_status("").is_err());
    }

    /// networks-only 探测契约（替换原 002 带规则探测段的 detect_args 测试）：
    /// 唯一查询 = 归类；压缩 JSON + UTF8 前缀保留
    #[test]
    fn detect_args_networks_only_contract() {
        let args = detect_args();
        let cmd = args.iter().position(|a| a == "-Command").unwrap();
        let script = &args[cmd + 1];
        assert!(script.contains("Get-NetConnectionProfile"), "唯一查询 = 归类探测");
        assert!(script.contains("ConvertTo-Json -Compress"), "契约：压缩 JSON");
        assert!(
            script.contains("OutputEncoding=[Text.Encoding]::UTF8"),
            "中文网络名防乱码"
        );
        assert!(
            !script.contains(concat!("Get-Net", "FirewallRule")),
            "防火墙规则探测段已退役（plan §3.6）"
        );
        assert!(!script.contains("-DisplayName"), "无规则名查询参数");
    }

    /// 事件/命令载荷契约：仅 networks（002 的规则字段与告警位不得回流）
    #[test]
    fn net_status_payload_networks_only() {
        let j = serde_json::to_string(&parse_net_status(JSON_ONE_PUBLIC).unwrap()).unwrap();
        assert!(j.contains(r#""networks":["#), "{j}");
        assert!(!j.contains(concat!("private", "Only")), "{j}");
        assert!(!j.contains(concat!("rule", "Present")), "{j}");
        assert!(!j.contains("\"alert\""), "{j}");
    }

    #[test]
    fn set_category_params_validate_and_map() {
        // 主定位按网络名（容忍 InterfaceIndex 漂移），序号兜底
        let p = set_category_params("603", 14, "private").unwrap();
        assert!(p.contains("$_.Name -eq '603'"), "{p}");
        assert!(p.contains("InterfaceIndex -eq 14"), "{p}");
        assert!(p.contains("-NetworkCategory Private"), "{p}");
        assert!(p.contains("-WindowStyle Hidden"), "提权后不残留控制台：{p}");

        // 网络名含单引号按 PS 规则翻倍
        let p2 = set_category_params("odd'name", 0, "public").unwrap();
        assert!(p2.contains("'odd''name'"), "{p2}");
        assert!(p2.contains("-NetworkCategory Public"), "{p2}");

        let e = set_category_params("603", 14, "auto").unwrap_err();
        assert!(e.contains("非法目标归类"), "AC7 命令校验分支：{e}");
        let e2 = set_category_params("", 0, "private").unwrap_err();
        assert!(e2.contains("缺少网络定位信息"), "{e2}");
    }

    // ── NetMonitor 状态机（脚本化探测输出驱动）─────────────────────────

    struct MockNetProbe {
        outs: StdMutex<VecDeque<Result<String, String>>>,
    }
    impl NetProbe for MockNetProbe {
        fn detect(&self) -> Result<String, String> {
            self.outs.lock().unwrap().pop_front().unwrap_or(Err("耗尽".into()))
        }
    }
    #[derive(Default)]
    struct MockSink {
        emissions: StdMutex<Vec<NetStatus>>,
    }
    impl NetEventSink for MockSink {
        fn emit_net_status(&self, st: &NetStatus) {
            self.emissions.lock().unwrap().push(st.clone());
        }
    }

    fn monitor_with(outs: Vec<Result<String, String>>) -> (NetMonitor, Arc<MockSink>) {
        let sink = Arc::new(MockSink::default());
        let m = NetMonitor::new(
            Arc::new(MockNetProbe { outs: StdMutex::new(outs.into()) }),
            sink.clone(),
        );
        (m, sink)
    }

    #[test]
    fn monitor_emits_only_on_change_and_degrades_silently() {
        // 序列：A（首查发声）→ A（不变不发声）→ B（变化发声）→ 探测失败（保持 B 静默）
        let (m, sink) = monitor_with(vec![
            Ok(JSON_ONE_PUBLIC.into()),
            Ok(JSON_ONE_PUBLIC.into()),
            Ok(JSON_ONE_PRIVATE.into()),
            Err("ps 超时".into()),
        ]);
        assert_eq!(
            m.refresh().unwrap().networks[0].category,
            NetCategory::Public,
            "首查：公用网络快照"
        );
        assert_eq!(sink.emissions.lock().unwrap().len(), 1);
        assert_eq!(
            m.refresh().unwrap().networks[0].category,
            NetCategory::Public,
            "重复探测结果不变：不发声"
        );
        assert_eq!(sink.emissions.lock().unwrap().len(), 1, "状态未变不发事件");
        assert_eq!(
            m.refresh().unwrap().networks[0].category,
            NetCategory::Private,
            "归类变化（切到专用网络）"
        );
        assert_eq!(sink.emissions.lock().unwrap().len(), 2, "变化才发声");
        let held = m.refresh().unwrap();
        assert_eq!(
            held.networks[0].category,
            NetCategory::Private,
            "探测失败保持上次状态（AC4）"
        );
        assert_eq!(sink.emissions.lock().unwrap().len(), 2, "失败不发声");
        assert_eq!(held.networks[0].name, "home", "失败后缓存仍为上次完整状态");
    }

    #[test]
    fn monitor_none_before_first_success() {
        let (m, _sink) = monitor_with(vec![Err("无网络".into())]);
        assert!(m.refresh().is_none(), "首查即失败 → None（AC4 静默）");
    }
}
