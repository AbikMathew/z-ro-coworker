use base64::Engine;
use xcap::Monitor;

#[tauri::command]
pub fn capture_screenshot() -> Result<String, String> {
    let monitors = Monitor::all().map_err(|e| format!("Failed to list monitors: {}", e))?;

    let monitor = monitors.into_iter().next().ok_or("No monitors found")?;

    let image = monitor
        .capture_image()
        .map_err(|e| format!("Failed to capture screen: {}", e))?;

    let mut png_bytes: Vec<u8> = Vec::new();
    image
        .write_to(
            &mut std::io::Cursor::new(&mut png_bytes),
            image::ImageFormat::Png,
        )
        .map_err(|e| format!("Failed to encode PNG: {}", e))?;

    let base64_image = base64::engine::general_purpose::STANDARD.encode(&png_bytes);

    println!(
        "[z-ro] Screenshot captured ({} bytes, {} base64 chars)",
        png_bytes.len(),
        base64_image.len()
    );

    Ok(base64_image)
}

#[tauri::command]
pub fn capture_screenshot_region(x: i32, y: i32, width: i32, height: i32) -> Result<String, String> {
    let monitors = Monitor::all().map_err(|e| format!("Failed to list monitors: {}", e))?;

    // Find the monitor that contains the requested region
    let monitor = monitors
        .into_iter()
        .find(|m| {
            let mx = m.x();
            let my = m.y();
            let mw = m.width() as i32;
            let mh = m.height() as i32;
            x >= mx && y >= my && x + width <= mx + mw && y + height <= my + mh
        })
        .ok_or("No monitor found for the requested region")?;

    let full_image = monitor
        .capture_image()
        .map_err(|e| format!("Failed to capture screen: {}", e))?;

    // Crop to region (coordinates relative to the monitor)
    let rel_x = (x - monitor.x()) as u32;
    let rel_y = (y - monitor.y()) as u32;
    let cropped = image::imageops::crop_imm(
        &full_image,
        rel_x,
        rel_y,
        width as u32,
        height as u32,
    )
    .to_image();

    let mut png_bytes: Vec<u8> = Vec::new();
    cropped
        .write_to(
            &mut std::io::Cursor::new(&mut png_bytes),
            image::ImageFormat::Png,
        )
        .map_err(|e| format!("Failed to encode PNG: {}", e))?;

    let base64_image = base64::engine::general_purpose::STANDARD.encode(&png_bytes);

    println!(
        "[z-ro] Region screenshot captured ({}x{} at {},{}, {} bytes)",
        width,
        height,
        x,
        y,
        png_bytes.len()
    );

    Ok(base64_image)
}
