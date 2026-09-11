//! 组网通道（spec 007 mesh）：EasyTier 私有组网（legacy 模式——T2 实测定案，
//! network_secret 派生传输加密；secure-mode 升级路径见 plan §7-R4）。
//!
//! 本模块承载：随包二进制完整性校验（T1，供应链防线 plan §7-R6）、
//! config.toml 渲染与前置校验（T4，唯一渲染路径 plan §4.3）、网段冲突检测、
//! `--check-config` 办后校验封装。服务管理与状态探询随 T7/T8 扩展。

use crate::consts::{
    EASYTIER_CLI_EXE_NAME, EASYTIER_CLI_SHA256, EASYTIER_CORE_EXE_NAME, EASYTIER_CORE_SHA256,
    PACKET_DLL_NAME, PACKET_DLL_SHA256, WINDIVERT_SYS_NAME, WINDIVERT_SYS_SHA256,
    WINTUN_DLL_NAME, WINTUN_DLL_SHA256,
};
use crate::dns_api::sha256_hex;
use crate::settings::{AccessChannel, MeshConfig};
use serde::Serialize;
use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// 栈目录下组网子目录名（plan §4.2：`<stack>/easytier/`）
pub const MESH_DIR: &str = "easytier";
/// network_secret 文件名（set-mesh-secret.ps1 写入，AC8：不进命令行/设置/日志）
pub const NETWORK_SECRET_FILE: &str = "network-secret";
/// config.toml 文件名（工作台唯一渲染）
pub const CONFIG_FILE: &str = "config.toml";
/// 宿主机在组网内的显示名（成员 peer 表 hostname 列；T2 实测字段名）
pub const MESH_HOSTNAME: &str = "ai-remote-workbench";
/// 收敛后的监听端口（默认六协议 11010-11013 全开 → 仅 tcp/udp；
/// T2 实测同机双实例默认端口冲突 fatal 的教训固化）
pub const MESH_LISTEN_PORT: u16 = 11010;
/// Windows 服务名（mesh-service.ps1 的管理对象；显示名见 plan §4.4）
pub const SERVICE_NAME: &str = "EasyTierMesh";
/// RPC 门户（仅绑 localhost：状态探询通道的安全边界，plan §2；
/// `easytier-cli --rpc` 探询与服务 binPath 的 `-r` 同值）
pub const RPC_PORTAL: &str = "127.0.0.1:15888";
/// 服务日志子目录（binPath `--file-log-dir` 指向；落位时预建，首启即可写日志）
pub const LOG_DIR: &str = "logs";

/// 校验目录内五个随包文件（easytier-core.exe / easytier-cli.exe / wintun.dll /
/// packet.dll / WinDivert64.sys）的 SHA256 与版本锁定值一致。
///
/// 调用方：栈目录落位复制前（T7，防篡改源）、装机向导组网分支（T13，办后校验）。
/// 失败返回首个不符项的可读原因（文件缺失或哈希不符，含文件名）——不含密钥
/// 类敏感信息，可直接透出 UI（spec AC4 口径）。
pub fn verify_easytier_binaries(dir: &Path) -> Result<(), String> {
    let checks: [(&str, &str); 5] = [
        (EASYTIER_CORE_EXE_NAME, EASYTIER_CORE_SHA256),
        (EASYTIER_CLI_EXE_NAME, EASYTIER_CLI_SHA256),
        (WINTUN_DLL_NAME, WINTUN_DLL_SHA256),
        (PACKET_DLL_NAME, PACKET_DLL_SHA256),
        (WINDIVERT_SYS_NAME, WINDIVERT_SYS_SHA256),
    ];
    for (name, expected) in checks {
        let path = dir.join(name);
        let bytes = std::fs::read(&path).map_err(|e| format!("{name} 读取失败：{e}"))?;
        let actual = sha256_hex(&bytes);
        if !actual.eq_ignore_ascii_case(expected) {
            return Err(format!(
                "{name} SHA256 校验不符（期望 {expected}，实际 {actual}）——疑似被篡改或版本不符"
            ));
        }
    }
    Ok(())
}

/// 随包五文件清单（落位复制的对象；哈希校验见 [`verify_easytier_binaries`]）
const MESH_BIN_FILES: [&str; 5] = [
    EASYTIER_CORE_EXE_NAME,
    EASYTIER_CLI_EXE_NAME,
    WINTUN_DLL_NAME,
    PACKET_DLL_NAME,
    WINDIVERT_SYS_NAME,
];

// ── 服务 binPath 构造与栈目录落位（T7，plan §4.4；AC8 命令行无密钥）─────────

/// 构造 EasyTierMesh 服务 binPath（纯函数，plan §4.4 形态）：
/// `"<stack>\easytier\easytier-core.exe" -c "<stack>\easytier\config.toml"
///  -r 127.0.0.1:15888 --file-log-dir "<stack>\easytier\logs"`
///
/// AC8：入参只有栈目录，输出是纯路径参数——network_secret 只进 config.toml
/// （渲染器职责），命令行永不携带密钥。运行时 binPath 由 mesh-service.ps1
/// 按同公式从 StackDir 自建（T9 曾经 `-BinPath` 传递，真机缺陷后废弃：
/// 内嵌双引号过不了 `-Command "..."` 包裹——见 service_install_params 注释）；
/// 本函数保留作双端形态契约锚（脚本公式若漂移，单测形态即失配暴露）。
/// 路径全部加引号：栈目录可配置，含空格时 SCM 命令行才不被拆断。
///
/// 可行性依据（v2.6.4 源码级取证）：core 的 main 无条件先走
/// `service_dispatcher::start`——被 SCM 拉起即进 win_service_main（从进程
/// 命令行解析 `-c` 参数）；控制台启动报 ERROR 0x427 被吞、继续走 CLI。
#[cfg(test)] // 生产不经此函数（binPath 由脚本自建）；仅单测编译锁形态契约
pub fn service_bin_path(stack_dir: &str) -> String {
    let et = Path::new(stack_dir).join(MESH_DIR);
    let q = |p: PathBuf| format!("\"{}\"", p.display());
    format!(
        "{} -c {} -r {RPC_PORTAL} --file-log-dir {}",
        q(et.join(EASYTIER_CORE_EXE_NAME)),
        q(et.join(CONFIG_FILE)),
        q(et.join(LOG_DIR)),
    )
}

/// 栈目录落位（T7，plan §4.2）：随包五文件从资源 bin 复制到
/// `<stack>/easytier/`，并预建 logs/。复制前后各过一遍哈希校验
/// （防篡改源 + 防复制损坏）；幂等（覆盖复制）。
/// config.toml / network-secret 不在此列：前者由渲染器产出（T9
/// mesh_apply_config），后者由 set-mesh-secret.ps1 交互写入（T10）。
pub fn stage_easytier_binaries(src_bin: &Path, stack_dir: &str) -> Result<PathBuf, String> {
    verify_easytier_binaries(src_bin)?;
    let dest = Path::new(stack_dir).join(MESH_DIR);
    std::fs::create_dir_all(dest.join(LOG_DIR))
        .map_err(|e| format!("创建 {} 失败：{e}", dest.join(LOG_DIR).display()))?;
    for name in MESH_BIN_FILES {
        std::fs::copy(src_bin.join(name), dest.join(name))
            .map_err(|e| format!("复制 {name} 到 {} 失败：{e}", dest.display()))?;
    }
    verify_easytier_binaries(&dest)?;
    Ok(dest)
}

// ── 状态探询与判定（T8，plan §3.3/§5.1；AC3/AC4）──────────────────────────
//
// 守护语义（plan §3.3 映射表）：mesh 由 SCM 承担拉起/自愈（T7 服务恢复策略），
// 工作台守护退化为**探询 + 如实上报**——`sc query` 服务态 + `easytier-cli
// peer` RPC 实况（RPC 实时接口无 004 的陈旧日志问题），不做应用层重启
//（重启需提权，T9 提供手动入口）。

/// mesh 状态事件名（前端监听；载荷 [`MeshStatus`]）
pub const EVENT_MESH_STATUS: &str = "mesh://status";
/// 状态探询周期（观察者语义，与 004 守护轮询同节拍）
pub const MESH_POLL_INTERVAL: Duration = Duration::from_secs(5);

// detail 稳定码（前端 `mesh.code.*` 双语映射，沿 LOGIN_FAILED_CODE 惯例：
// 固定枚举文案，构造上不可能含密钥——AC4「摘要不含密钥」的落点）
/// 密钥未写入（set-mesh-secret.ps1 指引，AC9）
pub const DETAIL_SECRET_MISSING: &str = "secret_missing";
/// 服务未安装（向导/设置区装服务入口，AC3 前置）
pub const DETAIL_SERVICE_MISSING: &str = "service_missing";
/// 服务已停止（现役 mesh 下离线；手动/提权启动）
pub const DETAIL_SERVICE_STOPPED: &str = "service_stopped";
/// 服务启动类型已被置 disabled（通道停用语义）
pub const DETAIL_SERVICE_DISABLED: &str = "service_disabled";
/// 服务进程在但 RPC 门户连不上（刚拉起/异常——按连接中如实上报）
pub const DETAIL_RPC_UNREACHABLE: &str = "rpc_unreachable";

/// 服务态（`sc query` 状态码 + `sc qc` 启动类型合成；查询免提权）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MeshServiceState {
    /// sc query 退出码非 0（1060 = 服务不存在）
    NotFound,
    /// 运行中（含 STOP_PENDING/PAUSED 等中间态的保守归并：START_PENDING 单列）
    Running,
    /// 启动中（SCM 拉起未就绪，宽限语义）
    StartPending,
    /// 已停止且启动类型未禁用
    Stopped,
    /// 已停止且启动类型 disabled（停用 mesh 的持久语义，plan §3.3）
    Disabled,
}

/// 对端摘要（`peer list -o json` 宽松提取）。RPC 输出本就不含网络参数
///（network_secret 只进 config.toml），故本结构天然无密钥（AC8/AC4）。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerBrief {
    /// 成员显示名（config.toml hostname / 成员设备自带名）
    pub hostname: String,
    /// 成员虚拟 IP（v2.6.4 序列化 `ipv4` 字段；本机/未分配为空 → None）
    pub ipv4: Option<String>,
    /// 延迟毫秒（CLI 格式化字符串 "46.62" 解析；本机 "-" → None）
    pub latency_ms: Option<f64>,
    /// 丢包率 0~1（CLI "0.0%" 剥百分号归一）
    pub loss_rate: Option<f64>,
    /// 本机自身（peer list 首项恒为本机，cost="Local"——判定在线必须排除）
    pub is_local: bool,
}

/// 组网状态（AC4 四态 + 非现役通道态；后者对齐 TunnelState::Inactive 呈现惯例）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "state")]
pub enum MeshState {
    /// 服务运行且有非本机成员可达
    Online,
    /// 服务运行/启动中但尚无非本机成员（或 RPC 未就绪）
    Connecting,
    /// 现役 mesh 下服务未跑（未装/停止/禁用）
    Offline,
    /// 密钥未写入（拒绝渲染/启动的前置未完成，AC9）
    NotConfigured,
    /// 当前通道非 mesh（看板不呈现组网实况）
    Inactive,
}

/// mesh 状态快照（事件载荷；detail 为稳定码，见 DETAIL_* 常量）
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeshStatus {
    #[serde(flatten)]
    pub state: MeshState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// 进入该状态的时间戳（epoch ms）
    pub since: u64,
    /// 服务态原样透出（前端可区分「未装/停止/禁用」）
    pub service: MeshServiceState,
    /// 当前成员列表（含本机项，isLocal 标记）
    pub peers: Vec<PeerBrief>,
}

/// 状态判定（AC4 纯函数，plan §5.1：服务 running 且有可用对端 → online；
/// running 且无对端 → connecting）。peers 含本机项（判定排除 is_local）。
pub fn judge_mesh_state(
    channel_is_mesh: bool,
    secret_configured: bool,
    service: MeshServiceState,
    peers: &[PeerBrief],
    rpc_ok: bool,
) -> (MeshState, Option<&'static str>) {
    if !channel_is_mesh {
        return (MeshState::Inactive, None);
    }
    if !secret_configured {
        return (MeshState::NotConfigured, Some(DETAIL_SECRET_MISSING));
    }
    match service {
        MeshServiceState::NotFound => (MeshState::Offline, Some(DETAIL_SERVICE_MISSING)),
        MeshServiceState::Disabled => (MeshState::Offline, Some(DETAIL_SERVICE_DISABLED)),
        MeshServiceState::Stopped => (MeshState::Offline, Some(DETAIL_SERVICE_STOPPED)),
        MeshServiceState::Running | MeshServiceState::StartPending => {
            if peers.iter().any(|p| !p.is_local) {
                (MeshState::Online, None)
            } else if rpc_ok {
                (MeshState::Connecting, None)
            } else {
                (MeshState::Connecting, Some(DETAIL_RPC_UNREACHABLE))
            }
        }
    }
}

// ── 进程操作（Windows 采集隔离，可 mock；惯例同 stop.rs 的 ProcessOps seam）──

/// mesh 探询接口（MeshMonitor 依赖注入；两项查询均免提权）
pub trait MeshOps: Send + Sync {
    /// 查询 EasyTierMesh 服务态（sc query + 必要时 sc qc）
    fn service_state(&self) -> MeshServiceState;
    /// `easytier-cli -p 127.0.0.1:15888 -o json peer list`（全局选项须在
    /// 子命令前——v2.6.4 clap 结构实测，置后会报 unexpected argument）
    fn query_peers(&self) -> Result<Vec<PeerBrief>, String>;
}

/// Windows 真实实现。cli 候选：资源 bin（随包）→ 栈目录 easytier/ 落位副本
///（升级解耦 + ACL 收紧，plan §4.2）。
pub struct WindowsMeshOps {
    bin_dir: Option<PathBuf>,
    stack_dir: String,
}

impl WindowsMeshOps {
    pub fn new(bin_dir: Option<PathBuf>, stack_dir: String) -> Self {
        Self { bin_dir, stack_dir }
    }

    fn resolve_cli(&self) -> Option<PathBuf> {
        let mut v = Vec::new();
        if let Some(dir) = &self.bin_dir {
            v.push(dir.join(EASYTIER_CLI_EXE_NAME));
        }
        v.push(
            Path::new(&self.stack_dir)
                .join(MESH_DIR)
                .join(EASYTIER_CLI_EXE_NAME),
        );
        v.into_iter().find(|p| p.is_file())
    }
}

impl MeshOps for WindowsMeshOps {
    fn service_state(&self) -> MeshServiceState {
        use std::os::windows::process::CommandExt;

        // sc.exe 行标签/状态码为纯 ASCII（不随系统语言本地化；其余文本 GBK
        // 编码经 lossy 转换为 U+FFFD，不影响 STATE 行的数字解析）
        let out = std::process::Command::new("sc.exe")
            .args(["query", SERVICE_NAME])
            .creation_flags(crate::scripts::CREATE_NO_WINDOW)
            .output();
        let Ok(out) = out else {
            return MeshServiceState::NotFound; // sc 不可用：保守按未装上报
        };
        if !out.status.success() {
            return MeshServiceState::NotFound; // 1060 = 服务不存在等
        }
        let text = String::from_utf8_lossy(&out.stdout);
        match parse_sc_query_state(&text) {
            Some(4) => MeshServiceState::Running,
            Some(2) => MeshServiceState::StartPending,
            _ => {
                // Stopped/StopPending/Paused：再查启动类型分辨 disabled（停用
                // mesh 的持久语义）——两次轻量 sc 调用，均免提权
                let qc = std::process::Command::new("sc.exe")
                    .args(["qc", SERVICE_NAME])
                    .creation_flags(crate::scripts::CREATE_NO_WINDOW)
                    .output();
                let disabled = qc.is_ok_and(|o| {
                    parse_sc_start_type(&String::from_utf8_lossy(&o.stdout)) == Some(4)
                });
                if disabled { MeshServiceState::Disabled } else { MeshServiceState::Stopped }
            }
        }
    }

    fn query_peers(&self) -> Result<Vec<PeerBrief>, String> {
        use std::os::windows::process::CommandExt;

        let cli = self
            .resolve_cli()
            .ok_or_else(|| "未找到 easytier-cli.exe（资源目录与栈目录均缺失）".to_string())?;
        let out = std::process::Command::new(&cli)
            .args(["-p", RPC_PORTAL, "-o", "json", "peer", "list"])
            .creation_flags(crate::scripts::CREATE_NO_WINDOW)
            .output()
            .map_err(|e| format!("easytier-cli 启动失败：{e}"))?;
        if !out.status.success() {
            // 无 core/RPC 未就绪：stderr 尾行做失败摘要（截断防长堆栈；
            // cli 参数不含密钥，stderr 不可能出现密钥——AC8）
            let stderr = String::from_utf8_lossy(&out.stderr);
            let last = stderr.lines().last().unwrap_or("").trim();
            let last: String = last.chars().take(160).collect();
            return Err(format!("RPC 探询失败：{last}"));
        }
        parse_peers_json(&String::from_utf8_lossy(&out.stdout))
    }
}

/// `sc query` 输出 → STATE 行状态码（4=RUNNING 2=START_PENDING 1=STOPPED
/// 3=STOP_PENDING 7=PAUSED；纯函数，单测覆盖真实缩进形态）
pub fn parse_sc_query_state(output: &str) -> Option<u32> {
    parse_sc_numeric_field(output, "STATE")
}

/// `sc qc` 输出 → START_TYPE 行类型码（2=AUTO_START 3=DEMAND_START 4=DISABLED）
pub fn parse_sc_start_type(output: &str) -> Option<u32> {
    parse_sc_numeric_field(output, "START_TYPE")
}

/// sc 输出的「LABEL : N TEXT」行取 N（标签行首匹配 + 冒号后首个词取数字）
fn parse_sc_numeric_field(output: &str, label: &str) -> Option<u32> {
    for line in output.lines() {
        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix(label) else { continue };
        let Some((_, value)) = rest.split_once(':') else { continue };
        let first = value.trim().split_whitespace().next()?;
        if let Ok(n) = first.parse() {
            return Some(n);
        }
    }
    None
}

/// `peer list -o json` 输出解析（纯函数）。v2.6.4 序列化 PeerTableItem
///（easytier-cli.rs `#[derive(serde::Serialize)]`，snake_case 字段：
/// hostname/ipv4/cost/lat_ms/loss_rate；lat/loss 为**格式化字符串**）。
/// 宽松提取：字段缺失/形态不符的条目跳过不阻断；整体非 JSON 数组才 Err。
pub fn parse_peers_json(stdout: &str) -> Result<Vec<PeerBrief>, String> {
    let parsed: serde_json::Value =
        serde_json::from_str(stdout.trim()).map_err(|e| format!("peer JSON 解析失败：{e}"))?;
    let serde_json::Value::Array(items) = parsed else {
        return Err("peer JSON 输出不是数组".into());
    };
    Ok(items
        .into_iter()
        .filter_map(|v| {
            let obj = v.as_object()?;
            Some(PeerBrief {
                hostname: obj.get("hostname")?.as_str()?.to_string(),
                ipv4: obj
                    .get("ipv4")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .map(str::to_string),
                latency_ms: obj.get("lat_ms").and_then(value_as_f64),
                loss_rate: obj.get("loss_rate").and_then(value_as_loss_rate),
                is_local: obj.get("cost").and_then(|v| v.as_str()) == Some("Local"),
            })
        })
        .collect())
}

/// 数值字段宽松取值：字符串（"-" 等不可解析 → None）或数字
fn value_as_f64(v: &serde_json::Value) -> Option<f64> {
    match v {
        serde_json::Value::String(s) => s.trim().parse().ok(),
        serde_json::Value::Number(n) => n.as_f64(),
        _ => None,
    }
}

/// 丢包率归一：CLI 字符串形态 "3.0%" 剥百分号 ÷100；数字形态直接取
///（超界值视为脏数据丢弃）
fn value_as_loss_rate(v: &serde_json::Value) -> Option<f64> {
    let raw = match v {
        serde_json::Value::String(s) => s.trim().trim_end_matches('%').parse::<f64>().ok()?,
        _ => return value_as_f64(v),
    };
    let normalized = if raw > 1.0 { raw / 100.0 } else { raw };
    (0.0..=1.0).contains(&normalized).then_some(normalized)
}

// ── 状态观察者（守护 tick 探询退化；IO 锁外纪律沿 tunnel.rs 死锁复盘）──────

/// mesh 状态事件出口（真实实现走 Tauri emit）
pub trait MeshEventSink: Send + Sync {
    fn emit_mesh_status(&self, status: &MeshStatus);
}

/// 组网状态观察者：周期探询（服务态 + RPC peers）→ 判定 → 变化发事件。
/// **只观察不动手**：拉起/自愈由 SCM 承担（plan §3.3），重启类操作走
/// T9 提权命令。
pub struct MeshMonitor {
    ops: Arc<dyn MeshOps>,
    /// 实时通道源（装配层接 SettingsState）
    channel: Arc<dyn Fn() -> AccessChannel + Send + Sync>,
    stack_dir: String,
    events: Arc<dyn MeshEventSink>,
    inner: Mutex<MeshStatus>,
}

impl MeshMonitor {
    pub fn new(
        ops: Arc<dyn MeshOps>,
        channel: Arc<dyn Fn() -> AccessChannel + Send + Sync>,
        stack_dir: String,
        events: Arc<dyn MeshEventSink>,
    ) -> Self {
        Self {
            ops,
            channel,
            stack_dir,
            events,
            inner: Mutex::new(MeshStatus {
                state: MeshState::Inactive,
                detail: None,
                since: now_ms(),
                service: MeshServiceState::NotFound,
                peers: Vec::new(),
            }),
        }
    }

    /// 当前快照（T9 mesh_status 命令数据源；启动兜底）
    pub fn status(&self) -> MeshStatus {
        self.inner.lock().expect("mesh 状态锁中毒").clone()
    }

    /// 启动轮询线程（装配层调用一次；随进程退出而止）
    pub fn spawn(self: &Arc<Self>) {
        let me = Arc::clone(self);
        std::thread::Builder::new()
            .name("wb-mesh-monitor".into())
            .spawn(move || loop {
                me.tick();
                std::thread::sleep(MESH_POLL_INTERVAL);
            })
            .expect("mesh 观察线程创建失败");
    }

    /// 探询单轮（pub 便于单测直调）。采集（sc/cli 子进程 + secret 文件读）
    /// 全在锁外，锁内只写快照与发事件——IO 锁外纪律（tunnel.rs 2026-09-10
    /// 死锁复盘）。peers 的延迟/丢包抖动视为变化（每轮如实发，前端 5s
    /// 刷新一行，与 domain://health 的每轮发同量级）。
    pub fn tick(&self) {
        let channel = (self.channel)();
        let secret_configured = read_network_secret(&self.stack_dir).is_some();
        let service = self.ops.service_state();
        // RPC 仅在服务运行态才有意义（未跑时省一次子进程调用，rpc_ok=false）
        let (peers, rpc_ok) = match service {
            MeshServiceState::Running | MeshServiceState::StartPending => {
                match self.ops.query_peers() {
                    Ok(p) => (p, true),
                    Err(_) => (Vec::new(), false),
                }
            }
            _ => (Vec::new(), false),
        };
        let (state, code) = judge_mesh_state(
            channel == AccessChannel::Mesh,
            secret_configured,
            service,
            &peers,
            rpc_ok,
        );
        let detail = code.map(str::to_string);
        let now = now_ms();
        let mut m = self.inner.lock().expect("mesh 状态锁中毒");
        let changed = m.state != state
            || m.detail != detail
            || m.service != service
            || m.peers != peers;
        if changed {
            *m = MeshStatus { state, detail, since: now, service, peers };
            self.events.emit_mesh_status(&m);
        } else {
            m.since = now;
        }
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// ── mesh-service.ps1 提权派发参数（T8 收摊停服务；T9 服务动作复用）─────────

/// mesh-service.ps1 的 UAC 派发参数串（ShellExecuteW runas 用；沿
/// visible_script_params 惯例：-Command "&" 包装 + -NoExit 留窗 + -Lang）。
/// 动作集见脚本 -Action（install/uninstall/start/stop/restart/status）。
pub fn service_action_params(
    scripts_dir: &Path,
    action: &str,
    stack_dir: &str,
    lang: crate::lang::Lang,
) -> String {
    // PS 单引号字面量（栈目录含空格安全，autostart::ps_quote 同规则）
    let quoted_stack = format!("'{}'", stack_dir.replace('\'', "''"));
    let args: Vec<&str> = vec!["-Action", action, "-StackDir", &quoted_stack];
    crate::scripts::visible_script_params(
        scripts_dir,
        crate::scripts::Script::MeshService,
        lang,
        &args,
    )
}

/// install 派发参数（mesh_install_service / apply 未装路径）。**不传 -BinPath**：
/// binPath 由脚本按同公式从 StackDir 自建（脚本内注释与 [`service_bin_path`]
/// 单测锁定形态一致性）。真机缺陷教训（2026-09-11）：经 `-Command "..."`
/// 包裹派发时，binPath 内嵌的路径双引号会提前终止外层引用、被 PowerShell
/// 剥除后触发脚本「binPath 与栈目录不符」误拒——参数串上不出现内嵌引号才是
/// 稳妥形态；AC8（binPath 无密钥）由公式本身保证，双端测试各自锁形。
pub fn service_install_params(
    scripts_dir: &Path,
    stack_dir: &str,
    lang: crate::lang::Lang,
) -> String {
    service_action_params(scripts_dir, "install", stack_dir, lang)
}

/// apply（mesh_apply_config / 切组网）的服务动作选择（纯函数）：服务未装 →
/// install（附 binPath，脚本幂等建档）；已装（含 Disabled）→ restart
///（config.toml 变更经重启生效；Disabled 下脚本报错留窗，属显式停用与
/// 启用路径交叉的罕见态，用户可读后先启用服务）
pub fn apply_service_action(service: MeshServiceState) -> &'static str {
    match service {
        MeshServiceState::NotFound => "install",
        _ => "restart",
    }
}

/// `sc stop` 退出码是否属于「无需再折腾」的静默完成（纯函数，单测锁定）：
/// 0 = 停止指令已受理；1062 = 服务本就未运行；1060 = 服务未安装。
/// 其余（含 5 = ACCESS_DENIED，旧装机无 SDDL 启停授权）→ 调用方回退 UAC 派发。
fn stop_exit_code_is_quiet(code: i32) -> bool {
    matches!(code, 0 | 1060 | 1062)
}

/// 退出收摊的静默停服（2026-09-11 需求方反馈：UAC 弹窗 + 可见脚本窗不可接受）。
/// 直接 `sc stop`（CREATE_NO_WINDOW）：装服务时 mesh-service.ps1 已 sdset 授予
/// 交互用户启/停权，此处免提权。返回 false = 需回退提权派发（旧装机未重装、
/// 授权缺失）；Err 仅记录用。全程异步语义不变：不等待服务真正停止（SCM 异步
/// 处理），退出流程不被阻塞。
pub fn stop_service_silent() -> Result<bool, String> {
    use std::os::windows::process::CommandExt;

    let out = std::process::Command::new("sc.exe")
        .args(["stop", SERVICE_NAME])
        .creation_flags(crate::scripts::CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("sc stop 启动失败：{e}"))?;
    let code = out.status.code().unwrap_or(-1);
    Ok(stop_exit_code_is_quiet(code))
}

// ── 组网诊断（T16，AC13；plan §5.1 契约）──────────────────────────────────
//
// T14 真机联调产出：用户排障需要工具化的自查入口。实测断点类型——网络拦截
// 节点端口（refused/timeout）、成员未获虚拟 IP（宿主机静态形态下网内无
// DHCP）、服务未装/停止、DNS 未对齐。六项独立不阻断；detail 为数据摘要
// （host:port=状态 / 计数 / 解析值），构造上不读密钥内容（AC8）。

/// 诊断单项结论（前端 `mesh.diag.<code>.ok|bad` 双语文案 + detail 摘要小字）
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagItem {
    /// 稳定码：service / secret / peer_reachable / members / local_nic /
    /// domain_chain（顺序即 [`DIAG_CODES`]）
    pub code: &'static str,
    pub ok: bool,
    /// 数据摘要（不含密钥——AC8；None = 无补充，序列化时省略）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// 六项稳定码（顺序 = 诊断与前端渲染顺序；单测锁定与各 judge 产物一致）
#[cfg(test)] // 诊断内核以字面量产出各码；此表仅单测编译锁顺序契约
pub const DIAG_CODES: [&str; 6] = [
    "service",
    "secret",
    "peer_reachable",
    "members",
    "local_nic",
    "domain_chain",
];

/// 对端 URI → (host, port)（诊断探测用；scheme 剥离、去路径，无端口默认
/// [`MESH_LISTEN_PORT`]）
fn peer_host_port(uri: &str) -> Option<(String, u16)> {
    let (_, rest) = uri.split_once("://")?;
    let host_port = rest.split('/').next()?;
    let (host, port) = match host_port.rsplit_once(':') {
        Some((h, p)) => (h, p.parse::<u16>().ok()?),
        None => (host_port, MESH_LISTEN_PORT),
    };
    (!host.is_empty()).then(|| (host.to_string(), port))
}

/// TCP 连接错误分类（refused=端口无服务或被网络拦截；timeout=疑似被拦——
/// T14 实测企业 WiFi 以 RST 拒绝出网，两者文案都给换网对照建议）
fn classify_connect(err: &std::io::Error) -> &'static str {
    match err.kind() {
        std::io::ErrorKind::ConnectionRefused => "refused",
        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock => "timeout",
        _ => "error",
    }
}

/// 单条 TCP 连通探测（3s 超时；域名解析失败归 `resolve`，多地址任一连通即过）
fn probe_tcp(host: &str, port: u16) -> Result<(), &'static str> {
    use std::net::ToSocketAddrs;

    let addrs: Vec<_> = (host, port)
        .to_socket_addrs()
        .map_err(|_| "resolve")?
        .collect();
    let mut last = "error";
    for addr in addrs {
        match std::net::TcpStream::connect_timeout(&addr, Duration::from_secs(3)) {
            Ok(_) => return Ok(()),
            Err(e) => last = classify_connect(&e),
        }
    }
    Err(last)
}

/// ①服务态判定（running/startPending 算通过——SCM 拉起中宽限；
/// detail = 服务态 camelCase 稳定值）
fn service_judge(state: MeshServiceState) -> DiagItem {
    let ok = matches!(state, MeshServiceState::Running | MeshServiceState::StartPending);
    let detail = serde_json::to_string(&state)
        .ok()
        .map(|s| s.trim_matches('"').to_string());
    DiagItem { code: "service", ok, detail }
}

/// ②密钥就绪判定（只测存在性，不读内容——AC8）
fn secret_judge(present: bool) -> DiagItem {
    DiagItem { code: "secret", ok: present, detail: None }
}

/// ③对端可达聚合（results = host, port, 状态码；空列表视为配置未就绪）
fn peers_judge(results: &[(String, u16, &'static str)]) -> DiagItem {
    if results.is_empty() {
        return DiagItem { code: "peer_reachable", ok: false, detail: Some("no_peers".into()) };
    }
    let ok = results.iter().all(|(_, _, r)| *r == "ok");
    let detail = results
        .iter()
        .map(|(h, p, r)| format!("{h}:{p}={r}"))
        .collect::<Vec<_>>()
        .join("; ");
    DiagItem { code: "peer_reachable", ok, detail: Some(detail) }
}

/// ④成员清单聚合（非本机成员；**无虚拟 IP 的成员 = 未完成地址分配**——
/// T14 实锤的 DHCP 形态问题，指引成员端改手动静态 IP）
fn members_judge(peers: &[PeerBrief]) -> DiagItem {
    let remote: Vec<&PeerBrief> = peers.iter().filter(|p| !p.is_local).collect();
    if remote.is_empty() {
        return DiagItem { code: "members", ok: false, detail: Some("no_member".into()) };
    }
    let no_addr = remote.iter().filter(|p| p.ipv4.is_none()).count();
    let detail = Some(format!("members={} no_addr={}", remote.len(), no_addr));
    DiagItem { code: "members", ok: no_addr == 0, detail }
}

/// ⑤本机虚拟网卡判定（虚拟 IP 已配置在本机网卡上）
fn local_nic_judge(addrs: &[Ipv4Addr], virtual_ip: &str) -> DiagItem {
    let ok = virtual_ip
        .parse::<Ipv4Addr>()
        .map(|ip| addrs.contains(&ip))
        .unwrap_or(false);
    DiagItem { code: "local_nic", ok, detail: None }
}

/// ⑥域名链路判定（纯函数：resolved = 本机视角公网解析的 v4 值，
/// tcp443 = 虚拟 IP:443 实测连通）
fn domain_chain_judge(resolved: Option<Ipv4Addr>, tcp443: bool, expect: Ipv4Addr) -> DiagItem {
    let detail = match resolved {
        None => Some("resolve_failed".to_string()),
        Some(ip) if ip != expect => Some(format!("resolved={ip}")),
        Some(_) if !tcp443 => Some("tcp443_unreachable".into()),
        Some(_) => Some(expect.to_string()),
    };
    DiagItem { code: "domain_chain", ok: resolved == Some(expect) && tcp443, detail }
}

/// ③的探测段：逐条 peers TCP connect（URI 解析失败的条目标 invalid）
fn diag_peers(peers: &[String]) -> DiagItem {
    let results = peers
        .iter()
        .map(|uri| match peer_host_port(uri) {
            Some((h, p)) => {
                let r = match probe_tcp(&h, p) {
                    Ok(()) => "ok",
                    Err(code) => code,
                };
                (h, p, r)
            }
            None => (uri.clone(), 0, "invalid"),
        })
        .collect::<Vec<_>>();
    peers_judge(&results)
}

/// ⑥的探测段：域名解析（首个 v4）+ 虚拟 IP:443 直连实测（不经域名——
/// 解析错时仍能区分「DNS 没对齐」与「服务/防火墙不通」）
fn diag_domain_chain(domain: &str, virtual_ip: &str) -> DiagItem {
    use std::net::ToSocketAddrs;

    let expect = match virtual_ip.parse::<Ipv4Addr>() {
        Ok(ip) => ip,
        Err(_) => return domain_chain_judge(None, false, Ipv4Addr::UNSPECIFIED),
    };
    if domain.trim().is_empty() {
        return domain_chain_judge(None, false, expect);
    }
    let resolved: Option<Ipv4Addr> = format!("{domain}:443")
        .to_socket_addrs()
        .ok()
        .and_then(|mut addrs| {
            addrs.find_map(|a| match a.ip() {
                std::net::IpAddr::V4(v4) => Some(v4),
                _ => None,
            })
        });
    let tcp443 = probe_tcp(virtual_ip, 443).is_ok();
    domain_chain_judge(resolved, tcp443, expect)
}

/// 组网诊断内核（AC13 六项，[`DIAG_CODES`] 顺序；各项独立不阻断）。探测含
/// 子进程与网络 IO（逐对端 3s 超时），命令层以 spawn_blocking 调用。
/// `domain` 传完整域名（未设置传空串——⑥按解析失败口径报）。
pub fn run_diagnostics(cfg: &MeshConfig, stack_dir: &str, domain: &str) -> Vec<DiagItem> {
    let ops = WindowsMeshOps::new(None, stack_dir.to_string());
    vec![
        service_judge(ops.service_state()),
        secret_judge(read_network_secret(stack_dir).is_some()),
        diag_peers(&cfg.peers),
        members_judge(&ops.query_peers().unwrap_or_default()),
        local_nic_judge(&local_ipv4_addrs(), &cfg.virtual_ip),
        diag_domain_chain(domain, &cfg.virtual_ip),
    ]
}

/// 配置生效流水线的磁盘段（T9 命令内核，纯 IO 可直测）：
/// 密钥就绪检查（AC9 拒绝前置）→ 渲染（校验 AC11）→ 二进制缺失时落位 →
/// 写 config.toml → `--check-config` 办后校验。返回 (config 路径, core exe)。
/// 校验失败时 config 已写盘——调用方不得派发重启，旧配置继续服务，修好再 apply。
pub fn prepare_stack(
    cfg: &MeshConfig,
    stack_dir: &str,
    src_bin: Option<&Path>,
) -> Result<(PathBuf, PathBuf), String> {
    // 顺序：先验密钥与配置（纯内存），再动盘——AC9 的拒绝必须先于任何落盘
    let secret = read_network_secret(stack_dir)
        .ok_or("组网密钥未配置：请先运行 set-mesh-secret.ps1 写入密钥（设置区有入口指引）")?;
    let toml = render_config_checked(cfg, &secret)?;
    let et_dir = Path::new(stack_dir).join(MESH_DIR);
    let core_exe = et_dir.join(EASYTIER_CORE_EXE_NAME);
    if !core_exe.is_file() {
        let src = src_bin.ok_or_else(|| {
            format!(
                "easytier 二进制未落位且资源脚本目录不可用：请重装工作台，或手动复制 easytier 五文件到 {}",
                et_dir.display()
            )
        })?;
        stage_easytier_binaries(src, stack_dir)?;
    }
    std::fs::create_dir_all(&et_dir)
        .map_err(|e| format!("创建 {} 失败：{e}", et_dir.display()))?;
    let config_path = et_dir.join(CONFIG_FILE);
    std::fs::write(&config_path, &toml)
        .map_err(|e| format!("写入 {} 失败：{e}", config_path.display()))?;
    check_config(&core_exe, &config_path)?;
    Ok((config_path, core_exe))
}

// ── config.toml 渲染（plan §4.3；T2 实测字段形态）──────────────────────────

/// TOML 渲染模板（字段顺序即产出顺序：标量在前、表在后——toml crate 要求）。
/// **无 [secure_mode] 段**：legacy 产品形态固化（T2 实测定案，plan §1 降级决策；
/// 升级路径注释见 plan §4.3——加 [secure_mode] enabled+成对 X25519 密钥即可）。
#[derive(Serialize)]
struct ConfigToml<'a> {
    hostname: &'a str,
    ipv4: &'a str,
    dhcp: bool,
    latency_first: bool,
    listeners: Vec<String>,
    network_identity: NetworkIdentityToml<'a>,
    /// serde 重命名 peer → 产出 [[peer]] 数组表（peers 逐条）
    #[serde(rename = "peer")]
    peers: Vec<PeerToml<'a>>,
}

#[derive(Serialize)]
struct NetworkIdentityToml<'a> {
    network_name: &'a str,
    network_secret: &'a str,
}

#[derive(Serialize)]
struct PeerToml<'a> {
    uri: &'a str,
}

/// 渲染 config.toml（纯函数）。调用方须先过 [`render_config_checked`]
/// （密钥就绪 + 配置校验），直接调用本函数仅用于测试。
fn render_config(cfg: &MeshConfig, secret: &str) -> Result<String, String> {
    let doc = ConfigToml {
        hostname: MESH_HOSTNAME,
        ipv4: &cfg.virtual_ip,
        dhcp: false,
        latency_first: true,
        listeners: vec![
            format!("tcp://0.0.0.0:{MESH_LISTEN_PORT}"),
            format!("udp://0.0.0.0:{MESH_LISTEN_PORT}"),
        ],
        network_identity: NetworkIdentityToml {
            network_name: &cfg.network_name,
            network_secret: secret,
        },
        peers: cfg
            .peers
            .iter()
            .map(|uri| PeerToml { uri })
            .collect::<Vec<_>>(),
    };
    toml::to_string_pretty(&doc).map_err(|e| format!("config.toml 渲染失败：{e}"))
}

/// 渲染入口（AC9 新语义：密钥就绪强制）。network_secret 缺失/为空 → 拒绝渲染
/// 并给脚本指引；配置字段非法 → 拒绝（AC11 校验）。secret 仅进入返回的文件
/// 内容（AC8：文件注入、命令行无密钥——binPath 由 T7 构造并断言）。
pub fn render_config_checked(cfg: &MeshConfig, secret: &str) -> Result<String, String> {
    let secret = secret.trim();
    if secret.is_empty() {
        return Err("组网密钥未配置：请先运行 set-mesh-secret.ps1 写入密钥（设置区有入口指引）".into());
    }
    validate_mesh_config(cfg)?;
    render_config(cfg, secret)
}

/// 读栈目录 network-secret 文件（None = 未配置 → 调用方拒绝渲染，AC9）
pub fn read_network_secret(stack_dir: &str) -> Option<String> {
    let path = Path::new(stack_dir).join(MESH_DIR).join(NETWORK_SECRET_FILE);
    let content = std::fs::read_to_string(path).ok()?;
    let trimmed = content.trim().to_string();
    (!trimmed.is_empty()).then_some(trimmed)
}

// ── MeshConfig 校验（AC11：非空/IP/网段/peers URI）─────────────────────────

/// 校验组网配置（纯函数）：网络名非空、虚拟 IP 合法且属于虚拟网段、
/// CIDR 格式合法、peers 非空且每条为 `scheme://host[:port]` 形态。
pub fn validate_mesh_config(cfg: &MeshConfig) -> Result<(), String> {
    if cfg.network_name.trim().is_empty() {
        return Err("网络名不能为空".into());
    }
    let ip: Ipv4Addr = cfg
        .virtual_ip
        .parse()
        .map_err(|_| format!("虚拟 IP 不合法：{}", cfg.virtual_ip))?;
    let cidr = parse_cidr(&cfg.virtual_cidr)
        .ok_or_else(|| format!("虚拟网段不合法（应为 x.x.x.x/nn 形态）：{}", cfg.virtual_cidr))?;
    if !cidr_contains(cidr, ip) {
        return Err(format!(
            "虚拟 IP {} 不在网段 {} 内",
            cfg.virtual_ip, cfg.virtual_cidr
        ));
    }
    if cfg.peers.is_empty() {
        return Err("对端节点列表不能为空（至少一条，默认社区节点）".into());
    }
    for uri in &cfg.peers {
        if !valid_peer_uri(uri) {
            return Err(format!("对端 URI 不合法（应为 scheme://host:port）：{uri}"));
        }
    }
    Ok(())
}

/// 对端 URI 宽松校验：`scheme://host[:port]`，无空白（格式细节由 easytier
/// --check-config 办后校验兜底，此处只拦明显错形）
fn valid_peer_uri(uri: &str) -> bool {
    match uri.split_once("://") {
        Some((scheme, rest)) => {
            !scheme.is_empty() && !rest.is_empty() && !uri.chars().any(char::is_whitespace)
        }
        None => false,
    }
}

// ── 网段冲突检测（plan §7-R7 / §6）─────────────────────────────────────────

/// 解析 CIDR 字符串 → (网络地址, 前缀长度)；非法返回 None
fn parse_cidr(s: &str) -> Option<(Ipv4Addr, u8)> {
    let (ip, prefix) = s.split_once('/')?;
    let ip: Ipv4Addr = ip.parse().ok()?;
    let prefix: u8 = prefix.parse().ok()?;
    (prefix <= 32).then_some((ip, prefix))
}

/// IP 是否落在 CIDR 内（prefix=0 全含；防 32-prefix=32 的移位溢出）
fn cidr_contains((net, prefix): (Ipv4Addr, u8), ip: Ipv4Addr) -> bool {
    if prefix == 0 {
        return true;
    }
    let mask = u32::MAX << (32 - prefix);
    u32::from(net) & mask == u32::from(ip) & mask
}

/// 虚拟网段 × 物理网卡 IPv4 冲突检测（纯函数，返回冲突的物理 IP 列表）。
///
/// 物理网卡前缀长度不可零成本获取，取保守近似双判据：
/// ① 物理 IP 落在虚拟 CIDR 内（精确，覆盖 /24 常规）；
/// ② 物理 IP 与虚拟 IP 的私有段自然前缀重合（10.x/172.16-31.x 比前两段、
///   192.168.x 比前三段——对 /16 以上大网保守报冲突）。
/// 已知漏报：物理 10.0.0.0/8 大网 + 虚拟 10.126.x（前两段不同）——个人机
/// 罕见，T2 实测本机 /24；网段可编辑兜底。
pub fn detect_subnet_conflict(
    physical: &[Ipv4Addr],
    virtual_ip: Ipv4Addr,
    virtual_cidr: &str,
) -> Vec<Ipv4Addr> {
    let Some(cidr) = parse_cidr(virtual_cidr) else {
        return Vec::new(); // 格式非法由 validate_mesh_config 报，此处不重复
    };
    physical
        .iter()
        .copied()
        .filter(|&ip| cidr_contains(cidr, ip) || same_private_prefix(ip, virtual_ip))
        .collect()
}

/// 私有段自然前缀重合判定（判据②的粒度：A/B 类私有段比前两段，C 类比前三段）
fn same_private_prefix(a: Ipv4Addr, b: Ipv4Addr) -> bool {
    let (a, b) = (u32::from(a).to_be_bytes(), u32::from(b).to_be_bytes());
    let a_private = |o: [u8; 4]| {
        if o[0] == 10 {
            2
        } else if o[0] == 172 && (16..=31).contains(&o[1]) {
            2
        } else if o[0] == 192 && o[1] == 168 {
            3
        } else {
            0
        }
    };
    match (a_private(a), a_private(b)) {
        (2, 2) => a[1] == b[1],
        (3, 3) => a[2] == b[2],
        _ => false,
    }
}

/// 本机物理网卡 IPv4 列表（生产侧采集；单测注入固定列表走纯函数）。
/// `list_afinet_netifas` 返回 (网卡名, IpAddr)，过滤回环与 IPv6。
#[cfg(windows)]
pub fn local_ipv4_addrs() -> Vec<Ipv4Addr> {
    local_ip_address::list_afinet_netifas()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(_, addr)| match addr {
            std::net::IpAddr::V4(v4) if !v4.is_loopback() => Some(v4),
            _ => None,
        })
        .collect()
}

// ── --check-config 办后校验（T9 mesh_apply_config 调用）────────────────────

/// 调 `easytier-core --check-config` 验证配置文件（办后校验，plan §5.1）。
/// GUI 子系统进程：stdout 不附着控制台，但 stderr 管道可读（T2 实测重定向
/// 可捕获日志）；判定以退出码为准，stderr 摘要做失败原因。
pub fn check_config(core_exe: &Path, config_path: &Path) -> Result<(), String> {
    let out = std::process::Command::new(core_exe)
        .arg("--check-config")
        .arg("-c")
        .arg(config_path)
        .output()
        .map_err(|e| format!("easytier-core 启动失败（{}）：{e}", core_exe.display()))?;
    if out.status.success() {
        Ok(())
    } else {
        let detail = String::from_utf8_lossy(&out.stderr);
        let detail = detail.lines().last().unwrap_or("").trim();
        Err(format!("配置校验未通过：{detail}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 随包目录实测：resources/bin 五文件哈希应全部通过（真文件，发布构建同源）
    #[test]
    fn bundled_binaries_pass_verification() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("bin");
        assert!(dir.is_dir(), "resources/bin 不存在：{dir:?}");
        verify_easytier_binaries(&dir).expect("随包 easytier 文件校验应通过");
    }

    /// 文件缺失 → 返回含文件名的可读错误（不 panic）
    #[test]
    fn missing_file_reports_error() {
        let dir = std::env::temp_dir().join("et-verify-missing-test");
        std::fs::create_dir_all(&dir).unwrap();
        let err = verify_easytier_binaries(&dir)
            .expect_err("空目录校验应失败");
        assert!(
            err.contains(EASYTIER_CORE_EXE_NAME),
            "错误信息应含文件名：{err}"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// 哈希不符（内容被篡改）→ 拒绝并给出期望/实际值
    #[test]
    fn tampered_file_fails_hash() {
        let dir = std::env::temp_dir().join("et-verify-tamper-test");
        std::fs::create_dir_all(&dir).unwrap();
        for name in [
            EASYTIER_CORE_EXE_NAME,
            EASYTIER_CLI_EXE_NAME,
            WINTUN_DLL_NAME,
            PACKET_DLL_NAME,
            WINDIVERT_SYS_NAME,
        ] {
            std::fs::write(dir.join(name), b"tampered").unwrap();
        }
        let err = verify_easytier_binaries(&dir)
            .expect_err("篡改内容校验应失败");
        assert!(err.contains("SHA256 校验不符"), "应报哈希不符：{err}");
        std::fs::remove_dir_all(&dir).ok();
    }

    // ── T4：渲染产物断言 ──────────────────────────────────────────────────

    fn sample_config() -> crate::settings::MeshConfig {
        crate::settings::MeshConfig {
            network_name: "office-net".into(),
            virtual_ip: "10.126.126.1".into(),
            virtual_cidr: "10.126.126.0/24".into(),
            peers: vec![
                "tcp://sh.vomiku.com:7910".into(),
                "udp://backup.example.com:11010".into(),
            ],
        }
    }

    /// 渲染产物：实测字段形态齐全（hostname/ipv4/dhcp/listeners/[network_identity]/
    /// [[peer]] 逐条），secret 进入文件内容（AC8：文件注入）
    #[test]
    fn rendered_config_contains_all_fields() {
        let toml = render_config_checked(&sample_config(), "  s3cret-value  \n")
            .expect("合法配置应渲染成功");
        // secret 前后空白被规整后注入
        assert!(toml.contains("network_secret = \"s3cret-value\""), "{toml}");
        assert!(toml.contains("network_name = \"office-net\""), "{toml}");
        assert!(toml.contains("hostname = \"ai-remote-workbench\""), "{toml}");
        assert!(toml.contains("ipv4 = \"10.126.126.1\""), "{toml}");
        assert!(toml.contains("dhcp = false"), "{toml}");
        assert!(toml.contains("latency_first = true"), "{toml}");
        assert!(toml.contains("tcp://0.0.0.0:11010"), "listeners 收敛 tcp：{toml}");
        assert!(toml.contains("udp://0.0.0.0:11010"), "listeners 收敛 udp：{toml}");
        assert_eq!(toml.matches("[[peer]]").count(), 2, "peers 逐条产出：{toml}");
        assert!(toml.contains("tcp://sh.vomiku.com:7910"), "{toml}");
        assert!(toml.contains("udp://backup.example.com:11010"), "{toml}");
    }

    /// legacy 产品形态固化：渲染产物不含 [secure_mode] 段（T2 实测定案，
    /// AC9 新语义落点；升级路径见 plan §4.3 注释）
    #[test]
    fn rendered_config_has_no_secure_mode_section() {
        let toml = render_config_checked(&sample_config(), "s3cret").unwrap();
        assert!(!toml.contains("secure_mode"), "legacy 形态不应含 secure_mode：{toml}");
    }

    /// AC9 新语义：secret 缺失/空白 → 拒绝渲染 + 脚本指引
    #[test]
    fn render_rejects_missing_secret_with_guidance() {
        for empty in ["", "   ", "\n"] {
            let err = render_config_checked(&sample_config(), empty)
                .expect_err("空密钥应拒绝");
            assert!(err.contains("set-mesh-secret.ps1"), "错误应含脚本指引：{err}");
        }
    }

    /// AC11 校验分支：网络名空 / IP 非法 / IP 不在网段 / peers 空 / URI 错形
    #[test]
    fn validate_mesh_config_rejects_bad_fields() {
        let mut cfg = sample_config();

        cfg.network_name = "  ".into();
        assert!(validate_mesh_config(&cfg).unwrap_err().contains("网络名"));

        cfg = sample_config();
        cfg.virtual_ip = "10.126.126.999".into();
        assert!(validate_mesh_config(&cfg).unwrap_err().contains("虚拟 IP"));

        cfg = sample_config();
        cfg.virtual_cidr = "10.200.0.0/24".into();
        assert!(validate_mesh_config(&cfg).unwrap_err().contains("不在网段"));

        cfg = sample_config();
        cfg.virtual_cidr = "10.126.126.0/33".into();
        assert!(validate_mesh_config(&cfg).unwrap_err().contains("网段不合法"));

        cfg = sample_config();
        cfg.peers = vec![];
        assert!(validate_mesh_config(&cfg).unwrap_err().contains("对端"));

        cfg = sample_config();
        cfg.peers = vec!["sh.vomiku.com:7910".into()];
        assert!(validate_mesh_config(&cfg).unwrap_err().contains("URI"));

        // 默认配置本身合法（默认值体检）
        assert!(validate_mesh_config(&crate::settings::MeshConfig::default()).is_ok());
    }

    // ── T4：网段冲突检测 ──────────────────────────────────────────────────

    fn ip(s: &str) -> Ipv4Addr {
        s.parse().unwrap()
    }

    /// 判据①：物理 IP 落在虚拟 CIDR 内（含 /24 之外的边界：/16、/32、/0）
    #[test]
    fn conflict_when_physical_ip_inside_virtual_cidr() {
        let vip = ip("10.126.126.1");
        let hits = detect_subnet_conflict(&[ip("192.168.3.13"), ip("10.126.126.5")], vip, "10.126.126.0/24");
        assert_eq!(hits, vec![ip("10.126.126.5")]);

        // /16 网段：相邻 /24 不冲突、同 /16 冲突
        assert!(detect_subnet_conflict(&[ip("10.126.200.5")], vip, "10.126.0.0/16").len() == 1);
        assert!(detect_subnet_conflict(&[ip("10.127.0.5")], vip, "10.126.0.0/16").is_empty());

        // 边界：/32 只含自身、/0 全含
        assert!(detect_subnet_conflict(&[ip("1.2.3.4")], vip, "10.126.126.1/32").is_empty());
        assert_eq!(detect_subnet_conflict(&[ip("1.2.3.4")], vip, "0.0.0.0/0").len(), 1);
    }

    /// 判据②：私有段自然前缀重合（大网保守近似）；跨私有段不误伤
    #[test]
    fn conflict_by_private_prefix_heuristic() {
        let vip = ip("10.126.126.1");
        // 物理 10.126.x（前两段同）→ 保守报冲突（真实掩码可能 /8）
        assert_eq!(
            detect_subnet_conflict(&[ip("10.126.3.5")], vip, "10.126.126.0/24"),
            vec![ip("10.126.3.5")]
        );
        // 物理 10.0.0.x（前两段不同）→ 不冲突（/8 大网漏报已知，注释声明）
        assert!(detect_subnet_conflict(&[ip("10.0.0.5")], vip, "10.126.126.0/24").is_empty());
        // 跨私有段：192.168 / 172.16 与 10.x 互不干扰
        assert!(detect_subnet_conflict(&[ip("192.168.3.13"), ip("172.17.0.1")], vip, "10.126.126.0/24").is_empty());
        // 192.168 同 C 段（虚拟若配 192.168.x）
        assert_eq!(
            detect_subnet_conflict(&[ip("192.168.3.13")], ip("192.168.3.1"), "192.168.3.0/24"),
            vec![ip("192.168.3.13")]
        );
        // T2-④ 实测形态：本机 192.168.3.13 × 默认虚拟段 → 无冲突
        assert!(detect_subnet_conflict(&[ip("192.168.3.13")], vip, "10.126.126.0/24").is_empty());
    }

    // ── T4：secret 文件读取 ───────────────────────────────────────────────

    /// 栈目录 network-secret 读取（AC8 注入路径）；缺失/空 → None
    #[test]
    fn read_network_secret_from_stack_dir() {
        let dir = std::env::temp_dir().join("et-secret-test");
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(read_network_secret(dir.to_str().unwrap()), None, "目录不存在 → None");

        std::fs::create_dir_all(dir.join(MESH_DIR)).unwrap();
        std::fs::write(dir.join(MESH_DIR).join(NETWORK_SECRET_FILE), "  topsecret\n").unwrap();
        assert_eq!(read_network_secret(dir.to_str().unwrap()).as_deref(), Some("topsecret"));

        std::fs::write(dir.join(MESH_DIR).join(NETWORK_SECRET_FILE), "   \n").unwrap();
        assert_eq!(read_network_secret(dir.to_str().unwrap()), None, "空白视为未配置");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── T4：--check-config 集成（真 exe + 渲染产物端到端，防字段名写错）────

    /// 渲染产物过 easytier-core --check-config（resources/bin 真 exe）：
    /// 字段名/结构若与 v2.6.4 不符在此暴露（GUI 子系统退出码判定）
    #[test]
    fn rendered_config_passes_easytier_check_config() {
        let dir = std::env::temp_dir().join("et-checkconfig-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let toml = render_config_checked(&crate::settings::MeshConfig::default(), "it-is-a-secret")
            .expect("渲染失败");
        let cfg_path = dir.join(CONFIG_FILE);
        std::fs::write(&cfg_path, &toml).unwrap();
        let core = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("bin")
            .join(EASYTIER_CORE_EXE_NAME);
        let result = check_config(&core, &cfg_path);
        let _ = std::fs::remove_dir_all(&dir);
        // 校验失败时带 easytier 的 stderr 摘要，便于诊断字段名回归
        result.unwrap_or_else(|e| panic!("渲染产物应通过 --check-config：{e}\n---\n{toml}"));
    }

    // ── T7：服务 binPath 构造 + 栈目录落位 ─────────────────────────────────

    /// AC8 断言：binPath 只含路径参数（-c/-r/--file-log-dir），连 secret 文件名
    /// 都不引用——密钥只经 config.toml 注入（渲染器职责），命令行无密钥
    #[test]
    fn bin_path_is_pure_paths_without_secret() {
        let bp = service_bin_path(r"D:\Software\cloudcli-https");
        assert!(
            bp.starts_with(r#""D:\Software\cloudcli-https\easytier\easytier-core.exe""#),
            "{bp}"
        );
        assert!(bp.contains(r#"-c "D:\Software\cloudcli-https\easytier\config.toml""#), "{bp}");
        assert!(bp.contains("-r 127.0.0.1:15888"), "{bp}");
        assert!(bp.contains(r#"--file-log-dir "D:\Software\cloudcli-https\easytier\logs""#), "{bp}");
        assert!(
            !bp.to_ascii_lowercase().contains("secret"),
            "binPath 不得含 secret 字样（AC8）：{bp}"
        );
        assert_eq!(bp.matches('"').count(), 6, "三个路径各自成对引号（RPC 为字面量）：{bp}");
    }

    /// 栈目录可配置：含空格时每个路径参数整体加引号，SCM 命令行不拆断
    #[test]
    fn bin_path_quotes_paths_with_spaces() {
        let bp = service_bin_path(r"D:\My Apps\stack");
        assert!(bp.contains(r#""D:\My Apps\stack\easytier\config.toml""#), "{bp}");
        assert!(bp.contains(r#""D:\My Apps\stack\easytier\logs""#), "{bp}");
    }

    /// 落位（真资源目录 → 临时栈目录）：五文件齐 + logs 预建 + 落位后哈希过 + 幂等
    #[test]
    fn staging_copies_files_and_passes_verification() {
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("resources").join("bin");
        let stack = std::env::temp_dir().join("et-stage-test");
        let _ = std::fs::remove_dir_all(&stack);
        let dest =
            stage_easytier_binaries(&src, stack.to_str().unwrap()).expect("落位应成功");
        assert_eq!(dest, stack.join(MESH_DIR));
        for name in MESH_BIN_FILES {
            assert!(dest.join(name).is_file(), "落位后缺 {name}");
        }
        assert!(dest.join(LOG_DIR).is_dir(), "logs 目录应预建");
        verify_easytier_binaries(&dest).expect("落位后校验应通过");
        // 幂等：重复落位（覆盖复制）不报错
        stage_easytier_binaries(&src, stack.to_str().unwrap()).expect("重复落位应成功");
        let _ = std::fs::remove_dir_all(&stack);
    }

    /// 落位拒绝篡改源：复制前哈希校验先行（AC1 供应链防线延伸到复制环节）
    #[test]
    fn staging_rejects_tampered_source() {
        let src = std::env::temp_dir().join("et-stage-badsrc");
        let _ = std::fs::remove_dir_all(&src);
        std::fs::create_dir_all(&src).unwrap();
        for name in MESH_BIN_FILES {
            std::fs::write(src.join(name), b"tampered").unwrap();
        }
        let err = stage_easytier_binaries(&src, "unused").expect_err("篡改源应拒绝");
        assert!(err.contains("SHA256"), "{err}");
        let _ = std::fs::remove_dir_all(&src);
    }

    // ── T9：apply 动作选择 + install 派发参数 + 生效流水线 ────────────────

    /// apply 动作选择：仅未装走 install（幂等建档），其余 restart
    #[test]
    fn apply_action_selects_install_only_when_missing() {
        assert_eq!(apply_service_action(MeshServiceState::NotFound), "install");
        for s in [
            MeshServiceState::Running,
            MeshServiceState::StartPending,
            MeshServiceState::Stopped,
            MeshServiceState::Disabled,
        ] {
            assert_eq!(apply_service_action(s), "restart", "{s:?}");
        }
    }

    /// 静默停服退出码映射：0/1060/1062 免兜底，其余（含 5 拒绝访问）回退 UAC
    #[test]
    fn stop_exit_code_quiet_mapping() {
        assert!(stop_exit_code_is_quiet(0), "受理");
        assert!(stop_exit_code_is_quiet(1060), "未安装");
        assert!(stop_exit_code_is_quiet(1062), "未运行");
        assert!(!stop_exit_code_is_quiet(5), "ACCESS_DENIED → 兜底");
        assert!(!stop_exit_code_is_quiet(1053), "异常 → 兜底");
    }

    /// install 派发参数：-Action install 且**不含 -BinPath**（真机缺陷教训：
    /// binPath 内嵌双引号过不了 -Command 包裹，由脚本按同公式自建——2026-09-11）
    #[test]
    fn install_params_omit_bin_path() {
        let dir = Path::new(r"C:\app\resources\bin");
        let stack = r"D:\Software\cloudcli-https";
        let params = service_install_params(dir, stack, crate::lang::Lang::Zh);
        assert!(params.contains("-Action install"), "{params}");
        assert!(!params.contains("-BinPath"), "binPath 必须由脚本自建：{params}");
        // binPath 内容不得经命令行传递（外层 -Command "..." 的包裹引号除外）
        assert!(
            !params.contains("easytier-core.exe"),
            "binPath 片段不得出现在派发参数：{params}"
        );
        // 形态契约锚仍在：脚本公式同源（service_bin_path 单测锁 AC8）
        let bp = service_bin_path(stack);
        assert!(!bp.to_ascii_lowercase().contains("secret"), "AC8：{bp}");
    }

    /// AC9：密钥缺失拒绝先于任何落盘（easytier 目录都不建）
    #[test]
    fn prepare_stack_rejects_missing_secret_before_any_write() {
        let stack = std::env::temp_dir().join("et-apply-nosecret");
        let _ = std::fs::remove_dir_all(&stack);
        std::fs::create_dir_all(&stack).unwrap();
        let err = prepare_stack(&sample_config(), stack.to_str().unwrap(), None)
            .expect_err("无密钥应拒绝");
        assert!(err.contains("set-mesh-secret"), "指引脚本名：{err}");
        assert!(
            !stack.join(MESH_DIR).exists(),
            "拒绝必须先于落盘（AC9 拒绝启动语义）"
        );
        let _ = std::fs::remove_dir_all(&stack);
    }

    /// 生效流水线端到端（真资源 bin 落位 + 真 exe --check-config）：
    /// config.toml 产出且含渲染内容
    #[test]
    fn prepare_stack_renders_stages_and_checks() {
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("resources").join("bin");
        let stack = std::env::temp_dir().join("et-apply-ok");
        let _ = std::fs::remove_dir_all(&stack);
        std::fs::create_dir_all(stack.join(MESH_DIR)).unwrap();
        std::fs::write(stack.join(MESH_DIR).join(NETWORK_SECRET_FILE), "it-is-a-secret\n").unwrap();
        let (cfg_path, core_exe) =
            prepare_stack(&sample_config(), stack.to_str().unwrap(), Some(&src))
                .expect("合法输入应通过");
        assert!(cfg_path.is_file() && core_exe.is_file());
        let content = std::fs::read_to_string(&cfg_path).unwrap();
        assert!(content.contains("network_name = \"office-net\""), "{content}");
        let _ = std::fs::remove_dir_all(&stack);
    }

    // ── T8：状态判定矩阵（AC4：四态 + Inactive + detail 稳定码）──────────

    /// 构造对端摘要的便捷函数（测试专用形态）
    fn peer(hostname: &str, is_local: bool) -> PeerBrief {
        PeerBrief {
            hostname: hostname.into(),
            ipv4: (!is_local).then(|| "10.126.126.2".into()),
            latency_ms: (!is_local).then(|| 46.62),
            loss_rate: (!is_local).then(|| 0.0),
            is_local,
        }
    }

    #[test]
    fn judge_mesh_state_full_matrix() {
        use MeshServiceState as S;
        use MeshState as M;

        // 非现役通道：其余输入一概不看（对齐 TunnelState::Inactive 呈现惯例）
        for service in [S::Running, S::Stopped] {
            assert_eq!(
                judge_mesh_state(false, true, service, &[peer("x", false)], true),
                (M::Inactive, None)
            );
        }
        // 密钥未写入 → NotConfigured + 脚本指引码（AC9 前置；服务态不看）
        assert_eq!(
            judge_mesh_state(true, false, S::Running, &[peer("x", false)], true),
            (M::NotConfigured, Some(DETAIL_SECRET_MISSING))
        );
        // 现役 + 密钥就绪 + 服务未跑 → Offline 三码（未装/停止/禁用）
        assert_eq!(
            judge_mesh_state(true, true, S::NotFound, &[], false),
            (M::Offline, Some(DETAIL_SERVICE_MISSING))
        );
        assert_eq!(
            judge_mesh_state(true, true, S::Stopped, &[], false),
            (M::Offline, Some(DETAIL_SERVICE_STOPPED))
        );
        assert_eq!(
            judge_mesh_state(true, true, S::Disabled, &[], false),
            (M::Offline, Some(DETAIL_SERVICE_DISABLED))
        );
        // 服务运行 + 非本机成员 → Online（StartPending 同判：SCM 拉起中已有成员
        // 说明是重连场景，如实报在线优于强制报连接中——AC2 重连呈现）
        for service in [S::Running, S::StartPending] {
            assert_eq!(
                judge_mesh_state(true, true, service, &[peer("self", true), peer("phone", false)], true),
                (M::Online, None),
                "service={service:?}"
            );
        }
        // 服务运行 + 仅本机项（peer list 首项恒为本机）→ Connecting；
        // RPC 正常 = 正常连接中（无码），RPC 失败 = rpc_unreachable
        let local_only = [peer("self", true)];
        assert_eq!(
            judge_mesh_state(true, true, S::Running, &local_only, true),
            (M::Connecting, None)
        );
        assert_eq!(
            judge_mesh_state(true, true, S::Running, &[], true),
            (M::Connecting, None)
        );
        assert_eq!(
            judge_mesh_state(true, true, S::Running, &local_only, false),
            (M::Connecting, Some(DETAIL_RPC_UNREACHABLE))
        );
        assert_eq!(
            judge_mesh_state(true, true, S::StartPending, &[], false),
            (M::Connecting, Some(DETAIL_RPC_UNREACHABLE))
        );
    }

    // ── T8：peer JSON 宽松解析（v2.6.4 序列化 PeerTableItem 形态）────────

    /// v2.6.4 真实形态（源码取证：snake_case 字段、lat/loss 为格式化字符串、
    /// 首项恒为本机 cost="Local" lat_ms="-"）
    #[test]
    fn parse_peers_json_real_shape() {
        let json = r#"[
            {"cidr":"10.126.126.1/24","ipv4":"10.126.126.1","hostname":"ai-remote-workbench","cost":"Local","lat_ms":"-","loss_rate":"-","rx_bytes":"-","tx_bytes":"-","tunnel_proto":"-","nat_type":"FullCone","id":"1","version":"2.6.4"},
            {"cidr":"10.126.126.2/24","ipv4":"10.126.126.2","hostname":"android","cost":"1","lat_ms":"46.62","loss_rate":"0.0%","rx_bytes":"1.2 MB","tx_bytes":"512 KB","tunnel_proto":"tcp6","nat_type":"Unknown","id":"2","version":"2.6.4"}
        ]"#;
        let peers = parse_peers_json(json).expect("真实形态应解析成功");
        assert_eq!(peers.len(), 2);
        assert!(peers[0].is_local, "首项恒为本机（cost=Local）");
        assert_eq!(peers[0].latency_ms, None, "本机 lat_ms='-' → None");
        assert_eq!(peers[0].loss_rate, None, "本机 loss_rate='-' → None");
        assert_eq!(peers[0].ipv4.as_deref(), Some("10.126.126.1"));
        assert!(!peers[1].is_local);
        assert_eq!(peers[1].hostname, "android");
        assert_eq!(peers[1].latency_ms, Some(46.62));
        assert_eq!(peers[1].loss_rate, Some(0.0));
        // 在线判定输入（排除本机）与本解析天然衔接
        assert!(peers.iter().any(|p| !p.is_local));
    }

    /// 宽松性：数字形态兼容、百分号归一、空数组 Ok、非数组 Err、坏条目跳过
    #[test]
    fn parse_peers_json_lenient_edges() {
        // 数字形态（升级版本若改序列化）+ loss_rate 百分数归一
        let numeric = r#"[{"hostname":"pc","ipv4":"10.126.126.3","cost":"1","lat_ms":12.5,"loss_rate":"3.0%"}]"#;
        let peers = parse_peers_json(numeric).unwrap();
        assert_eq!(peers[0].latency_ms, Some(12.5));
        assert_eq!(peers[0].loss_rate, Some(0.03), "3.0% → 0.03");
        // 已是 0~1 的数字直接取
        let frac = r#"[{"hostname":"pc","cost":"1","lat_ms":1.0,"loss_rate":0.5}]"#;
        assert_eq!(parse_peers_json(frac).unwrap()[0].loss_rate, Some(0.5));
        // 空数组（仅本机的服务通常至少 1 项，此为防御）与空串
        assert!(parse_peers_json("[]").unwrap().is_empty());
        assert!(parse_peers_json("[ ]").unwrap().is_empty());
        // 非数组 → Err
        assert!(parse_peers_json(r#"{"peers":[]}"#).is_err());
        assert!(parse_peers_json("not json").is_err());
        // 缺 hostname 的条目跳过不阻断（宁缺毋断）
        let partial = r#"[{"cost":"Local"},{"hostname":"ok","cost":"1"}]"#;
        let peers = parse_peers_json(partial).unwrap();
        assert_eq!(peers.len(), 1);
        assert_eq!(peers[0].hostname, "ok");
        // 超界丢包率（脏数据）丢弃
        let dirty = r#"[{"hostname":"pc","cost":"1","lat_ms":"1","loss_rate":"250%"}]"#;
        assert_eq!(parse_peers_json(dirty).unwrap()[0].loss_rate, None);
    }

    // ── T8：sc.exe 输出解析（数字不本地化；其余文本 GBK 无关紧要）────────

    #[test]
    fn parse_sc_query_state_real_shapes() {
        // 中文 Windows 实测形态（行标签 ASCII、多空格缩进）
        let running = "SERVICE_NAME: EasyTierMesh\r\n\
        \x20\x20\x20\x20\x20\x20\x20\x20TYPE               : 10  WIN32_OWN_PROCESS\r\n\
        \x20\x20\x20\x20\x20\x20\x20\x20STATE              : 4  RUNNING \r\n\
        \x20\x20\x20\x20\x20\x20\x20\x20(NOT_STOPPABLE,NOT_PAUSABLE,IGNORES_SHUTDOWN)";
        assert_eq!(parse_sc_query_state(running), Some(4));
        let stopped = "SERVICE_NAME: EasyTierMesh\r\n        STATE              : 1  STOPPED ";
        assert_eq!(parse_sc_query_state(stopped), Some(1));
        let pending = "        STATE              : 2  START_PENDING ";
        assert_eq!(parse_sc_query_state(pending), Some(2));
        // 服务不存在（1060）的输出无 STATE 行 → None
        assert_eq!(parse_sc_query_state(""), None);
    }

    #[test]
    fn parse_sc_start_type_real_shapes() {
        // T7 安装形态：delayed-auto（2 AUTO_START (DELAYED)）；停用形态：4 DISABLED
        let delayed = "SERVICE_NAME: EasyTierMesh\r\n\
        \x20\x20\x20\x20\x20\x20\x20\x20START_TYPE         : 2   AUTO_START  (DELAYED)";
        assert_eq!(parse_sc_start_type(delayed), Some(2));
        let disabled = "        START_TYPE         : 4   DISABLED";
        assert_eq!(parse_sc_start_type(disabled), Some(4));
        assert_eq!(parse_sc_start_type("BINARY_PATH_NAME   : x"), None);
    }

    // ── T8：MeshMonitor tick（IO 锁外 + 变化才发事件）─────────────────────

    /// mock 探询源：服务态与 peers 可编程切换
    struct MockMeshOps {
        service: std::sync::Mutex<MeshServiceState>,
        peers: std::sync::Mutex<Result<Vec<PeerBrief>, String>>,
    }

    impl MeshOps for MockMeshOps {
        fn service_state(&self) -> MeshServiceState {
            *self.service.lock().unwrap()
        }
        fn query_peers(&self) -> Result<Vec<PeerBrief>, String> {
            self.peers.lock().unwrap().clone()
        }
    }

    /// 事件出口 mock：记录全部快照
    #[derive(Default)]
    struct RecordingSink {
        emitted: std::sync::Mutex<Vec<MeshState>>,
    }

    impl MeshEventSink for RecordingSink {
        fn emit_mesh_status(&self, _status: &MeshStatus) {
            self.emitted.lock().unwrap().push(_status.state);
        }
    }

    fn monitor_with(
        ops: MeshServiceState,
        peers: Vec<PeerBrief>,
        channel: AccessChannel,
    ) -> (std::sync::Arc<MeshMonitor>, std::sync::Arc<RecordingSink>) {
        let ops = std::sync::Arc::new(MockMeshOps {
            service: std::sync::Mutex::new(ops),
            peers: std::sync::Mutex::new(Ok(peers)),
        });
        let sink = std::sync::Arc::new(RecordingSink::default());
        let monitor = std::sync::Arc::new(MeshMonitor::new(
            ops,
            std::sync::Arc::new(move || channel),
            // tick 只读 secret 文件（缺目录 → None 即未配置），用临时路径隔离
            std::env::temp_dir().join("et-monitor-no-secret-test").to_string_lossy().into_owned(),
            sink.clone(),
        ));
        (monitor, sink)
    }

    #[test]
    fn monitor_tick_reports_online_and_dedupes_events() {
        // 栈目录写真 secret（Online 链路要求密钥就绪——AC9 前置与判定衔接）
        let stack = std::env::temp_dir().join("et-monitor-tick-test");
        let _ = std::fs::remove_dir_all(&stack);
        std::fs::create_dir_all(stack.join(MESH_DIR)).unwrap();
        std::fs::write(stack.join(MESH_DIR).join(NETWORK_SECRET_FILE), "s3cret\n").unwrap();
        let ops = std::sync::Arc::new(MockMeshOps {
            service: std::sync::Mutex::new(MeshServiceState::Running),
            peers: std::sync::Mutex::new(Ok(vec![peer("self", true), peer("phone", false)])),
        });
        let sink2 = std::sync::Arc::new(RecordingSink::default());
        let monitor = std::sync::Arc::new(MeshMonitor::new(
            ops.clone(),
            std::sync::Arc::new(|| AccessChannel::Mesh),
            stack.to_string_lossy().into_owned(),
            sink2.clone(),
        ));

        monitor.tick();
        assert_eq!(monitor.status().state, MeshState::Online, "有非本机成员应在线");
        assert_eq!(*sink2.emitted.lock().unwrap(), vec![MeshState::Online]);

        // 同输入再 tick：状态不变不重发（detail/peers 均未变）
        monitor.tick();
        assert_eq!(sink2.emitted.lock().unwrap().len(), 1, "同输入不重发事件");

        // 服务转停止 → Offline 再发恰一次；peers 随之清空
        *ops.service.lock().unwrap() = MeshServiceState::Stopped;
        monitor.tick();
        let status = monitor.status();
        assert_eq!(status.state, MeshState::Offline);
        assert_eq!(status.detail.as_deref(), Some(DETAIL_SERVICE_STOPPED));
        assert!(status.peers.is_empty());
        assert_eq!(*sink2.emitted.lock().unwrap(), vec![MeshState::Online, MeshState::Offline]);

        let _ = std::fs::remove_dir_all(&stack);
    }

    #[test]
    fn monitor_tick_reports_not_configured_without_secret() {
        // 无密钥（monitor_with 的栈目录不存在）→ NotConfigured + 指引码
        let (monitor, sink) = monitor_with(
            MeshServiceState::Running,
            vec![peer("phone", false)],
            AccessChannel::Mesh,
        );
        monitor.tick();
        let status = monitor.status();
        assert_eq!(status.state, MeshState::NotConfigured);
        assert_eq!(status.detail.as_deref(), Some(DETAIL_SECRET_MISSING));
        assert_eq!(sink.emitted.lock().unwrap().len(), 1);
    }

    // ── T8：mesh-service.ps1 派发参数（收摊停服务；UAC runas 用）──────────

    #[test]
    fn service_action_params_shape() {
        let params = service_action_params(
            Path::new(r"D:\Apps\resources\bin"),
            "stop",
            r"D:\My Stack\cloudcli-https",
            crate::lang::Lang::Zh,
        );
        assert!(params.contains("mesh-service.ps1"), "{params}");
        assert!(params.contains("-Action stop"), "{params}");
        // 栈目录单引号字面量（含空格安全，内部单引号翻倍）
        assert!(params.contains("-StackDir 'D:\\My Stack\\cloudcli-https'"), "{params}");
        assert!(params.contains("-Lang zh"), "{params}");
        // -Command "&" 包装（visible_script_params 契约：脚本内 exit 不杀窗口）
        assert!(params.contains("-Command \"& 'D:"), "{params}");
        assert!(params.contains("-NoExit"), "{params}");
        // 单引号转义：路径含 ' 时翻倍
        let quoted = service_action_params(
            Path::new(r"D:\bin"),
            "install",
            r"D:\jack's",
            crate::lang::Lang::En,
        );
        assert!(quoted.contains(r"-StackDir 'D:\jack''s'"), "{quoted}");
        assert!(quoted.contains("-Lang en"), "{quoted}");
    }

    // ── T16：组网诊断（纯函数判定 + URI 解析 + 序列化形态；探测 IO 不进单测）─

    /// 对端 URI 解析：scheme 剥离、去路径、无端口默认 11010、坏形拒绝
    #[test]
    fn peer_host_port_parses_schemes_and_defaults() {
        assert_eq!(
            peer_host_port("tcp://sh.vomiku.com:7910"),
            Some(("sh.vomiku.com".into(), 7910))
        );
        assert_eq!(
            peer_host_port("udp://backup.example.com"),
            Some(("backup.example.com".into(), MESH_LISTEN_PORT)),
            "无端口默认 11010"
        );
        assert_eq!(
            peer_host_port("tcp://1.2.3.4:11010/"),
            Some(("1.2.3.4".into(), 11010)),
            "带路径尾斜杠不破坏 host:port"
        );
        assert_eq!(peer_host_port("no_scheme"), None);
        assert_eq!(peer_host_port("tcp://:7910"), None, "空 host 拒绝");
        assert_eq!(peer_host_port("tcp://host:notaport"), None, "坏端口拒绝");
    }

    /// 连接错误分类（refused/timeout 两类各给换网对照建议——T14 实测拦截形态）
    #[test]
    fn connect_error_classified() {
        use std::io::ErrorKind;
        assert_eq!(
            classify_connect(&std::io::Error::from(ErrorKind::ConnectionRefused)),
            "refused"
        );
        assert_eq!(classify_connect(&std::io::Error::from(ErrorKind::TimedOut)), "timeout");
        assert_eq!(classify_connect(&std::io::Error::from(ErrorKind::WouldBlock)), "timeout");
        assert_eq!(
            classify_connect(&std::io::Error::from(ErrorKind::PermissionDenied)),
            "error"
        );
    }

    /// DiagItem 序列化：camelCase 字段、None detail 省略
    #[test]
    fn diag_items_serialize_and_skip_empty_detail() {
        let with = DiagItem { code: "service", ok: true, detail: Some("running".into()) };
        let without = DiagItem { code: "secret", ok: false, detail: None };
        let j1 = serde_json::to_string(&with).unwrap();
        let j2 = serde_json::to_string(&without).unwrap();
        assert!(j1.contains("\"code\":\"service\""), "{j1}");
        assert!(j1.contains("\"ok\":true") && j1.contains("\"detail\":\"running\""), "{j1}");
        assert!(!j2.contains("detail"), "None detail 应省略：{j2}");
    }

    /// 六项判定（各 judge 纯函数覆盖通过/失败分支；码集合与 DIAG_CODES 一致）
    #[test]
    fn diag_judges_cover_six_codes() {
        // ①服务：running/startPending 宽限通过，其余失败；detail = 稳定态值
        assert!(service_judge(MeshServiceState::Running).ok);
        assert!(service_judge(MeshServiceState::StartPending).ok);
        assert_eq!(service_judge(MeshServiceState::Running).detail.as_deref(), Some("running"));
        assert!(!service_judge(MeshServiceState::NotFound).ok);
        assert!(!service_judge(MeshServiceState::Disabled).ok);
        // ②密钥
        assert!(secret_judge(true).ok);
        assert!(!secret_judge(false).ok);
        // ③对端聚合：全通 / 有失败（detail 逐条）/ 空列表
        let r = peers_judge(&[
            ("a.com".into(), 7910, "ok"),
            ("b.com".into(), 11010, "refused"),
        ]);
        assert!(!r.ok);
        assert!(r.detail.as_deref().unwrap().contains("b.com:11010=refused"));
        assert!(peers_judge(&[("a.com".into(), 1, "ok")]).ok);
        assert_eq!(peers_judge(&[]).detail.as_deref(), Some("no_peers"));
        // ④成员：无成员 / 有成员带地址 / 有成员缺地址（T14 DHCP 形态实锤）
        let remote_ok = PeerBrief {
            hostname: "phone".into(),
            ipv4: Some("10.126.126.2".into()),
            latency_ms: Some(5.7),
            loss_rate: Some(0.0),
            is_local: false,
        };
        let remote_no_addr = PeerBrief {
            hostname: "phone".into(),
            ipv4: None,
            latency_ms: None,
            loss_rate: None,
            is_local: false,
        };
        assert_eq!(members_judge(&[]).detail.as_deref(), Some("no_member"));
        assert!(members_judge(&[remote_ok]).ok);
        let m = members_judge(&[remote_no_addr]);
        assert!(!m.ok, "成员无虚拟 IP 应失败（no_addr）");
        assert!(m.detail.as_deref().unwrap().contains("no_addr=1"));
        // ⑤本机网卡
        let addrs = [
            "10.126.126.1".parse::<Ipv4Addr>().unwrap(),
            "192.168.3.13".parse().unwrap(),
        ];
        assert!(local_nic_judge(&addrs, "10.126.126.1").ok);
        assert!(!local_nic_judge(&addrs, "10.126.126.99").ok);
        // ⑥域名链路：对齐+443 通 / 解析不等 / 443 不通 / 解析失败
        let expect: Ipv4Addr = "10.126.126.1".parse().unwrap();
        assert!(domain_chain_judge(Some(expect), true, expect).ok);
        assert!(!domain_chain_judge(Some("1.2.3.4".parse().unwrap()), true, expect).ok);
        assert!(!domain_chain_judge(Some(expect), false, expect).ok);
        assert!(!domain_chain_judge(None, true, expect).ok);
        // 码集合与顺序锁定
        let items = [
            service_judge(MeshServiceState::Running),
            secret_judge(true),
            peers_judge(&[("a".into(), 1, "ok")]),
            members_judge(&[]),
            local_nic_judge(&[], "10.126.126.1"),
            domain_chain_judge(Some(expect), true, expect),
        ];
        let codes: Vec<_> = items.iter().map(|i| i.code).collect();
        assert_eq!(codes, DIAG_CODES);
    }

    /// AC8 口径：诊断结果不回显密钥（各 judge 产物对密钥值穿底断言，
    /// 沿 bin_path 测试口径）
    #[test]
    fn diag_details_never_contain_secret() {
        let marker = "s3cret-value";
        let items = [
            service_judge(MeshServiceState::Stopped),
            secret_judge(false),
            peers_judge(&[("sh.vomiku.com".into(), 7910, "timeout")]),
            members_judge(&[]),
            local_nic_judge(&[], "10.126.126.1"),
            domain_chain_judge(None, false, "10.126.126.1".parse().unwrap()),
        ];
        for i in items {
            let s = serde_json::to_string(&i).unwrap();
            assert!(!s.contains(marker), "诊断结果不得含密钥值：{s}");
        }
    }
}
