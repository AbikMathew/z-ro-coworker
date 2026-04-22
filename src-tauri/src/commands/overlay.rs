use tauri::{AppHandle, Emitter, Manager};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlayData {
    pub elements: Vec<OverlayElement>,
}

/// One element rendered by the overlay webview.
///
/// Coordinates are NORMALIZED 0.0–1.0 — the renderer multiplies by its own
/// viewport size to get pixels. The overlay window is sized to cover the
/// primary monitor (see `src-tauri/src/overlay/mod.rs`), so `(1, 1)` lands at
/// the bottom-right of the student's visible screen.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlayElement {
    /// `"arrow"` | `"box"` | `"tooltip"` (plus legacy `"highlight"` kept for
    /// backward compatibility with any existing caller).
    pub element_type: String,
    pub x: f64,
    pub y: f64,
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub text: Option<String>,
    pub color: Option<String>,
}

#[tauri::command]
pub fn show_overlay(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("overlay") {
        window.show().map_err(|e| e.to_string())?;
        println!("[z-ro] Overlay shown");
    } else {
        return Err("Overlay window not found".to_string());
    }
    Ok(())
}

#[tauri::command]
pub fn hide_overlay(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("overlay") {
        window.hide().map_err(|e| e.to_string())?;
        println!("[z-ro] Overlay hidden");
    }
    Ok(())
}

#[tauri::command]
pub fn update_overlay(app: AppHandle, data: OverlayData) -> Result<(), String> {
    // Emit the overlay data to the overlay window
    app.emit_to("overlay", "overlay-update", &data)
        .map_err(|e| e.to_string())?;
    println!("[z-ro] Overlay updated with {} elements", data.elements.len());
    Ok(())
}
