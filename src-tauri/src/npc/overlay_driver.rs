//! Converts the LLM's `OverlayCommand`s into concrete overlay updates on the
//! Tauri overlay window.
//!
//! The LLM emits NORMALIZED coordinates (0.0–1.0 of the screenshot frame).
//! The overlay window is sized to cover the entire primary monitor
//! (see `src-tauri/src/overlay/mod.rs`), so the renderer can multiply these
//! 0-1 values by its own viewport size without any backend translation. The
//! driver's job is therefore thin: filter commands, pass them through as
//! `OverlayElement`s, show the window, and auto-hide after a timeout.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};
use tokio::task::JoinHandle;

use crate::commands::overlay::{OverlayData, OverlayElement};
use crate::npc::voice::pipeline::OverlayCommand;

/// How long an overlay stays on screen before auto-hiding.
pub const AUTO_HIDE_SECS: u64 = 15;

/// Shared handle to the pending auto-hide timer. Each new overlay cancels the
/// prior pending timer so a fresh 15s countdown begins.
pub type AutoHideState = Arc<Mutex<Option<JoinHandle<()>>>>;

pub fn new_auto_hide_state() -> AutoHideState {
    Arc::new(Mutex::new(None))
}

/// Apply overlay commands produced by one LLM turn.
///
/// - Any `"clear"` command short-circuits: cancel pending hide, hide window.
/// - Otherwise: translate each command to an `OverlayElement` (pass-through,
///   since coords are already normalized), emit `"overlay-update"`, show
///   the window, schedule/reset the 15s auto-hide.
pub fn apply(
    app: &AppHandle,
    commands: &[OverlayCommand],
    auto_hide: &AutoHideState,
) -> Result<(), String> {
    if commands.is_empty() {
        return Ok(());
    }

    // If anything in this batch is a "clear", treat the whole batch as a
    // clear. (LLMs sometimes chain "clear" before a new point — we still
    // honor the most-recent intent by always starting fresh.)
    if commands.iter().any(|c| c.action == "clear") {
        cancel_auto_hide(auto_hide);
        hide_window(app);
        println!("[z-ro:npc] overlay: clear");
        // If ONLY "clear" was emitted, we're done.
        if commands.iter().all(|c| c.action == "clear") {
            return Ok(());
        }
        // Otherwise fall through so the non-clear commands render fresh.
    }

    let mut elements = Vec::with_capacity(commands.len());
    for cmd in commands.iter().filter(|c| c.action != "clear") {
        if let Some(el) = command_to_element(cmd) {
            log_cmd(cmd);
            elements.push(el);
        } else {
            eprintln!(
                "[z-ro:npc] overlay: skipping {} (missing coords)",
                cmd.action
            );
        }
    }

    if elements.is_empty() {
        return Ok(());
    }

    app.emit_to("overlay", "overlay-update", &OverlayData { elements })
        .map_err(|e| format!("overlay emit failed: {}", e))?;

    if let Some(window) = app.get_webview_window("overlay") {
        window
            .show()
            .map_err(|e| format!("overlay show failed: {}", e))?;
    }

    schedule_auto_hide(app.clone(), auto_hide.clone());
    Ok(())
}

/// Translate one `OverlayCommand` to an `OverlayElement` for the renderer.
/// Returns `None` when the command is missing required coordinates (so the
/// caller can log and skip rather than pushing garbage to the UI).
fn command_to_element(cmd: &OverlayCommand) -> Option<OverlayElement> {
    match cmd.action.as_str() {
        "arrow" | "tooltip" | "box" | "highlight" => {
            let x = cmd.x?;
            let y = cmd.y?;
            // Clamp to [0, 1] defensively — LLMs occasionally emit 1.05.
            Some(OverlayElement {
                element_type: cmd.action.clone(),
                x: x.clamp(0.0, 1.0) as f64,
                y: y.clamp(0.0, 1.0) as f64,
                width: cmd.width.map(|w| w.clamp(0.0, 1.0) as f64),
                height: cmd.height.map(|h| h.clamp(0.0, 1.0) as f64),
                text: cmd.text.clone(),
                color: cmd.color.clone(),
            })
        }
        other => {
            eprintln!("[z-ro:npc] overlay: unknown action \"{}\"", other);
            None
        }
    }
}

fn log_cmd(cmd: &OverlayCommand) {
    let txt = cmd.text.as_deref().unwrap_or("");
    match (cmd.x, cmd.y) {
        (Some(x), Some(y)) => println!(
            "[z-ro:npc] overlay: {}@({:.2},{:.2}) \"{}\"",
            cmd.action, x, y, txt
        ),
        _ => println!("[z-ro:npc] overlay: {} \"{}\"", cmd.action, txt),
    }
}

fn hide_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("overlay") {
        let _ = window.hide();
    }
}

fn cancel_auto_hide(state: &AutoHideState) {
    if let Ok(mut slot) = state.lock() {
        if let Some(handle) = slot.take() {
            handle.abort();
        }
    }
}

fn schedule_auto_hide(app: AppHandle, state: AutoHideState) {
    // Cancel any prior pending hide — a fresh command restarts the timer.
    cancel_auto_hide(&state);
    let state_for_task = state.clone();
    let handle = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(AUTO_HIDE_SECS)).await;
        hide_window(&app);
        // Clear our own handle slot once the timer fires so the driver
        // doesn't try to abort a finished task later.
        if let Ok(mut slot) = state_for_task.lock() {
            *slot = None;
        }
        println!(
            "[z-ro:npc] overlay: auto-hidden after {}s",
            AUTO_HIDE_SECS
        );
    });
    if let Ok(mut slot) = state.lock() {
        *slot = Some(handle);
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn cmd(action: &str, x: Option<f32>, y: Option<f32>) -> OverlayCommand {
        OverlayCommand {
            action: action.into(),
            x,
            y,
            width: None,
            height: None,
            text: None,
            color: None,
        }
    }

    #[test]
    fn test_command_to_element_arrow() {
        let c = cmd("arrow", Some(0.5), Some(0.75));
        let el = command_to_element(&c).unwrap();
        assert_eq!(el.element_type, "arrow");
        assert!((el.x - 0.5).abs() < 1e-6);
        assert!((el.y - 0.75).abs() < 1e-6);
    }

    #[test]
    fn test_command_to_element_box_keeps_size() {
        let mut c = cmd("box", Some(0.1), Some(0.2));
        c.width = Some(0.3);
        c.height = Some(0.05);
        let el = command_to_element(&c).unwrap();
        // f32 → f64 cast introduces minor precision artifacts, so compare
        // with an epsilon rather than exact equality.
        assert!((el.width.unwrap() - 0.3).abs() < 1e-6);
        assert!((el.height.unwrap() - 0.05).abs() < 1e-6);
    }

    #[test]
    fn test_command_to_element_missing_coords_is_none() {
        let c = cmd("arrow", None, Some(0.5));
        assert!(command_to_element(&c).is_none());
    }

    #[test]
    fn test_command_to_element_unknown_action_is_none() {
        let c = cmd("spin", Some(0.5), Some(0.5));
        assert!(command_to_element(&c).is_none());
    }

    #[test]
    fn test_command_to_element_clamps_out_of_range() {
        let c = cmd("arrow", Some(1.2), Some(-0.3));
        let el = command_to_element(&c).unwrap();
        assert_eq!(el.x, 1.0);
        assert_eq!(el.y, 0.0);
    }

    #[test]
    fn test_command_to_element_preserves_text_color() {
        let mut c = cmd("arrow", Some(0.5), Some(0.5));
        c.text = Some("Click here".into());
        c.color = Some("blue".into());
        let el = command_to_element(&c).unwrap();
        assert_eq!(el.text.as_deref(), Some("Click here"));
        assert_eq!(el.color.as_deref(), Some("blue"));
    }

    #[test]
    fn test_new_auto_hide_state_starts_empty() {
        let state = new_auto_hide_state();
        assert!(state.lock().unwrap().is_none());
    }
}
