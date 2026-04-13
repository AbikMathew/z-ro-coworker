#![cfg(target_os = "macos")]

/// macOS native screen capture.
///
/// Placeholder: when we migrate off `screencapture` CLI (see
/// `commands/screenshot.rs`) and onto ScreenCaptureKit, the real
/// implementation goes here.
#[allow(dead_code)]
pub fn capture_primary_display() -> Result<Vec<u8>, String> {
    Err("macOS native ScreenCaptureKit capture not yet implemented".to_string())
}
