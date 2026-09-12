//! 局域网边界防火墙契约（spec 010）：规则名/退出码/例外 TTL 常量 + lan-guard.ps1
//! 提权派发参数构造 + status 探测解析 + 健康判定（judge_health）+ 12h 回落
//! watcher（LanGuardMonitor）。
//!
//! 分工（tasks T2/T3/T4）：T2 落契约常量与派发参数构造；T3 落探测/解析/
//! 健康判定/回落状态机（本批）；T4 落命令层接线（lan_guard_* 四命令）与
//! settings/组合派发。规则 CRUD 单点在 lan-guard.ps1（plan §2），Rust 侧
//! 常量是「双端形态契约锚」（mesh.rs service_bin_path 先例：脚本同值漂移即
//! 单测失配暴露）。

// 契约锚常量（规则名/退出码/例外 Profile/RemoteAddress）由脚本同值锁形、仅单测
// 消费，生产路径不触达——allow(dead_code) 为其保留（移除即契约锚告警）。
#![allow(dead_code)]

use crate::lang::Lang;
use crate::network::{NetCategory, NetworkEntry};
use serde::Serialize;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

// ── 契约常量（T1 冻结，tasks T1 备注；plan §3.2/§4.2/§5.1）────────────────

/// lan-guard.ps1 脚本名（打包子集登记 + 双目录同步，plan §5.1/R9）
pub const SCRIPT_FILE: &str = "lan-guard.ps1";
/// 443 白名单规则名（脚本 ensure-whitelist 创建：入站 TCP 443 +
/// RemoteAddress=虚拟网段 + InterfaceAlias=TUN 运行期解析，Profile Any）
pub const RULE_MESH_HTTPS: &str = "CloudCLI Mesh HTTPS 443";
/// 3001 例外规则名（exception-on 创建；默认不存在——名字含 Exception 与旧规则
/// 明确区分，例外态检测与旧规则残留检测互不歧义，plan §3.2 命名理由）
pub const RULE_LAN_EXCEPTION: &str = "CloudCLI LAN 3001 Exception";
/// 旧 443 规则（Private 语义；migrate 幂等删除；存在性检测 = 迁移横幅，T3）
pub const LEGACY_RULE_443: &str = "CloudCLI LAN HTTPS 443";
/// 旧 3001 规则（migrate 幂等删除）
pub const LEGACY_RULE_3001: &str = "CloudCLI LAN 3001";

/// 例外规则契约：仅专用网络生效（脚本 New-NetFirewallRule -Profile Private
/// 同值——例外面向物理局域网的最小暴露面，plan §3.2）
pub const EXCEPTION_PROFILE: &str = "Private";
/// 例外规则契约：远端限本机子网（严格于旧规则 Any，精确落地「同网段」语义）
pub const EXCEPTION_REMOTE_ADDRESS: &str = "LocalSubnet";

/// 退出码：成功 / 幂等跳过（plan §3.2；工作台以 status 复测为准，仅手工排查用）
pub const EXIT_OK: i32 = 0;
/// 退出码：前置不满足（非管理员 / 参数非法）
pub const EXIT_PRECONDITION: i32 = 1;
/// 退出码：TUN 未解析（-WaitTun 耗尽不建规则——组网不在时成员本就无 TUN
/// 路由，规则缺失不构成暴露，属「休眠」而非异常）
pub const EXIT_TUN_UNRESOLVED: i32 = 3;

/// 例外自动回落 TTL = 12h（需求方定案；`now - since == TTL` 即到期，T3 边界单测）
pub const EXCEPTION_TTL_SECS: u64 = 12 * 60 * 60;
/// 白名单健康轮询周期 = 60s（plan §3.5：健康态无需告警级实时性）
pub const POLL_INTERVAL_SECS: u64 = 60;

// ── 动作集与提权派发参数构造（plan §5.1）────────────────────────────────────

/// lan-guard.ps1 动作集（与脚本 -Action ValidateSet 一一对应）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LanGuardAction {
    /// 只读探测（免提权）：输出 plan §4.2 压缩 JSON；tun=null=休眠态
    Status,
    /// 443 白名单幂等就位（§3.2：已存在且 RemoteAddress+InterfaceAlias 均匹配
    /// → 跳过；失配/不存在 → Remove+New；TUN 未解析 → exit 3 不建规则）
    EnsureWhitelist,
    /// 3001 例外开启（Private + LocalSubnet，无接口条件；同形幂等）
    ExceptionOn,
    /// 3001 例外关闭（仅删，不存在亦成功）
    ExceptionOff,
    /// 存量迁移：删两条旧规则（幂等）→ ensure-whitelist 逻辑复用
    Migrate,
}

impl LanGuardAction {
    /// 脚本 -Action 实参（kebab-case，脚本 ValidateSet 同值）
    pub fn as_arg(self) -> &'static str {
        match self {
            Self::Status => "status",
            Self::EnsureWhitelist => "ensure-whitelist",
            Self::ExceptionOn => "exception-on",
            Self::ExceptionOff => "exception-off",
            Self::Migrate => "migrate",
        }
    }

    /// 是否需要提权（status 只读免提权；其余动作脚本侧自检管理员，非管理员 exit 1）
    pub fn is_elevated(self) -> bool {
        self != Self::Status
    }
}

/// PowerShell 单引号字面量：内部单引号按规则翻倍（network.rs set_category_params
/// 与 autostart::ps_quote 同款；含空格参数天然安全）
pub fn ps_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// lan-guard.ps1 的提权隐藏派发参数串（ShellExecuteW lpParameters；plan §2：
/// runas + `-WindowStyle Hidden` fire-and-forget——提权窗进程无法回传 stdout，
/// 真相以 status 复测为准（plan R3）；退出码契约仅供命令行手工排查）。
///
/// - ensure-whitelist / migrate 必传 `-Cidr` + `-VirtualIp`，status 必传
///   `-VirtualIp`；缺失/空白 → Err 交上层提示（对应脚本侧参数非法 exit 1 的
///   前置分支，AC1 派发面）；
/// - **例外动作仅派发 `-Action`**：规则契约（`-Profile Private +
///   -RemoteAddress LocalSubnet`，无接口条件）由脚本单点承载，Rust 侧仅以
///   [`EXCEPTION_PROFILE`] / [`EXCEPTION_REMOTE_ADDRESS`] 锁形（AC6 断言面）；
///   多余的 cidr/virtual_ip/wait_tun 入参不透传（§5.1 例外动作无参数）；
/// - `-WaitTun <sec>` 原样透传（apply 组合联动传 20，plan §3.5；脚本默认 0）。
pub fn dispatch_params(
    scripts_dir: &Path,
    action: LanGuardAction,
    cidr: Option<&str>,
    virtual_ip: Option<&str>,
    wait_tun: Option<u32>,
    lang: Lang,
) -> Result<String, String> {
    let inner = script_invocation(scripts_dir, action, cidr, virtual_ip, wait_tun, lang)?;
    Ok(format!("-NoProfile -NonInteractive -WindowStyle Hidden -Command \"{inner}\""))
}

/// 脚本调用语句（`& '<脚本>' -Lang x -Action y [args]`，dispatch_params 的
/// `-Command` 内层；参数校验与拼接规则见 [`dispatch_params`] 文档——T2 冻结
/// 契约原样下沉，行为逐字节不变，由 T2 单测背书）
fn script_invocation(
    scripts_dir: &Path,
    action: LanGuardAction,
    cidr: Option<&str>,
    virtual_ip: Option<&str>,
    wait_tun: Option<u32>,
    lang: Lang,
) -> Result<String, String> {
    let needs_cidr =
        matches!(action, LanGuardAction::EnsureWhitelist | LanGuardAction::Migrate);
    let needs_vip = needs_cidr || action == LanGuardAction::Status;
    if needs_cidr && cidr.map_or(true, |c| c.trim().is_empty()) {
        return Err(format!("lan-guard {} 需要 -Cidr（组网虚拟网段）", action.as_arg()));
    }
    if needs_vip && virtual_ip.map_or(true, |v| v.trim().is_empty()) {
        return Err(format!(
            "lan-guard {} 需要 -VirtualIp（宿主机组网虚拟 IP）",
            action.as_arg()
        ));
    }
    let script = ps_quote(&scripts_dir.join(SCRIPT_FILE).to_string_lossy());
    let mut inner =
        format!("& {script} -Lang {} -Action {}", crate::scripts::lang_arg(lang), action.as_arg());
    if needs_cidr {
        if let Some(c) = cidr {
            inner.push_str(&format!(" -Cidr {}", ps_quote(c.trim())));
        }
    }
    if needs_vip {
        if let Some(v) = virtual_ip.map(str::trim).filter(|s| !s.is_empty()) {
            inner.push_str(&format!(" -VirtualIp {}", ps_quote(v)));
        }
    }
    if wait_tun.is_some() && needs_cidr {
        // -WaitTun 只属于建白名单的动作（exception-on/off 无 TUN 语义）
        inner.push_str(&format!(" -WaitTun {}", wait_tun.unwrap()));
    }
    Ok(inner)
}

/// apply 组合派发的 ensure-whitelist 段（plan §3.5/AC9，T4）：以 `; ` 起始的
/// PS 语句，由 mesh.rs 拼装到 mesh-service 服务动作之后、收尾提示之前——
/// **同一可见脚本窗顺序执行、单次 UAC**（改网段必经 apply 的唯一变更入口，
/// 合并消双弹窗）；段内脚本 exit 1/3 只退出脚本作用域，服务段不受阻断
/// （§7-R4 白名单失败不回滚服务段）。
pub fn ensure_whitelist_segment(
    scripts_dir: &Path,
    cidr: &str,
    virtual_ip: &str,
    wait_tun: u32,
    lang: Lang,
) -> Result<String, String> {
    let inner = script_invocation(
        scripts_dir,
        LanGuardAction::EnsureWhitelist,
        Some(cidr),
        Some(virtual_ip),
        Some(wait_tun),
        lang,
    )?;
    Ok(format!("; {inner}"))
}

// ── status 免提权探测（T3；plan §4.2 契约）──────────────────────────────────

/// 白名单健康事件名（载荷 [`LanHealth`]；60s 轮询 + 动作后即时刷新，变化才发声）
pub const EVENT_LAN_GUARD_CHANGED: &str = "languard://changed";
/// 轮询周期（秒 → Duration；POLL_INTERVAL_SECS = 60 为 T1 冻结契约值）
pub const LAN_POLL_INTERVAL: Duration = Duration::from_secs(POLL_INTERVAL_SECS);
/// 单次 status 探测超时（network.rs DETECT_TIMEOUT 同量级；超时判失败静默降级）
const STATUS_TIMEOUT: Duration = Duration::from_secs(10);
/// apply 组合派发时 ensure-whitelist 段的 TUN 就绪等待（plan §3.5：服务重启后
/// 适配器就绪窗口，2s×10 轮询；R4）
pub const APPLY_WAIT_TUN_SECS: u32 = 20;
/// 例外到期回落的重试窗口（plan §3.4：UAC 拒绝/失败 → 每 30min 重试）
pub const REVERT_RETRY_INTERVAL: Duration = Duration::from_secs(30 * 60);

/// status 探测的 powershell 参数（network.rs detect_args 同构：-File 直跑脚本 +
/// CREATE_NO_WINDOW + 捕获 stdout）。脚本保证 stdout 仅一行压缩 JSON（Write-Host
/// 走信息流不污染管道，plan §4.2）；status 输出与语言无关，不传 -Lang（默认 auto）。
/// VirtualIp 是 TUN 解析锚点（§3.3），缺失/空白 → Err（对应脚本参数非法 exit 1 前置）。
pub fn status_args(scripts_dir: &Path, virtual_ip: &str) -> Result<Vec<String>, String> {
    let vip = virtual_ip.trim();
    if vip.is_empty() {
        return Err("lan-guard status 需要 -VirtualIp（宿主机组网虚拟 IP）".into());
    }
    Ok(vec![
        "-NoProfile".into(),
        "-NonInteractive".into(),
        "-ExecutionPolicy".into(),
        "Bypass".into(),
        "-File".into(),
        scripts_dir.join(SCRIPT_FILE).to_string_lossy().into_owned(),
        "-Action".into(),
        LanGuardAction::Status.as_arg().into(),
        "-VirtualIp".into(),
        vip.into(),
    ])
}

/// status JSON 契约解析产物（plan §4.2：legacy443/legacy3001/mesh443/exc3001/tun）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanGuardProbe {
    /// 旧 443 规则（`CloudCLI LAN HTTPS 443`）存在 → 迁移横幅（AC10）
    pub legacy443: bool,
    /// 旧 3001 规则存在
    pub legacy3001: bool,
    /// 443 白名单规则实况
    pub mesh443: WhitelistProbe,
    /// 3001 例外规则实况
    pub exc3001: ExceptionProbe,
    /// 持有 -VirtualIp 的适配器（None = 未解析到 → 休眠态）
    pub tun: Option<TunInfo>,
}

/// 443 白名单规则实况（profile 沿实测 flags 0=Any/1=Domain/2=Private/4=Public，
/// 仅作展示与例外规则的 Private 断言，**不参与白名单判定**——plan §4.2）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WhitelistProbe {
    pub present: bool,
    /// RemoteAddress（脚本 Convert-ToCidrForm 已归一为 /nn 形态）
    pub remote: Option<String>,
    /// InterfaceAlias（规则绑定的 TUN 接口名）
    pub iface: Option<String>,
    pub profile: u32,
}

/// 3001 例外规则实况
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExceptionProbe {
    pub present: bool,
    pub profile: u32,
}

/// TUN 适配器信息（解析锚点 = 持有 virtual_ip 的 IPv4 适配器，plan §3.3）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TunInfo {
    pub name: String,
    pub ip: String,
}

/// status 输出 → [`LanGuardProbe`]（纯函数；容错缺字段/null，沿 parse_net_status
/// 先例——`tun:null` 即休眠，字段缺失按不存在/未解析处理，整体非 JSON 才 Err）。
pub fn parse_status(raw: &str) -> Result<LanGuardProbe, String> {
    let v: serde_json::Value =
        serde_json::from_str(raw.trim()).map_err(|e| format!("status 输出非 JSON：{e}"))?;
    let tun = v["tun"]
        .get("name")
        .and_then(|n| n.as_str())
        .filter(|s| !s.is_empty())
        .map(|name| TunInfo {
            name: name.to_string(),
            ip: v["tun"]["ip"].as_str().unwrap_or_default().to_string(),
        });
    Ok(LanGuardProbe {
        legacy443: v["legacy443"].as_bool().unwrap_or(false),
        legacy3001: v["legacy3001"].as_bool().unwrap_or(false),
        mesh443: WhitelistProbe {
            present: v["mesh443"]["present"].as_bool().unwrap_or(false),
            remote: v["mesh443"]["remote"].as_str().map(str::to_string),
            iface: v["mesh443"]["iface"].as_str().map(str::to_string),
            profile: v["mesh443"]["profile"].as_u64().unwrap_or(0) as u32,
        },
        exc3001: ExceptionProbe {
            present: v["exc3001"]["present"].as_bool().unwrap_or(false),
            profile: v["exc3001"]["profile"].as_u64().unwrap_or(0) as u32,
        },
        tun,
    })
}

// ── 健康判定（plan §3.5 judge_health 纯函数）────────────────────────────────

/// 443 白名单健康五态（前端 `languard.whitelist` chip 分类：正常 / 休眠 / 待修复）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum WhitelistState {
    /// present ∧ remote==cidr ∧ iface==tun.name 三元全匹配
    Ok,
    /// 规则不存在（TUN 在场——待修复；TUN 缺席归 dormant）
    Missing,
    /// RemoteAddress ≠ 当前虚拟网段（改网段后未联动/规则被改）
    StaleCidr,
    /// InterfaceAlias ≠ 当前 TUN 接口名（服务重装/驱动升级重建适配器，plan R1）
    StaleIface,
    /// TUN 未解析（组网不在——休眠非异常：成员本就无 TUN 路由，规则缺失不构成暴露）
    Dormant,
}

/// 例外开关四态（plan §3.4 状态机；前端按 state tag 渲染剩余时长）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "state")]
pub enum ExceptionState {
    /// 开关关 ∧ 规则不在（默认收口态）
    Off,
    /// 放行中（剩余秒数 = TTL − (now−since)，下限 0）
    #[serde(rename_all = "camelCase")]
    On { remaining_secs: u64 },
    /// 满 12h（回落未完成——仍放行，watcher 重试中，AC8 如实呈现）
    Expired,
    /// 开关开 ∧ 规则不在（「例外已请求但规则未生效」——引导重新开启）
    Pending,
}

/// 白名单健康快照（`languard://changed` 事件与 lan_guard_status 命令载荷）
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LanHealth {
    pub whitelist: WhitelistState,
    /// 任一旧规则存在 → 迁移横幅（AC10）
    pub legacy_present: bool,
    pub exception: ExceptionState,
    /// 例外生效 ∧ ∃公用活动网络（如实提示：Private 规则直访不生效，plan R5）
    pub public_blocks_exception: bool,
}

/// 例外是否到期（纯函数；边界 `now - since == ttl` 即到期，plan §4.1）。
/// since == 0（标记损坏/未记时点）按已到期处理——fail-safe 收紧方向：触发回落
/// 删除规则而非无限期放行（plan R8：设置损坏走收紧方向）。
pub fn exception_expired(now_ms: u64, since_ms: u64, ttl_secs: u64) -> bool {
    now_ms >= since_ms.saturating_add(ttl_secs.saturating_mul(1000))
}

/// 例外剩余秒数（纯函数；到期后为 0，不为负）
pub fn exception_remaining_secs(now_ms: u64, since_ms: u64, ttl_secs: u64) -> u64 {
    let elapsed = now_ms.saturating_sub(since_ms) / 1000;
    ttl_secs.saturating_sub(elapsed)
}

/// 当前毫秒时间戳（epoch ms；时钟早退归 0——仅用于 since 记时与剩余展示）
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 健康判定（plan §3.5 纯函数，全分支单测覆盖）。
/// 输入 = status 探测实况 + 期望网段（mesh 设置）+ 例外标记（settings）+
/// 活动网络归类（NetMonitor 注入）+ now；输出 = [`LanHealth`]。
///
/// **CIDR 契约依赖（T2 实测钉死）**：Windows 防火墙把 /24 存成掩码形态
/// （255.255.255.0），lan-guard.ps1 的 Convert-ToCidrForm 已在脚本侧归一为
/// `/nn` 输出——Rust 比对**直接用 /24 字符串相等**，不得自行解析掩码形态；
/// 若脚本归一逻辑漂移，此处失配即 stale_cidr（单测锁定该依赖）。
///
/// 白名单五态判定顺序：TUN 缺席 → dormant（组网不在，失配修复无从谈起，
/// ensure-whitelist 亦会 exit 3）；TUN 在场时 present → cidr → iface 逐级比对。
pub fn judge_health(
    probe: &LanGuardProbe,
    expected_cidr: &str,
    exception_enabled: bool,
    exception_since_ms: u64,
    networks: &[NetworkEntry],
    now_ms: u64,
) -> LanHealth {
    let whitelist = match &probe.tun {
        None => WhitelistState::Dormant,
        Some(tun) => {
            let m = &probe.mesh443;
            if !m.present {
                WhitelistState::Missing
            } else if m.remote.as_deref() != Some(expected_cidr) {
                WhitelistState::StaleCidr
            } else if m.iface.as_deref() != Some(tun.name.as_str()) {
                WhitelistState::StaleIface
            } else {
                WhitelistState::Ok
            }
        }
    };
    let exc_present = probe.exc3001.present;
    let exception = if exception_enabled {
        if !exc_present {
            ExceptionState::Pending
        } else if exception_expired(now_ms, exception_since_ms, EXCEPTION_TTL_SECS) {
            ExceptionState::Expired
        } else {
            ExceptionState::On {
                remaining_secs: exception_remaining_secs(
                    now_ms,
                    exception_since_ms,
                    EXCEPTION_TTL_SECS,
                ),
            }
        }
    } else if exc_present {
        // 标记丢失但规则仍在（settings 损坏/外部建规则）：以实况为准如实显示
        // 放行中，不谎报 off（plan R8 判定口径；剩余时长按满额展示）
        ExceptionState::On { remaining_secs: EXCEPTION_TTL_SECS }
    } else {
        ExceptionState::Off
    };
    let public_blocks_exception = matches!(exception, ExceptionState::On { .. } | ExceptionState::Expired)
        && networks.iter().any(|n| n.category == NetCategory::Public);
    LanHealth {
        whitelist,
        legacy_present: probe.legacy443 || probe.legacy3001,
        exception,
        public_blocks_exception,
    }
}

// ── 12h 回落决策（plan §3.4 状态机；watcher 每轮调用）──────────────────────

/// 回落决策（plan §3.4 表的动作列；呈现列由 judge_health 承载）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RevertDecision {
    /// 无动作（开关关 / on 倒计时中）
    Noop,
    /// pending（开 ∧ 未到期 ∧ 规则不在）：观察不动，引导重新开启
    PendingObserve,
    /// 开 ∧ 到期 ∧ 规则不在：**免 UAC** 本地清标记（删除无对象）
    ClearMarkLocally,
    /// 开 ∧ 到期 ∧ 规则在：UAC 派发 exception-off（拒绝/失败 30min 后重试，AC8）
    DispatchExceptionOff,
}

/// 回落决策矩阵（纯函数，plan §3.4 表全行；到期行优先于 pending 行——
/// 「到期 ∧ 规则不在」若判 pending 则标记永不清理，状态机死锁）
pub fn revert_decision(exception_enabled: bool, expired: bool, rule_present: bool) -> RevertDecision {
    if !exception_enabled {
        return RevertDecision::Noop;
    }
    match (expired, rule_present) {
        (true, true) => RevertDecision::DispatchExceptionOff,
        (true, false) => RevertDecision::ClearMarkLocally,
        (false, true) => RevertDecision::Noop,
        (false, false) => RevertDecision::PendingObserve,
    }
}

/// 回落派发是否到点（纯函数：从未派发过 → 立即；距上次 < 30min → 等待）
fn revert_due(last_attempt: Option<Instant>, now: Instant) -> bool {
    last_attempt.map_or(true, |t| now.duration_since(t) >= REVERT_RETRY_INTERVAL)
}

// ── 白名单健康监视器（NetMonitor 同构：probe/sink/缓存/变化才发声）──────────

/// 单轮探测输入（装配层每次 tick 现取：virtual_ip/cidr 随设置联动，例外标记
/// 随 settings，活动网络随 NetMonitor 缓存——plan §3.5 输入源）
#[derive(Debug, Clone, PartialEq)]
pub struct LanInputs {
    pub virtual_ip: String,
    pub cidr: String,
    pub exception_enabled: bool,
    pub exception_since_ms: u64,
    pub networks: Vec<NetworkEntry>,
}

/// 探测出口（真实实现拉起 PowerShell 跑 lan-guard status；单测脚本化输出）
pub trait LanProbe: Send + Sync {
    /// 返回 status 原始 stdout；Err = 探测失败（保持上次缓存，AC4 同口径）
    fn status(&self, virtual_ip: &str) -> Result<String, String>;
}

/// 事件出口（真实实现 Tauri emit `languard://changed`；单测断言事件序列）
pub trait LanEventSink: Send + Sync {
    fn emit_lan_health(&self, health: &LanHealth);
}

/// 回落执行面（真实实现 UAC 派发 exception-off / 本地写 settings；单测记录调用）
pub trait RevertExecutor: Send + Sync {
    /// UAC 派发 exception-off（fire-and-forget，真相以 status 复测为准）；
    /// Err = 未派发（脚本目录缺失/UAC 拒绝）——30min 后重试（AC8）
    fn dispatch_exception_off(&self) -> Result<(), String>;
    /// 免 UAC 本地清标记（enabled:false / since:0；幂等）
    fn clear_exception_mark(&self);
}

/// 白名单健康监视器：refresh 即时探测 + 缓存 + 变化去重发声；tick 在 refresh
/// 之上叠加 12h 回落决策；spawn 首轮 tick 即「应用启动补回落自检」（plan §3.4
/// 末行）。锁纪律：std Mutex 不可重入——单次加锁，事件在锁释放后发（002 先例）。
#[derive(Clone)]
pub struct LanGuardMonitor {
    probe: Arc<dyn LanProbe>,
    inputs: Arc<dyn Fn() -> LanInputs + Send + Sync>,
    events: Arc<dyn LanEventSink>,
    /// None = 只观察不回落（纯测试/降级装配）
    revert: Option<Arc<dyn RevertExecutor>>,
    last: Arc<Mutex<Option<LanHealth>>>,
    last_revert_attempt: Arc<Mutex<Option<Instant>>>,
}

impl LanGuardMonitor {
    pub fn new(
        probe: Arc<dyn LanProbe>,
        inputs: Arc<dyn Fn() -> LanInputs + Send + Sync>,
        events: Arc<dyn LanEventSink>,
    ) -> Self {
        Self {
            probe,
            inputs,
            events,
            revert: None,
            last: Arc::new(Mutex::new(None)),
            last_revert_attempt: Arc::new(Mutex::new(None)),
        }
    }

    /// 挂接回落执行面（装配层注入；仅观察场景不调用）
    pub fn with_revert(mut self, revert: Arc<dyn RevertExecutor>) -> Self {
        self.revert = Some(revert);
        self
    }

    /// 当前缓存快照（lan_guard_status 失败回退的数据源；None = 尚无成功探测）
    pub fn current(&self) -> Option<LanHealth> {
        self.last.lock().expect("白名单状态锁中毒").clone()
    }

    /// 即时探测一次（lan_guard_status 命令与动作后刷新的入口）：成功刷新缓存
    /// 并去重发声；失败回上次缓存，无缓存 → None（AC4 静默降级同口径）
    pub fn refresh(&self) -> Option<LanHealth> {
        self.refresh_with(&(self.inputs)()).0
    }

    /// 探询单轮（pub 便于单测直调）：refresh + 12h 回落决策。
    /// **探测失败轮不做回落决策**——回缓存的快照不代表实况，按陈旧数据派发
    /// 删除是盲动（UAC 误弹/误清标记）。
    pub fn tick(&self) {
        let inputs = (self.inputs)();
        let (health, fresh) = self.refresh_with(&inputs);
        let Some(health) = health else { return };
        if !fresh {
            return; // 本次可见态来自缓存回退，非新探测
        }
        let expired =
            exception_expired(now_ms(), inputs.exception_since_ms, EXCEPTION_TTL_SECS);
        // 规则实况从健康态反推：Off=关∧不在、Pending=开∧不在 → 不在；
        // On/Expired → 在（judge_health 与本判定同源，单测矩阵互相锁定）
        let rule_present =
            !matches!(health.exception, ExceptionState::Off | ExceptionState::Pending);
        match revert_decision(inputs.exception_enabled, expired, rule_present) {
            RevertDecision::Noop | RevertDecision::PendingObserve => {}
            RevertDecision::ClearMarkLocally => {
                if let Some(r) = &self.revert {
                    r.clear_exception_mark();
                }
            }
            RevertDecision::DispatchExceptionOff => {
                let Some(r) = &self.revert else { return };
                let due = {
                    let g = self.last_revert_attempt.lock().expect("回落窗口锁中毒");
                    revert_due(*g, Instant::now())
                };
                if due {
                    *self.last_revert_attempt.lock().expect("回落窗口锁中毒") = Some(Instant::now());
                    if let Err(e) = r.dispatch_exception_off() {
                        log::warn!(
                            "例外回落派发未完成（30min 后重试；看板按「已到期仍放行」如实呈现，AC8）：{e}"
                        );
                    }
                }
            }
        }
    }

    /// 启动轮询线程（随进程退出而止）；**首轮 tick 即启动补回落自检**——
    /// enabled ∧ expired ∧ 规则在 → 先派发回落再进轮询；规则不在 → 免 UAC 清
    /// 标记（plan §3.4「应用退出期间到期」行）
    pub fn spawn(self: &Arc<Self>) -> std::thread::JoinHandle<()> {
        let me = Arc::clone(self);
        std::thread::Builder::new()
            .name("wb-languard-poller".into())
            .spawn(move || loop {
                me.tick();
                std::thread::sleep(LAN_POLL_INTERVAL);
            })
            .expect("白名单轮询线程创建失败")
    }

    /// refresh 的内核（inputs 已就取，避免 tick 内二次读取设置产生撕裂）：
    /// 成功且与上次不同 → 更新缓存并发事件；失败静默保持上次。
    /// 返回 (本次可见态, 是否新探测成功)——tick 以前者驱动调用方、以后者
    /// 把关回落决策（缓存回退 ≠ 实况）。
    fn refresh_with(&self, inputs: &LanInputs) -> (Option<LanHealth>, bool) {
        let parsed = self
            .probe
            .status(&inputs.virtual_ip)
            .ok()
            .as_deref()
            .map(parse_status);
        match parsed {
            Some(Ok(probe)) => {
                let health = judge_health(
                    &probe,
                    &inputs.cidr,
                    inputs.exception_enabled,
                    inputs.exception_since_ms,
                    &inputs.networks,
                    now_ms(),
                );
                let mut guard = self.last.lock().expect("白名单状态锁中毒");
                let changed = guard.as_ref() != Some(&health);
                let visible = health.clone();
                if changed {
                    *guard = Some(health);
                }
                drop(guard);
                if changed {
                    self.events.emit_lan_health(&visible);
                }
                (Some(visible), true)
            }
            _ => {
                log::debug!("白名单探测失败：保持上次状态（静默降级，AC4 同口径）");
                (self.last.lock().expect("白名单状态锁中毒").clone(), false)
            }
        }
    }
}

// ── 真实探测实现（Windows；PsNetProbe 同款：CREATE_NO_WINDOW + 超时杀）──────

/// PowerShell 隐藏执行 lan-guard status（只读免提权）
#[cfg(windows)]
pub struct PsLanGuardProbe {
    /// None = 脚本目录不可用（spec §4.5：相关功能禁用，探测恒失败静默降级）
    scripts_dir: Option<std::path::PathBuf>,
}

#[cfg(windows)]
impl PsLanGuardProbe {
    pub fn new(scripts_dir: Option<std::path::PathBuf>) -> Self {
        Self { scripts_dir }
    }
}

#[cfg(windows)]
impl LanProbe for PsLanGuardProbe {
    fn status(&self, virtual_ip: &str) -> Result<String, String> {
        use std::io::Read;
        use std::os::windows::process::CommandExt;
        use std::process::{Command, Stdio};

        let dir = self
            .scripts_dir
            .as_ref()
            .ok_or_else(|| "脚本目录不可用：白名单探测禁用（spec §4.5）".to_string())?;
        let mut child = Command::new("powershell.exe")
            .args(status_args(dir, virtual_ip)?)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .creation_flags(crate::scripts::CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| format!("powershell 拉起失败：{e}"))?;

        // 就绪等待（超时即杀）：只读探测，超时静默降级即可
        let deadline = Instant::now() + STATUS_TIMEOUT;
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

// ── 单元测试（契约锁定：动作集 / 参数串形态 / 单引号转义 / 例外无接口条件）──

#[cfg(test)]
mod tests {
    use super::*;

    const DIR: &str = r"C:\app\resources\bin";
    const CIDR: &str = "10.126.126.0/24";
    const VIP: &str = "10.126.126.1";

    fn params(action: LanGuardAction) -> String {
        dispatch_params(
            Path::new(DIR),
            action,
            Some(CIDR),
            Some(VIP),
            None,
            crate::lang::Lang::Zh,
        )
        .unwrap()
    }

    /// 动作集契约：五个动作与脚本 -Action ValidateSet 一一对应；status 是唯一
    /// 免提权动作（plan §5.1 提权列）
    #[test]
    fn action_set_matches_script_contract() {
        assert_eq!(LanGuardAction::Status.as_arg(), "status");
        assert_eq!(LanGuardAction::EnsureWhitelist.as_arg(), "ensure-whitelist");
        assert_eq!(LanGuardAction::ExceptionOn.as_arg(), "exception-on");
        assert_eq!(LanGuardAction::ExceptionOff.as_arg(), "exception-off");
        assert_eq!(LanGuardAction::Migrate.as_arg(), "migrate");
        assert!(!LanGuardAction::Status.is_elevated(), "status 只读免提权");
        for a in [
            LanGuardAction::EnsureWhitelist,
            LanGuardAction::ExceptionOn,
            LanGuardAction::ExceptionOff,
            LanGuardAction::Migrate,
        ] {
            assert!(a.is_elevated(), "{a:?} 须提权");
        }
    }

    /// T1 冻结契约锚：规则名四常量逐字锁定（plan §3.2；脚本内同名漂移即失配）
    #[test]
    fn rule_name_constants_frozen() {
        assert_eq!(RULE_MESH_HTTPS, "CloudCLI Mesh HTTPS 443");
        assert_eq!(RULE_LAN_EXCEPTION, "CloudCLI LAN 3001 Exception");
        assert_eq!(LEGACY_RULE_443, "CloudCLI LAN HTTPS 443");
        assert_eq!(LEGACY_RULE_3001, "CloudCLI LAN 3001");
    }

    /// ensure-whitelist 派发参数：-Cidr/-VirtualIp 单引号字面量 + -WaitTun 透传 +
    /// 隐藏窗 + -Lang（AC1/AC9 派发面，plan §3.5 `-WaitTun 20`）
    #[test]
    fn ensure_whitelist_params_carry_cidr_vip_waittun() {
        let p = dispatch_params(
            Path::new(DIR),
            LanGuardAction::EnsureWhitelist,
            Some(CIDR),
            Some(VIP),
            Some(20),
            Lang::Zh,
        )
        .unwrap();
        assert!(p.contains(SCRIPT_FILE), "{p}");
        assert!(p.contains("-Action ensure-whitelist"), "{p}");
        assert!(p.contains("-Cidr '10.126.126.0/24'"), "{p}");
        assert!(p.contains("-VirtualIp '10.126.126.1'"), "{p}");
        assert!(p.contains("-WaitTun 20"), "{p}");
        assert!(p.contains("-WindowStyle Hidden"), "提权隐藏派发（plan §2）：{p}");
        assert!(p.contains("-NoProfile -NonInteractive"), "{p}");
        assert!(p.contains("-Lang zh"), "-Lang 对齐程序语言：{p}");
        assert!(p.contains("-Command \"&"), "脚本经 & 调用（exit 不连窗）：{p}");
    }

    /// ps_quote：脚本路径与参数值内的单引号按 PS 规则翻倍（含空格/引号注入面）
    #[test]
    fn dispatch_params_quote_single_quotes() {
        let p = dispatch_params(
            Path::new(r"D:\odd'name"),
            LanGuardAction::EnsureWhitelist,
            Some("10.0.0.0/24'x"),
            Some("10.0.0.1"),
            None,
            Lang::En,
        )
        .unwrap();
        assert!(p.contains(r"'D:\odd''name\lan-guard.ps1'"), "{p}");
        assert!(p.contains(r"-Cidr '10.0.0.0/24''x'"), "{p}");
        assert!(p.contains("-Lang en"), "{p}");
    }

    /// 例外派发（AC6 断言面）：参数串只含 -Action，**无任何接口/网段条件**；
    /// 规则契约 Private + LocalSubnet 由常量锁形（脚本 New-NetFirewallRule 同值）
    #[test]
    fn exception_params_have_no_interface_condition() {
        assert_eq!(EXCEPTION_PROFILE, "Private");
        assert_eq!(EXCEPTION_REMOTE_ADDRESS, "LocalSubnet");

        let on = params(LanGuardAction::ExceptionOn);
        assert!(on.contains("-Action exception-on"), "{on}");
        assert!(!on.contains("-InterfaceAlias"), "例外无接口条件（TUN 绑定仅属 443 白名单）：{on}");
        assert!(!on.contains("-Cidr") && !on.contains("-VirtualIp") && !on.contains("-WaitTun"), "{on}");

        let off = params(LanGuardAction::ExceptionOff);
        assert!(off.contains("-Action exception-off"), "{off}");
        assert!(!off.contains("-InterfaceAlias"), "{off}");
    }

    /// migrate / status 参数形态：migrate 携带网段两参（删旧 + ensure 复用）；
    /// status 只携 -VirtualIp（tun 解析锚点）
    #[test]
    fn migrate_and_status_params_shape() {
        let m = params(LanGuardAction::Migrate);
        assert!(m.contains("-Action migrate"), "{m}");
        assert!(m.contains("-Cidr '10.126.126.0/24'"), "{m}");
        assert!(m.contains("-VirtualIp '10.126.126.1'"), "{m}");

        let s = dispatch_params(
            Path::new(DIR),
            LanGuardAction::Status,
            None,
            Some(VIP),
            None,
            Lang::Zh,
        )
        .unwrap();
        assert!(s.contains("-Action status"), "{s}");
        assert!(s.contains("-VirtualIp '10.126.126.1'"), "{s}");
        assert!(!s.contains("-Cidr"), "{s}");
    }

    /// 参数校验：ensure-whitelist/migrate 的 Cidr/VirtualIp 与 status 的
    /// VirtualIp 缺失/空白 → Err（对应脚本 exit 1 的参数前置）；例外动作无需参数
    #[test]
    fn dispatch_params_reject_missing_required_args() {
        for action in [LanGuardAction::EnsureWhitelist, LanGuardAction::Migrate] {
            assert!(dispatch_params(Path::new(DIR), action, None, Some(VIP), None, Lang::Zh).is_err());
            assert!(
                dispatch_params(Path::new(DIR), action, Some("  "), Some(VIP), None, Lang::Zh).is_err(),
                "空白 Cidr 视同缺失：{action:?}"
            );
            assert!(dispatch_params(Path::new(DIR), action, Some(CIDR), None, None, Lang::Zh).is_err());
            assert!(
                dispatch_params(Path::new(DIR), action, Some(CIDR), Some(""), None, Lang::Zh).is_err(),
                "空白 VirtualIp 视同缺失：{action:?}"
            );
        }
        assert!(dispatch_params(Path::new(DIR), LanGuardAction::Status, None, None, None, Lang::Zh).is_err());
        assert!(dispatch_params(Path::new(DIR), LanGuardAction::ExceptionOn, None, None, None, Lang::Zh).is_ok());
        assert!(dispatch_params(Path::new(DIR), LanGuardAction::ExceptionOff, None, None, None, Lang::Zh).is_ok());
    }

    /// 退出码 / TTL / 轮询常量冻结（plan §3.2、§3.4、§3.5；T1 备注）
    #[test]
    fn exit_codes_ttl_and_poll_frozen() {
        assert_eq!((EXIT_OK, EXIT_PRECONDITION, EXIT_TUN_UNRESOLVED), (0, 1, 3));
        assert_eq!(EXCEPTION_TTL_SECS, 12 * 60 * 60, "例外 12h 自动回落（AC8）");
        assert_eq!(POLL_INTERVAL_SECS, 60, "健康轮询 60s");
    }

    // ── T3：status 免提权探测参数与 JSON 解析（plan §4.2）─────────────────

    /// status 探测参数：-File 直跑脚本 + -Action status + -VirtualIp（TUN 解析
    /// 锚点），不携 -Cidr/-WaitTun；空白 VirtualIp → Err（脚本 exit 1 前置同口径）
    #[test]
    fn status_args_carry_contract() {
        let args = status_args(Path::new(DIR), VIP).unwrap();
        let file = args.iter().position(|a| a == "-File").unwrap();
        assert!(
            args[file + 1].replace('/', "\\").ends_with(&format!("\\{}", SCRIPT_FILE)),
            "脚本全路径：{args:?}"
        );
        assert!(args.windows(2).any(|w| w[0] == "-Action" && w[1] == "status"), "{args:?}");
        assert!(args.windows(2).any(|w| w[0] == "-VirtualIp" && w[1] == VIP), "{args:?}");
        assert!(!args.contains(&"-Cidr".to_string()), "status 无需网段：{args:?}");
        assert!(!args.contains(&"-Lang".to_string()), "status 输出与语言无关（JSON）：{args:?}");
        // TUN 解析锚点缺失/空白 → Err（对应脚本参数非法 exit 1）
        assert!(status_args(Path::new(DIR), "").is_err());
        assert!(status_args(Path::new(DIR), "   ").is_err());
    }

    /// plan §4.2 契约样例全字段解析（T2 真机实测形态：/24 归一、profile flags、
    /// tun 对象两字段）
    #[test]
    fn parse_status_full_contract() {
        let raw = r#"{"legacy443":true,"legacy3001":false,
 "mesh443":{"present":true,"remote":"10.126.126.0/24","iface":"et_8_1999","profile":0},
 "exc3001":{"present":false,"profile":2},
 "tun":{"name":"et_8_1999","ip":"10.126.126.1"}}"#;
        let p = parse_status(raw).unwrap();
        assert!(p.legacy443 && !p.legacy3001, "任一旧规则在 → 迁移横幅（AC10）");
        assert!(p.mesh443.present);
        assert_eq!(p.mesh443.remote.as_deref(), Some("10.126.126.0/24"));
        assert_eq!(p.mesh443.iface.as_deref(), Some("et_8_1999"));
        assert_eq!(p.mesh443.profile, 0, "白名单 Profile=Any（新契约，flags 0）");
        assert!(!p.exc3001.present);
        assert_eq!(p.exc3001.profile, 2, "例外 Private flags=2（仅展示）");
        assert_eq!(
            p.tun.as_ref().map(|t| (t.name.as_str(), t.ip.as_str())),
            Some(("et_8_1999", "10.126.126.1"))
        );
    }

    /// 容错缺字段/null（沿 parse_net_status 先例）：tun:null 即休眠；规则缺席时
    /// remote/iface 为 null → None；legacy 缺失 → false
    #[test]
    fn parse_status_tolerates_missing_and_null() {
        // 收口默认态：无双旧规则、白名单缺席（remote/iface=null）、例外缺席、TUN 不在
        let raw = r#"{"legacy443":false,"legacy3001":false,
 "mesh443":{"present":false,"remote":null,"iface":null,"profile":0},
 "exc3001":{"present":false,"profile":0},
 "tun":null}"#;
        let p = parse_status(raw).unwrap();
        assert!(!p.legacy443 && !p.legacy3001);
        assert!(!p.mesh443.present && p.mesh443.remote.is_none() && p.mesh443.iface.is_none());
        assert!(p.tun.is_none(), "tun:null = 休眠态（plan §4.2）");

        // 整字段缺失：默认 false/None，不 Err
        let sparse = parse_status("{}").unwrap();
        assert!(!sparse.legacy443 && !sparse.mesh443.present && !sparse.exc3001.present);
        assert!(sparse.tun.is_none());
        // tun 对象缺 ip 字段容忍（name 是解析判据）
        let p2 = parse_status(r#"{"tun":{"name":"et_x"}}"#).unwrap();
        assert_eq!(p2.tun.as_ref().unwrap().name, "et_x");
        assert_eq!(p2.tun.as_ref().unwrap().ip, "");
    }

    #[test]
    fn parse_status_rejects_non_json() {
        assert!(parse_status("not json").is_err());
        assert!(parse_status("").is_err());
    }

    // ── T3：judge_health 全分支（plan §3.4/§3.5）──────────────────────────

    /// 探测实况构造器（测试便捷形态）：TUN 在场 et_8_1999@10.126.126.1，
    /// 白名单按给定 remote/iface/present，默认无旧规则、例外缺席
    fn probe_with(remote: Option<&str>, iface: Option<&str>, present: bool) -> LanGuardProbe {
        LanGuardProbe {
            legacy443: false,
            legacy3001: false,
            mesh443: WhitelistProbe {
                present,
                remote: remote.map(str::to_string),
                iface: iface.map(str::to_string),
                profile: 0,
            },
            exc3001: ExceptionProbe { present: false, profile: 0 },
            tun: Some(TunInfo { name: "et_8_1999".into(), ip: "10.126.126.1".into() }),
        }
    }

    const TUN: &str = "et_8_1999";

    fn judge(probe: &LanGuardProbe, enabled: bool, since: u64, now: u64, publics: usize) -> LanHealth {
        let networks: Vec<NetworkEntry> = (0..publics)
            .map(|i| NetworkEntry {
                name: format!("net{i}"),
                if_index: i as u32 + 1,
                category: NetCategory::Public,
            })
            .collect();
        judge_health(probe, CIDR, enabled, since, &networks, now)
    }

    /// 白名单五态：ok / missing / stale_cidr / stale_iface / dormant（plan §3.5）；
    /// 含 **CIDR 契约依赖钉**——掩码形态（Windows 存储原样）≠ /24，脚本侧归一是
    /// 前置，归一逻辑漂移在此暴露为 stale_cidr
    #[test]
    fn judge_health_whitelist_five_states() {
        let now = 1_000_000_000_000u64;
        // ok：三元全匹配
        let h = judge(&probe_with(Some(CIDR), Some(TUN), true), false, 0, now, 0);
        assert_eq!(h.whitelist, WhitelistState::Ok);
        // missing：TUN 在场但规则不存在（待修复；修复按钮消费 ensure-whitelist）
        let h = judge(&probe_with(None, None, false), false, 0, now, 0);
        assert_eq!(h.whitelist, WhitelistState::Missing);
        // stale_cidr：接口对但网段失配（改网段未联动，AC9 判定面）
        let h = judge(&probe_with(Some("10.200.0.0/24"), Some(TUN), true), false, 0, now, 0);
        assert_eq!(h.whitelist, WhitelistState::StaleCidr);
        // CIDR 契约钉：掩码形态（脚本未归一的漂移形态）判 stale_cidr 而非 ok
        let h = judge(&probe_with(Some("10.126.126.0/255.255.255.0"), Some(TUN), true), false, 0, now, 0);
        assert_eq!(h.whitelist, WhitelistState::StaleCidr, "脚本 Convert-ToCidrForm 归一前置：掩码形态必须判失配");
        // stale_iface：网段对但接口失配（服务重装重建适配器，plan R1）
        let h = judge(&probe_with(Some(CIDR), Some("et_old"), true), false, 0, now, 0);
        assert_eq!(h.whitelist, WhitelistState::StaleIface);
        // dormant：TUN 未解析（组网不在——休眠非异常，含规则在场/缺席两形态）
        let mut no_tun = probe_with(Some(CIDR), Some(TUN), true);
        no_tun.tun = None;
        assert_eq!(judge(&no_tun, false, 0, now, 0).whitelist, WhitelistState::Dormant, "规则在场但 TUN 缺席：休眠优先（修复会 exit 3 无意义）");
        let mut no_tun_missing = probe_with(None, None, false);
        no_tun_missing.tun = None;
        assert_eq!(judge(&no_tun_missing, false, 0, now, 0).whitelist, WhitelistState::Dormant);
        // iface 多值（规则绑定多接口的漂移形态）→ 失配
        let h = judge(&probe_with(Some(CIDR), Some("et_old,et_8_1999"), true), false, 0, now, 0);
        assert_eq!(h.whitelist, WhitelistState::StaleIface);
    }

    /// 例外四态（off / on{remaining} / expired / pending）+ R8 口径：
    /// 标记丢失但规则仍在 → 如实显示 on（不谎报 off）
    #[test]
    fn judge_health_exception_four_states() {
        let now = 1_000_000_000_000u64;
        let ttl_ms = EXCEPTION_TTL_SECS * 1000;
        let mut probe = probe_with(None, None, false);
        probe.exc3001.present = false;
        // off：关 ∧ 规则不在（默认收口态，AC5）
        assert_eq!(judge(&probe, false, 0, now, 0).exception, ExceptionState::Off);
        // on：开 ∧ 规则在 ∧ 未满 12h（剩余 = TTL − elapsed）
        let mut on = probe.clone();
        on.exc3001.present = true;
        let since = now - 3 * 3600 * 1000;
        assert_eq!(
            judge(&on, true, since, now, 0).exception,
            ExceptionState::On { remaining_secs: EXCEPTION_TTL_SECS - 3 * 3600 }
        );
        // expired：开 ∧ 规则在 ∧ 满 12h（回落未完成仍放行，AC8 如实呈现）
        assert_eq!(judge(&on, true, now - ttl_ms, now, 0).exception, ExceptionState::Expired);
        // pending：开 ∧ 规则不在（派发成功但未生效/被中途删 → 引导重新开启）
        assert_eq!(judge(&probe, true, since, now, 0).exception, ExceptionState::Pending);
        // R8：关 ∧ 规则在（标记丢失/外部建规则）→ 以实况为准 on（满额展示）
        assert_eq!(
            judge(&on, false, 0, now, 0).exception,
            ExceptionState::On { remaining_secs: EXCEPTION_TTL_SECS }
        );
    }

    /// legacy_present（任一旧规则 → 迁移横幅，AC10）× public_blocks_exception
    /// （例外生效 ∧ ∃公用活动网络，plan R5）
    #[test]
    fn judge_health_legacy_and_public_blocks() {
        let now = 1_000_000_000_000u64;
        let since = now - 3600 * 1000;
        let mut probe = probe_with(None, None, false);
        probe.exc3001.present = true;
        // legacy：两条任一为真即横幅
        assert!(!judge(&probe, true, since, now, 0).legacy_present);
        probe.legacy443 = true;
        assert!(judge(&probe, true, since, now, 0).legacy_present);
        probe.legacy443 = false;
        probe.legacy3001 = true;
        assert!(judge(&probe, true, since, now, 0).legacy_present);
        // public_blocks：on + 公用 → 真；on 无公用 → 假；off/pending + 公用 → 假
        assert!(judge(&probe, true, since, now, 1).public_blocks_exception, "on ∧ 公用：直访不生效提示（R5）");
        assert!(!judge(&probe, true, since, now, 0).public_blocks_exception);
        probe.exc3001.present = false;
        assert!(!judge(&probe, false, 0, now, 1).public_blocks_exception, "off 不受公用影响");
        assert!(!judge(&probe, true, since, now, 1).public_blocks_exception, "pending 无规则，无「不生效」可言");
        // expired 仍放行 → 公用提示保持（回落完成前暴露面还在）
        probe.exc3001.present = true;
        assert!(judge(&probe, true, now - EXCEPTION_TTL_SECS * 1000, now, 1).public_blocks_exception);
    }

    /// 12h 回落边界（AC8，需求方定案 `now - since == 12h` 即到期）+ 剩余秒数
    #[test]
    fn exception_ttl_boundary_and_remaining() {
        let since = 1_000_000_000_000u64;
        let ttl = EXCEPTION_TTL_SECS;
        // ==TTL 即到期（边界钉）；-1s 未到期；+1s 到期
        assert!(exception_expired(since + ttl * 1000, since, ttl));
        assert!(!exception_expired(since + ttl * 1000 - 1, since, ttl));
        assert!(exception_expired(since + ttl * 1000 + 1, since, ttl));
        // since=0（标记损坏）fail-safe：立即到期（触发回落 = 收紧方向，R8）
        assert!(exception_expired(since, 0, ttl));
        // 剩余秒数：TTL − elapsed，下限 0
        assert_eq!(exception_remaining_secs(since, since, ttl), ttl);
        assert_eq!(exception_remaining_secs(since + 3600 * 1000, since, ttl), ttl - 3600);
        assert_eq!(exception_remaining_secs(since + ttl * 1000 + 5, since, ttl), 0, "到期后不为负");
    }

    /// 回落决策矩阵（plan §3.4 表全行；到期行优先于 pending 行）
    #[test]
    fn revert_decision_matrix() {
        use RevertDecision as D;
        // 关：无回落语义（其余输入不看）
        for (expired, present) in [(true, true), (true, false), (false, true), (false, false)] {
            assert_eq!(revert_decision(false, expired, present), D::Noop, "expired={expired} present={present}");
        }
        // 开 ∧ 满 12h ∧ 规则在 → UAC 派发 exception-off（watcher 立即，AC8）
        assert_eq!(revert_decision(true, true, true), D::DispatchExceptionOff);
        // 开 ∧ 满 12h ∧ 规则不在（已被外部删/上次已删成）→ 免 UAC 本地清标记
        assert_eq!(revert_decision(true, true, false), D::ClearMarkLocally);
        // 开 ∧ 未满 12h ∧ 规则在 → on 倒计时（watcher 每分钟核对剩余）
        assert_eq!(revert_decision(true, false, true), D::Noop);
        // 开 ∧ 未满 12h ∧ 规则不在 → pending（引导重新开启，不动）
        assert_eq!(revert_decision(true, false, false), D::PendingObserve);
    }

    /// 回落派发窗口：从未派发 → 立即；距上次 <30min → 等待（AC8 每 30min 重试）
    #[test]
    fn revert_due_respects_retry_window() {
        let now = Instant::now();
        assert!(revert_due(None, now), "从未派发：立即");
        assert!(!revert_due(Some(now - Duration::from_secs(29 * 60)), now), "29min：窗口内不重试");
        assert!(revert_due(Some(now - Duration::from_secs(30 * 60)), now), "满 30min：到点");
        assert!(revert_due(Some(now - Duration::from_secs(31 * 60)), now), "超窗：到点");
    }

    /// LanHealth 序列化契约（前端 types.ts 对齐：camelCase + exception tag 形态）
    #[test]
    fn lan_health_serializes_frontend_contract() {
        let h = LanHealth {
            whitelist: WhitelistState::StaleIface,
            legacy_present: true,
            exception: ExceptionState::On { remaining_secs: 3600 },
            public_blocks_exception: true,
        };
        let j = serde_json::to_string(&h).unwrap();
        assert!(j.contains(r#""whitelist":"staleIface""#), "{j}");
        assert!(j.contains(r#""legacyPresent":true"#), "{j}");
        assert!(j.contains(r#""exception":{"state":"on","remainingSecs":3600}"#), "{j}");
        assert!(j.contains(r#""publicBlocksException":true"#), "{j}");
        // 其余四态 tag 形态
        for (state, frag) in [
            (ExceptionState::Off, r#""state":"off""#),
            (ExceptionState::Expired, r#""state":"expired""#),
            (ExceptionState::Pending, r#""state":"pending""#),
        ] {
            let j = serde_json::to_string(&LanHealth {
                whitelist: WhitelistState::Dormant,
                legacy_present: false,
                exception: state,
                public_blocks_exception: false,
            })
            .unwrap();
            assert!(j.contains(frag), "{state:?}: {j}");
        }
        // whitelist 五态字符串（前端 chip 分类键）
        for (s, tag) in [
            (WhitelistState::Ok, "ok"),
            (WhitelistState::Missing, "missing"),
            (WhitelistState::StaleCidr, "staleCidr"),
            (WhitelistState::StaleIface, "staleIface"),
            (WhitelistState::Dormant, "dormant"),
        ] {
            let j = serde_json::to_string(&s).unwrap();
            assert_eq!(j, format!(r#""{tag}""#), "{s:?}");
        }
    }

    // ── T3：LanGuardMonitor 状态机（脚本化 probe 输出驱动，零真实进程）────

    use std::collections::VecDeque;
    use std::sync::Mutex as StdMutex;

    /// mock 探测源：按序返回脚本化 stdout（耗尽 → Err，模拟探测失败）
    struct MockLanProbe {
        outs: StdMutex<VecDeque<Result<String, String>>>,
    }
    impl LanProbe for MockLanProbe {
        fn status(&self, _vip: &str) -> Result<String, String> {
            self.outs.lock().unwrap().pop_front().unwrap_or(Err("耗尽".into()))
        }
    }

    /// 事件出口 mock：记录全部载荷
    #[derive(Default)]
    struct RecordingLanSink {
        emissions: StdMutex<Vec<LanHealth>>,
    }
    impl LanEventSink for RecordingLanSink {
        fn emit_lan_health(&self, h: &LanHealth) {
            self.emissions.lock().unwrap().push(h.clone());
        }
    }

    /// 回落执行面 mock：记录 UAC 派发与本地清标记
    #[derive(Default)]
    struct MockRevert {
        dispatched: StdMutex<u32>,
        cleared: StdMutex<u32>,
    }
    impl RevertExecutor for MockRevert {
        fn dispatch_exception_off(&self) -> Result<(), String> {
            *self.dispatched.lock().unwrap() += 1;
            Ok(())
        }
        fn clear_exception_mark(&self) {
            *self.cleared.lock().unwrap() += 1;
        }
    }

    /// 可变输入仓（测试中切换例外标记/网络注入）
    type SharedInputs = StdMutex<LanInputs>;

    fn inputs(enabled: bool, since: u64, publics: usize) -> LanInputs {
        LanInputs {
            virtual_ip: "10.126.126.1".into(),
            cidr: CIDR.into(),
            exception_enabled: enabled,
            exception_since_ms: since,
            networks: (0..publics)
                .map(|i| NetworkEntry {
                    name: format!("net{i}"),
                    if_index: i as u32 + 1,
                    category: NetCategory::Public,
                })
                .collect(),
        }
    }

    /// 收口态脚本输出直拼（plan §4.2 形态；TUN 在场 et_8_1999@10.126.126.1）
    fn status_raw(mesh_present: bool, remote: Option<&str>, iface: Option<&str>, exc_present: bool) -> String {
        let mesh = if mesh_present {
            format!(
                r#"{{"present":true,"remote":{},"iface":{},"profile":0}}"#,
                serde_json::to_string(remote.unwrap_or("")).unwrap(),
                serde_json::to_string(iface.unwrap_or("")).unwrap(),
            )
        } else {
            r#"{"present":false,"remote":null,"iface":null,"profile":0}"#.into()
        };
        format!(
            r#"{{"legacy443":false,"legacy3001":false,"mesh443":{mesh},"exc3001":{{"present":{exc_present},"profile":2}},"tun":{{"name":"et_8_1999","ip":"10.126.126.1"}}}}"#
        )
    }

    fn monitor_with(
        outs: Vec<Result<String, String>>,
        shared: Arc<SharedInputs>,
        revert: Arc<MockRevert>,
    ) -> LanGuardMonitor {
        let sink = Arc::new(RecordingLanSink::default());
        LanGuardMonitor::new(
            Arc::new(MockLanProbe { outs: StdMutex::new(outs.into()) }),
            Arc::new(move || shared.lock().unwrap().clone()),
            sink,
        )
        .with_revert(revert)
    }

    /// refresh 状态机：A（首查发声）→ A（不变不发声）→ B（变化发声）→ 探测失败
    /// （保持 B 静默）；首查即失败 → None（沿 monitor_emits_only_on_change 先例）
    #[test]
    fn monitor_refresh_dedupes_and_degrades() {
        let shared = Arc::new(StdMutex::new(inputs(false, 0, 0)));
        let revert = Arc::new(MockRevert::default());
        let ok_a = status_raw(true, Some(CIDR), Some(TUN), false);
        let ok_b = status_raw(true, Some("10.200.0.0/24"), Some(TUN), false);
        let (m, sink) = {
            let sink = Arc::new(RecordingLanSink::default());
            let m = LanGuardMonitor::new(
                Arc::new(MockLanProbe {
                    outs: StdMutex::new(vec![
                        Ok(ok_a.clone()),
                        Ok(ok_a),
                        Ok(ok_b),
                        Err("ps 超时".into()),
                        Err("ps 超时".into()),
                    ]
                    .into()),
                }),
                Arc::new({
                    let s = shared.clone();
                    move || s.lock().unwrap().clone()
                }),
                sink.clone(),
            );
            (m, sink)
        };
        // 首查：ok 态发声一次
        let first = m.refresh().unwrap();
        assert_eq!(first.whitelist, WhitelistState::Ok);
        assert_eq!(sink.emissions.lock().unwrap().len(), 1);
        // 重复不变：不发声
        assert_eq!(m.refresh().unwrap().whitelist, WhitelistState::Ok);
        assert_eq!(sink.emissions.lock().unwrap().len(), 1, "状态未变不发事件");
        // 网段失配 → stale_cidr 变化发声（AC9 判定面）
        assert_eq!(m.refresh().unwrap().whitelist, WhitelistState::StaleCidr);
        assert_eq!(sink.emissions.lock().unwrap().len(), 2, "变化才发声");
        // 探测失败：回缓存（staleCidr）且不发声
        let held = m.refresh().unwrap();
        assert_eq!(held.whitelist, WhitelistState::StaleCidr);
        assert_eq!(sink.emissions.lock().unwrap().len(), 2, "失败不发声");
        assert!(held.public_blocks_exception == false);
        // 无缓存时首查即失败 → None（lan_guard_status「无缓存 → 探测失败态」数据源）
        let shared_empty = Arc::new(StdMutex::new(inputs(false, 0, 0)));
        let m2 = {
            let sink = Arc::new(RecordingLanSink::default());
            LanGuardMonitor::new(
                Arc::new(MockLanProbe { outs: StdMutex::new(vec![Err("无脚本".into())].into()) }),
                Arc::new({
                    let s = shared_empty.clone();
                    move || s.lock().unwrap().clone()
                }),
                sink,
            )
        };
        assert!(m2.refresh().is_none(), "首查即失败 → None");
        assert!(m2.current().is_none());
        let _ = revert;
    }

    /// 启动补回落 + 30min 重试窗（plan §3.4：开 ∧ 到期 ∧ 规则在 → UAC 派发，
    /// 窗口内不重复；规则消失后 → 免 UAC 清标记）
    #[test]
    fn monitor_tick_dispatches_revert_then_clears_mark() {
        let since = now_ms() - EXCEPTION_TTL_SECS * 1000 - 1000; // 已到期
        let shared = Arc::new(StdMutex::new(inputs(true, since, 0)));
        let revert = Arc::new(MockRevert::default());
        let m = {
            let s = shared.clone();
            LanGuardMonitor::new(
                Arc::new(MockLanProbe {
                    outs: StdMutex::new(vec![
                        Ok(status_raw(true, Some(CIDR), Some(TUN), true)), // 规则在 → 派发
                        Ok(status_raw(true, Some(CIDR), Some(TUN), true)), // 窗口内：不重发
                        Ok(status_raw(false, None, None, false)),          // 规则已删 → 清标记
                    ]
                    .into()),
                }),
                Arc::new(move || s.lock().unwrap().clone()),
                Arc::new(RecordingLanSink::default()),
            )
            .with_revert(revert.clone())
        };
        // 第一轮（= 启动补回落自检）：到期 ∧ 规则在 → UAC 派发恰一次
        m.tick();
        assert_eq!(*revert.dispatched.lock().unwrap(), 1, "启动补回落：到期∧规则在 → UAC 派发");
        assert_eq!(*revert.cleared.lock().unwrap(), 0);
        // 第二轮（数秒内）：仍在 30min 重试窗 → 不重复派发（看板保持 expired 如实呈现）
        m.tick();
        assert_eq!(*revert.dispatched.lock().unwrap(), 1, "30min 重试窗内不重复弹 UAC（AC8）");
        // 第三轮：脚本已删规则 → 免 UAC 本地清标记恰一次
        m.tick();
        assert_eq!(*revert.dispatched.lock().unwrap(), 1);
        assert_eq!(*revert.cleared.lock().unwrap(), 1, "规则不在 → 免 UAC 清标记（plan §3.4）");
    }

    /// 到期 ∧ 规则不在的启动自检：直接清标记，零 UAC；派发失败（UAC 拒绝）也计入
    /// 窗口（下一窗口重试，AC8），且探测失败轮不做任何回落决策
    #[test]
    fn monitor_tick_clears_mark_without_uac_and_skips_on_probe_failure() {
        let since = now_ms() - EXCEPTION_TTL_SECS * 1000 - 1000;
        let shared = Arc::new(StdMutex::new(inputs(true, since, 0)));
        let revert = Arc::new(MockRevert::default());
        let m = {
            let s = shared.clone();
            LanGuardMonitor::new(
                Arc::new(MockLanProbe {
                    outs: StdMutex::new(vec![
                        Ok(status_raw(false, None, None, false)), // 到期 ∧ 规则不在 → 清标记
                        Err("ps 超时".into()),                    // 探测失败：实况未知
                    ]
                    .into()),
                }),
                Arc::new(move || s.lock().unwrap().clone()),
                Arc::new(RecordingLanSink::default()),
            )
            .with_revert(revert.clone())
        };
        m.tick();
        assert_eq!(*revert.cleared.lock().unwrap(), 1, "到期∧规则不在 → 免 UAC 清标记");
        assert_eq!(*revert.dispatched.lock().unwrap(), 0, "无规则可删，零 UAC");
        // 探测失败轮：规则实况未知 → 不做任何回落决策（盲动防线）
        m.tick();
        assert_eq!(*revert.cleared.lock().unwrap(), 1);
        assert_eq!(*revert.dispatched.lock().unwrap(), 0);
    }

    /// 未到期轮不触发任何回落（on 倒计时 / pending 观察均不动手）
    #[test]
    fn monitor_tick_holds_while_counting_down() {
        let since = now_ms() - 3600 * 1000; // 1h 前，未到期
        let shared = Arc::new(StdMutex::new(inputs(true, since, 0)));
        let revert = Arc::new(MockRevert::default());
        let m = {
            let s = shared.clone();
            LanGuardMonitor::new(
                Arc::new(MockLanProbe {
                    outs: StdMutex::new(vec![
                        Ok(status_raw(true, Some(CIDR), Some(TUN), true)), // on：倒计时
                        Ok(status_raw(false, None, None, false)),          // pending：观察
                    ]
                    .into()),
                }),
                Arc::new(move || s.lock().unwrap().clone()),
                Arc::new(RecordingLanSink::default()),
            )
            .with_revert(revert.clone())
        };
        m.tick();
        m.tick();
        assert_eq!(*revert.dispatched.lock().unwrap(), 0, "未到期零派发");
        assert_eq!(*revert.cleared.lock().unwrap(), 0, "pending 观察不动（引导重新开启）");
    }
}
