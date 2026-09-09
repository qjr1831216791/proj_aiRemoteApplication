//! 退出流（T10：AC13~17，plan §3.2 收摊退出时序）。
//!
//! 关键设计（ADR-0002 / AC17 反面教训）：收摊动作**只挂在程序自己发起的显式
//! 退出**路径——托盘「退出」（读 exitAction：keep 直退 / stop 先收摊再退）与
//! 托盘「停止服务并退出」（无条件收摊）。意图经 [`ExitGate`] 在 `app.exit`
//! **之前**置位，`RunEvent::ExitRequested` 钩子只认标志；Windows 关机/注销
//! 无人置位 → 钩子直接放行、绝不做 30s 收摊（若挂在无条件路径上会阻止关机）。
//!
//! 收摊 = `stop_all`（T9 管线：内部先取消/等待在途启动，再逐组件停止），
//! 外层总超时 30s 硬顶：超时记日志放行退出（AC14）。

use crate::orchestrator::Orchestrator;
use crate::probe::ComponentId;
use crate::settings::{ExitAction, SettingsState};
use crate::stop::StopOutcome;
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

/// 收摊总超时（AC14：超时记日志放行退出）
pub const SHUTDOWN_TOTAL_TIMEOUT: Duration = Duration::from_secs(30);

/// 退出语义（AC13~15 三分支的归一）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitSemantics {
    /// 保留服务直接退出（AC13 默认）
    Keep,
    /// 先收摊（与 AC2 等价的停止）再退出（AC14/15）
    StopServices,
}

/// 决策（纯函数）：设置的退出行为 + 本次是否显式要求停服务 → 语义。
/// 托盘「停止服务并退出」显式传 true，**无视** exitAction（AC14）；
/// 托盘「退出」/前端 `quit(false)` 按 exitAction 分流（AC13 keep / AC15 stop）。
pub fn decide_semantics(exit_action: ExitAction, explicit_stop_services: bool) -> ExitSemantics {
    if explicit_stop_services || exit_action == ExitAction::Stop {
        ExitSemantics::StopServices
    } else {
        ExitSemantics::Keep
    }
}

/// 退出意图门：显式退出前登记、退出钩子只认标志（AC17 的机制核心）。
#[derive(Default)]
pub struct ExitGate {
    shutdown_requested: Mutex<bool>,
    last_request: Mutex<Option<ExitSemantics>>,
}

impl ExitGate {
    pub fn new() -> Self {
        Self::default()
    }

    /// 登记退出意图（必须先于 `app.exit` 调用；仅 StopServices 置收摊标志）
    pub fn request(&self, semantics: ExitSemantics) {
        *self.last_request.lock().expect("退出意图锁中毒") = Some(semantics);
        let armed = semantics == ExitSemantics::StopServices;
        *self.shutdown_requested.lock().expect("退出意图锁中毒") = armed;
    }

    /// 最近一次登记的意图（None = 从未显式退出 → 关机/注销/异常路径）
    pub fn last_request(&self) -> Option<ExitSemantics> {
        *self.last_request.lock().expect("退出意图锁中毒")
    }

    /// 退出钩子取走收摊请求：取后即清（one-shot，重复 ExitRequested 不再收摊）
    pub fn take_shutdown_request(&self) -> bool {
        let mut flag = self.shutdown_requested.lock().expect("退出意图锁中毒");
        let taken = *flag;
        *flag = false;
        taken
    }
}

/// 服务收摊 seam（Orchestrator 实现；单测 mock 调用次数/阻塞行为）
pub trait ServiceStopper: Send + Sync {
    fn stop_all_services(&self) -> Vec<(ComponentId, StopOutcome)>;
}

impl ServiceStopper for Orchestrator {
    fn stop_all_services(&self) -> Vec<(ComponentId, StopOutcome)> {
        self.stop_all()
    }
}

/// 收摊结论（AC14：completed=false 即总超时放行，outcomes 为空）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShutdownReport {
    pub completed: bool,
    pub outcomes: Vec<(ComponentId, StopOutcome)>,
}

/// 执行收摊：`stop_all` 在工作线程执行，总超时硬顶；超时后调用方先行放行
/// （线程可能仍在停止中途，随进程退出被终结——AC14 接受的语义）。
pub fn run_shutdown(stopper: Arc<dyn ServiceStopper>, timeout: Duration) -> ShutdownReport {
    let (tx, rx) = mpsc::channel();
    let spawned = std::thread::Builder::new()
        .name("wb-exit-shutdown".into())
        .spawn(move || {
            // 线程可能超时后仍在运行（停止中途），随进程退出被终结——AC14 语义
            let _ = tx.send(stopper.stop_all_services());
        });
    match spawned {
        Ok(_) => match rx.recv_timeout(timeout) {
            Ok(outcomes) => ShutdownReport { completed: true, outcomes },
            Err(mpsc::RecvTimeoutError::Timeout) => {
                log::error!("收摊总超时（{timeout:?}）：记日志放行退出（AC14）");
                ShutdownReport { completed: false, outcomes: Vec::new() }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                // 收摊线程 panic/通道关闭：与超时同语义（放行，退出不被阻塞）
                log::error!("收摊线程异常结束：按超时语义放行退出（AC14）");
                ShutdownReport { completed: false, outcomes: Vec::new() }
            }
        },
        // 线程创建失败（OOM 级）：退出流不因此卡死，放行并留日志
        Err(e) => {
            log::error!("收摊线程创建失败（{e}）：放行退出（AC14 超时语义）");
            ShutdownReport { completed: false, outcomes: Vec::new() }
        }
    }
}

/// 退出钩子核心（lib.rs 的 RunEvent::ExitRequested 调用）。
/// 只认 [`ExitGate`] 标志：无意图直接放行（AC17），有意图收摊恰一次。
/// 返回是否执行了收摊。
pub fn handle_exit_requested(
    gate: &ExitGate,
    stopper: Arc<dyn ServiceStopper>,
    timeout: Duration,
) -> bool {
    // AC17 机制核心：关机/注销/崩溃链路无人置位 → 零收摊、立即放行。
    // （若在此无条件收摊，30s 停止会阻止 Windows 关机。）
    if !gate.take_shutdown_request() {
        match gate.last_request() {
            Some(ExitSemantics::Keep) => {
                log::info!("显式保留退出（AC13）：不收摊，服务继续运行");
            }
            _ => log::info!("无显式退出意图（关机/注销/异常路径）：直接放行，不收摊（AC17）"),
        }
        return false;
    }
    log::info!("显式收摊退出：停止在途启动与全部服务（总超时 {timeout:?}，AC14）");
    let begun = Instant::now();
    let report = run_shutdown(stopper, timeout);
    if report.completed {
        log::info!("收摊完成（耗时 {:?}）：{:?}", begun.elapsed(), report.outcomes);
    } else {
        log::error!(
            "收摊未完成即放行退出。手动排查：netstat -ano | findstr \":3001 :443 :9876\" \
             定位残留 PID 后 taskkill /F /T /PID <PID>"
        );
    }
    true
}

// ── Tauri 胶水（托盘两菜单与前端 quit 命令共用的显式退出入口）──────────────

/// 显式退出：登记意图 → `app.exit(0)`。顺序硬约束：request 先于 exit，
/// 收摊实际发生在 RunEvent::ExitRequested 钩子（只认标志）。
pub fn request_exit(app: &tauri::AppHandle, explicit_stop_services: bool) {
    use tauri::Manager;
    let action = app.state::<SettingsState>().current().exit_action;
    let semantics = decide_semantics(action, explicit_stop_services);
    log::info!(
        "退出请求（explicit_stop_services={explicit_stop_services}，exitAction={action:?}）→ {semantics:?}"
    );
    app.state::<ExitGate>().request(semantics);
    app.exit(0);
}

/// 前端退出入口（plan §5.1：`quit(stop_services)`）
#[tauri::command]
pub fn quit(app: tauri::AppHandle, stop_services: bool) {
    request_exit(&app, stop_services);
}

// ── 单元测试（宪法 §1：先红后绿）────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// 收摊 mock：记录调用次数，可选阻塞（构造总超时场景）
    struct MockStopper {
        calls: AtomicUsize,
        delay: Option<Duration>,
    }

    impl MockStopper {
        fn immediate() -> Arc<Self> {
            Arc::new(Self { calls: AtomicUsize::new(0), delay: None })
        }

        fn blocking(delay: Duration) -> Arc<Self> {
            Arc::new(Self { calls: AtomicUsize::new(0), delay: Some(delay) })
        }
    }

    impl ServiceStopper for MockStopper {
        fn stop_all_services(&self) -> Vec<(ComponentId, StopOutcome)> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if let Some(d) = self.delay {
                std::thread::sleep(d);
            }
            vec![
                (ComponentId::CloudCli, StopOutcome::Stopped),
                (ComponentId::Caddy, StopOutcome::Stopped),
                (ComponentId::DdnsGo, StopOutcome::AlreadyStopped),
            ]
        }
    }

    #[test]
    fn shutdown_total_timeout_is_30s() {
        // AC14：收摊总超时 30s
        assert_eq!(SHUTDOWN_TOTAL_TIMEOUT, Duration::from_secs(30));
    }

    #[test]
    fn decide_semantics_three_branches() {
        // AC13：默认 keep → 保留服务直接退出
        assert_eq!(decide_semantics(ExitAction::Keep, false), ExitSemantics::Keep);
        // AC15：exitAction=stop → 普通退出也收摊
        assert_eq!(
            decide_semantics(ExitAction::Stop, false),
            ExitSemantics::StopServices
        );
        // AC14：显式「停止服务并退出」无视 keep
        assert_eq!(
            decide_semantics(ExitAction::Keep, true),
            ExitSemantics::StopServices
        );
        assert_eq!(
            decide_semantics(ExitAction::Stop, true),
            ExitSemantics::StopServices
        );
    }

    #[test]
    fn gate_is_one_shot_and_keep_does_not_arm() {
        let gate = ExitGate::new();
        assert!(!gate.take_shutdown_request(), "无意图不得触发收摊（AC17）");
        assert_eq!(gate.last_request(), None);

        gate.request(ExitSemantics::Keep);
        assert_eq!(gate.last_request(), Some(ExitSemantics::Keep));
        assert!(!gate.take_shutdown_request(), "Keep 退出不置收摊标志（AC13）");

        gate.request(ExitSemantics::StopServices);
        assert!(gate.take_shutdown_request(), "StopServices 须触发收摊");
        assert!(!gate.take_shutdown_request(), "取后即清：重复 ExitRequested 不再收摊");
    }

    #[test]
    fn exit_requested_without_intent_skips_shutdown() {
        // AC17：关机/注销路径无人置位 → ExitRequested 直接放行、零收摊调用
        let gate = ExitGate::new();
        let stopper = MockStopper::immediate();
        let began = Instant::now();
        let handled = handle_exit_requested(&gate, stopper.clone(), SHUTDOWN_TOTAL_TIMEOUT);
        assert!(!handled, "无意图不得执行收摊");
        assert_eq!(stopper.calls.load(Ordering::SeqCst), 0, "绝不能调用 stop_all");
        assert!(began.elapsed() < Duration::from_secs(1), "放行必须立即返回");
    }

    #[test]
    fn exit_requested_with_intent_stops_exactly_once() {
        // AC14/15：意图 → 收摊恰一次；重复钩子触发不再收摊（one-shot 防重入）
        let gate = ExitGate::new();
        gate.request(ExitSemantics::StopServices);
        let stopper = MockStopper::immediate();
        assert!(handle_exit_requested(&gate, stopper.clone(), SHUTDOWN_TOTAL_TIMEOUT));
        assert_eq!(stopper.calls.load(Ordering::SeqCst), 1, "stop_all 应恰好调用一次");
        // 收摊完成后再次触发的 ExitRequested（如窗口链）不再收摊
        assert!(!handle_exit_requested(&gate, stopper.clone(), SHUTDOWN_TOTAL_TIMEOUT));
        assert_eq!(stopper.calls.load(Ordering::SeqCst), 1, "重复触发不得二次收摊");
    }

    #[test]
    fn shutdown_total_timeout_releases_exit() {
        // AC14：收摊超预算（mock 阻塞 800ms）→ 总超时（150ms）放行，不等线程完成
        let stopper = MockStopper::blocking(Duration::from_millis(800));
        let began = Instant::now();
        let report = run_shutdown(stopper.clone(), Duration::from_millis(150));
        assert!(!report.completed, "总超时应放行（completed=false）");
        assert!(report.outcomes.is_empty(), "超时放行时无组件结论");
        assert!(
            began.elapsed() < Duration::from_millis(600),
            "放行不得等待收摊线程完成（实际 {:?}）",
            began.elapsed()
        );
        assert_eq!(stopper.calls.load(Ordering::SeqCst), 1, "stop_all 已被调用（线程仍阻塞）");
        // 等待收摊线程退出，避免测试进程残留阻塞线程占用 mock
        std::thread::sleep(Duration::from_millis(900));
    }

    #[test]
    fn shutdown_normal_path_collects_outcomes() {
        let stopper = MockStopper::immediate();
        let report = run_shutdown(stopper.clone(), SHUTDOWN_TOTAL_TIMEOUT);
        assert!(report.completed, "正常路径应完成");
        assert_eq!(report.outcomes.len(), 3, "三组件各一结论");
        assert_eq!(stopper.calls.load(Ordering::SeqCst), 1);
    }
}
