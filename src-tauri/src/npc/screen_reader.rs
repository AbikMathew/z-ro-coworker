use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tauri::{AppHandle, Emitter};

use crate::context::WindowInfo;
use crate::npc::frame_buffer::FrameBuffer;

/// Our own PID, captured at process start.  Compared against the focused
/// app's PID to reliably detect when z-ro itself is focused — regardless of
/// how the process name or bundle ID appear in dev vs release builds.
static SELF_PID: std::sync::LazyLock<u32> = std::sync::LazyLock::new(std::process::id);

/// Minimum AX tree length (in bytes) below which we consider the tree "thin"
/// and fall back to a screenshot on idle (no-task) turns. When a task is
/// active the screenshot is always attached — accuracy matters more than
/// tokens for guidance.  Electron / canvas-heavy apps often return < 200
/// bytes (just a top-level window element with no children).
const THIN_AX_THRESHOLD: usize = 200;

/// JPEG quality for screenshots pulled from the SCK FrameBuffer. Bumped
/// from 60 to 80 so Haiku can resolve dense-UI targets from the image.
const SCREENSHOT_JPEG_QUALITY: u8 = 80;

/// Snapshot of the user's screen at a point in time.
///
/// Two context sources are available: the accessibility tree (`ax_tree`) —
/// fast and compact but unreliable in Electron / canvas / browser apps —
/// and the screenshot (`screenshot_b64`) pulled from the SCK frame ring.
///
/// During an active task the screenshot is always attached. Idle turns only
/// attach it when the AX tree is thin (< 200 B).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScreenContext {
    /// Focused window metadata (app name, window title, bundle id).
    pub window: WindowInfo,
    /// Indented text representation of the window's AX tree.
    /// Format: each line is `<indent><role_description> "<title>"`.
    /// Depth-limited to 6 levels, size-capped at 4 KB.
    pub ax_tree: Option<String>,
    /// Base64-encoded JPEG screenshot (quality 80) from the SCK frame ring.
    pub screenshot_b64: Option<String>,
    /// Pixel width of the captured screenshot — matches SCK's configured
    /// output width. None when no capture session is live.
    #[serde(default)]
    pub capture_width_px: Option<u32>,
    /// Pixel height of the captured screenshot.
    #[serde(default)]
    pub capture_height_px: Option<u32>,
    /// Primary monitor width in logical points (physical px / scale). None
    /// when the AppHandle isn't installed or the monitor can't be resolved.
    #[serde(default)]
    pub monitor_width_pt: Option<f64>,
    /// Primary monitor height in logical points.
    #[serde(default)]
    pub monitor_height_pt: Option<f64>,
    /// Backing scale factor the screenshot was captured at. SCK delivers
    /// pre-scaled frames (1.0 unless the capture path changes).
    #[serde(default)]
    pub scale_factor: Option<f32>,
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
    /// Shared ring of frames produced by the SCK capture stream. Empty until
    /// the user picks a source via the native picker; the NPC silently runs
    /// without visual context until then (AX tree only).
    frame_buffer: FrameBuffer,
    /// True whenever the coordinator has an active task. Used by `capture()`
    /// to always attach a screenshot during a task so the LLM can place
    /// arrows precisely, even in AX-rich apps.
    task_active: Arc<AtomicBool>,
}

/// Signature used to de-dupe poll-tick log lines.  We only print a log when
/// something meaningful changes (focus / tree size bucket / screenshot gate).
#[derive(Debug, Clone, PartialEq)]
struct PollSig {
    process: String,
    title: String,
    tree_size_bucket: u32, // bucketed to avoid chatter from ±10B jitter
}

impl ScreenReader {
    pub fn new(frame_buffer: FrameBuffer) -> Self {
        let last_external = Arc::new(Mutex::new(None));
        let app_handle = Arc::new(OnceLock::new());
        let task_active = Arc::new(AtomicBool::new(false));

        // Background thread tracks external app focus so the LLM always has
        // AX-tree context from the window the user was last interacting with.
        // It does NOT capture screenshots — that job belongs to the SCK
        // stream which fills `frame_buffer` continuously.
        {
            let ext = last_external.clone();
            let ah = app_handle.clone();
            std::thread::Builder::new()
                .name("npc-screen-poll".to_string())
                .spawn(move || {
                    println!("[z-ro:npc] Screen poller started (AX-tree only)");
                    let mut last_sig: Option<PollSig> = None;
                    loop {
                        Self::poll_once(&ext, &mut last_sig, &ah);
                        std::thread::sleep(Duration::from_millis(1000));
                    }
                })
                .expect("Failed to spawn screen polling thread");
        }

        Self { last_external, app_handle, frame_buffer, task_active }
    }

    /// Install the Tauri `AppHandle` so the screen poller can emit
    /// `context-update` events to the frontend. Called once from
    /// `lib.rs` inside `.setup(...)`. Subsequent calls are no-ops.
    pub fn set_app_handle(&self, app: AppHandle) {
        let _ = self.app_handle.set(app);
    }

    /// Signal to the screen reader whether a task is currently active. When
    /// `true`, `capture()` always attaches the latest screenshot regardless
    /// of AX-tree thickness, so guidance arrows can be placed accurately in
    /// AX-poor apps (VS Code, browsers, canvas). Called by the coordinator
    /// whenever it reads task state for an NPC turn.
    pub fn set_task_active(&self, active: bool) {
        self.task_active.store(active, Ordering::Relaxed);
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

        let (window, ax_tree) = if is_self_app(&window) {
            // User is focused on z-ro — fall back to the AX tree + window of
            // the last external app they interacted with.
            if let Ok(guard) = self.last_external.lock() {
                if let Some(ref prev) = *guard {
                    (prev.window.clone(), prev.ax_tree.clone())
                } else {
                    eprintln!(
                        "[z-ro:npc] capture: no external context yet — \
                         NPC will have no AX context"
                    );
                    (window, None)
                }
            } else {
                (window, None)
            }
        } else {
            let ax = read_ax_tree().ok().filter(|s| !s.is_empty());
            (window, ax)
        };

        // Pull the latest frame ONCE — we use it for both the screenshot
        // and the pixel-dimension metadata so the two can never drift.
        let latest_frame = self.frame_buffer.latest();
        let task_active = self.task_active.load(Ordering::Relaxed);
        let thin_ax = ax_tree.as_ref().map_or(true, |t| t.len() < THIN_AX_THRESHOLD);
        // Attach the image when a task is active (guidance needs it) OR
        // when the AX tree is too thin to drive coord placement alone.
        let want_screenshot = task_active || thin_ax;

        let screenshot_b64 = if want_screenshot {
            latest_frame.as_ref().and_then(|f| {
                match f.to_jpeg_base64(SCREENSHOT_JPEG_QUALITY) {
                    Ok(b64) => Some(b64),
                    Err(e) => {
                        eprintln!("[z-ro:npc] Frame→JPEG failed: {e}");
                        None
                    }
                }
            })
        } else {
            None
        };

        let (capture_width_px, capture_height_px, scale_factor) = latest_frame
            .as_ref()
            .map(|f| (Some(f.width), Some(f.height), Some(f.scale_factor)))
            .unwrap_or((None, None, None));
        let (monitor_width_pt, monitor_height_pt) = self.read_monitor_size();

        let ctx = ScreenContext {
            window: window.clone(),
            ax_tree: ax_tree.clone(),
            screenshot_b64,
            capture_width_px,
            capture_height_px,
            monitor_width_pt,
            monitor_height_pt,
            scale_factor,
            captured_at_ms: now_millis(),
        };

        // Refresh last_external so subsequent "z-ro is focused" turns still
        // see fresh AX context. Strip ephemeral fields (screenshot + dims)
        // since the live capture path will repopulate them from the ring.
        if !is_self_app(&ctx.window) {
            if let Ok(mut guard) = self.last_external.lock() {
                *guard = Some(ScreenContext {
                    window: ctx.window.clone(),
                    ax_tree: ctx.ax_tree.clone(),
                    captured_at_ms: ctx.captured_at_ms,
                    ..Default::default()
                });
            }
        }

        Ok(ctx)
    }

    /// Look up the primary monitor size in logical points via the installed
    /// Tauri `AppHandle`. Returns `(None, None)` when the handle isn't
    /// installed yet or when the monitor can't be resolved.
    fn read_monitor_size(&self) -> (Option<f64>, Option<f64>) {
        let Some(app) = self.app_handle.get() else {
            return (None, None);
        };
        #[cfg(target_os = "macos")]
        {
            use tauri::Manager;
            match app.primary_monitor() {
                Ok(Some(m)) => {
                    let scale = m.scale_factor();
                    let phys = m.size();
                    if scale > 0.0 {
                        let mw = phys.width as f64 / scale;
                        let mh = phys.height as f64 / scale;
                        (Some(mw), Some(mh))
                    } else {
                        (None, None)
                    }
                }
                _ => (None, None),
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = app;
            (None, None)
        }
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

        // Only log when something meaningful changed
        let sig = PollSig {
            process: window.process_name.clone(),
            title: window.title.clone(),
            tree_size_bucket: (tree_size as u32) / 250,
        };
        let focus_changed = last_sig.as_ref() != Some(&sig);
        if focus_changed {
            println!(
                "[z-ro:npc] focus: {} — \"{}\" | ax_tree={tree_size}B",
                window.process_name, window.title,
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

        // Cache AX context for "z-ro is focused" turns. Screenshots + dim
        // metadata come from the SCK FrameBuffer at capture() time, not here.
        let ctx = ScreenContext {
            window,
            ax_tree,
            captured_at_ms: now_millis(),
            ..Default::default()
        };

        if let Ok(mut guard) = last_external.lock() {
            *guard = Some(ctx);
        }
    }
}

// The legacy xcap-based screenshot path was removed in Phase 2. The SCK
// FrameBuffer is now the single source of truth for screen pixels — frames
// arrive there at 2 fps whenever a capture session is live.

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
            captured_at_ms: 1000,
            ..Default::default()
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
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(ctx.ax_tree.is_none());
        assert!(ctx.capture_width_px.is_none());
        assert!(ctx.monitor_width_pt.is_none());
    }

    #[test]
    fn test_screen_context_roundtrip_with_capture_metadata() {
        // New in Phase 1: capture dims + monitor dims get serialized so the
        // prompt builder can pass them to the LLM.
        let ctx = ScreenContext {
            window: WindowInfo::default(),
            ax_tree: Some("tree".to_string()),
            screenshot_b64: Some("img".to_string()),
            capture_width_px: Some(1440),
            capture_height_px: Some(900),
            monitor_width_pt: Some(1512.0),
            monitor_height_pt: Some(982.0),
            scale_factor: Some(2.0),
            captured_at_ms: 42,
        };
        let json = serde_json::to_string(&ctx).unwrap();
        let back: ScreenContext = serde_json::from_str(&json).unwrap();
        assert_eq!(back.capture_width_px, Some(1440));
        assert_eq!(back.capture_height_px, Some(900));
        assert_eq!(back.monitor_width_pt, Some(1512.0));
        assert_eq!(back.scale_factor, Some(2.0));
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
