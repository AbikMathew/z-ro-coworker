use crate::context::WindowInfo;
use std::process::Command;

pub fn get_active_window() -> Result<WindowInfo, String> {
    // Use AppleScript to get the frontmost application info
    let script = r#"
        tell application "System Events"
            set frontApp to first application process whose frontmost is true
            set appName to name of frontApp
            set windowTitle to ""
            try
                set windowTitle to name of front window of frontApp
            end try
            set bundleId to bundle identifier of frontApp
            return appName & "|" & windowTitle & "|" & bundleId
        end tell
    "#;

    let output = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .map_err(|e| format!("Failed to run osascript: {}", e))?;

    let result = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let parts: Vec<&str> = result.splitn(3, '|').collect();

    Ok(WindowInfo {
        process_name: parts.first().unwrap_or(&"Unknown").to_string(),
        title: parts.get(1).unwrap_or(&"").to_string(),
        bundle_id: parts.get(2).map(|s| s.to_string()),
    })
}
