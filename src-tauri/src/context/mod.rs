pub mod platform;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowInfo {
    pub title: String,
    pub process_name: String,
    pub bundle_id: Option<String>,
    /// Process ID of the owning application (macOS only).
    #[serde(default)]
    pub pid: Option<u32>,
}

pub fn get_active_window_info() -> Result<WindowInfo, String> {
    platform::get_active_window()
}
