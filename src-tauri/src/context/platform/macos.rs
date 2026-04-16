use crate::context::WindowInfo;
use std::process::Command;

pub fn get_active_window() -> Result<WindowInfo, String> {
    // Use AppleScript to get the frontmost application info including PID
    let script = r#"
        tell application "System Events"
            set frontApp to first application process whose frontmost is true
            set appName to name of frontApp
            set windowTitle to ""
            try
                set windowTitle to name of front window of frontApp
            end try
            set bundleId to bundle identifier of frontApp
            set appPid to unix id of frontApp
            return appName & "|" & windowTitle & "|" & bundleId & "|" & appPid
        end tell
    "#;

    let output = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .map_err(|e| format!("Failed to run osascript: {}", e))?;

    let result = String::from_utf8_lossy(&output.stdout).trim().to_string();

    // Parse right-to-left so window titles containing '|' are handled correctly.
    // AppleScript format: appName|windowTitle|bundleId|pid
    // App names never contain '|'; bundle IDs never contain '|'; only window
    // titles (e.g. "cat file | grep foo" in Terminal) may contain them.
    let (without_pid, pid_str) = result.rsplit_once('|').unwrap_or((&result, ""));
    let (without_bundle, bundle_str) = without_pid.rsplit_once('|').unwrap_or((without_pid, ""));
    let (app_name, window_title) = without_bundle.split_once('|').unwrap_or((without_bundle, ""));

    Ok(WindowInfo {
        process_name: app_name.to_string(),
        title: window_title.to_string(),
        bundle_id: if bundle_str.is_empty() { None } else { Some(bundle_str.to_string()) },
        pid: pid_str.trim().parse::<u32>().ok(),
    })
}
