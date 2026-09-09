//! 桌面控制台入口（Tauri 2）。
//!
//! - T1：日志插件 + 默认窗口（visible:false，前端就绪后显形）
//! - T2：单实例（会话内插件激活 + Global 互斥体跨会话唯一）、托盘骨架、
//!   关 X 最小化到托盘（AC18）、RunEvent::ExitRequested 退出钩子骨架
//! - T5~T15：业务命令（设置/探测/编排/自启/界面）逐步接入
//!   - T10：退出流装配——编排器接入运行时 + ExitGate 意图 + 退出钩子收摊

mod autostart;
mod commands;
mod consts;
mod exit_flow;
mod lang;
mod orchestrator;
mod probe;
mod scripts;
mod settings;
mod single_instance;
mod startup;
mod stop;
mod tray;
mod urls;

use tauri::{Emitter, Manager};

pub fn run() {
    // ── 单实例（spec §4.5）───────────────────────────────────────────────
    // 先于任何插件/窗口判定：跨会话唯一靠 Global\ 互斥体（见 single_instance 模块
    // 注释），会话内重复启动的「激活 + 退出」由 tauri-plugin-single-instance 承担。
    let instance_kind = match single_instance::probe() {
        single_instance::GlobalPrimary::OtherSession => {
            // 跨会话重复：无法激活另一会话的窗口 → 明确提示后退出（多会话原则）
            let l = lang::detect_system_lang();
            single_instance::show_other_session_hint(l);
            return;
        }
        single_instance::GlobalPrimary::Acquired(guard) => {
            // 互斥体持有至进程退出：mem::forget 防 guard 被 Drop 提前释放；
            // 进程终止时内核回收句柄，异常死亡也不会残留"僵尸互斥体"
            std::mem::forget(guard);
            InstanceKind::Primary
        }
        single_instance::GlobalPrimary::SameSession => InstanceKind::SameSessionDuplicate,
        single_instance::GlobalPrimary::Unavailable => InstanceKind::Degraded,
    };

    tauri::Builder::default()
        // 官方要求：single-instance 插件必须最先注册
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            // 同会话重复启动 → 激活已有实例主窗（spec §4.5；此时主窗可能隐藏于托盘）
            log::info!("同会话重复启动：激活已有主窗口（spec §4.5）");
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.show();
                let _ = win.unminimize();
                let _ = win.set_focus();
            }
        }))
        .plugin(
            tauri_plugin_log::Builder::new()
                .targets(if cfg!(debug_assertions) {
                    vec![
                        tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Stdout),
                        tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Webview),
                    ]
                } else {
                    // 发布态写文件：路径由插件默认（app log dir），T7 接入设置页"打开日志目录"
                    vec![tauri_plugin_log::Target::new(
                        tauri_plugin_log::TargetKind::LogDir { file_name: None },
                    )]
                })
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            settings::get_settings,
            settings::save_settings,
            lang::set_language,
            exit_flow::quit,
            autostart::set_autostart_services,
            autostart::set_autostart_app,
            startup::is_hidden_startup,
            // T13/T15 UI 命令层（plan §5.1 剩余项）
            commands::get_status,
            commands::get_urls,
            commands::start_all,
            commands::stop_all,
            commands::start_one,
            commands::stop_one,
            commands::open_external,
            commands::run_tool,
            commands::scripts_availability,
            commands::open_logs_dir,
        ])
        .setup(move |app| {
            // 防御：同会话重复实例本应已被插件在其 setup（早于本回调）拦截退出；
            // 走到这里仍是重复身份，说明主实例的激活窗口查找失败等异常路径——
            // 直接退出本实例，避免同会话双实例双托盘。
            if instance_kind == InstanceKind::SameSessionDuplicate {
                log::error!("同会话重复实例未被插件拦截（主实例窗口查找失败），本实例退出");
                std::process::exit(0);
            }

            // ── 设置加载（T5，AC23/24）───────────────────────────────────
            // 缺失 → 默认值；损坏 → 核心 load 已改名 .bad-<ts> 留档并回默认，
            // 此处补发 settings://repaired 通知前端（T14 监听；若前端未就绪，
            // 其首次 get_settings 读到的默认值即恢复结果，日志亦有留痕）。
            let (settings_state, repaired_backup) =
                settings::SettingsState::load_at(settings::settings_path());
            if let Some(backup) = &repaired_backup {
                log::warn!(
                    "设置文件损坏：已留档为 {} 并回退默认值（AC24）",
                    backup.display()
                );
            }
            app.manage(settings_state);
            if let Some(backup) = repaired_backup {
                let _ = app.emit(
                    settings::EVENT_REPAIRED,
                    serde_json::json!({ "backupPath": backup.to_string_lossy() }),
                );
            }

            // ── 启动参数解析（T12：--hidden 静默入托盘，AC10）────────────────
            let argv: Vec<String> = std::env::args().collect();
            let startup_plan = startup::plan_from(
                &argv,
                app.state::<settings::SettingsState>()
                    .current()
                    .link_start_services,
            );
            // 前端 show 门控数据源（就绪后经 is_hidden_startup 查询）
            app.manage(startup::StartupState::from_plan(&startup_plan));

            // ── 生效语言（AC25：显式选择优先于系统显示语言）──────────────────
            let (language_setting, scripts_override) = {
                let s = app.state::<settings::SettingsState>().current();
                (s.language, s.scripts_dir_override)
            };
            let effective_lang = lang::resolve_setting(language_setting);
            // 共享语言态：set_language 为唯一写者；托盘/脚本派发/状态 detail 实时读
            app.manage(lang::LanguageState::new(effective_lang));

            // ── 编排器装配（T8/T9 实现首次接入运行时；T10 收摊/T11/T12 消费）──
            let log_dir = app.path().app_log_dir().unwrap_or_else(|e| {
                log::warn!("应用日志目录不可用（{e}）：组件日志退回临时目录");
                std::env::temp_dir()
            });
            let exe_dir = std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|d| d.to_path_buf()));
            let (scripts_dir, scripts_disabled_reason) =
                match scripts::locate(scripts_override.as_deref(), exe_dir.as_deref()) {
                    scripts::ScriptsResolution::Found { dir, source } => {
                        log::info!(
                            "sprint0 脚本目录：{}（来源 {}）",
                            dir.display(),
                            source.as_str()
                        );
                        (Some(dir), None)
                    }
                    scripts::ScriptsResolution::Disabled { reason } => {
                        // spec §4.5：脚本缺失 → 相关功能禁用并给原因，不崩溃
                        log::warn!("{reason}");
                        (None, Some(reason))
                    }
                };
            let mut orch_cfg =
                orchestrator::OrchestratorConfig::new(effective_lang, log_dir.clone());
            orch_cfg.scripts_dir = scripts_dir.clone();
            let lang_handle = app.handle().clone();
            let orch = build_orchestrator(app.handle().clone(), orch_cfg)
                // 语言切换后脚本 -Lang 与状态 detail 即时跟随（AC25）
                .with_lang_source(std::sync::Arc::new(move || {
                    lang_handle.state::<lang::LanguageState>().current()
                }));
            app.manage(orch.clone());
            // 前台轮询器（AC4 ≤5s；plan §8 前台 2s）：句柄随 setup 结束丢弃——
            // PollerHandle 无 Drop 停止语义，轮询线程随进程退出而止
            let _poller = orch.spawn_poller();

            // ── 退出流（T10）：意图门注册（托盘/quit 在 app.exit 前置位）─────
            app.manage(exit_flow::ExitGate::new());

            // ── 自启上下文（T11/T15）：脚本目录 + 禁用原因 + 日志目录 ────────
            app.manage(autostart::AutostartContext {
                scripts_dir,
                scripts_disabled_reason,
                log_dir,
            });

            tray::setup(app, effective_lang)?;
            log::info!("托盘就绪（语言：{effective_lang:?}）");

            // ── 登录联动（T12：AC11/12）——置末尾：全部子系统就绪后再拉服务 ──
            startup::apply_link_start(&orch, &startup_plan);
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    // AC18：关 X = 最小化到托盘（prevent_close 后仅隐藏，
                    // 进程与托盘存活）；真正退出走托盘菜单
                    api.prevent_close();
                    if let Err(e) = window.hide() {
                        log::error!("隐藏主窗口失败：{e}");
                    }
                    log::info!("主窗口关闭请求 → 最小化到托盘（AC18）");
                }
            }
        })
        .build(tauri::generate_context!())
        .expect("error while running tauri application")
        .run(|app, event| {
            if let tauri::RunEvent::ExitRequested { code, .. } = event {
                // T10 退出钩子：只认 ExitGate 标志（AC17）——显式收摊退出在此
                // 执行总超时 30s 的 stop_all；关机/注销/无意图路径零收摊直接放行。
                // ADR-0002：三组件独立于程序存活、不挂 kill-on-close Job，
                // 退出绝不无条件携带服务进程。
                log::info!("ExitRequested(code={code:?})");
                let stopper: std::sync::Arc<dyn exit_flow::ServiceStopper> = {
                    let orch = app.state::<orchestrator::Orchestrator>();
                    std::sync::Arc::new(orch.inner().clone())
                };
                let gate = app.state::<exit_flow::ExitGate>();
                exit_flow::handle_exit_requested(&gate, stopper, exit_flow::SHUTDOWN_TOTAL_TIMEOUT);
            }
        });
}

/// 真实编排器装配（平台采集层注入；Windows-only，ADR-0001）
fn build_orchestrator(
    app: tauri::AppHandle,
    cfg: orchestrator::OrchestratorConfig,
) -> orchestrator::Orchestrator {
    #[cfg(windows)]
    {
        orchestrator::Orchestrator::new(
            cfg,
            std::sync::Arc::new(probe::WindowsProbe),
            std::sync::Arc::new(scripts::ProcessExecutor),
            std::sync::Arc::new(stop::SysinfoProcessOps),
            std::sync::Arc::new(orchestrator::TauriStatusEmitter::new(app)),
            std::sync::Arc::new(orchestrator::FsLogTailReader),
        )
    }
    #[cfg(not(windows))]
    {
        let _ = (app, cfg);
        unreachable!("本项目仅面向 Windows（ADR-0001）")
    }
}

/// 本进程的单实例身份（setup 闭包捕获用）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InstanceKind {
    /// 全机首个实例
    Primary,
    /// 同会话重复（正常路径下到不了应用 setup）
    SameSessionDuplicate,
    /// Global 互斥体不可用，仅会话内单实例
    Degraded,
}
