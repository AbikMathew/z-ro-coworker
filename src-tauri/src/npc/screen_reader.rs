use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::context::WindowInfo;

/// Our own PID, captured at process start.  Compared against the focused
/// app's PID to reliably detect when z-ro itself is focused — regardless of
/// how the process name or bundle ID appear in dev vs release builds.
static SELF_PID: std::sync::LazyLock<u32> = std::sync::LazyLock::new(std::process::id);

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
    /// Base64-encoded JPEG screenshot (low quality, ~50 KB).  Only present
    /// when the AX tree is empty/unavailable.
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
}

impl ScreenReader {
    pub fn new() -> Self {
        let last_external = Arc::new(Mutex::new(None));

        // Spawn a background thread that continuously tracks external app focus.
        {
            let ext = last_external.clone();
            std::thread::Builder::new()
                .name("npc-screen-poll".to_string())
                .spawn(move || {
                    println!("[z-ro:npc] Screen poller started");
                    loop {
                        Self::poll_once(&ext);
                        std::thread::sleep(Duration::from_millis(1000));
                    }
                })
                .expect("Failed to spawn screen polling thread");
        }

        Self { last_external }
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
                    println!(
                        "[z-ro:npc] Self-focused → returning last external: {} - \"{}\"",
                        ctx.window.process_name, ctx.window.title
                    );
                    return Ok(ctx.clone());
                }
            }
            // No external context yet — return z-ro's own context (better than nothing)
            println!("[z-ro:npc] Self-focused but no external context captured yet");
            let ctx = ScreenContext {
                window,
                ax_tree: None,
                screenshot_b64: None,
                captured_at_ms: now_millis(),
            };
            return Ok(ctx);
        }

        // Focused on an external app — capture fresh context
        let ax_tree = read_ax_tree();
        let ctx = ScreenContext {
            window,
            ax_tree: ax_tree.ok().filter(|s| !s.is_empty()),
            screenshot_b64: None,
            captured_at_ms: now_millis(),
        };

        // Also update last_external
        if let Ok(mut guard) = self.last_external.lock() {
            *guard = Some(ctx.clone());
        }

        Ok(ctx)
    }

    /// Called by the background polling thread every second.
    fn poll_once(last_external: &Arc<Mutex<Option<ScreenContext>>>) {
        let window = match crate::context::get_active_window_info() {
            Ok(w) => w,
            Err(_) => return, // Can't get window info — skip this tick
        };

        if is_self_app(&window) {
            return; // z-ro is focused — keep the existing external context
        }

        let ax_tree = read_ax_tree();
        let tree_size = ax_tree.as_ref().map(|t| t.len()).unwrap_or(0);
        println!(
            "[z-ro:npc] poll: external app={} pid={:?} title=\"{}\" ax_tree={}B",
            window.process_name,
            window.pid,
            window.title,
            tree_size,
        );

        let ctx = ScreenContext {
            window,
            ax_tree: ax_tree.ok().filter(|s| !s.is_empty()),
            screenshot_b64: None,
            captured_at_ms: now_millis(),
        };

        if let Ok(mut guard) = last_external.lock() {
            *guard = Some(ctx);
        }
    }
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
}
