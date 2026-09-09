//! 托盘骨架（spec §4.2：托盘常驻是程序固有形态）。
//!
//! 六项菜单：启动/停止/打开工作台/显示主界面 为占位 handler（编排接线随
//! T13/T15 前端接入），「停止服务并退出」「退出」已接真语义（T10 退出流：
//! 经 `exit_flow::request_exit` 登记 ExitGate 意图后 `app.exit`，收摊在
//! RunEvent::ExitRequested 钩子执行——绝不挂在无条件退出路径，AC17）。
//!
//! explorer 重启/崩溃后托盘图标的重挂由底层 tray-icon crate 的
//! TaskbarCreated 广播机制承担（plan §7 风险行），无需手工处理。

use crate::lang::{self, Lang};
use tauri::App;

/// 创建托盘图标与菜单。语言由启动时探测（AC25 骨架；
/// T14 接入设置后支持切换语言并重建菜单）。
pub fn setup(app: &App, lang: Lang) -> tauri::Result<()> {
    use tauri::menu::{Menu, MenuItem};
    use tauri::tray::TrayIconBuilder;

    let t = lang::tray_texts(lang);
    let item_start = MenuItem::with_id(app, "start", t.start, true, None::<&str>)?;
    let item_stop = MenuItem::with_id(app, "stop", t.stop, true, None::<&str>)?;
    let item_open_workbench =
        MenuItem::with_id(app, "open_workbench", t.open_workbench, true, None::<&str>)?;
    let item_show_main = MenuItem::with_id(app, "show_main", t.show_main, true, None::<&str>)?;
    let item_stop_and_exit =
        MenuItem::with_id(app, "stop_and_exit", t.stop_and_exit, true, None::<&str>)?;
    let item_quit = MenuItem::with_id(app, "quit", t.quit, true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &item_start,
            &item_stop,
            &item_open_workbench,
            &item_show_main,
            &item_stop_and_exit,
            &item_quit,
        ],
    )?;

    let _tray = TrayIconBuilder::with_id("workbench")
        .icon(
            app.default_window_icon()
                .expect("tauri.conf.json 已配置图标，此处不应为空")
                .clone(),
        )
        .tooltip(t.tooltip)
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "start" => log::info!("托盘「启动」触发（占位 handler，编排 T8 接入）"),
            "stop" => log::info!("托盘「停止」触发（占位 handler，停止管线 T9 接入）"),
            "open_workbench" => log::info!("托盘「打开工作台」触发（占位 handler，open_external T15 接入）"),
            "show_main" => log::info!("托盘「显示主界面」触发（占位 handler，交互随 T13/T14 完善）"),
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
        })
        .build(app)?;

    Ok(())
}
