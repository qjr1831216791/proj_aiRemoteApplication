//! 启动参数与登录联动（T12：AC10/11/12，spec §4.2 / 决策 7）。
//!
//! - `--hidden`（登录计划任务拉起，T11 注册）：静默入托盘——主窗不弹、不开
//!   浏览器（AC10）。Rust 侧持 hidden 标志，前端页面就绪后经 `is_hidden_startup`
//!   查询决定是否 show（T1 的"就绪后显形"由此受 Rust 门控）。
//! - 登录联动（AC11）：`linkStartServices` 开 → `start_all()` 补齐未运行服务
//!   （幂等，已在运行组件由守卫跳过；不自动开浏览器，openPageOnStart 与此
//!   独立且默认 false）；关 → 仅探测展示（AC12）。**手动双击（无 --hidden）
//!   同样适用联动**（决策 7）。

use crate::orchestrator::Orchestrator;

/// 静默启动参数（与 T11 自启任务 Action 参数契约一致）
pub const HIDDEN_ARG: &str = "--hidden";

/// 启动参数解析结果
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StartupFlags {
    pub hidden: bool,
}

/// 启动计划（参数 × 联动设置的组合决策）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StartupPlan {
    /// --hidden：静默入托盘（AC10/§4.2）
    pub hidden: bool,
    /// 是否联动补齐服务（AC11/12；与 hidden 正交——手动双击同样联动）
    pub link_start: bool,
}

/// 纯解析：全参数中精确匹配 `--hidden`（大小写敏感；argv[0] 是路径不会误中）
pub fn parse_args(args: &[String]) -> StartupFlags {
    StartupFlags {
        hidden: args.iter().any(|a| a == HIDDEN_ARG),
    }
}

/// 启动计划组合：args × linkStartServices（AC11 开/AC12 关 两分支的数据源）
pub fn plan_from(args: &[String], link_start_services: bool) -> StartupPlan {
    StartupPlan {
        hidden: parse_args(args).hidden,
        link_start: link_start_services,
    }
}

/// 主窗是否允许显示（前端就绪后经命令查询；--hidden 保持隐藏）
pub fn should_show_main_window(plan: &StartupPlan) -> bool {
    !plan.hidden
}

/// 执行联动（装配层在 setup 末尾调用一次）：
/// link_start=true → start_all（AC1/AC3 等价补齐，幂等）；false → 不拉起。
/// 不在此处开浏览器——openPageOnStart 与联动独立且默认 false（T15 前端接入）。
pub fn apply_link_start(orch: &Orchestrator, plan: &StartupPlan) {
    if plan.link_start {
        log::info!("启动联动开启：补齐未运行的服务（AC11/AC12，决策 7）");
        orch.start_all();
    } else {
        log::info!("启动联动关闭：仅探测展示，不拉起服务（AC12）");
    }
}

// ── Tauri 胶水 ─────────────────────────────────────────────────────────────

/// 启动形态（lib.rs setup 注入；前端 show 前查询）
pub struct StartupState {
    pub hidden: bool,
}

impl StartupState {
    /// 由启动计划构造：hidden = not should_show_main_window（单一语义来源，
    /// 使 should_show_main_window 成为生产路径而非仅测试断言的纯函数）
    pub fn from_plan(plan: &StartupPlan) -> Self {
        Self { hidden: !should_show_main_window(plan) }
    }
}

/// 前端查询：本次启动是否 --hidden（true 则不 show 主窗，AC10）
#[tauri::command]
pub fn is_hidden_startup(state: tauri::State<'_, StartupState>) -> bool {
    state.hidden
}

// ── 单元测试（宪法 §1：先红后绿）────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::Lang;
    use crate::orchestrator::test_support::*;
    use crate::probe::ProbeState;
    use crate::stop::StopConfig;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::Duration;

    fn args(list: &[&str]) -> Vec<String> {
        let mut v: Vec<String> = vec!["workbench.exe".into()];
        v.extend(list.iter().map(|s| (*s).to_string()));
        v
    }

    #[test]
    fn hidden_flag_parsed_from_args() {
        // 精确匹配、大小写敏感；其他参数不误中
        assert!(parse_args(&args(&["--hidden"])).hidden);
        assert!(!parse_args(&args(&[])).hidden);
        assert!(parse_args(&args(&["--foo", "--hidden"])).hidden);
        assert!(!parse_args(&args(&["--Hidden"])).hidden, "大小写敏感");
        assert!(!parse_args(&args(&["--hidden=1"])).hidden, "仅精确匹配");
    }

    #[test]
    fn plan_combines_args_and_link_setting() {
        // AC11：自启（--hidden）+ 联动开 → 隐藏启动并补齐服务
        assert_eq!(
            plan_from(&args(&["--hidden"]), true),
            StartupPlan { hidden: true, link_start: true }
        );
        // AC11 后半：联动关 → 仅启动程序不拉服务
        assert_eq!(
            plan_from(&args(&["--hidden"]), false),
            StartupPlan { hidden: true, link_start: false }
        );
        // AC12（决策 7）：手动双击（无 --hidden）+ 联动开 → 同样补齐
        assert_eq!(
            plan_from(&args(&[]), true),
            StartupPlan { hidden: false, link_start: true }
        );
    }

    #[test]
    fn hidden_startup_suppresses_main_window() {
        // AC10：--hidden 时主窗不弹（show 门控在 Rust 侧，前端只消费查询结果）
        assert!(!should_show_main_window(&plan_from(&args(&["--hidden"]), true)));
        assert!(should_show_main_window(&plan_from(&args(&[]), false)));
        // 前端查询语义：StartupState 由计划构造，如实携带 hidden 标志
        assert!(StartupState::from_plan(&plan_from(&args(&["--hidden"]), true)).hidden);
        assert!(!StartupState::from_plan(&plan_from(&args(&[]), false)).hidden);
    }

    /// mock 编排器（联动分支断言 start_all 是否被触发）
    fn mock_orch() -> (Orchestrator, Arc<MockExecutor>) {
        let probe = Arc::new(ScriptedProbe::new());
        for id in crate::orchestrator::COMPONENT_ORDER {
            // 守卫过（Stopped）→ 拉起 → 就绪（Running）：证明 start_all 真实派发
            probe.enqueue(id, &[ProbeState::Stopped, running()]);
        }
        let exec = Arc::new(MockExecutor::new());
        let mut cfg = crate::orchestrator::OrchestratorConfig::new(Lang::Zh, PathBuf::from(r"D:\t"));
        cfg.poll_interval = Duration::from_millis(5);
        cfg.start_timeout = Duration::from_millis(300);
        cfg.stop = StopConfig {
            caddy_stop_timeout: Duration::from_millis(200),
            component_timeout: Duration::from_secs(2),
            port_grace: Duration::from_millis(0),
        };
        cfg.scripts_dir = Some(PathBuf::from(r"D:\scripts"));
        let orch = Orchestrator::new(
            cfg,
            probe,
            exec.clone(),
            Arc::new(MockProcessOps::new()),
            Arc::new(MockEventSink::default()),
            Arc::new(MockLogTail::default()),
        );
        (orch, exec)
    }

    #[test]
    fn link_start_enabled_starts_all_components() {
        // AC11/12：联动开 → start_all 恰一次（CloudCLI 脚本 ×1 + 原生派发 ×1；
        // ddns-go 已随直连通道退役——spec 008）
        let (orch, exec) = mock_orch();
        apply_link_start(&orch, &StartupPlan { hidden: true, link_start: true });
        for id in crate::orchestrator::COMPONENT_ORDER {
            assert!(
                orch.wait_start_idle(id, Duration::from_secs(2)),
                "{id:?} 应被派发启动"
            );
        }
        assert_eq!(exec.executed.lock().unwrap().len(), 1, "CloudCLI 脚本恰一次");
        assert_eq!(exec.dispatched.lock().unwrap().len(), 1, "Caddy 原生派发");
    }

    #[test]
    fn link_start_disabled_starts_nothing() {
        // AC11 后半/AC12 关：联动关 → 仅探测展示，零拉起
        let (orch, exec) = mock_orch();
        apply_link_start(&orch, &StartupPlan { hidden: false, link_start: false });
        assert!(
            exec.executed.lock().unwrap().is_empty(),
            "不得执行启动脚本"
        );
        assert!(
            exec.dispatched.lock().unwrap().is_empty(),
            "不得派发原生命令"
        );
        for id in crate::orchestrator::COMPONENT_ORDER {
            assert!(!orch.cancel_start(id), "{id:?} 不应有在途启动");
        }
    }
}
