//! Tauri commands that expose the global event listener to the frontend.
//!
//! The listener itself is started by `lib.rs` at boot. These commands only
//! surface the status so the UI can prompt the user to grant Accessibility
//! permission if events aren't flowing.

use serde::Serialize;
use tauri::State;

#[cfg(target_os = "macos")]
use crate::npc::events::{self, EventBus};

#[derive(Serialize)]
pub struct EventsStatus {
    /// Listener thread was spawned and hasn't exited yet. If false, the
    /// user likely hasn't granted Accessibility permission — events are
    /// silently dropped by macOS.
    pub armed: bool,
    /// Does this process currently hold Accessibility permission?
    pub accessibility_trusted: bool,
}

#[cfg(target_os = "macos")]
#[tauri::command]
pub fn events_status(bus: State<'_, EventBus>) -> EventsStatus {
    EventsStatus {
        armed: bus.is_armed(),
        accessibility_trusted: events::is_accessibility_trusted(),
    }
}

/// Stub for non-macOS builds so the command handler list in `lib.rs`
/// doesn't need to be `#[cfg]`-fenced.
#[cfg(not(target_os = "macos"))]
#[tauri::command]
pub fn events_status() -> EventsStatus {
    EventsStatus {
        armed: false,
        accessibility_trusted: false,
    }
}
