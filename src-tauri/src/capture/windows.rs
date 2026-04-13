#![cfg(target_os = "windows")]

/// Windows native screen capture (Graphics.Capture API).
#[allow(dead_code)]
pub fn capture_primary_display() -> Result<Vec<u8>, String> {
    Err("Windows Graphics.Capture not yet implemented".to_string())
}
