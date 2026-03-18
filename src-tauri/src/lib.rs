mod commands;
mod task_engine;
mod context;
mod ai;
mod overlay;
mod storage;

use std::sync::Mutex;
use task_engine::machine::TaskMachine;
use ai::bridge::AiBridge;

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let tasks_dir = std::env::current_dir()
        .unwrap_or_default()
        .join("../tasks");

    let api_key = std::env::var("OPENAI_API_KEY").unwrap_or_default();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(Mutex::new(TaskMachine::new(tasks_dir)))
        .manage(Mutex::new(AiBridge::new(api_key)))
        .invoke_handler(tauri::generate_handler![
            greet,
            commands::task::start_task,
            commands::task::advance_step,
            commands::task::get_task_state,
            commands::context::get_active_window,
            commands::overlay::show_overlay,
            commands::overlay::hide_overlay,
            commands::ai::request_help,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
