use crate::context::WindowInfo;

#[tauri::command]
pub fn get_active_window() -> Result<WindowInfo, String> {
    crate::context::get_active_window_info().map_err(|e| e.to_string())
}
