use crate::ai::bridge::AiBridge;
use tauri::State;
use std::sync::Mutex;

#[tauri::command]
pub async fn request_help(
    question: String,
    task_context: String,
    bridge: State<'_, Mutex<AiBridge>>,
) -> Result<String, String> {
    let bridge_clone = {
        let b = bridge.lock().map_err(|e| e.to_string())?;
        b.clone()
    };
    bridge_clone.ask(&question, &task_context).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn analyze_screenshot(
    base64_image: String,
    question: String,
    task_context: String,
    bridge: State<'_, Mutex<AiBridge>>,
) -> Result<String, String> {
    let bridge_clone = {
        let b = bridge.lock().map_err(|e| e.to_string())?;
        b.clone()
    };
    bridge_clone
        .analyze_screenshot(&base64_image, &question, &task_context)
        .await
        .map_err(|e| e.to_string())
}
