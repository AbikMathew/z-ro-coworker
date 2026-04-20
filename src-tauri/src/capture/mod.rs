//! Native screen capture via Apple ScreenCaptureKit.
//!
//! v1 is macOS-only (14 Sonoma or newer — required by `SCContentSharingPicker`,
//! the native "which window do you want to share?" dialog). Windows and Linux
//! are deferred until the mac pipeline is stable; targeting them now would
//! require three separate capture stories, each with its own permission flow.

#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(target_os = "macos")]
pub use macos::{request_picker_and_start, CaptureError, CaptureSession, PickerResult};

#[cfg(not(target_os = "macos"))]
compile_error!(
    "z-ro cowork v1 only builds on macOS 14+. Native capture on Windows/Linux \
     is on the roadmap but requires UIA+Graphics.Capture / AT-SPI+PipeWire, \
     which are separate work streams."
);
