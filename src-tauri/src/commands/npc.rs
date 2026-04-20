use base64::Engine;

use crate::npc::coordinator::{NpcCoordinator, NpcStatus, VoiceAskResult};
use crate::npc::llm_providers::ModelInfo;
use std::sync::Arc;
use tauri::State;

// NpcCoordinator is Send + Sync (all fields are Arc-wrapped), so it can be
// shared across commands via Arc<NpcCoordinator> without an outer Mutex.
// Holding a Mutex across async LLM calls would block npc_get_status and
// npc_interrupt for the full round-trip duration.

#[tauri::command]
pub async fn npc_activate(
    coordinator: State<'_, Arc<NpcCoordinator>>,
) -> Result<(), String> {
    coordinator.activate().await
}

#[tauri::command]
pub async fn npc_deactivate(
    coordinator: State<'_, Arc<NpcCoordinator>>,
) -> Result<(), String> {
    coordinator.deactivate().await
}

#[tauri::command]
pub async fn npc_start_listening(
    coordinator: State<'_, Arc<NpcCoordinator>>,
) -> Result<(), String> {
    coordinator.start_listening().await
}

#[tauri::command]
pub async fn npc_stop_listening(
    coordinator: State<'_, Arc<NpcCoordinator>>,
) -> Result<(), String> {
    coordinator.stop_listening().await
}

#[tauri::command]
pub async fn npc_ask_text(
    question: String,
    coordinator: State<'_, Arc<NpcCoordinator>>,
) -> Result<String, String> {
    coordinator.ask_text(question).await
}

#[tauri::command]
pub async fn npc_get_status(
    coordinator: State<'_, Arc<NpcCoordinator>>,
) -> Result<NpcStatus, String> {
    Ok(coordinator.get_status())
}

#[tauri::command]
pub async fn npc_interrupt(
    coordinator: State<'_, Arc<NpcCoordinator>>,
) -> Result<(), String> {
    coordinator.interrupt().await
}

/// Toggle continuous on-air voice mode. The frontend owns the mic + VAD;
/// this command just records whether on-air is live so other backend
/// logic (Phase 3 proactive speech, Phase 4 WrongMove) can condition on it.
#[tauri::command]
pub async fn npc_set_on_air(
    active: bool,
    coordinator: State<'_, Arc<NpcCoordinator>>,
) -> Result<(), String> {
    coordinator.set_on_air_active(active);
    Ok(())
}

/// Process a voice turn: base64-encoded audio bytes in, transcript + TTS audio out.
///
/// `filename` is a hint for the STT provider (e.g. `"audio.webm"`).
#[tauri::command]
pub async fn npc_ask_voice(
    audio_b64: String,
    filename: String,
    coordinator: State<'_, Arc<NpcCoordinator>>,
) -> Result<VoiceAskResult, String> {
    let audio_bytes = base64::engine::general_purpose::STANDARD
        .decode(audio_b64.as_bytes())
        .map_err(|e| format!("Invalid base64 audio: {}", e))?;
    coordinator.ask_voice(audio_bytes, filename).await
}

/// List all LLM models that can be chosen (filtered by which API keys are configured).
#[tauri::command]
pub async fn npc_list_models(
    coordinator: State<'_, Arc<NpcCoordinator>>,
) -> Result<Vec<ModelInfo>, String> {
    Ok(coordinator.list_available_models())
}

/// Hot-swap the active LLM provider and persist the choice so it survives
/// a restart. The next NPC turn uses the new provider.
#[tauri::command]
pub async fn npc_set_model(
    provider: String,
    model: String,
    coordinator: State<'_, Arc<NpcCoordinator>>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    coordinator.set_llm(&provider, &model)?;

    // Fire-and-forget persist. We already swapped successfully — if the
    // disk write fails the user's in-memory choice is still correct,
    // they just lose it on next boot. Log so the failure is visible.
    use tauri::Manager;
    if let Ok(app_data_dir) = app.path().app_data_dir() {
        let settings = crate::npc::settings_store::NpcSettings {
            llm_provider: Some(provider.clone()),
            llm_model: Some(model.clone()),
        };
        if let Err(e) = settings.save(&app_data_dir) {
            eprintln!(
                "[z-ro:npc] hot-swap OK but persist failed: {e}"
            );
        }
    }
    Ok(())
}
