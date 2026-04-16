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

    /// Append a turn. If we exceed `max_turns`, the oldest is evicted.
    pub fn push(&mut self, turn: ConversationTurn) {
        self.turns.push(turn);
        if self.turns.len() > self.max_turns {
            self.turns.remove(0);
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
