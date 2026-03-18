#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "linux")]
mod linux;

use super::WindowInfo;

pub fn get_active_window() -> Result<WindowInfo, String> {
    #[cfg(target_os = "macos")]
    return macos::get_active_window();

    #[cfg(target_os = "windows")]
    return windows::get_active_window();

    #[cfg(target_os = "linux")]
    return linux::get_active_window();

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    Err("Unsupported platform".to_string())
}
