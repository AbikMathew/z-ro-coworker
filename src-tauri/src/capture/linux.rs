#![cfg(target_os = "linux")]

/// Linux native screen capture (PipeWire portal).
#[allow(dead_code)]
pub fn capture_primary_display() -> Result<Vec<u8>, String> {
    todo!("Linux PipeWire portal capture not yet implemented")
}
