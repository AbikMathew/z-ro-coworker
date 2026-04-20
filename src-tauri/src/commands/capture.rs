//! Tauri commands that expose the SCK capture pipeline to the frontend.
//!
//! The frontend owns the UX: when the user clicks "Share your screen", it
//! calls `capture_request_picker`, which shows the native macOS picker and
//! — on successful selection — starts an SCStream feeding frames into the
//! app-wide `FrameBuffer`. `capture_stop` tears the session down.

use std::sync::Mutex;

use serde::Serialize;
use tauri::State;

use crate::capture::{self, CaptureSession, PickerResult};
use crate::npc::frame_buffer::FrameBuffer;

/// State slot holding the currently-active capture session, if any. Wrapped
/// in a `Mutex` because Tauri commands run on a multithreaded async runtime
/// and we need exclusive access to start/stop.
pub struct CaptureState {
    pub session: Mutex<Option<CaptureSession>>,
    pub buffer: FrameBuffer,
}

impl CaptureState {
    pub fn new(buffer: FrameBuffer) -> Self {
        Self {
            session: Mutex::new(None),
            buffer,
        }
    }
}

#[derive(Serialize)]
pub struct CaptureStatus {
    pub active: bool,
    pub frames_in_ring: usize,
    pub latest_width: Option<u32>,
    pub latest_height: Option<u32>,
    pub latest_age_ms: Option<u64>,
}

/// Present the native `SCContentSharingPicker`, wait for the user's choice,
/// and start a stream into the shared FrameBuffer. Replaces any session
/// already in flight.
#[tauri::command]
pub async fn capture_request_picker(
    state: State<'_, CaptureState>,
) -> Result<PickerResult, String> {
    // Drop any prior session BEFORE showing the new picker, so a user who
    // clicks "Share" twice doesn't end up with two live streams.
    {
        let mut slot = state.session.lock().map_err(|e| e.to_string())?;
        *slot = None;
    }

    let buffer = state.buffer.clone();
    let (session, result) = capture::request_picker_and_start(buffer)
        .await
        .map_err(String::from)?;

    let mut slot = state.session.lock().map_err(|e| e.to_string())?;
    *slot = Some(session);
    Ok(result)
}

/// Stop the active capture session. No-op if none is running.
#[tauri::command]
pub fn capture_stop(state: State<'_, CaptureState>) -> Result<(), String> {
    let mut slot = state.session.lock().map_err(|e| e.to_string())?;
    *slot = None; // Drop runs stop_capture and clears the ring buffer.
    Ok(())
}

/// Current capture status. Cheap to call — the frontend polls this from the
/// "Zee is watching …" indicator.
#[tauri::command]
pub fn capture_status(state: State<'_, CaptureState>) -> Result<CaptureStatus, String> {
    let active = state
        .session
        .lock()
        .map_err(|e| e.to_string())?
        .is_some();

    let latest = state.buffer.latest();
    let (latest_width, latest_height, latest_age_ms) = match latest {
        Some(frame) => (
            Some(frame.width),
            Some(frame.height),
            Some(
                std::time::Instant::now()
                    .saturating_duration_since(frame.captured_at)
                    .as_millis() as u64,
            ),
        ),
        None => (None, None, None),
    };

    Ok(CaptureStatus {
        active,
        frames_in_ring: state.buffer.len(),
        latest_width,
        latest_height,
        latest_age_ms,
    })
}
