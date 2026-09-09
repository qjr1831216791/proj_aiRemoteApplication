/// 桌面控制台入口（Tauri 2）。
/// T1：最小骨架（日志插件 + 默认窗口）；单实例/托盘在 T2，业务命令在 T5~T15 逐步接入。
pub fn run() {
    tauri::Builder::default()
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
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
