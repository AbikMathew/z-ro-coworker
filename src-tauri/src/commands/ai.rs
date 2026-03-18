use crate::ai::bridge::AiBridge;
use tauri::State;
use std::sync::Mutex;

#[tauri::command]
pub async fn request_help(
    question: String,
    task_context: String,
    bridge: State<'_, Mutex<AiBridge>>,
) -> Result<String, String> {
    // Clone the bridge data we need before dropping the lock
    let bridge_clone = {
        let b = bridge.lock().map_err(|e| e.to_string())?;
        b.clone()
    };
    bridge_clone.ask(&question, &task_context).await.map_err(|e| e.to_string())
}
