mod commands;
mod task_engine;
mod context;
mod ai;
mod overlay;
mod storage;
mod capture;

use std::sync::Mutex;
use task_engine::machine::TaskMachine;
use ai::bridge::AiBridge;

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Load .env from the project root (one level up from src-tauri)
    let project_root = std::env::current_dir()
        .unwrap_or_default()
        .join("..");

    let env_path = project_root.join(".env");
    if env_path.exists() {
        dotenvy::from_path(&env_path).ok();
        println!("[z-ro] Loaded .env from {:?}", env_path);
    } else {
        println!("[z-ro] No .env found at {:?}", env_path);
    }

    let tasks_dir = project_root.join("tasks");
    println!("[z-ro] Loading tasks from {:?}", tasks_dir);

    let api_key = std::env::var("OPENAI_API_KEY").unwrap_or_default();
    println!("[z-ro] OpenAI API key: {}", if api_key.is_empty() { "NOT SET" } else { "configured" });

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(Mutex::new(TaskMachine::new(tasks_dir)))
        .manage(Mutex::new(AiBridge::new(api_key)))
        .setup(|app| {
            // Create the overlay window on startup (hidden by default)
            let handle = app.handle().clone();
            if let Err(e) = overlay::create_overlay_window(&handle) {
                eprintln!("[z-ro] Failed to create overlay window: {}", e);
            } else {
                println!("[z-ro] Overlay window created (hidden)");
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            commands::task::start_task,
            commands::task::advance_step,
            commands::task::get_task_state,
            commands::task::list_tasks,
            commands::context::get_active_window,
            commands::overlay::show_overlay,
            commands::overlay::hide_overlay,
            commands::overlay::update_overlay,
            commands::ai::request_help,
            commands::ai::analyze_screenshot,
            commands::screenshot::capture_screenshot,
            commands::screenshot::capture_screenshot_region,
            commands::env::get_gemini_api_key,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
