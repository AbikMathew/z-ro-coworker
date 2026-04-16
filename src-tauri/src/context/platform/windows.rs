use crate::context::WindowInfo;

pub fn get_active_window() -> Result<WindowInfo, String> {
    // TODO: Implement Windows active window detection using Win32 API
    Ok(WindowInfo {
        title: "Not implemented".to_string(),
        process_name: "Unknown".to_string(),
        bundle_id: None,
        pid: None,
    })
}
