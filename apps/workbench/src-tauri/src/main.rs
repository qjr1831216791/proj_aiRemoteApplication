// Prevents additional console window on Windows in release; `tauri dev` keeps it for debugging
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    ai_remote_workbench_lib::run()
}
