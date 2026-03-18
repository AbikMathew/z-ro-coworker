pub mod platform;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowInfo {
    pub title: String,
    pub process_name: String,
    pub bundle_id: Option<String>,
}

pub fn get_active_window_info() -> Result<WindowInfo, String> {
    platform::get_active_window()
}
