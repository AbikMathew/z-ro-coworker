mod commands;
mod task_engine;
mod context;
mod ai;
mod overlay;
mod storage;
mod capture;
mod npc;

use std::sync::{Arc, Mutex};
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

    // Shared TaskMachine: used by both task commands and NPC coordinator
    let task_machine = Arc::new(Mutex::new(TaskMachine::new(tasks_dir)));

    // ── NPC Coordinator setup ──────────────────────────────────────
    let npc_model = std::env::var("NPC_MODEL")
        .unwrap_or_else(|_| "gpt-4o-mini".to_string());
    let npc_api_key = api_key.clone(); // reuse OpenAI key for NPC LLM

    let llm_provider = Box::new(
        npc::llm_providers::openai::OpenAiLlm::new(npc_api_key, npc_model.clone()),
    );
    let pipeline = npc::voice::pipeline::VoicePipeline::new(None, None, llm_provider);
    let npc_coordinator = npc::NpcCoordinator::new(pipeline, task_machine.clone());

    println!(
        "[z-ro:npc] NPC coordinator initialized (model: {}, state: idle)",
        npc_model
    );

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(task_machine)
        .manage(Mutex::new(AiBridge::new(api_key)))
        .manage(Arc::new(npc_coordinator))
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
            commands::npc::npc_activate,
            commands::npc::npc_deactivate,
            commands::npc::npc_start_listening,
            commands::npc::npc_stop_listening,
            commands::npc::npc_ask_text,
            commands::npc::npc_get_status,
            commands::npc::npc_interrupt,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
