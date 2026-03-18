use tauri::{AppHandle, WebviewWindowBuilder, WebviewUrl};

pub fn create_overlay_window(app: &AppHandle) -> Result<(), String> {
    let _overlay = WebviewWindowBuilder::new(
        app,
        "overlay",
        WebviewUrl::App("/overlay".into()),
    )
    .title("Z-RO Overlay")
    .decorations(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .visible(false)
    .build()
    .map_err(|e| format!("Failed to create overlay window: {}", e))?;

    Ok(())
}
