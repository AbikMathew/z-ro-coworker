use tauri::{AppHandle, LogicalPosition, LogicalSize, Manager, WebviewUrl, WebviewWindowBuilder};

/// Create the transparent always-on-top overlay window.
///
/// The window is sized to cover the entire primary monitor so that the
/// normalized (0–1) coordinates emitted by the LLM map directly onto the
/// student's visible screen. When no primary monitor is reported (CI, some
/// headless setups), fall back to a 1440×900 centered window.
pub fn create_overlay_window(app: &AppHandle) -> Result<(), String> {
    // Figure out the primary monitor's LOGICAL size (i.e. pre-DPI), which is
    // the coord space Tauri and the browser agree on.
    let (width, height, pos_x, pos_y) = match app.primary_monitor() {
        Ok(Some(monitor)) => {
            let scale = monitor.scale_factor();
            let size = monitor.size();
            let pos = monitor.position();
            (
                size.width as f64 / scale,
                size.height as f64 / scale,
                pos.x as f64 / scale,
                pos.y as f64 / scale,
            )
        }
        _ => (1440.0, 900.0, 0.0, 0.0),
    };

    let _overlay = WebviewWindowBuilder::new(
        app,
        "overlay",
        WebviewUrl::App("/overlay".into()),
    )
    .title("Z-RO Overlay")
    .transparent(true)
    .decorations(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .visible(false)
    .inner_size(width, height)
    .position(pos_x, pos_y)
    .build()
    .map_err(|e| format!("Failed to create overlay window: {}", e))?;

    // Tauri occasionally rounds inner_size differently from what we asked;
    // reassert logical size + position so the overlay always covers the full
    // primary monitor corner-to-corner.
    if let Some(window) = app.get_webview_window("overlay") {
        let _ = window.set_size(LogicalSize::new(width, height));
        let _ = window.set_position(LogicalPosition::new(pos_x, pos_y));

        // Make the window itself click-through. On macOS this flips
        // NSWindow.ignoresMouseEvents, so mouse input falls through to
        // whatever app is beneath the overlay (VS Code, Chrome, etc.).
        // Without this, even a `pointer-events: none` DOM can't save us —
        // the webview swallows events at the OS layer.
        let _ = window.set_ignore_cursor_events(true);
    }

    Ok(())
}
