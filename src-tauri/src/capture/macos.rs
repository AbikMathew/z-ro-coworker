#![cfg(target_os = "macos")]

//! SCK capture pipeline: show the native content-sharing picker, start an
//! `SCStream` on the user's choice, and push BGRA frames into a `FrameBuffer`.
//!
//! Why ScreenCaptureKit (SCK)?
//! - It is the Apple-blessed API for screen/window capture since macOS 12.3.
//! - Its `SCContentSharingPicker` (macOS 14+) is the only path that avoids
//!   the monthly "requesting to bypass the system private window picker"
//!   nag Apple added in Sequoia for apps using alternative pickers.
//! - Permission is granted implicitly by picking in the system UI — no
//!   separate TCC Screen Recording prompt.
//!
//! Threading model:
//! - The picker is shown through `AsyncSCContentSharingPicker`, so the
//!   caller `.await`s on an executor-agnostic future. That future hides the
//!   Apple dispatch-queue callback internally.
//! - Frames arrive on SCK's Grand-Central-Dispatch queue — NOT a tokio
//!   thread. `FrameHandler` must be `Send` and do zero `.await` work.
//!   We lock the ring buffer synchronously and return immediately so the
//!   dispatch queue is unblocked.
//! - `CaptureSession` holds the `SCStream`. Dropping the session stops capture
//!   (via `stop_capture()` in `Drop`).

use std::time::Instant;

use bytes::Bytes;
use screencapturekit::async_api::AsyncSCContentSharingPicker;
use screencapturekit::content_sharing_picker::{
    SCContentSharingPickerConfiguration, SCContentSharingPickerMode, SCPickerOutcome,
};
use screencapturekit::cv::CVPixelBufferLockFlags;
use screencapturekit::prelude::*;

use crate::npc::frame_buffer::{Frame, FrameBuffer};

/// Target capture dimensions. 1440 px long-edge is plenty for an LLM at
/// typical laptop displays and keeps vision token counts modest. The real
/// display resolution is what the user's monitor reports; we downscale in
/// SCK's config rather than post-capture so we pay the resize cost once,
/// inside Apple's framework.
const CAPTURE_WIDTH: u32 = 1440;
const CAPTURE_HEIGHT: u32 = 900;

/// Errors surfaced to callers. Kept flat (`String`-convertible via `Display`)
/// because Tauri commands serialise errors as strings to the frontend.
#[derive(Debug)]
pub enum CaptureError {
    Picker(String),
    Stream(String),
    Cancelled,
}

impl std::fmt::Display for CaptureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Picker(m) => write!(f, "picker error: {m}"),
            Self::Stream(m) => write!(f, "stream error: {m}"),
            Self::Cancelled => write!(f, "the user cancelled the picker"),
        }
    }
}

impl std::error::Error for CaptureError {}

impl From<CaptureError> for String {
    fn from(e: CaptureError) -> String {
        e.to_string()
    }
}

/// Metadata we bubble back to the UI after the user picks a source — useful
/// for the "Zee is watching …" chip on the task header.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PickerResult {
    pub width: u32,
    pub height: u32,
    pub source_kind: &'static str,
    /// Bounds of the captured region on the user's screen(s), in **logical
    /// points** (matches what `app.primary_monitor().size() / scale_factor`
    /// returns). Consumed by the overlay driver to translate LLM coords
    /// from capture-local to monitor-local space.
    pub bounds: CaptureBounds,
}

/// Captured region's position + size on-screen, in logical points. `origin`
/// is top-left in the global screen coord system.
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct CaptureBounds {
    pub origin_x: f64,
    pub origin_y: f64,
    pub width: f64,
    pub height: f64,
}

/// Live SCK capture session. Drop to stop the stream; the `Drop` impl calls
/// `stop_capture()` so a missed explicit `stop()` still cleans up.
pub struct CaptureSession {
    stream: SCStream,
    buffer: FrameBuffer,
    /// Screen bounds of the captured region, used by the overlay driver
    /// to translate LLM coords from capture-local to monitor-local.
    pub bounds: CaptureBounds,
}

impl CaptureSession {
    /// Stop capture explicitly. Also fine to just drop the session — the
    /// `Drop` impl does the same work.
    pub fn stop(self) {
        // `drop(self)` is implicit at end of scope.
    }
}

impl Drop for CaptureSession {
    fn drop(&mut self) {
        if let Err(e) = self.stream.stop_capture() {
            eprintln!("[z-ro:capture] stop_capture failed: {e:?}");
        }
        self.buffer.clear();
        println!("[z-ro:capture] session stopped, ring cleared");
    }
}

/// Show the native picker asynchronously, wait for the user's choice, and
/// start an `SCStream` that pushes BGRA frames into `buffer`. Returns
/// `Err(CaptureError::Cancelled)` if the user dismissed the picker.
pub async fn request_picker_and_start(
    buffer: FrameBuffer,
) -> Result<(CaptureSession, PickerResult), CaptureError> {
    let mut config = SCContentSharingPickerConfiguration::new();
    config.set_allowed_picker_modes(&[
        SCContentSharingPickerMode::SingleWindow,
        SCContentSharingPickerMode::SingleDisplay,
        SCContentSharingPickerMode::SingleApplication,
    ]);

    let outcome = AsyncSCContentSharingPicker::show(&config).await;
    let result = match outcome {
        SCPickerOutcome::Picked(r) => r,
        SCPickerOutcome::Cancelled => return Err(CaptureError::Cancelled),
        SCPickerOutcome::Error(msg) => return Err(CaptureError::Picker(msg)),
    };

    let (src_w, src_h) = result.pixel_size();
    let (rect_x, rect_y, rect_w, rect_h) = result.rect();
    let bounds = CaptureBounds {
        origin_x: rect_x,
        origin_y: rect_y,
        width: rect_w,
        height: rect_h,
    };
    let source_kind = if !result.windows().is_empty() {
        "window"
    } else if !result.displays().is_empty() {
        "display"
    } else {
        "application"
    };
    let filter = result.filter();

    let stream_config = SCStreamConfiguration::new()
        .with_width(CAPTURE_WIDTH)
        .with_height(CAPTURE_HEIGHT)
        .with_pixel_format(PixelFormat::BGRA);

    let mut stream = SCStream::new(&filter, &stream_config);

    let handler = FrameHandler {
        buffer: buffer.clone(),
    };
    stream.add_output_handler(handler, SCStreamOutputType::Screen);

    stream
        .start_capture()
        .map_err(|e| CaptureError::Stream(format!("{e:?}")))?;

    println!(
        "[z-ro:capture] stream started: source={source_kind} native={src_w}x{src_h} \
         captured@{CAPTURE_WIDTH}x{CAPTURE_HEIGHT} \
         bounds=({rect_x:.0},{rect_y:.0}) {rect_w:.0}x{rect_h:.0}pt"
    );

    Ok((
        CaptureSession { stream, buffer, bounds },
        PickerResult {
            width: CAPTURE_WIDTH,
            height: CAPTURE_HEIGHT,
            source_kind,
            bounds,
        },
    ))
}

struct FrameHandler {
    buffer: FrameBuffer,
}

impl SCStreamOutputTrait for FrameHandler {
    fn did_output_sample_buffer(
        &self,
        sample_buffer: CMSampleBuffer,
        of_type: SCStreamOutputType,
    ) {
        // SCK delivers audio samples through the same trait — filter to
        // screen frames so we don't try to parse audio data as BGRA pixels.
        if !matches!(of_type, SCStreamOutputType::Screen) {
            return;
        }

        let Some(pixel_buffer) = sample_buffer.image_buffer() else {
            return;
        };

        // READ_ONLY lock is sufficient — we never mutate SCK's buffer, we
        // just memcpy out into our own allocation.
        let guard = match pixel_buffer.lock(CVPixelBufferLockFlags::READ_ONLY) {
            Ok(g) => g,
            Err(_) => return,
        };

        let width = guard.width() as u32;
        let height = guard.height() as u32;
        let bytes_per_row = guard.bytes_per_row() as u32;
        // Copy the bytes out before the guard drops, otherwise the pointer
        // dangles. `Bytes::copy_from_slice` is one memcpy.
        let pixels = Bytes::copy_from_slice(guard.as_slice());
        // Guard drops at end of scope, unlocking the CVPixelBuffer.

        let frame = Frame {
            pixels,
            width,
            height,
            bytes_per_row,
            scale_factor: 1.0, // SCK already scaled us to CAPTURE_WIDTH x CAPTURE_HEIGHT
            captured_at: Instant::now(),
        };

        self.buffer.push(frame);
    }
}

/// `true` iff the buffer has a frame younger than `max_age`. Used by the
/// coordinator to decide whether screen context is fresh enough to inject
/// into the next LLM call.
pub fn has_fresh_frame(buffer: &FrameBuffer, max_age: std::time::Duration) -> bool {
    match buffer.latest() {
        Some(arc_frame) => {
            Instant::now().saturating_duration_since(arc_frame.captured_at) <= max_age
        }
        None => false,
    }
}
