//! 局域网边界防火墙契约（spec 010）：规则名/退出码/例外 TTL 常量 + lan-guard.ps1
//! 提权派发参数构造。
//!
//! 分工（tasks T2/T3/T4）：本批（T2）只落**契约层**——常量与派发参数构造及其
//! 单测；status 探测解析 / 健康判定（judge_health）/ 12h 回落 watcher 由 T3
//! 实现，命令层接线（lan_guard_* 四命令）由 T4 实现。规则 CRUD 单点在
//! lan-guard.ps1（plan §2），Rust 侧常量是「双端形态契约锚」（mesh.rs
//! service_bin_path 先例：脚本同值漂移即单测失配暴露）。

#![allow(dead_code)] // 本批仅契约层：T3 探测/健康判定、T4 命令层接线后移除

use crate::lang::Lang;
use std::path::Path;

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
    Ok(format!("-NoProfile -NonInteractive -WindowStyle Hidden -Command \"{inner}\""))
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
}
