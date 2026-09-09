//! 桌面控制台入口（Tauri 2）。
//!
//! - T1：日志插件 + 默认窗口（visible:false，前端就绪后显形）
//! - T2：单实例（会话内插件激活 + Global 互斥体跨会话唯一）、托盘骨架、
//!   关 X 最小化到托盘（AC18）、RunEvent::ExitRequested 退出钩子骨架
//! - T5~T15：业务命令（设置/探测/编排/自启/界面）逐步接入

mod consts;
mod lang;
mod probe;
mod scripts;
mod settings;
mod single_instance;
mod tray;

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

            let l = lang::detect_system_lang();
            tray::setup(app, l)?;
            log::info!("托盘骨架就绪（语言：{l:?}）");
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
        .run(|_app, event| {
            if let tauri::RunEvent::ExitRequested { code, .. } = event {
                // 退出钩子骨架：默认放行；T10 按 exitAction 分流
                // （keep 直退 / stop 先收摊总超时 30s）。
                // ADR-0002：三组件独立于程序存活、不挂 kill-on-close Job，
                // 本钩子不做也不需要任何"连带停止"——退出绝不携带服务进程。
                log::info!("ExitRequested(code={code:?})：放行退出（exitAction 分流 T10 接入）");
            }
        });
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
