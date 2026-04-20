#![cfg(target_os = "macos")]

//! Global OS event listener (mouse clicks + key presses) for the reactive
//! coordinator loop. Uses Apple's `CGEventTap` via `core_graphics`'s safe
//! wrapper — not `rdev`.
//!
//! Why not `rdev`? On macOS 15+, `rdev::listen` crashes with
//! `EXC_BREAKPOINT` → `dispatch_assert_queue_fail` the first time the
//! user types anything after a tap is active. `rdev` calls
//! `TSMGetInputSourceProperty` from the event-tap worker thread to
//! resolve keycodes into human-readable strings (the `event.name`
//! field), and the Text Services Manager now hard-asserts that API is
//! main-thread-only. The crate is effectively unmaintained, so we skip
//! it.
//!
//! We don't need keycode-to-character translation — the reactive loop
//! just needs "something happened" signals, not the literal 'a'. So we
//! forward the raw macOS virtual keycode and let consumers decide.
//!
//! Permission model:
//! - Requires **Accessibility** permission: System Settings → Privacy &
//!   Security → Accessibility → add z-ro. `CGEventTap::new` returns
//!   `Err(())` when permission is missing, which we surface via the
//!   `armed` flag so the frontend can nudge the user.
//!
//! Threading:
//! - We spawn a dedicated thread, create the tap there, add its mach
//!   port source to the thread-local `CFRunLoop`, and call
//!   `CFRunLoop::run_current()`. That blocks forever; there's no
//!   clean teardown in v1 — the thread leaks until the process exits,
//!   which matches rdev's behaviour and is fine for a desktop app.
//! - The tap callback fires on the run-loop thread, NOT a tokio thread.
//!   Everything in the callback is synchronous; it uses
//!   `broadcast::Sender::send` which doesn't await.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use core_foundation::runloop::{kCFRunLoopCommonModes, CFRunLoop};
use core_graphics::event::{
    CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CGEventType,
    EventField,
};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

/// Events relayed to the coordinator. `at_ms` is unix-ms for cross-event
/// ordering; `x`/`y` are global screen pixels.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NpcEvent {
    /// A mouse button was pressed. `x`/`y` come straight from
    /// `CGEvent::location()`.
    Click { x: f64, y: f64, at_ms: u64 },
    /// A keyboard key was pressed. `keycode` is the raw macOS virtual
    /// keycode (0–127). We deliberately skip character translation —
    /// calling `TSMGetInputSourceProperty` from this thread on macOS 15+
    /// hard-aborts the process via `dispatch_assert_queue_fail`.
    KeyPress { keycode: u16, at_ms: u64 },
}

/// Broadcast-channel fanout for `NpcEvent`s.
#[derive(Clone)]
pub struct EventBus {
    tx: broadcast::Sender<NpcEvent>,
    armed: Arc<AtomicBool>,
}

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        let (tx, _rx) = broadcast::channel(capacity);
        Self {
            tx,
            armed: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<NpcEvent> {
        self.tx.subscribe()
    }

    pub fn is_armed(&self) -> bool {
        self.armed.load(Ordering::Acquire)
    }

    /// Spawn the event-tap thread. Idempotent — a second call after a
    /// successful first is a no-op. Returns an error only if the OS
    /// thread spawn fails; Accessibility-permission denial surfaces as
    /// `is_armed() == false` after the tap thread exits silently.
    pub fn start(&self) -> Result<(), String> {
        if self.armed.swap(true, Ordering::AcqRel) {
            return Ok(());
        }
        let tx = self.tx.clone();
        let armed = self.armed.clone();
        std::thread::Builder::new()
            .name("npc-event-tap".to_string())
            .spawn(move || run_event_tap(tx, armed))
            .map_err(|e| format!("spawn event-tap thread: {e}"))?;
        Ok(())
    }
}

/// Body of the event-tap thread. Creates the tap, attaches it to the
/// thread-local CF run loop, and blocks in `CFRunLoop::run_current()`.
/// If permission is missing or the tap can't be created, logs and
/// clears `armed` so the frontend can show the "grant permission" nudge.
fn run_event_tap(tx: broadcast::Sender<NpcEvent>, armed: Arc<AtomicBool>) {
    println!("[z-ro:events] event-tap thread starting");

    let tap = CGEventTap::new(
        CGEventTapLocation::HID,
        CGEventTapPlacement::HeadInsertEventTap,
        CGEventTapOptions::ListenOnly,
        vec![
            CGEventType::LeftMouseDown,
            CGEventType::RightMouseDown,
            CGEventType::OtherMouseDown,
            CGEventType::KeyDown,
        ],
        move |_proxy, etype, event| {
            // Extract the fields we care about and forward. Do NOT call
            // TSM here — it's a main-thread-only API since macOS 15
            // and abort()s the process from this thread.
            let at_ms = now_ms();
            let npc_event = match etype {
                CGEventType::LeftMouseDown
                | CGEventType::RightMouseDown
                | CGEventType::OtherMouseDown => {
                    let p = event.location();
                    Some(NpcEvent::Click {
                        x: p.x,
                        y: p.y,
                        at_ms,
                    })
                }
                CGEventType::KeyDown => {
                    let kc = event
                        .get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE)
                        as u16;
                    Some(NpcEvent::KeyPress { keycode: kc, at_ms })
                }
                _ => None,
            };
            if let Some(e) = npc_event {
                // Ignore send errors — no subscribers yet is fine, we
                // still want the OS event to pass through.
                let _ = tx.send(e);
            }
            // `None` = pass the event through unchanged. Returning an
            // event here would be re-inserted into the stream, which
            // would be weird for a ListenOnly tap.
            None
        },
    );

    let tap = match tap {
        Ok(t) => t,
        Err(()) => {
            eprintln!(
                "[z-ro:events] CGEventTap::new failed — likely missing \
                 Accessibility permission. Grant it in System Settings → \
                 Privacy & Security → Accessibility, then restart the app."
            );
            armed.store(false, Ordering::Release);
            return;
        }
    };

    // SAFETY: CGEventTap's docstring example uses an unsafe block here
    // because `create_runloop_source` / `add_source` are on raw
    // core_foundation types. The invariants are trivial: the source
    // comes from our mach port which we own, and the run loop is the
    // thread-local one which lives until the thread exits.
    unsafe {
        let loop_source = match tap.mach_port.create_runloop_source(0) {
            Ok(s) => s,
            Err(_) => {
                eprintln!("[z-ro:events] create_runloop_source failed");
                armed.store(false, Ordering::Release);
                return;
            }
        };
        CFRunLoop::get_current().add_source(&loop_source, kCFRunLoopCommonModes);
        tap.enable();
        println!("[z-ro:events] tap installed — entering run loop");
        CFRunLoop::run_current();
    }

    // Unreachable in practice — `run_current` blocks forever.
    armed.store(false, Ordering::Release);
}

/// Probe whether this process currently has macOS Accessibility
/// permission. Does NOT pop the system prompt.
pub fn is_accessibility_trusted() -> bool {
    extern "C" {
        fn AXIsProcessTrusted() -> bool;
    }
    unsafe { AXIsProcessTrusted() }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn new_event_bus_starts_disarmed() {
        let bus = EventBus::new(16);
        assert!(!bus.is_armed());
    }

    #[tokio::test]
    async fn subscribers_receive_independently() {
        let bus = EventBus::new(16);
        let mut a = bus.subscribe();
        let mut b = bus.subscribe();
        bus.tx
            .send(NpcEvent::KeyPress {
                keycode: 36,
                at_ms: 1,
            })
            .unwrap();
        match (a.recv().await.unwrap(), b.recv().await.unwrap()) {
            (
                NpcEvent::KeyPress { keycode: ka, .. },
                NpcEvent::KeyPress { keycode: kb, .. },
            ) => {
                assert_eq!(ka, 36);
                assert_eq!(kb, 36);
            }
            _ => panic!("wrong event variant"),
        }
    }

    #[test]
    fn serde_round_trip_click() {
        let e = NpcEvent::Click {
            x: 512.5,
            y: 320.25,
            at_ms: 1_700_000_000_000,
        };
        let j = serde_json::to_string(&e).unwrap();
        assert!(j.contains("click"), "got {j}");
        let back: NpcEvent = serde_json::from_str(&j).unwrap();
        match back {
            NpcEvent::Click { x, y, at_ms } => {
                assert!((x - 512.5).abs() < 1e-6);
                assert!((y - 320.25).abs() < 1e-6);
                assert_eq!(at_ms, 1_700_000_000_000);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn serde_round_trip_key_press() {
        let e = NpcEvent::KeyPress {
            keycode: 36,
            at_ms: 1,
        };
        let j = serde_json::to_string(&e).unwrap();
        assert!(j.contains("key_press"), "got {j}");
        assert!(j.contains("36"));
        let back: NpcEvent = serde_json::from_str(&j).unwrap();
        match back {
            NpcEvent::KeyPress { keycode: kc, at_ms } => {
                assert_eq!(kc, 36);
                assert_eq!(at_ms, 1);
            }
            _ => panic!("wrong variant"),
        }
    }
}
