//! Barge-in / cancellation primitives for the NPC turn loop.
//!
//! The coordinator holds a single `InterruptController`. Each turn obtains a
//! fresh `InterruptHandle` that wraps a `Notify::notified()` future — when
//! the coordinator's public `interrupt()` method is called, the handle's
//! future wakes, the pipeline drops the LLM receiver (which causes the HTTP
//! SSE stream to be closed upstream), and the turn returns early.
//!
//! Why `Notify` rather than `AtomicBool`? A notify-based future integrates
//! naturally with `tokio::select!` in the streaming loop — no polling, no
//! spurious wake-ups, and the very first chunk after cancel is dropped
//! instead of being appended to the response.

use std::sync::Arc;

use tokio::sync::Notify;

/// Coordinator-scoped controller. One instance is kept on `NpcCoordinator`
/// for the life of the app; each turn calls `new_turn()` to get its handle.
pub struct InterruptController {
    notify: Arc<Notify>,
}

impl InterruptController {
    pub fn new() -> Self {
        Self {
            notify: Arc::new(Notify::new()),
        }
    }

    /// Begin a fresh turn. Returns a clone of the underlying `Notify` that
    /// the pipeline races against its LLM receiver.
    pub fn new_turn(&self) -> InterruptHandle {
        InterruptHandle {
            notify: self.notify.clone(),
        }
    }

    /// Signal every in-flight turn to cancel. Safe to call when no turn is
    /// running — it just wakes zero waiters.
    pub fn cancel(&self) {
        self.notify.notify_waiters();
    }
}

impl Default for InterruptController {
    fn default() -> Self {
        Self::new()
    }
}

/// Passed into `VoicePipeline::process_*` — the pipeline calls `cancelled()`
/// once to get a future it can `select!` against.
#[derive(Clone)]
pub struct InterruptHandle {
    notify: Arc<Notify>,
}

impl InterruptHandle {
    /// Wait until `InterruptController::cancel()` is called. The future is
    /// armed at the moment `cancelled()` is invoked; a cancel fired *before*
    /// that moment will NOT wake it (by design — each turn only cares about
    /// cancels that happen during its own lifetime).
    pub async fn cancelled(&self) {
        self.notify.notified().await;
    }

    /// Construct a handle whose future never completes — useful in tests
    /// and one-shot contexts where the caller is not wiring cancellation.
    /// The backing `Notify` has no siblings, so `cancel()` is impossible.
    pub fn never_cancels() -> Self {
        Self {
            notify: Arc::new(Notify::new()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn cancel_wakes_waiter_immediately() {
        let ctrl = InterruptController::new();
        let handle = ctrl.new_turn();

        let cancel_task = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(10)).await;
            ctrl.cancel();
        });

        // Should return in ~10ms, not block forever.
        let waited = tokio::time::timeout(Duration::from_secs(1), handle.cancelled()).await;
        assert!(waited.is_ok(), "cancelled() did not resolve after cancel()");
        cancel_task.await.unwrap();
    }

    #[tokio::test]
    async fn no_cancel_means_future_pends() {
        let ctrl = InterruptController::new();
        let handle = ctrl.new_turn();

        let waited = tokio::time::timeout(Duration::from_millis(50), handle.cancelled()).await;
        assert!(waited.is_err(), "cancelled() resolved without a cancel");
    }

    #[tokio::test]
    async fn cancel_before_turn_does_not_affect_next_turn() {
        let ctrl = InterruptController::new();
        // Fire a stray cancel with no waiters.
        ctrl.cancel();
        // New turn's handle should still be fully armed.
        let handle = ctrl.new_turn();
        let waited = tokio::time::timeout(Duration::from_millis(50), handle.cancelled()).await;
        assert!(waited.is_err(), "stray cancel leaked into next turn");
    }
}
