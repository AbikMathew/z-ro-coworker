#![cfg(target_os = "macos")]

//! Global OS event listener (mouse clicks + key presses) for the reactive
//! coordinator loop. Backed by `rdev::listen`, which spins its own Core
//! Foundation run loop on a dedicated thread.
//!
//! Permission model:
//! - Requires **Accessibility** permission on macOS: System Settings →
//!   Privacy & Security → Accessibility → add z-ro. Without it, `rdev::listen`
//!   returns and the listener thread exits; we log that clearly so the failure
//!   mode is diagnosable.
//! - No TCC prompt is fired automatically — macOS simply shows a one-time
//!   permission prompt the first time we call a privileged AX API. We expose
//!   a runtime probe via `is_accessibility_trusted()` so the frontend can
//!   explain the requirement if events aren't arriving.
//!
//! Threading:
//! - `rdev::listen` blocks forever; it's run on a dedicated `std::thread`.
//! - There is no clean way to stop `rdev::listen` once it's started. We set
//!   a shared "disarmed" flag that suppresses event fanout — the thread
//!   itself leaks until the process exits, which is fine for a long-running
//!   desktop app.
//! - Click coordinates aren't on the `ButtonPress` event — rdev only emits
//!   them via `MouseMove`. We track the last-known position in an atomic
//!   pair and look it up on each click.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

/// Events relayed to the coordinator. `at_ms` is unix-ms for cross-event
/// ordering; `x`/`y` are global screen pixels (rdev's convention).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NpcEvent {
    /// A mouse button was pressed. `x`/`y` are the last-observed cursor
    /// position at the moment of the click.
    Click { x: f64, y: f64, at_ms: u64 },
    /// A keyboard key was pressed. `key` is a debug-formatted variant name
    /// (e.g. `"Return"`, `"KeyA"`) — enough to spot "enter was pressed"
    /// without committing to a specific keymap abstraction.
    KeyPress { key: String, at_ms: u64 },
}

/// Broadcast-channel fanout for `NpcEvent`s. Cheap to clone — internally
/// an `Arc` to the sender. Subscribers call `.subscribe()` to get their
/// own receiver.
#[derive(Clone)]
pub struct EventBus {
    tx: broadcast::Sender<NpcEvent>,
    /// Flips `true` once `start()` has successfully spawned the listener
    /// thread. Flips `false` if that thread ever exits (e.g. on permission
    /// denial). Consumers poll this to decide whether to show a
    /// "grant permission" nudge in the UI.
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

    /// Start the listener on a background thread. Idempotent — a second
    /// call after a successful first is a no-op. Returns an error only if
    /// the thread itself can't be spawned; missing Accessibility permission
    /// surfaces later as `is_armed() == false` after a short delay.
    pub fn start(&self) -> Result<(), String> {
        if self.armed.swap(true, Ordering::AcqRel) {
            return Ok(()); // already armed
        }
        let tx = self.tx.clone();
        let armed = self.armed.clone();
        let last_x = Arc::new(AtomicU64::new(0));
        let last_y = Arc::new(AtomicU64::new(0));
        let lx = last_x.clone();
        let ly = last_y.clone();

        std::thread::Builder::new()
            .name("npc-event-listener".to_string())
            .spawn(move || {
                println!("[z-ro:events] listener thread starting — rdev::listen blocks");
                let listen_res = rdev::listen(move |event| {
                    match event.event_type {
                        rdev::EventType::MouseMove { x, y } => {
                            // Store positions as f64-in-u64 to stay lock-free.
                            lx.store(x.to_bits(), Ordering::Release);
                            ly.store(y.to_bits(), Ordering::Release);
                        }
                        rdev::EventType::ButtonPress(_) => {
                            let x = f64::from_bits(lx.load(Ordering::Acquire));
                            let y = f64::from_bits(ly.load(Ordering::Acquire));
                            // send() is infallible w.r.t. subscribers but
                            // can fail if the channel is closed — swallow.
                            let _ = tx.send(NpcEvent::Click {
                                x,
                                y,
                                at_ms: now_ms(),
                            });
                        }
                        rdev::EventType::KeyPress(k) => {
                            let _ = tx.send(NpcEvent::KeyPress {
                                key: format!("{k:?}"),
                                at_ms: now_ms(),
                            });
                        }
                        _ => {}
                    }
                });
                if let Err(e) = listen_res {
                    eprintln!(
                        "[z-ro:events] listener exited: {e:?} — Accessibility \
                         permission may be missing (System Settings → Privacy & \
                         Security → Accessibility → add z-ro)"
                    );
                }
                armed.store(false, Ordering::Release);
            })
            .map_err(|e| format!("spawn listener: {e}"))?;

        Ok(())
    }
}

/// Probe whether this process currently has macOS Accessibility permission.
/// Uses the `AXIsProcessTrusted` API — does NOT pop the system prompt.
/// Calls the frontend make to decide whether to show a "grant permission"
/// nudge in the UI.
pub fn is_accessibility_trusted() -> bool {
    // SAFETY: AXIsProcessTrusted takes no arguments, returns a Boolean,
    // has no side effects, and is safe to call from any thread.
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
        // Manually publish — exercising broadcast semantics without touching
        // the real listener thread.
        let bus = EventBus::new(16);
        let mut a = bus.subscribe();
        let mut b = bus.subscribe();

        bus.tx
            .send(NpcEvent::KeyPress {
                key: "Return".to_string(),
                at_ms: 1,
            })
            .unwrap();

        let ea = a.recv().await.unwrap();
        let eb = b.recv().await.unwrap();
        match (ea, eb) {
            (NpcEvent::KeyPress { key: ka, .. }, NpcEvent::KeyPress { key: kb, .. }) => {
                assert_eq!(ka, "Return");
                assert_eq!(kb, "Return");
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
}
