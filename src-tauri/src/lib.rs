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

    let api_keys = Arc::new(npc::llm_providers::ApiKeys::from_env());
    println!(
        "[z-ro] OpenAI API key: {}",
        if api_keys.openai.is_empty() { "NOT SET" } else { "configured" }
    );
    println!(
        "[z-ro] Gemini API key: {}",
        if api_keys.gemini.is_empty() { "NOT SET" } else { "configured" }
    );
    println!(
        "[z-ro] Anthropic API key: {}",
        if api_keys.anthropic.is_empty() { "NOT SET" } else { "configured" }
    );
    println!(
        "[z-ro] Groq API key: {}",
        if api_keys.groq.is_empty() { "NOT SET" } else { "configured" }
    );

    // Shared TaskMachine: used by both task commands and NPC coordinator
    let task_machine = Arc::new(Mutex::new(TaskMachine::new(tasks_dir)));

    // ── NPC Coordinator setup ──────────────────────────────────────
    //
    // Default provider: Gemini 2.0 Flash if the key is set (better UI vision
    // than gpt-4o-mini, strictly cheaper), otherwise OpenAI gpt-4o-mini,
    // otherwise fall back to an OpenAI stub (which will error on first call).
    // `NPC_PROVIDER` / `NPC_MODEL` env vars override.
    let (default_provider, default_model): (String, String) = match (
        std::env::var("NPC_PROVIDER").ok(),
        std::env::var("NPC_MODEL").ok(),
    ) {
        (Some(p), Some(m)) => (p, m),
        _ if !api_keys.gemini.is_empty() => ("gemini".into(), "gemini-2.0-flash".into()),
        _ => ("openai".into(), "gpt-4o-mini".into()),
    };

    let llm_provider = npc::llm_providers::build_provider(
        &default_provider,
        &default_model,
        &api_keys,
    )
    .unwrap_or_else(|e| {
        eprintln!(
            "[z-ro:npc] Failed to build default provider ({}/{}): {}. Falling back to OpenAI stub.",
            default_provider, default_model, e
        );
        // Stub provider with whatever's configured — calls will fail loudly.
        Arc::new(npc::llm_providers::openai::OpenAiLlm::new(
            api_keys.openai.clone(),
            "gpt-4o-mini".to_string(),
        ))
    });

    // STT: only wire if Groq key is configured
    let stt_provider: Option<Box<dyn npc::voice::stt::SttProvider>> = if !api_keys.groq.is_empty() {
        println!("[z-ro:npc] STT: Groq Whisper enabled");
        Some(Box::new(
            npc::stt_providers::groq_whisper::GroqWhisperStt::new(api_keys.groq.clone()),
        ))
    } else {
        println!("[z-ro:npc] STT: disabled (no GROQ_API_KEY)");
        None
    };

    // TTS: only wire if OpenAI key is configured
    let tts_provider: Option<Box<dyn npc::voice::tts::TtsProvider>> = if !api_keys.openai.is_empty() {
        println!("[z-ro:npc] TTS: OpenAI tts-1 enabled (voice=onyx)");
        Some(Box::new(
            npc::tts_providers::openai_tts::OpenAiTts::new(api_keys.openai.clone()),
        ))
    } else {
        println!("[z-ro:npc] TTS: disabled (no OPENAI_API_KEY)");
        None
    };

    let pipeline = npc::voice::pipeline::VoicePipeline::new(stt_provider, tts_provider, llm_provider);
    let npc_coordinator = npc::NpcCoordinator::new(pipeline, task_machine.clone(), api_keys.clone());

    println!(
        "[z-ro:npc] NPC coordinator initialized (provider: {}, model: {}, state: idle)",
        default_provider, default_model
    );

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(task_machine)
        .manage(Mutex::new(AiBridge::new(api_keys.openai.clone())))
        .manage(api_keys.clone())
        .manage(Arc::new(npc_coordinator))
        .setup(|app| {
            use tauri::Manager;
            // Create the overlay window on startup (hidden by default)
            let handle = app.handle().clone();
            if let Err(e) = overlay::create_overlay_window(&handle) {
                eprintln!("[z-ro] Failed to create overlay window: {}", e);
            } else {
                println!("[z-ro] Overlay window created (hidden)");
            }

            // Install the AppHandle into the NPC coordinator so the
            // overlay driver can emit events and show/hide the overlay
            // window from inside `dispatch_overlay`.
            let coord = app.state::<Arc<npc::NpcCoordinator>>();
            coord.set_app_handle(handle);
            println!("[z-ro:npc] AppHandle installed on coordinator");

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
            commands::npc::npc_ask_voice,
            commands::npc::npc_get_status,
            commands::npc::npc_interrupt,
            commands::npc::npc_list_models,
            commands::npc::npc_set_model,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
