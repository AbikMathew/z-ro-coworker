// Platform-abstracted screen capture module.
//
// This is a structural scaffold for future Rust-side screen capture beyond
// the browser's getDisplayMedia API. The /coworker route currently captures
// frames in the frontend (via getDisplayMedia) and sends them over WebSocket
// to Gemini Live, so nothing here is invoked yet. The module layout is ready
// for native capture (ScreenCaptureKit, Graphics.Capture, PipeWire) without
// needing to restructure.

#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "windows")]
pub mod windows;
#[cfg(target_os = "linux")]
pub mod linux;

/// Capture the primary display as a raw byte buffer (format TBD per platform).
///
/// Dispatches to the platform-specific implementation at compile time.
#[allow(dead_code)]
pub fn capture_primary_display() -> Result<Vec<u8>, String> {
    #[cfg(target_os = "macos")]
    {
        macos::capture_primary_display()
    }
    #[cfg(target_os = "windows")]
    {
        windows::capture_primary_display()
    }
    #[cfg(target_os = "linux")]
    {
        linux::capture_primary_display()
    }
}
