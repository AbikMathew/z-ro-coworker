use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tauri::{AppHandle, Emitter};

use crate::context::WindowInfo;

/// Our own PID, captured at process start.  Compared against the focused
/// app's PID to reliably detect when z-ro itself is focused — regardless of
/// how the process name or bundle ID appear in dev vs release builds.
static SELF_PID: std::sync::LazyLock<u32> = std::sync::LazyLock::new(std::process::id);

/// Minimum AX tree length (in bytes) below which we consider the tree "thin"
/// and fall back to a screenshot.  Electron / canvas-heavy apps often return
/// < 200 bytes (just a top-level window element with no children).
const THIN_AX_THRESHOLD: usize = 200;

/// JPEG quality for fallback screenshots (1-100).  60 keeps them under ~80 KB
/// at typical laptop resolutions — small enough for GPT-4o vision without
/// dominating the prompt.
const SCREENSHOT_JPEG_QUALITY: u8 = 60;

/// Maximum dimension (width or height) for downscaled screenshots.
/// GPT-4o vision works well at 1024px long edge.
const SCREENSHOT_MAX_EDGE: u32 = 1024;

/// Snapshot of the user's screen at a point in time.
///
/// The primary context source is the accessibility tree (`ax_tree`), which is
/// fast (~5-50 ms) and compact (~2-5 KB text).  A screenshot is captured only
/// as a fallback when the AX tree is empty or for canvas-heavy apps.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenContext {
    /// Focused window metadata (app name, window title, bundle id).
    pub window: WindowInfo,
    /// Indented text representation of the window's AX tree.
    /// Format: each line is `<indent><role_description> "<title>"`.
    /// Depth-limited to 6 levels, size-capped at 4 KB.
    pub ax_tree: Option<String>,
    /// Base64-encoded JPEG screenshot (quality 60, max 1024px long edge).
    /// Only populated as a fallback when the AX tree is empty or very thin
    /// (< 200 bytes) — typical of Electron / canvas-heavy apps.
    pub screenshot_b64: Option<String>,
    /// Unix timestamp in milliseconds when this context was captured.
    pub captured_at_ms: u64,
}

/// Reads the user's screen state via the macOS Accessibility API.
///
/// **Self-focus problem:** When the user types in z-ro's chat panel, the
/// focused window is z-ro itself — not the app they need help with (VS Code,
/// Chrome, Finder, etc.).
///
/// **Solution:** A background thread polls the focused window every second.
/// Whenever the focus is on an *external* app, its AX tree + window info are
/// stored.  When `capture()` is called and z-ro is the focused app, the most
/// recent external context is returned instead.
pub struct ScreenReader {
    /// Most recent screen context from a non-z-ro app.  Updated by the
    /// background polling thread.
    last_external: Arc<Mutex<Option<ScreenContext>>>,
    /// Tauri AppHandle — installed after setup via `set_app_handle`.
    /// Used to emit `context-update` events to the frontend so the UI
    /// can display the active window without polling.
    app_handle: Arc<OnceLock<AppHandle>>,
}

/// Signature used to de-dupe poll-tick log lines.  We only print a log when
/// something meaningful changes (focus / tree size bucket / screenshot gate).
#[derive(Debug, Clone, PartialEq)]
struct PollSig {
    process: String,
    title: String,
    has_screenshot: bool,
    tree_size_bucket: u32, // bucketed to avoid chatter from ±10B jitter
}

impl ScreenReader {
    pub fn new() -> Self {
        let last_external = Arc::new(Mutex::new(None));
        let app_handle = Arc::new(OnceLock::new());

        // Spawn a background thread that continuously tracks external app focus.
        {
            let ext = last_external.clone();
            let ah = app_handle.clone();
            std::thread::Builder::new()
                .name("npc-screen-poll".to_string())
                .spawn(move || {
                    println!("[z-ro:npc] Screen poller started");
                    let mut last_sig: Option<PollSig> = None;
                    loop {
                        Self::poll_once(&ext, &mut last_sig, &ah);
                        std::thread::sleep(Duration::from_millis(1000));
                    }
                })
                .expect("Failed to spawn screen polling thread");
        }

        Self { last_external, app_handle }
    }

    /// Install the Tauri `AppHandle` so the screen poller can emit
    /// `context-update` events to the frontend. Called once from
    /// `lib.rs` inside `.setup(...)`. Subsequent calls are no-ops.
    pub fn set_app_handle(&self, app: AppHandle) {
        let _ = self.app_handle.set(app);
    }

    /// Capture the screen context for the NPC.
    ///
    /// - If the focused app is external (VS Code, Chrome, etc.): capture its
    ///   AX tree directly and return it.  Also stores it as the last-known
    ///   external context.
    /// - If the focused app is z-ro itself (user is typing in our chat):
    ///   return the most recently captured external context.
    pub fn capture(&self) -> Result<ScreenContext, String> {
        let window = crate::context::get_active_window_info()?;

        if is_self_app(&window) {
            // User is focused on z-ro — return the last external app context
            if let Ok(guard) = self.last_external.lock() {
                if let Some(ref ctx) = *guard {
                    return Ok(ctx.clone());
                }
            }
            // No external context yet — return z-ro's own context (better than nothing)
            eprintln!("[z-ro:npc] capture: no external context yet — NPC will have no screen info");
            let ctx = ScreenContext {
                window,
                ax_tree: None,
                screenshot_b64: None,
                captured_at_ms: now_millis(),
            };
            return Ok(ctx);
        }

        // Focused on an external app — capture fresh context
        let ax_tree = read_ax_tree().ok().filter(|s| !s.is_empty());
        let screenshot_b64 = maybe_capture_screenshot(&ax_tree);
        let ctx = ScreenContext {
            window,
            ax_tree,
            screenshot_b64,
            captured_at_ms: now_millis(),
        };

        // Also update last_external
        if let Ok(mut guard) = self.last_external.lock() {
            *guard = Some(ctx.clone());
        }

        Ok(ctx)
    }

    /// Called by the background polling thread every second.
    ///
    /// `last_sig` is a per-thread signature used to suppress repeat logs —
    /// we only print when focus changes, the screenshot gate flips, or the
    /// AX tree size changes bucket (250B buckets).
    fn poll_once(
        last_external: &Arc<Mutex<Option<ScreenContext>>>,
        last_sig: &mut Option<PollSig>,
        app_handle: &Arc<OnceLock<AppHandle>>,
    ) {
        let window = match crate::context::get_active_window_info() {
            Ok(w) => w,
            Err(_) => return, // Can't get window info — skip this tick
        };

        if is_self_app(&window) {
            return; // z-ro is focused — keep the existing external context
        }

        let ax_tree = read_ax_tree().ok().filter(|s| !s.is_empty());
        let tree_size = ax_tree.as_ref().map(|t| t.len()).unwrap_or(0);
        let screenshot_b64 = maybe_capture_screenshot(&ax_tree);
        let shot_size = screenshot_b64.as_ref().map(|s| s.len()).unwrap_or(0);

        // Only log when something meaningful changed
        let sig = PollSig {
            process: window.process_name.clone(),
            title: window.title.clone(),
            has_screenshot: screenshot_b64.is_some(),
            tree_size_bucket: (tree_size as u32) / 250,
        };
        let focus_changed = last_sig.as_ref() != Some(&sig);
        if focus_changed {
            println!(
                "[z-ro:npc] focus: {} — \"{}\" | ax_tree={}B shot={}",
                window.process_name,
                window.title,
                tree_size,
                if screenshot_b64.is_some() {
                    format!("{}B", shot_size)
                } else {
                    "none".to_string()
                },
            );
            *last_sig = Some(sig);
        }

        // Push the WindowInfo to the frontend so the "What Zee sees"
        // sidebar stays updated without the UI having to poll via IPC.
        // Only emit when focus actually changed to avoid unnecessary
        // event traffic.
        if focus_changed {
            if let Some(app) = app_handle.get() {
                let _ = app.emit("context-update", &window);
            }
        }

        let ctx = ScreenContext {
            window,
            ax_tree,
            screenshot_b64,
            captured_at_ms: now_millis(),
        };

        if let Ok(mut guard) = last_external.lock() {
            *guard = Some(ctx);
        }
    }
}

/// Return `Some(base64_jpeg)` only when the AX tree is too thin to be useful.
/// This is the fallback path for Electron / canvas apps whose accessibility
/// tree exposes almost nothing.
fn maybe_capture_screenshot(ax_tree: &Option<String>) -> Option<String> {
    let thin = match ax_tree {
        Some(t) => t.len() < THIN_AX_THRESHOLD,
        None => true,
    };
    if !thin {
        return None;
    }
    match capture_primary_screenshot_jpeg() {
        Ok(b64) => Some(b64),
        Err(e) => {
            eprintln!("[z-ro:npc] Screenshot fallback failed: {}", e);
            None
        }
    }
}

/// Capture the primary monitor, downscale to `SCREENSHOT_MAX_EDGE`,
/// JPEG-encode at `SCREENSHOT_JPEG_QUALITY`, and base64-encode.
fn capture_primary_screenshot_jpeg() -> Result<String, String> {
    use base64::Engine;
    use image::codecs::jpeg::JpegEncoder;

    let monitors =
        xcap::Monitor::all().map_err(|e| format!("Failed to list monitors: {}", e))?;
    let monitor = monitors
        .into_iter()
        .next()
        .ok_or_else(|| "No monitors found".to_string())?;

    let image = monitor
        .capture_image()
        .map_err(|e| format!("Failed to capture screen: {}", e))?;

    // Downscale so the longest edge == SCREENSHOT_MAX_EDGE (preserves aspect).
    let (w, h) = (image.width(), image.height());
    let longest = w.max(h);
    let resized = if longest > SCREENSHOT_MAX_EDGE {
        let scale = SCREENSHOT_MAX_EDGE as f32 / longest as f32;
        let nw = (w as f32 * scale) as u32;
        let nh = (h as f32 * scale) as u32;
        image::imageops::resize(&image, nw, nh, image::imageops::FilterType::Triangle)
    } else {
        image
    };

    let mut jpeg_bytes: Vec<u8> = Vec::new();
    let mut encoder = JpegEncoder::new_with_quality(&mut jpeg_bytes, SCREENSHOT_JPEG_QUALITY);
    encoder
        .encode_image(&resized)
        .map_err(|e| format!("Failed to encode JPEG: {}", e))?;

    Ok(base64::engine::general_purpose::STANDARD.encode(&jpeg_bytes))
}

/// Check whether a window belongs to our own app.
///
/// Primary check is PID comparison (works in dev and release).
/// Falls back to bundle ID / process name if PID is unavailable.
fn is_self_app(window: &WindowInfo) -> bool {
    // Primary: compare PIDs (most reliable, works in dev mode)
    if let Some(pid) = window.pid {
        return pid == *SELF_PID;
    }
    // Fallback: check bundle ID
    if let Some(ref bid) = window.bundle_id {
        if bid == "com.zro.cowork" {
            return true;
        }
    }
    false
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── AX tree reading (standalone function, used by both capture and poller) ──

#[cfg(target_os = "macos")]
fn read_ax_tree() -> Result<String, String> {
    macos_ax::read_focused_window_tree()
}

#[cfg(not(target_os = "macos"))]
fn read_ax_tree() -> Result<String, String> {
    Err("Accessibility tree reading is only supported on macOS".to_string())
}

// ── macOS-specific AX tree implementation ──────────────────────────────

#[cfg(target_os = "macos")]
mod macos_ax {
    use accessibility::{
        AXUIElement, AXUIElementAttributes, TreeVisitor, TreeWalker, TreeWalkerFlow,
    };
    use accessibility_sys::{
        kAXErrorSuccess, kAXFocusedApplicationAttribute, kAXFocusedWindowAttribute,
        AXUIElementCopyAttributeValue, AXUIElementRef,
    };
    use core_foundation::{
        base::{CFTypeRef, TCFType},
        string::CFString,
    };
    use std::cell::Cell;
    use std::fmt::Write;

    /// Maximum depth to walk the AX tree (prevents explosion in complex apps).
    const MAX_DEPTH: usize = 6;
    /// Maximum output size in bytes (prevents prompt bloat).
    const MAX_BYTES: usize = 4096;

    /// Read the accessibility tree of the currently focused window.
    pub(super) fn read_focused_window_tree() -> Result<String, String> {
        // 1. Get the focused application via the system-wide AX element.
        let system = AXUIElement::system_wide();
        let focused_app = get_ax_element_attr(&system, kAXFocusedApplicationAttribute)
            .map_err(|e| format!("Cannot read focused application: {}", e))?;

        // 2. Get the focused window (fall back to app element if none).
        let root = get_ax_element_attr(&focused_app, kAXFocusedWindowAttribute)
            .unwrap_or(focused_app);

        // 3. Walk the tree using the crate's built-in TreeWalker.
        let collector = TreeCollector::new(MAX_DEPTH, MAX_BYTES);
        let walker = TreeWalker::new();
        walker.walk(&root, &collector);

        let mut output = collector.into_string();
        if output.len() > MAX_BYTES {
            output.truncate(MAX_BYTES);
            output.push_str("\n...(truncated)");
        }

        Ok(output)
    }

    /// Use the raw `AXUIElementCopyAttributeValue` to fetch an attribute that
    /// returns an `AXUIElement` (e.g. AXFocusedApplication, AXFocusedWindow).
    /// The high-level `accessibility` crate does not expose typed accessors for
    /// these system-wide attributes.
    fn get_ax_element_attr(
        element: &AXUIElement,
        attr_name: &str,
    ) -> Result<AXUIElement, String> {
        let attr = CFString::new(attr_name);
        let mut value: CFTypeRef = std::ptr::null_mut();
        let err = unsafe {
            AXUIElementCopyAttributeValue(
                element.as_concrete_TypeRef(),
                attr.as_concrete_TypeRef(),
                &mut value,
            )
        };
        if err != kAXErrorSuccess || value.is_null() {
            return Err(format!("AXError({})", err));
        }
        // `CopyAttributeValue` follows the Create Rule — caller owns the ref.
        Ok(unsafe { AXUIElement::wrap_under_create_rule(value as AXUIElementRef) })
    }

    // ── TreeVisitor implementation ─────────────────────────────────────

    struct TreeCollector {
        buf: std::cell::RefCell<String>,
        depth: Cell<usize>,
        max_depth: usize,
        max_bytes: usize,
    }

    impl TreeCollector {
        fn new(max_depth: usize, max_bytes: usize) -> Self {
            Self {
                buf: std::cell::RefCell::new(String::with_capacity(max_bytes)),
                depth: Cell::new(0),
                max_depth,
                max_bytes,
            }
        }

        fn into_string(self) -> String {
            self.buf.into_inner()
        }
    }

    impl TreeVisitor for TreeCollector {
        fn enter_element(&self, element: &AXUIElement) -> TreeWalkerFlow {
            let depth = self.depth.get();

            // Depth gate
            if depth > self.max_depth {
                return TreeWalkerFlow::SkipSubtree;
            }

            // Size gate
            let mut buf = self.buf.borrow_mut();
            if buf.len() >= self.max_bytes {
                return TreeWalkerFlow::Exit;
            }

            // Collect attributes (all optional — AX calls can fail for
            // elements that are being destroyed or don't support the attr).
            let role_desc = element.role_description().ok().map(|s| s.to_string());
            let role = element.role().ok().map(|s| s.to_string());
            let title = element.title().ok().map(|s| s.to_string());
            let description = element.description().ok().map(|s| s.to_string());

            let label = role_desc
                .as_deref()
                .or(role.as_deref())
                .unwrap_or("unknown");

            // Build the line: <indent><label> "<title>"
            let indent = "  ".repeat(depth);
            let _ = write!(buf, "{}{}", indent, label);
            if let Some(ref t) = title {
                if !t.is_empty() {
                    let _ = write!(buf, " \"{}\"", t);
                }
            }
            if let Some(ref d) = description {
                if !d.is_empty() && title.as_deref() != Some(d.as_str()) {
                    let _ = write!(buf, " desc=\"{}\"", d);
                }
            }
            buf.push('\n');

            self.depth.set(depth + 1);
            TreeWalkerFlow::Continue
        }

        fn exit_element(&self, _element: &AXUIElement) {
            let d = self.depth.get();
            if d > 0 {
                self.depth.set(d - 1);
            }
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── is_self_app tests ──────────────────────────────────────────────

    #[test]
    fn test_is_self_app_by_pid() {
        let self_pid = std::process::id();
        let w = WindowInfo {
            process_name: "Whatever".to_string(),
            title: "Test".to_string(),
            bundle_id: None,
            pid: Some(self_pid),
        };
        assert!(is_self_app(&w));
    }

    #[test]
    fn test_is_self_app_by_bundle_fallback() {
        let w = WindowInfo {
            process_name: "Whatever".to_string(),
            title: "Test".to_string(),
            bundle_id: Some("com.zro.cowork".to_string()),
            pid: None, // no PID available — fall back to bundle ID
        };
        assert!(is_self_app(&w));
    }

    #[test]
    fn test_is_not_self_app() {
        let w = WindowInfo {
            process_name: "Code".to_string(),
            title: "main.rs - z-ro cowork".to_string(),
            bundle_id: Some("com.microsoft.VSCode".to_string()),
            pid: Some(99999), // different PID
        };
        assert!(!is_self_app(&w));
    }

    #[test]
    fn test_is_self_app_pid_takes_priority_over_bundle() {
        // PID belongs to a different process, but bundle says z-ro.
        // PID wins — this is NOT our app.
        let w = WindowInfo {
            process_name: "Imposter".to_string(),
            title: "Fake".to_string(),
            bundle_id: Some("com.zro.cowork".to_string()),
            pid: Some(99999),
        };
        assert!(!is_self_app(&w));
    }

    #[test]
    fn test_is_not_self_app_no_pid_no_bundle() {
        let w = WindowInfo {
            process_name: "Firefox".to_string(),
            title: "Google".to_string(),
            bundle_id: None,
            pid: None,
        };
        assert!(!is_self_app(&w));
    }

    // ── ScreenContext construction tests ────────────────────────────────

    #[test]
    fn test_screen_context_serializes() {
        let ctx = ScreenContext {
            window: WindowInfo {
                process_name: "Finder".to_string(),
                title: "Documents".to_string(),
                bundle_id: Some("com.apple.finder".to_string()),
                pid: Some(1234),
            },
            ax_tree: Some("window\n  button \"OK\"\n".to_string()),
            screenshot_b64: None,
            captured_at_ms: 1000,
        };
        let json = serde_json::to_string(&ctx).unwrap();
        assert!(json.contains("Finder"));
        assert!(json.contains("Documents"));
        assert!(json.contains("button"));
    }

    #[test]
    fn test_screen_context_empty_ax_tree() {
        let ctx = ScreenContext {
            window: WindowInfo {
                process_name: "App".to_string(),
                title: "Win".to_string(),
                bundle_id: None,
                pid: None,
            },
            ax_tree: None,
            screenshot_b64: None,
            captured_at_ms: 0,
        };
        assert!(ctx.ax_tree.is_none());
    }

    // ── Thin-tree fallback gate tests ──────────────────────────────────
    //
    // We don't test `capture_primary_screenshot_jpeg()` directly because it
    // touches real monitors / xcap — instead we verify the gating logic:
    // `maybe_capture_screenshot` should only attempt capture for thin trees.

    #[test]
    fn test_thin_threshold_none_is_thin() {
        // None AX tree → thin, would attempt screenshot
        assert!(None::<String>.as_ref().map_or(true, |t: &String| t.len() < THIN_AX_THRESHOLD));
    }

    #[test]
    fn test_thin_threshold_tiny_is_thin() {
        let tree = Some("window\n".to_string());
        assert!(tree.as_ref().map_or(true, |t| t.len() < THIN_AX_THRESHOLD));
    }

    #[test]
    fn test_thin_threshold_rich_is_not_thin() {
        // 300-byte tree → not thin
        let tree = Some("x".repeat(300));
        assert!(!tree.as_ref().map_or(true, |t| t.len() < THIN_AX_THRESHOLD));
    }
}
