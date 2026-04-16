use crate::npc::coordinator::{NpcCoordinator, NpcStatus};
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
