use serde::{Deserialize, Serialize};

/// A single turn in the NPC conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationTurn {
    /// "user" or "assistant"
    pub role: String,
    /// Text content of the turn
    pub content: String,
    /// Unix timestamp in milliseconds
    pub timestamp_ms: u64,
    /// Brief summary of what was on screen at the time (for long-range context)
    pub screen_summary: Option<String>,
}

/// Rolling-window conversation memory.
///
/// Keeps the last `max_turns` turns so the prompt stays bounded. Older turns
/// are silently dropped — if we ever need long-range recall, Phase 3's
/// screenpipe integration fills that gap.
pub struct ConversationMemory {
    turns: Vec<ConversationTurn>,
    max_turns: usize,
}

impl ConversationMemory {
    pub fn new(max_turns: usize) -> Self {
        Self {
            turns: Vec::with_capacity(max_turns),
            max_turns,
        }
    }

    /// Append a turn. If we exceed `max_turns`, we evict the oldest
    /// short-filler turn from the existing history first ("ok", "yeah",
    /// "got it"); only if no fillers exist do we fall back to dropping
    /// the oldest. This matters once continuous on-air mode lands in
    /// Phase 2 — VAD turns fill the buffer in roughly a minute and we
    /// don't want every substantive turn to get purged along with them.
    pub fn push(&mut self, turn: ConversationTurn) {
        self.turns.push(turn);
        if self.turns.len() > self.max_turns {
            let idx = self
                .turns
                .iter()
                .position(is_short_filler)
                .unwrap_or(0);
            self.turns.remove(idx);
        }
    }

    /// Return the last `n` turns (or fewer if the history is shorter).
    pub fn recent(&self, n: usize) -> &[ConversationTurn] {
        let start = self.turns.len().saturating_sub(n);
        &self.turns[start..]
    }

    /// Wipe the conversation (new session).
    pub fn clear(&mut self) {
        self.turns.clear();
    }

    pub fn len(&self) -> usize {
        self.turns.len()
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.turns.is_empty()
    }
}

/// Common single-word / short acknowledgements that carry no context worth
/// preserving. When the rolling-window history is full, these are evicted
/// before we start losing substantive turns.
fn is_short_filler(turn: &ConversationTurn) -> bool {
    let trimmed = turn.content.trim();
    if trimmed.is_empty() || trimmed.len() > 24 {
        return false;
    }
    let lower = trimmed.to_lowercase();
    let clean = lower.trim_end_matches(|c: char| ".!?,".contains(c));
    matches!(
        clean,
        "ok" | "okay"
            | "yeah"
            | "yes"
            | "yep"
            | "yup"
            | "no"
            | "nope"
            | "sure"
            | "right"
            | "got it"
            | "thanks"
            | "thank you"
            | "cool"
            | "nice"
            | "good"
            | "fine"
            | "alright"
            | "uh huh"
            | "mhm"
            | "k"
            | "done"
            | "next"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_turn(role: &str, content: &str) -> ConversationTurn {
        ConversationTurn {
            role: role.to_string(),
            content: content.to_string(),
            timestamp_ms: 0,
            screen_summary: None,
        }
    }

    #[test]
    fn test_push_and_eviction() {
        let mut mem = ConversationMemory::new(3);
        mem.push(make_turn("user", "one"));
        mem.push(make_turn("assistant", "two"));
        mem.push(make_turn("user", "three"));
        assert_eq!(mem.len(), 3);

        mem.push(make_turn("assistant", "four"));
        assert_eq!(mem.len(), 3);
        assert_eq!(mem.recent(10)[0].content, "two"); // "one" evicted
    }

    #[test]
    fn test_recent_fewer_than_requested() {
        let mut mem = ConversationMemory::new(20);
        mem.push(make_turn("user", "only"));
        assert_eq!(mem.recent(5).len(), 1);
    }

    #[test]
    fn test_clear() {
        let mut mem = ConversationMemory::new(20);
        mem.push(make_turn("user", "hello"));
        mem.push(make_turn("assistant", "hi"));
        mem.clear();
        assert!(mem.is_empty());
    }

    #[test]
    fn test_eviction_preserves_order() {
        let mut mem = ConversationMemory::new(3);
        for i in 1..=6 {
            mem.push(make_turn("user", &format!("msg-{}", i)));
        }
        assert_eq!(mem.len(), 3);
        let recent = mem.recent(10);
        assert_eq!(recent[0].content, "msg-4");
        assert_eq!(recent[1].content, "msg-5");
        assert_eq!(recent[2].content, "msg-6");
    }

    #[test]
    fn test_recent_exact_count() {
        let mut mem = ConversationMemory::new(20);
        for i in 1..=10 {
            mem.push(make_turn("user", &format!("msg-{}", i)));
        }
        let recent = mem.recent(3);
        assert_eq!(recent.len(), 3);
        assert_eq!(recent[0].content, "msg-8");
        assert_eq!(recent[1].content, "msg-9");
        assert_eq!(recent[2].content, "msg-10");
    }

    #[test]
    fn test_empty_memory() {
        let mem = ConversationMemory::new(10);
        assert!(mem.is_empty());
        assert_eq!(mem.len(), 0);
        assert_eq!(mem.recent(5).len(), 0);
    }

    #[test]
    fn test_max_turns_one() {
        let mut mem = ConversationMemory::new(1);
        mem.push(make_turn("user", "first"));
        mem.push(make_turn("assistant", "second"));
        assert_eq!(mem.len(), 1);
        assert_eq!(mem.recent(10)[0].content, "second");
    }

    #[test]
    fn test_filler_evicted_before_substantive_turns() {
        // When the buffer is full, a short acknowledgement from earlier in
        // the conversation is dropped instead of the oldest substantive turn.
        let mut mem = ConversationMemory::new(3);
        mem.push(make_turn("user", "How do I save this file?"));
        mem.push(make_turn("assistant", "Ok")); // filler
        mem.push(make_turn("user", "What about undo?"));
        // Pushing a 4th exceeds max_turns — the filler "Ok" should go, not
        // the oldest substantive question.
        mem.push(make_turn("assistant", "Press Cmd+Z to undo."));

        let remaining: Vec<&str> = mem.recent(10).iter().map(|t| t.content.as_str()).collect();
        assert_eq!(remaining.len(), 3);
        assert!(remaining.contains(&"How do I save this file?"));
        assert!(remaining.contains(&"What about undo?"));
        assert!(remaining.contains(&"Press Cmd+Z to undo."));
        assert!(!remaining.iter().any(|c| *c == "Ok"));
    }

    #[test]
    fn test_multiple_fillers_oldest_filler_goes_first() {
        // Two fillers in history; the older one gets evicted on the first
        // overflow, preserving the newer filler (it may still be useful
        // context for what just happened).
        let mut mem = ConversationMemory::new(4);
        mem.push(make_turn("user", "yeah")); // older filler
        mem.push(make_turn("user", "Help me rename this"));
        mem.push(make_turn("assistant", "Tell me the new name."));
        mem.push(make_turn("user", "got it")); // newer filler — buffer full
        // Pushing one more overflows — "yeah" should go first (oldest filler).
        mem.push(make_turn("assistant", "Great. Let me know when done."));

        let remaining: Vec<&str> = mem.recent(10).iter().map(|t| t.content.as_str()).collect();
        assert_eq!(remaining.len(), 4);
        assert!(!remaining.contains(&"yeah"));
        assert!(remaining.contains(&"got it"));
        assert!(remaining.contains(&"Help me rename this"));
    }

    #[test]
    fn test_no_fillers_falls_back_to_fifo() {
        // If nothing matches the filler list, the oldest substantive turn
        // is evicted as before (preserves existing test_push_and_eviction).
        let mut mem = ConversationMemory::new(2);
        mem.push(make_turn("user", "First substantive question."));
        mem.push(make_turn("assistant", "A thoughtful answer."));
        mem.push(make_turn("user", "Second question."));
        let remaining: Vec<&str> = mem.recent(10).iter().map(|t| t.content.as_str()).collect();
        assert_eq!(remaining.len(), 2);
        assert!(!remaining.contains(&"First substantive question."));
    }

    #[test]
    fn test_filler_detection_is_punctuation_tolerant() {
        let mut mem = ConversationMemory::new(2);
        mem.push(make_turn("user", "Ok."));
        mem.push(make_turn("user", "An important question."));
        mem.push(make_turn("assistant", "An important answer."));
        let remaining: Vec<&str> = mem.recent(10).iter().map(|t| t.content.as_str()).collect();
        assert!(!remaining.contains(&"Ok."));
    }

    #[test]
    fn test_turn_fields_preserved() {
        let mut mem = ConversationMemory::new(10);
        mem.push(ConversationTurn {
            role: "user".to_string(),
            content: "hello".to_string(),
            timestamp_ms: 12345,
            screen_summary: Some("VS Code - main.rs".to_string()),
        });
        let turn = &mem.recent(1)[0];
        assert_eq!(turn.role, "user");
        assert_eq!(turn.timestamp_ms, 12345);
        assert_eq!(turn.screen_summary.as_deref(), Some("VS Code - main.rs"));
    }
}
