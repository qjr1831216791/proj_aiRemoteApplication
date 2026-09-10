//! 停止管线（T9：AC2、spec §4.4 逐字落地；stop-server.ps1 语义的程序内等效实现）。
//!
//! 分组件路径（spec §4.4 停止顺序与兜底）：
//! - **CloudCLI**：定位 3001 监听 PID → 身份校验（node.exe）→ **先收集进程树快照
//!   再逐杀**（`taskkill /T` 语义的手工实现：监听者先死会使子进程脱树成孤儿，
//!   故必须先枚举后动手，spec §4.1 为此不直接复用 stop-server.ps1）→ 端口复核
//! - **Caddy**：`caddy stop`（5s 超时）→ 失败则按 443 找监听进程、校验可执行
//!   路径为 StackDir 下 caddy.exe 后强杀 → 复核
//! - **ddns-go**：按可执行路径匹配进程（非进程名）强杀 → 复核
//!
//! 单组件 10s 预算：超时记日志 + detail 给手动排查命令，放行不阻塞其余组件
//! （AC2）。进程操作全部经 `ProcessOps` seam（taskkill/进程枚举也抽象），
//! 单测断言调用顺序与参数。
//!
//! detail 文案经 [`crate::lang::stop_texts`] 双语化（AC25：用户可见路径
//! zh/en 同源；调用方传生效语言，编排器取实时语言源）。

use crate::consts::{CADDY_PORT, CLOUDCLI_EXE_NAME, CLOUDCLI_PORT, DDNSGO_PORT, DEFAULT_STACK_DIR};
use crate::lang::StopTexts;
use crate::probe::{paths_equal, StatusProbe};
use crate::scripts::{CommandExecutor, CommandSpec, CREATE_NO_WINDOW, ExecOutcome};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// `caddy stop` 超时（spec §4.4）
pub const CADDY_STOP_TIMEOUT: Duration = Duration::from_secs(5);
/// 单组件停止预算（AC2：10s 超时记日志放行）
pub const COMPONENT_STOP_TIMEOUT: Duration = Duration::from_secs(10);
/// 杀进程后复核前的端口释放缓冲（对齐 stop-server.ps1 的 500ms）
pub const PORT_RELEASE_GRACE: Duration = Duration::from_millis(500);

// ── 配置与结论 ─────────────────────────────────────────────────────────────

/// 停止管线配置（默认值取自常量；单测注入毫秒级值）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StopConfig {
    pub caddy_stop_timeout: Duration,
    pub component_timeout: Duration,
    pub port_grace: Duration,
}

impl Default for StopConfig {
    fn default() -> Self {
        Self {
            caddy_stop_timeout: CADDY_STOP_TIMEOUT,
            component_timeout: COMPONENT_STOP_TIMEOUT,
            port_grace: PORT_RELEASE_GRACE,
        }
    }
}

/// 单组件停止结论（orchestrator 据此迁移状态）
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StopOutcome {
    /// 本就不在运行（无监听/无进程）
    AlreadyStopped,
    /// 停止成功且端口复核通过
    Stopped,
    /// 复核未通过 / 身份不符拒杀等：附原因 + 手动排查命令（AC2 failed 态）
    Failed(String),
    /// 单组件预算耗尽：记日志 + 手动排查命令，放行不阻塞其余组件（AC2）
    TimedOut(String),
}

// ── 进程操作 seam（taskkill/进程枚举抽象；单测 mock 断言顺序与参数）──────

pub trait ProcessOps: Send + Sync {
    /// pid 的全部后代进程（进程树快照，不含自身；先收集后杀的依据）
    fn descendants(&self, pid: u32) -> Vec<u32>;
    /// 按可执行完整路径枚举进程（ddns-go 判据；路径大小写不敏感）
    fn pids_by_exe(&self, exe: &str) -> Vec<u32>;
    /// 强杀单个进程（taskkill /F 语义；目标已退出视为成功，幂等）
    fn kill(&self, pid: u32) -> Result<(), String>;
}

/// Windows 真实实现：sysinfo 快照枚举 + TerminateProcess
pub struct SysinfoProcessOps;

#[cfg(windows)]
impl ProcessOps for SysinfoProcessOps {
    fn descendants(&self, pid: u32) -> Vec<u32> {
        use std::collections::{HashMap, HashSet, VecDeque};
        use sysinfo::{Pid, ProcessesToUpdate, System};

        let mut sys = System::new();
        sys.refresh_processes(ProcessesToUpdate::All, true);
        // parent 链建子进程索引，自监听 PID 广度优先收集全部后代
        let mut children: HashMap<Pid, Vec<Pid>> = HashMap::new();
        for (proc_pid, proc) in sys.processes() {
            if let Some(parent) = proc.parent() {
                children.entry(parent).or_default().push(*proc_pid);
            }
        }
        let root = Pid::from_u32(pid);
        let mut seen: HashSet<Pid> = HashSet::from([root]);
        let mut out: Vec<u32> = Vec::new();
        let mut queue: VecDeque<Pid> = VecDeque::from([root]);
        while let Some(cur) = queue.pop_front() {
            for child in children.get(&cur).into_iter().flatten() {
                if seen.insert(*child) {
                    out.push(child.as_u32());
                    queue.push_back(*child);
                }
            }
        }
        out
    }

    fn pids_by_exe(&self, exe: &str) -> Vec<u32> {
        use sysinfo::{ProcessesToUpdate, System};

        let mut sys = System::new();
        sys.refresh_processes(ProcessesToUpdate::All, true);
        sys.processes()
            .iter()
            .filter(|(_, proc)| {
                proc.exe()
                    .map(|e| paths_equal(&e.to_string_lossy(), exe))
                    .unwrap_or(false)
            })
            .map(|(pid, _)| pid.as_u32())
            .collect()
    }

    fn kill(&self, pid: u32) -> Result<(), String> {
        use sysinfo::{Pid, ProcessesToUpdate, System};

        let mut sys = System::new();
        sys.refresh_processes(ProcessesToUpdate::Some(&[Pid::from_u32(pid)]), true);
        match sys.process(Pid::from_u32(pid)) {
            Some(proc) if proc.kill() => Ok(()),
            // 目标已退出：停止语义幂等成功
            None => Ok(()),
            Some(_) => Err(format!("结束进程失败（pid={pid}，TerminateProcess 被拒）")),
        }
    }
}

/// 非 Windows 兜底（项目仅面向 Windows，此分支仅为可编译性）
#[cfg(not(windows))]
impl ProcessOps for SysinfoProcessOps {
    fn descendants(&self, _pid: u32) -> Vec<u32> {
        Vec::new()
    }
    fn pids_by_exe(&self, _exe: &str) -> Vec<u32> {
        Vec::new()
    }
    fn kill(&self, pid: u32) -> Result<(), String> {
        Err(format!("结束进程仅支持 Windows（pid={pid}）"))
    }
}

// ── 管线（纯函数化：依赖全注入；调用顺序即语义）──────────────────────────

/// 预算检查：到期 → TimedOut（AC2：放行不阻塞）
fn budget_expired(
    cfg: &StopConfig,
    deadline: Instant,
    port: u16,
    texts: &StopTexts,
) -> Option<StopOutcome> {
    if Instant::now() >= deadline {
        Some(StopOutcome::TimedOut(texts.timeout_released(
            cfg.component_timeout.as_secs(),
            port,
        )))
    } else {
        None
    }
}

/// 端口复核：grace 缓冲后单次探测（对齐 stop-server.ps1 语义）
fn verify_port_released(
    probe: &dyn StatusProbe,
    port: u16,
    cfg: &StopConfig,
    deadline: Instant,
    texts: &StopTexts,
) -> StopOutcome {
    if let Some(t) = budget_expired(cfg, deadline, port, texts) {
        return t;
    }
    std::thread::sleep(cfg.port_grace);
    let holders = probe.port_holders(port);
    if holders.listen_addrs.is_empty() {
        return StopOutcome::Stopped;
    }
    let name = holders
        .processes
        .iter()
        .find_map(|p| p.exe_name.clone())
        .unwrap_or_else(|| "unknown".into());
    StopOutcome::Failed(texts.verify_failed(port, &name))
}

/// Caddy 优雅停命令（`<StackDir>\caddy.exe stop`，5s 超时，stdio→日志）
pub fn caddy_stop_spec(cfg: &StopConfig, log_dir: &Path, stack_dir: &str) -> CommandSpec {
    CommandSpec {
        program: format!(r"{stack_dir}\caddy.exe"),
        args: vec!["stop".into()],
        creation_flags: CREATE_NO_WINDOW,
        stdout_log: Some(log_dir.join("caddy-stop.log")),
        stderr_log: Some(log_dir.join("caddy-stop.err.log")),
        timeout: cfg.caddy_stop_timeout,
        working_dir: Some(PathBuf::from(stack_dir)),
    }
}

/// CloudCLI 停止：定位 → 身份校验 → 先收集进程树再逐杀 → 复核（spec §4.4）
pub fn stop_cloudcli(
    probe: &dyn StatusProbe,
    procs: &dyn ProcessOps,
    cfg: &StopConfig,
    texts: &StopTexts,
    deadline: Instant,
) -> StopOutcome {
    if let Some(t) = budget_expired(cfg, deadline, CLOUDCLI_PORT, texts) {
        return t;
    }
    // 1. 定位监听者
    let holders = probe.port_holders(CLOUDCLI_PORT);
    if holders.listen_addrs.is_empty() {
        return StopOutcome::AlreadyStopped;
    }
    // 2. 身份校验：监听者必须是 node.exe（CloudCLI 宿主），否则拒杀
    let mut victims: Vec<u32> = Vec::new();
    for p in &holders.processes {
        let is_node = p
            .exe_name
            .as_deref()
            .map(|n| n.eq_ignore_ascii_case(CLOUDCLI_EXE_NAME))
            .unwrap_or(false);
        if is_node && !victims.contains(&p.pid) {
            victims.push(p.pid);
        }
    }
    if victims.is_empty() {
        let name = holders
            .processes
            .iter()
            .find_map(|p| p.exe_name.clone())
            .unwrap_or_else(|| "unknown".into());
        return StopOutcome::Failed(texts.refuse_cloudcli(CLOUDCLI_PORT, &name));
    }
    // 3. 先收集进程树快照再动手（监听者先死会让 claude 会话子进程脱树成孤儿）
    let mut tree: Vec<u32> = Vec::new();
    for &pid in &victims {
        for child in procs.descendants(pid) {
            if !tree.contains(&child) && !victims.contains(&child) {
                tree.push(child);
            }
        }
    }
    // 4. 逐杀：子先父后（叶向上）
    for &pid in &tree {
        if let Some(t) = budget_expired(cfg, deadline, CLOUDCLI_PORT, texts) {
            return t;
        }
        if let Err(e) = procs.kill(pid) {
            log::warn!("结束 CloudCLI 子进程失败（pid={pid}）：{e}");
        }
    }
    for &pid in &victims {
        if let Some(t) = budget_expired(cfg, deadline, CLOUDCLI_PORT, texts) {
            return t;
        }
        if let Err(e) = procs.kill(pid) {
            log::warn!("结束 CloudCLI 监听进程失败（pid={pid}）：{e}");
        }
    }
    // 5. 端口复核
    verify_port_released(probe, CLOUDCLI_PORT, cfg, deadline, texts)
}

/// Caddy 停止：caddy stop（5s）→ 443+路径校验兜底强杀 → 复核（spec §4.4）
pub fn stop_caddy(
    probe: &dyn StatusProbe,
    exec: &dyn CommandExecutor,
    procs: &dyn ProcessOps,
    cfg: &StopConfig,
    texts: &StopTexts,
    log_dir: &Path,
    stack_dir: &str,
    deadline: Instant,
) -> StopOutcome {
    if let Some(t) = budget_expired(cfg, deadline, CADDY_PORT, texts) {
        return t;
    }
    if probe.port_holders(CADDY_PORT).listen_addrs.is_empty() {
        return StopOutcome::AlreadyStopped;
    }
    // 1. 优雅停：caddy stop（admin API；5s 超时由 CommandSpec 承载）
    let graceful_ok = matches!(
        exec.execute(&caddy_stop_spec(cfg, log_dir, stack_dir)),
        ExecOutcome::Exited(0)
    );
    if graceful_ok {
        match verify_port_released(probe, CADDY_PORT, cfg, deadline, texts) {
            StopOutcome::Stopped => return StopOutcome::Stopped,
            // 预算耗尽必须放行上报；端口仍占则落入兜底强杀
            t @ StopOutcome::TimedOut(_) => return t,
            StopOutcome::Failed(_) | StopOutcome::AlreadyStopped => {}
        }
    } else {
        log::warn!("caddy stop 未成功（假停风险，plan §7）：转入 443 端口兜底强杀");
    }
    // 2. 兜底：443 找监听进程 → 校验可执行路径为本栈 caddy.exe → 强杀
    if let Some(t) = budget_expired(cfg, deadline, CADDY_PORT, texts) {
        return t;
    }
    let holders = probe.port_holders(CADDY_PORT);
    if holders.listen_addrs.is_empty() {
        return StopOutcome::Stopped; // 优雅停生效（复核间隙内已释放）
    }
    let expected = format!(r"{stack_dir}\caddy.exe");
    let victim = holders
        .processes
        .iter()
        .find(|p| p.exe.as_deref().map(|e| paths_equal(e, &expected)).unwrap_or(false));
    let Some(victim) = victim else {
        let name = holders
            .processes
            .iter()
            .find_map(|p| p.exe_name.clone())
            .unwrap_or_else(|| "unknown".into());
        return StopOutcome::Failed(texts.refuse_caddy(CADDY_PORT, &name));
    };
    let pid = victim.pid;
    let tree = procs.descendants(pid);
    for child in &tree {
        if let Some(t) = budget_expired(cfg, deadline, CADDY_PORT, texts) {
            return t;
        }
        if let Err(e) = procs.kill(*child) {
            log::warn!("结束 caddy 子进程失败（pid={child}）：{e}");
        }
    }
    if let Some(t) = budget_expired(cfg, deadline, CADDY_PORT, texts) {
        return t;
    }
    if let Err(e) = procs.kill(pid) {
        log::warn!("结束 caddy 监听进程失败（pid={pid}）：{e}");
    }
    // 3. 端口复核
    verify_port_released(probe, CADDY_PORT, cfg, deadline, texts)
}

/// ddns-go 停止：按可执行路径匹配（非进程名）强杀 → 复核（spec §4.4）
pub fn stop_ddnsgo(
    probe: &dyn StatusProbe,
    procs: &dyn ProcessOps,
    cfg: &StopConfig,
    texts: &StopTexts,
    stack_dir: &str,
    deadline: Instant,
) -> StopOutcome {
    if let Some(t) = budget_expired(cfg, deadline, DDNSGO_PORT, texts) {
        return t;
    }
    // 判据是可执行全路径（同名不同路径的进程不碰）
    let expected = format!(r"{stack_dir}\ddns-go.exe");
    let pids = procs.pids_by_exe(&expected);
    if pids.is_empty() {
        // 无本栈进程：复核端口，被无关进程占时如实报告
        let holders = probe.port_holders(DDNSGO_PORT);
        if holders.listen_addrs.is_empty() {
            return StopOutcome::AlreadyStopped;
        }
        let name = holders
            .processes
            .iter()
            .find_map(|p| p.exe_name.clone())
            .unwrap_or_else(|| "unknown".into());
        return StopOutcome::Failed(texts.refuse_ddnsgo(DDNSGO_PORT, &name));
    }
    for &pid in &pids {
        if let Some(t) = budget_expired(cfg, deadline, DDNSGO_PORT, texts) {
            return t;
        }
        if let Err(e) = procs.kill(pid) {
            log::warn!("结束 ddns-go 进程失败（pid={pid}）：{e}");
        }
    }
    verify_port_released(probe, DDNSGO_PORT, cfg, deadline, texts)
}

// ── 单元测试（先红后绿：桩期 todo!()）──────────────────────────────────────

#[cfg(test)]
mod tests {
    use crate::orchestrator::test_support::*;
    use crate::probe::PortHolders;
    use super::*;

    /// 中文词条（zh 与历史文案逐字一致，既有断言不变）
    fn zh() -> crate::lang::StopTexts {
        crate::lang::stop_texts(crate::lang::Lang::Zh)
    }

    /// 快速配置：毫秒级预算（默认 10s 会拖慢单测）
    fn fast_cfg() -> StopConfig {
        StopConfig {
            caddy_stop_timeout: Duration::from_millis(500),
            component_timeout: Duration::from_secs(2),
            port_grace: Duration::from_millis(0),
        }
    }

    fn deadline(cfg: &StopConfig) -> Instant {
        Instant::now() + cfg.component_timeout
    }

    fn node_holder(pid: u32) -> PortHolders {
        holders(
            &[addr([127, 0, 0, 1])],
            &[(
                pid,
                Some(r"C:\Program Files\nodejs\node.exe"),
                Some("node.exe"),
            )],
        )
    }

    fn caddy_holder(pid: u32) -> PortHolders {
        holders(
            &[addr([0, 0, 0, 0])],
            &[(
                pid,
                Some(r"D:\Software\cloudcli-https\caddy.exe"),
                Some("caddy.exe"),
            )],
        )
    }

    #[test]
    fn stop_config_defaults_follow_spec() {
        // spec §4.4 / AC2：caddy stop 5s、单组件 10s、复核缓冲 500ms
        let cfg = StopConfig::default();
        assert_eq!(cfg.caddy_stop_timeout, Duration::from_secs(5));
        assert_eq!(cfg.component_timeout, Duration::from_secs(10));
        assert_eq!(cfg.port_grace, Duration::from_millis(500));
    }

    #[test]
    fn cloudcli_collects_tree_before_killing_then_verifies() {
        // spec §4.4 核心语义：树收集 → 逐杀（子先父后）→ 端口复核，顺序不可乱
        let probe = ScriptedProbe::new();
        let log = probe.log();
        // 定位时占用（node.exe 身份匹配）→ 复核时已释放
        probe.enqueue_holders(CLOUDCLI_PORT, &[node_holder(100), PortHolders::default()]);
        let procs = MockProcessOps::with_log(log.clone());
        procs.set_descendants(100, &[101, 102]);

        let outcome = stop_cloudcli(&probe, &procs, &fast_cfg(), &zh(), deadline(&fast_cfg()));
        assert_eq!(outcome, StopOutcome::Stopped);

        let entries = log.snapshot();
        let pos = |needle: &str| entries.iter().position(|e| e == needle).unwrap_or_else(|| {
            panic!("调用序列缺 {needle}：{entries:?}")
        });
        let (i_locate, i_verify) = (
            entries.iter().position(|e| e == "probe:port=3001").unwrap(),
            entries.iter().rposition(|e| e == "probe:port=3001").unwrap(),
        );
        let i_tree = pos("procs:descendants(100)->[101, 102]");
        let i_kill_child = pos("procs:kill(101)");
        let i_kill_child2 = pos("procs:kill(102)");
        let i_kill_root = pos("procs:kill(100)");
        assert!(i_locate < i_tree, "必须先定位监听者再收集树：{entries:?}");
        assert!(i_tree < i_kill_child, "必须先收集进程树再动手杀（spec §4.4）：{entries:?}");
        assert!(i_kill_child < i_kill_child2 && i_kill_child2 < i_kill_root, "子进程先杀、监听者最后：{entries:?}");
        assert!(i_kill_root < i_verify, "杀完才复核端口：{entries:?}");
    }

    #[test]
    fn cloudcli_refuses_to_kill_identity_mismatch() {
        // AC7 同源判据：监听者非 node.exe → 拒杀并报告占用者
        let probe = ScriptedProbe::new();
        probe.pin_holders(
            CLOUDCLI_PORT,
            holders(
                &[addr([127, 0, 0, 1])],
                &[(77, Some(r"C:\Windows\System32\svchost.exe"), Some("svchost.exe"))],
            ),
        );
        let log = probe.log();
        let procs = MockProcessOps::with_log(log.clone());

        let outcome = stop_cloudcli(&probe, &procs, &fast_cfg(), &zh(), deadline(&fast_cfg()));
        match outcome {
            StopOutcome::Failed(detail) => {
                assert!(detail.contains("svchost.exe"), "应指出占用者：{detail}");
                assert!(detail.contains("拒绝"), "应说明拒杀：{detail}");
                assert!(detail.contains("netstat"), "应给手动排查命令：{detail}");
            }
            other => panic!("应为 Failed，实际 {other:?}"),
        }
        assert!(!log.snapshot().iter().any(|e| e.starts_with("procs:kill")), "身份不符不得杀进程");
    }

    #[test]
    fn cloudcli_verify_failure_reports_manual_commands() {
        // 杀完端口仍被占（进程立即重监听等）→ failed 附原因 + 排查命令（AC2）
        let probe = ScriptedProbe::new();
        probe.pin_holders(CLOUDCLI_PORT, node_holder(100)); // 定位与复核均占用
        let log = probe.log();
        let procs = MockProcessOps::with_log(log.clone());

        let outcome = stop_cloudcli(&probe, &procs, &fast_cfg(), &zh(), deadline(&fast_cfg()));
        match outcome {
            StopOutcome::Failed(detail) => {
                assert!(detail.contains("复核未通过"), "{detail}");
                assert!(detail.contains("netstat -ano | findstr :3001"), "{detail}");
                assert!(detail.contains("taskkill"), "{detail}");
            }
            other => panic!("应为 Failed，实际 {other:?}"),
        }
        assert!(log.snapshot().iter().any(|e| e == "procs:kill(100)"), "身份匹配的监听者应被结束");
    }

    #[test]
    fn caddy_graceful_stop_releases_port() {
        // 主路径：caddy stop（exit 0）→ 复核通过；无强杀
        let probe = ScriptedProbe::new();
        probe.enqueue_holders(CADDY_PORT, &[caddy_holder(200), PortHolders::default()]);
        let log = probe.log();
        let exec = MockExecutor::with_log(log.clone()); // execute 默认 Exited(0)
        let procs = MockProcessOps::with_log(log.clone());

        let cfg = StopConfig {
            caddy_stop_timeout: CADDY_STOP_TIMEOUT, // 断言默认契约（mock 不耗时）
            ..fast_cfg()
        };
        let outcome = stop_caddy(&probe, &exec, &procs, &cfg, &zh(), Path::new(r"D:\logs"), DEFAULT_STACK_DIR, deadline(&cfg));
        assert_eq!(outcome, StopOutcome::Stopped);

        let executed = exec.executed.lock().unwrap();
        assert_eq!(executed.len(), 1, "caddy stop 应恰好执行一次");
        assert_eq!(executed[0].program, r"D:\Software\cloudcli-https\caddy.exe");
        assert_eq!(executed[0].args, vec!["stop".to_string()]);
        assert_eq!(executed[0].timeout, Duration::from_secs(5), "spec §4.4：caddy stop 5s 超时");
        drop(executed);
        assert!(!log.snapshot().iter().any(|e| e.starts_with("procs:")), "优雅路径不应触进程操作");
    }

    #[test]
    fn caddy_stop_failure_falls_back_to_path_checked_kill() {
        // caddy stop 失败（admin API 不可达）→ 443 找监听 + 校验 StackDir 路径 → 强杀 → 复核
        let probe = ScriptedProbe::new();
        // 调用序列：初始检查(占) → 优雅停失败 → 兜底定位(占) → 杀 → 复核(空)
        probe.enqueue_holders(
            CADDY_PORT,
            &[caddy_holder(200), caddy_holder(200), PortHolders::default()],
        );
        let log = probe.log();
        let exec = MockExecutor::with_exits(&[ExecOutcome::Exited(1)]);
        let procs = MockProcessOps::with_log(log.clone());
        procs.set_descendants(200, &[201]);

        let cfg = fast_cfg();
        let outcome = stop_caddy(&probe, &exec, &procs, &cfg, &zh(), Path::new(r"D:\logs"), DEFAULT_STACK_DIR, deadline(&cfg));
        assert_eq!(outcome, StopOutcome::Stopped);

        let entries = log.snapshot();
        // MockExecutor 未绑日志 → 用规格断言顺序代替：先执行过 caddy stop，再杀进程
        assert_eq!(exec.executed.lock().unwrap().len(), 1, "兜底前应已尝试 caddy stop");
        let i_tree = entries.iter().position(|e| e == "procs:descendants(200)->[201]").unwrap();
        let i_kill = entries.iter().position(|e| e == "procs:kill(200)").unwrap();
        let i_verify = entries.iter().rposition(|e| e == "probe:port=443").unwrap();
        assert!(i_tree < i_kill && i_kill < i_verify, "树收集 → 杀 → 复核：{entries:?}");
        assert!(entries.iter().any(|e| e == "procs:kill(201)"), "caddy 子进程一并结束");
    }

    #[test]
    fn caddy_fallback_refuses_foreign_listener() {
        // 443 被非本栈 caddy.exe 占用（路径不符）→ 拒杀（防误杀无关进程）
        let probe = ScriptedProbe::new();
        let foreign = holders(
            &[addr([0, 0, 0, 0])],
            &[(300, Some(r"E:\elsewhere\caddy.exe"), Some("caddy.exe"))],
        );
        probe.enqueue_holders(CADDY_PORT, &[foreign.clone(), foreign]);
        let log = probe.log();
        let exec = MockExecutor::with_exits(&[ExecOutcome::Exited(1)]);
        let procs = MockProcessOps::with_log(log.clone());

        let cfg = fast_cfg();
        let outcome = stop_caddy(&probe, &exec, &procs, &cfg, &zh(), Path::new(r"D:\logs"), DEFAULT_STACK_DIR, deadline(&cfg));
        match outcome {
            StopOutcome::Failed(detail) => {
                assert!(detail.contains("caddy.exe"), "应指出占用者：{detail}");
                assert!(detail.contains("拒绝"), "{detail}");
            }
            other => panic!("应为 Failed，实际 {other:?}"),
        }
        assert!(!log.snapshot().iter().any(|e| e.starts_with("procs:kill")), "路径不符不得强杀");
    }

    #[test]
    fn ddnsgo_kills_by_executable_path_then_verifies() {
        // spec §4.4：按可执行路径匹配（非进程名/非端口定位）
        let probe = ScriptedProbe::new();
        probe.enqueue_holders(DDNSGO_PORT, &[PortHolders::default()]);
        let log = probe.log();
        let procs = MockProcessOps::with_log(log.clone());
        procs.set_by_exe(r"D:\Software\cloudcli-https\ddns-go.exe", &[300, 301]);

        let outcome = stop_ddnsgo(&probe, &procs, &fast_cfg(), &zh(), DEFAULT_STACK_DIR, deadline(&fast_cfg()));
        assert_eq!(outcome, StopOutcome::Stopped);

        let entries = log.snapshot();
        assert!(
            entries.iter().any(|e| e == "procs:pids_by_exe(D:\\Software\\cloudcli-https\\ddns-go.exe)->[300, 301]"),
            "应以栈目录全路径匹配进程：{entries:?}"
        );
        assert!(entries.iter().any(|e| e == "procs:kill(300)") && entries.iter().any(|e| e == "procs:kill(301)"));
        let i_kill = entries.iter().position(|e| e == "procs:kill(301)").unwrap();
        let i_verify = entries.iter().rposition(|e| e == "probe:port=9876").unwrap();
        assert!(i_kill < i_verify, "杀完才复核：{entries:?}");
    }

    #[test]
    fn ddnsgo_reports_foreign_port_holder() {
        // 无本栈 ddns-go 进程但 9876 被占 → 如实报告，不误杀占用者
        let probe = ScriptedProbe::new();
        probe.pin_holders(
            DDNSGO_PORT,
            holders(&[addr([127, 0, 0, 1])], &[(9, Some(r"C:\x\other.exe"), Some("other.exe"))]),
        );
        let log = probe.log();
        let procs = MockProcessOps::with_log(log.clone());

        let outcome = stop_ddnsgo(&probe, &procs, &fast_cfg(), &zh(), DEFAULT_STACK_DIR, deadline(&fast_cfg()));
        match outcome {
            StopOutcome::Failed(detail) => {
                assert!(detail.contains("other.exe"), "{detail}");
                assert!(detail.contains("拒绝"), "{detail}");
            }
            other => panic!("应为 Failed，实际 {other:?}"),
        }
        assert!(!log.snapshot().iter().any(|e| e.starts_with("procs:kill")));
    }

    #[test]
    fn stopped_components_are_noop() {
        // 均未运行 → AlreadyStopped，零执行器/进程调用
        let probe = ScriptedProbe::new(); // holders 默认空
        let log = probe.log();
        let exec = MockExecutor::with_log(log.clone());
        let procs = MockProcessOps::with_log(log.clone());
        let cfg = fast_cfg();

        assert_eq!(
            stop_cloudcli(&probe, &procs, &cfg, &zh(), deadline(&cfg)),
            StopOutcome::AlreadyStopped
        );
        assert_eq!(
            stop_caddy(&probe, &exec, &procs, &cfg, &zh(), Path::new(r"D:\logs"), DEFAULT_STACK_DIR, deadline(&cfg)),
            StopOutcome::AlreadyStopped
        );
        assert_eq!(
            stop_ddnsgo(&probe, &procs, &cfg, &zh(), DEFAULT_STACK_DIR, deadline(&cfg)),
            StopOutcome::AlreadyStopped
        );
        let entries = log.snapshot();
        // ddns-go 的按路径探测（pids_by_exe）属正常调用；不应发生的是杀与树收集
        assert!(
            !entries.iter().any(|e| e.starts_with("procs:kill") || e.starts_with("procs:descendants")),
            "不应有杀进程/树收集：{entries:?}"
        );
        assert!(!entries.iter().any(|e| e.starts_with("exec:")), "不应执行 caddy stop：{entries:?}");
    }

    #[test]
    fn exhausted_budget_times_out_with_manual_hint() {
        // AC2：单组件预算耗尽 → TimedOut + 手动排查命令（放行由 stop_all 语义保证）
        let probe = ScriptedProbe::new();
        probe.pin_holders(CLOUDCLI_PORT, node_holder(100));
        let procs = MockProcessOps::new();
        let cfg = StopConfig {
            component_timeout: Duration::ZERO, // 入口即超预算
            ..fast_cfg()
        };

        let outcome = stop_cloudcli(&probe, &procs, &cfg, &zh(), deadline(&cfg));
        match outcome {
            StopOutcome::TimedOut(detail) => {
                assert!(detail.contains("netstat -ano | findstr :3001"), "{detail}");
                assert!(detail.contains("taskkill"), "{detail}");
            }
            other => panic!("应为 TimedOut，实际 {other:?}"),
        }
    }

    #[test]
    fn verify_waits_port_release_grace() {
        // 复核前须等待端口释放缓冲（对齐 stop-server.ps1 的 500ms 语义）
        let probe = ScriptedProbe::new();
        probe.enqueue_holders(CLOUDCLI_PORT, &[node_holder(100), PortHolders::default()]);
        let procs = MockProcessOps::new();
        let cfg = StopConfig {
            port_grace: Duration::from_millis(120),
            ..fast_cfg()
        };

        let started = Instant::now();
        let outcome = stop_cloudcli(&probe, &procs, &cfg, &zh(), deadline(&cfg));
        assert_eq!(outcome, StopOutcome::Stopped);
        assert!(started.elapsed() >= Duration::from_millis(120), "应等待 grace 后再复核");
    }

    // ── T18：detail 双语（AC25：用户可见路径随生效语言）──────────────────

    #[test]
    fn refusal_details_follow_effective_lang() {
        // 拒杀路径：En 词条生效且不混入中文；zh 路径见上方既有断言
        let probe = ScriptedProbe::new();
        probe.pin_holders(
            CLOUDCLI_PORT,
            holders(
                &[addr([127, 0, 0, 1])],
                &[(77, Some(r"C:\Windows\System32\svchost.exe"), Some("svchost.exe"))],
            ),
        );
        let procs = MockProcessOps::new();
        let en = crate::lang::stop_texts(crate::lang::Lang::En);

        match stop_cloudcli(&probe, &procs, &fast_cfg(), &en, deadline(&fast_cfg())) {
            StopOutcome::Failed(detail) => {
                assert!(detail.contains("refusing to kill"), "{detail}");
                assert!(detail.contains("netstat -ano | findstr :3001"), "{detail}");
                assert!(!detail.contains('拒'), "英文词条不得混入中文：{detail}");
            }
            other => panic!("应为 Failed，实际 {other:?}"),
        }
    }

    #[test]
    fn timeout_detail_follows_effective_lang() {
        // 预算耗尽路径：En 词条生效（AC2 放行语义 + 排查命令）
        let probe = ScriptedProbe::new();
        probe.pin_holders(CLOUDCLI_PORT, node_holder(100));
        let procs = MockProcessOps::new();
        let cfg = StopConfig {
            component_timeout: Duration::ZERO,
            ..fast_cfg()
        };
        let en = crate::lang::stop_texts(crate::lang::Lang::En);

        match stop_cloudcli(&probe, &procs, &cfg, &en, deadline(&cfg)) {
            StopOutcome::TimedOut(detail) => {
                assert!(detail.contains("Stop timed out"), "{detail}");
                assert!(detail.contains("taskkill"), "{detail}");
                assert!(!detail.contains('停'), "英文词条不得混入中文：{detail}");
            }
            other => panic!("应为 TimedOut，实际 {other:?}"),
        }
    }
}
