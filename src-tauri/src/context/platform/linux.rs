use crate::context::WindowInfo;

pub fn get_active_window() -> Result<WindowInfo, String> {
    // TODO: Implement Linux active window detection using xdotool or similar
    Ok(WindowInfo {
        title: "Not implemented".to_string(),
        process_name: "Unknown".to_string(),
        bundle_id: None,
        pid: None,
    })
}
