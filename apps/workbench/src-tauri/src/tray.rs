//! 托盘（spec §4.2：托盘常驻是程序固有形态；T14 起支持语言切换重建）。
//!
//! 六项菜单：启动/停止（编排器实时接线 T13）、打开工作台/显示主界面（T15/T13
//! 接线）、「停止服务并退出」「退出」走 T10 退出流——经
//! `exit_flow::request_exit` 登记 ExitGate 意图后 `app.exit`，收摊在
//! RunEvent::ExitRequested 钩子执行（绝不挂在无条件退出路径，AC17）。
//!
//! 语言切换（AC25 立即生效）：`lang::set_language` 调 [`rebuild`] 重建菜单与
//! tooltip，无需重启程序。文案与前端 `src/i18n/{zh,en}.ts` 词条同源同步。
//!
//! explorer 重启/崩溃后托盘图标的重挂由底层 tray-icon crate 的
//! TaskbarCreated 广播机制承担（plan §7 风险行），无需手工处理。

use crate::lang::{self, Lang};
use tauri::{App, AppHandle, Manager, Runtime};

/// 托盘图标 id（rebuild 定位用，与 setup 保持一致）
const TRAY_ID: &str = "workbench";

/// 构建托盘菜单（setup 与 rebuild 共用；词条按生效语言取）
fn build_menu<R: Runtime>(manager: &impl Manager<R>, lang: Lang) -> tauri::Result<tauri::menu::Menu<R>> {
    use tauri::menu::{Menu, MenuItem};

    let t = lang::tray_texts(lang);
    let item_start = MenuItem::with_id(manager, "start", t.start, true, None::<&str>)?;
    let item_stop = MenuItem::with_id(manager, "stop", t.stop, true, None::<&str>)?;
    let item_open_workbench =
        MenuItem::with_id(manager, "open_workbench", t.open_workbench, true, None::<&str>)?;
    let item_show_main = MenuItem::with_id(manager, "show_main", t.show_main, true, None::<&str>)?;
    let item_stop_and_exit =
        MenuItem::with_id(manager, "stop_and_exit", t.stop_and_exit, true, None::<&str>)?;
    let item_quit = MenuItem::with_id(manager, "quit", t.quit, true, None::<&str>)?;
    Menu::with_items(
        manager,
        &[
            &item_start,
            &item_stop,
            &item_open_workbench,
            &item_show_main,
            &item_stop_and_exit,
            &item_quit,
        ],
    )
}

/// 创建托盘图标与菜单（语言由启动时生效语言决定，AC25）
pub fn setup(app: &App, lang: Lang) -> tauri::Result<()> {
    use tauri::tray::TrayIconBuilder;

    let menu = build_menu(app, lang)?;
    let t = lang::tray_texts(lang);
    let _tray = TrayIconBuilder::with_id(TRAY_ID)
        .icon(
            app.default_window_icon()
                .expect("tauri.conf.json 已配置图标，此处不应为空")
                .clone(),
        )
        .tooltip(t.tooltip)
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| handle_menu_event(app, event))
        .build(app)?;

    Ok(())
}

/// 语言切换后重建托盘菜单与 tooltip（AC25 立即生效，无需重启）。
/// 托盘缺失（异常态）只记日志不报错：设置已落盘，语言切换语义仍成立。
pub fn rebuild(app: &AppHandle, lang: Lang) -> tauri::Result<()> {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        log::error!("托盘图标 {TRAY_ID} 不存在，无法重建菜单（语言：{lang:?}）");
        return Ok(());
    };
    let menu = build_menu(app, lang)?;
    tray.set_menu(Some(menu))?;
    tray.set_tooltip(Some(lang::tray_texts(lang).tooltip))?;
    Ok(())
}

/// 菜单事件分发（setup 的 on_menu_event 闭包；全部接线实义，无占位）
fn handle_menu_event(app: &AppHandle, event: tauri::menu::MenuEvent) {
    match event.id.as_ref() {
        "start" => {
            log::info!("托盘「启动」：start_all（AC1/AC3 守卫幂等）");
            app.state::<crate::orchestrator::Orchestrator>().start_all();
        }
        "stop" => {
            // 停止管线单组件 10s 预算：放后台线程执行，不阻塞菜单事件循环
            log::info!("托盘「停止」：stop_all（AC2，含会话结束提示由前端/文案承担）");
            let orch = app.state::<crate::orchestrator::Orchestrator>().inner().clone();
            std::thread::Builder::new()
                .name("wb-tray-stop".into())
                .spawn(move || log_stop_result(orch.stop_all()))
                .ok();
        }
        "open_workbench" => {
            log::info!("托盘「打开工作台」：默认浏览器打开域名入口");
            if let Err(code) = crate::scripts::open_url(crate::consts::WORKBENCH_URL) {
                log::error!("打开工作台页面失败（ShellExecuteW {code}）");
            }
        }
        "show_main" => {
            log::info!("托盘「显示主界面」");
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.show();
                let _ = win.unminimize();
                let _ = win.set_focus();
            }
        }
        "stop_and_exit" => {
            // AC14：显式收摊（总超时 30s 放行）再退出。意图登记在
            // request_exit（先置标志后 app.exit），收摊实际执行于
            // RunEvent::ExitRequested 钩子（见 lib.rs / exit_flow.rs）
            log::info!("托盘「停止服务并退出」：显式收摊（AC14）");
            crate::exit_flow::request_exit(app, true);
        }
        "quit" => {
            // AC13/15：按 exitAction 分流——keep 直退（服务保留）/
            // stop 先收摊再退。ADR-0002：默认不携带服务进程
            log::info!("托盘「退出」：按 exitAction 分流（AC13/AC15）");
            crate::exit_flow::request_exit(app, false);
        }
        unknown => log::warn!("托盘菜单未知项：{unknown}"),
    }
}

/// 停止结论日志化（AC2：失败/超时组件记录原因）
fn log_stop_result(outcomes: Vec<(crate::probe::ComponentId, crate::stop::StopOutcome)>) {
    for (id, outcome) in outcomes {
        match outcome {
            crate::stop::StopOutcome::Stopped | crate::stop::StopOutcome::AlreadyStopped => {
                log::info!("托盘停止：组件 {} 完成（{outcome:?}）", id.as_str());
            }
            crate::stop::StopOutcome::Failed(detail) | crate::stop::StopOutcome::TimedOut(detail) => {
                log::error!("托盘停止：组件 {} 未完全停止：{detail}", id.as_str());
            }
        }
    }
}
