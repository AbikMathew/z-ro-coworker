use serde::{Deserialize, Serialize};
use std::fmt::Write;

use crate::task_engine::types::TaskState;

use super::conversation::ConversationTurn;
use super::screen_reader::ScreenContext;

/// A single message in the LLM conversation (OpenAI-style format).
///
/// `images_b64` is only used on the current user turn when the screen reader
/// falls back to a screenshot (thin AX tree apps like Electron / canvas).
/// History turns always leave it empty.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmMessage {
    pub role: String,
    pub content: String,
    /// Base64-encoded JPEG images to attach. When non-empty, providers that
    /// support vision use the multimodal content array format.
    #[serde(default)]
    pub images_b64: Vec<String>,
}

/// Assembles the full prompt (system + messages) for each NPC turn.
///
/// Responsibilities:
/// - System prompt with NPC personality rules
/// - Screen context injection (AX tree / window info)
/// - Task context injection (current step, hints, validation)
/// - Conversation history formatting
pub struct PromptBuilder;

impl PromptBuilder {
    pub fn new() -> Self {
        Self
    }

    /// Build the complete message list for the LLM.
    ///
    /// Returns `(system_prompt, messages)` where `messages` contains the
    /// conversation history plus the current user turn with injected context.
    pub fn build(
        &self,
        screen: &ScreenContext,
        task: Option<&TaskState>,
        history: &[ConversationTurn],
        user_input: &str,
    ) -> (String, Vec<LlmMessage>) {
        let system = self.system_prompt();

        let mut messages: Vec<LlmMessage> = Vec::with_capacity(history.len() + 1);

        // Append conversation history (text-only — images never kept in history)
        for turn in history {
            messages.push(LlmMessage {
                role: turn.role.clone(),
                content: turn.content.clone(),
                images_b64: Vec::new(),
            });
        }

        // Build the current user message with injected context.
        // Context blocks are prepended to the user's actual text so the LLM
        // sees them as part of the user turn (keeps assistant turns clean).
        let mut user_msg = String::new();
        user_msg.push_str(&self.format_screen_context(screen));
        if let Some(ts) = task {
            user_msg.push_str(&self.format_task_context(ts));
        }
        user_msg.push_str(user_input);

        // Attach the fallback screenshot (if any) to this user turn only.
        let images_b64 = screen
            .screenshot_b64
            .as_ref()
            .map(|b| vec![b.clone()])
            .unwrap_or_default();

        messages.push(LlmMessage {
            role: "user".to_string(),
            content: user_msg,
            images_b64,
        });

        (system, messages)
    }

    /// NPC personality + behavioural rules.
    fn system_prompt(&self) -> String {
        r#"You are Zee, a senior coworker at a tech company. You sit next to the student and help them learn workplace skills. You are friendly, casual, and supportive — like a colleague who has been here a few years and remembers what it was like to be new.

Rules:
1. NEVER give direct answers. Ask guiding questions instead.
   Bad: "Click File > Save As"
   Good: "Where do you usually save files in this app? Check the menu bar."

2. You can SEE the student's screen. A screenshot and/or UI tree is attached
   to every user message. ALWAYS use it — never say "I can't see" or "I can't
   access a screenshot". Reference what you see specifically, by exact label.
   Bad: "What app are you using?"
   Good: "I see you have VS Code open with main.py — nice."
   Good: "In the top-right I see a gear icon labeled Settings — try that."

3. Keep responses SHORT. You are talking, not writing an essay.
   Max 2-3 sentences per response. If they need more, they will ask.

4. You know the current task and step. Reference it naturally.
   Bad: "Your current task is 'Navigate the Filesystem', step 2 of 5."
   Good: "For this step, you need to find the Documents folder."

5. If the student seems frustrated, be encouraging but do not patronize.
   Bad: "Great job! You are doing amazing!"
   Good: "Yeah, that part trips everyone up at first. Here's a trick…"

6. If you genuinely do not know something, say so. Do not make stuff up.

7. If you want to highlight something on screen, emit an overlay command as a fenced code block:
   ```overlay
   {"action":"highlight","target":"<ax_element_description>","color":"blue"}
   ```
   Only do this when it is genuinely helpful, not every response."#
            .to_string()
    }

    /// Format the screen context as a concise block for injection.
    fn format_screen_context(&self, screen: &ScreenContext) -> String {
        let mut out = String::from("[Screen Context]\n");
        let _ = write!(
            out,
            "App: {} | Window: \"{}\"\n",
            screen.window.process_name, screen.window.title
        );
        if let Some(ref bundle) = screen.window.bundle_id {
            let _ = write!(out, "Bundle: {}\n", bundle);
        }
        if let Some(ref tree) = screen.ax_tree {
            let _ = write!(out, "UI Tree:\n{}\n", tree);
        } else {
            out.push_str("(no UI tree available)\n");
        }
        if screen.screenshot_b64.is_some() {
            out.push_str(
                "(screenshot attached — this app has a thin AX tree, use the image to see what the user is looking at)\n",
            );
        }
        out.push('\n');
        out
    }

    /// Format the task context as a concise block for injection.
    fn format_task_context(&self, task: &TaskState) -> String {
        let mut out = String::from("[Task Context]\n");
        let _ = write!(
            out,
            "Task: \"{}\" (step {}/{})\n",
            task.task_id,
            task.current_step_index + 1,
            task.total_steps
        );
        let _ = write!(
            out,
            "Current step: \"{}\"\n",
            task.current_step.instruction
        );
        if !task.current_step.hints.is_empty() {
            let _ = write!(
                out,
                "Hints available: {:?}\n",
                task.current_step.hints
            );
        }
        if let Some(ref val) = task.current_step.validation {
            let _ = write!(out, "Validation type: {}\n", val.rule_type);
        }
        out.push('\n');
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::WindowInfo;
    use crate::task_engine::types::{Step, TaskStatus};

    fn sample_screen() -> ScreenContext {
        ScreenContext {
            window: WindowInfo {
                title: "Documents".to_string(),
                process_name: "Finder".to_string(),
                bundle_id: Some("com.apple.finder".to_string()),
                pid: Some(1234),
            },
            ax_tree: Some("window \"Documents\"\n  toolbar\n    button \"Back\"\n".to_string()),
            screenshot_b64: None,
            captured_at_ms: 0,
        }
    }

    fn sample_task() -> TaskState {
        TaskState {
            task_id: "navigate-fs".to_string(),
            current_step_index: 1,
            current_step: Step {
                id: "step-2".to_string(),
                instruction: "Open the Documents folder in Finder".to_string(),
                hints: vec!["Look for it in the sidebar".to_string()],
                validation: None,
                overlay: None,
                xp: 10,
            },
            total_steps: 5,
            status: TaskStatus::InProgress,
            xp_earned: 10,
        }
    }

    #[test]
    fn test_build_includes_screen_context() {
        let pb = PromptBuilder::new();
        let screen = sample_screen();
        let (_, messages) = pb.build(&screen, None, &[], "Help me");
        let user_msg = &messages.last().unwrap().content;
        assert!(user_msg.contains("Finder"));
        assert!(user_msg.contains("Documents"));
        assert!(user_msg.contains("UI Tree:"));
    }

    #[test]
    fn test_build_includes_task_context() {
        let pb = PromptBuilder::new();
        let screen = sample_screen();
        let task = sample_task();
        let (_, messages) = pb.build(&screen, Some(&task), &[], "What now?");
        let user_msg = &messages.last().unwrap().content;
        assert!(user_msg.contains("step 2/5"));
        assert!(user_msg.contains("Open the Documents folder"));
    }

    #[test]
    fn test_build_without_task() {
        let pb = PromptBuilder::new();
        let screen = sample_screen();
        let (_, messages) = pb.build(&screen, None, &[], "Hey");
        let user_msg = &messages.last().unwrap().content;
        assert!(!user_msg.contains("[Task Context]"));
    }

    #[test]
    fn test_build_preserves_history() {
        let pb = PromptBuilder::new();
        let screen = sample_screen();
        let history = vec![
            ConversationTurn {
                role: "user".to_string(),
                content: "Hello".to_string(),
                timestamp_ms: 0,
                screen_summary: None,
            },
            ConversationTurn {
                role: "assistant".to_string(),
                content: "Hi there!".to_string(),
                timestamp_ms: 1,
                screen_summary: None,
            },
        ];
        let (_, messages) = pb.build(&screen, None, &history, "More help");
        // history(2) + current user(1) = 3 messages
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0].role, "user");
        assert_eq!(messages[0].content, "Hello");
        assert_eq!(messages[1].role, "assistant");
    }

    #[test]
    fn test_system_prompt_is_non_empty() {
        let pb = PromptBuilder::new();
        let sys = pb.system_prompt();
        assert!(sys.contains("Zee"));
        assert!(sys.contains("coworker"));
    }

    #[test]
    fn test_system_prompt_contains_key_rules() {
        let pb = PromptBuilder::new();
        let sys = pb.system_prompt();
        // Must contain the "don't give direct answers" rule
        assert!(sys.contains("NEVER give direct answers"));
        // Must mention overlay command format
        assert!(sys.contains("overlay"));
        // Must mention keeping responses short
        assert!(sys.contains("SHORT"));
    }

    #[test]
    fn test_screen_context_format_no_tree() {
        let pb = PromptBuilder::new();
        let screen = ScreenContext {
            window: WindowInfo {
                title: "Untitled".to_string(),
                process_name: "TextEdit".to_string(),
                bundle_id: None,
                pid: Some(5678),
            },
            ax_tree: None,
            screenshot_b64: None,
            captured_at_ms: 0,
        };
        let (_, messages) = pb.build(&screen, None, &[], "Help");
        let user_msg = &messages.last().unwrap().content;
        assert!(user_msg.contains("TextEdit"));
        assert!(user_msg.contains("(no UI tree available)"));
    }

    #[test]
    fn test_task_context_includes_hints() {
        let pb = PromptBuilder::new();
        let screen = sample_screen();
        let task = sample_task();
        let (_, messages) = pb.build(&screen, Some(&task), &[], "What now?");
        let user_msg = &messages.last().unwrap().content;
        assert!(user_msg.contains("Look for it in the sidebar"));
    }

    #[test]
    fn test_task_context_includes_validation_type() {
        let pb = PromptBuilder::new();
        let screen = sample_screen();
        let mut task = sample_task();
        task.current_step.validation = Some(crate::task_engine::types::ValidationRule {
            rule_type: "window_match".to_string(),
            match_rule: None,
            fallback_ai: None,
        });
        let (_, messages) = pb.build(&screen, Some(&task), &[], "Check");
        let user_msg = &messages.last().unwrap().content;
        assert!(user_msg.contains("Validation type: window_match"));
    }

    #[test]
    fn test_user_input_appended_at_end() {
        let pb = PromptBuilder::new();
        let screen = sample_screen();
        let (_, messages) = pb.build(&screen, None, &[], "MY_UNIQUE_QUESTION_TEXT");
        let user_msg = &messages.last().unwrap().content;
        // User text should be at the end, after context blocks
        assert!(user_msg.ends_with("MY_UNIQUE_QUESTION_TEXT"));
    }

    #[test]
    fn test_build_with_long_history() {
        let pb = PromptBuilder::new();
        let screen = sample_screen();
        let history: Vec<ConversationTurn> = (0..20)
            .map(|i| ConversationTurn {
                role: if i % 2 == 0 { "user" } else { "assistant" }.to_string(),
                content: format!("message {}", i),
                timestamp_ms: i as u64,
                screen_summary: None,
            })
            .collect();
        let (_, messages) = pb.build(&screen, None, &history, "newest");
        // 20 history + 1 current = 21
        assert_eq!(messages.len(), 21);
        assert_eq!(messages[0].content, "message 0");
        assert!(messages.last().unwrap().content.ends_with("newest"));
    }

    #[test]
    fn test_build_attaches_screenshot_on_user_turn() {
        let pb = PromptBuilder::new();
        let mut screen = sample_screen();
        screen.screenshot_b64 = Some("FAKE_B64_IMAGE".to_string());
        let (_, messages) = pb.build(&screen, None, &[], "What's on screen?");
        let last = messages.last().unwrap();
        assert_eq!(last.images_b64.len(), 1);
        assert_eq!(last.images_b64[0], "FAKE_B64_IMAGE");
        // Context block should hint about the screenshot
        assert!(last.content.contains("screenshot attached"));
    }

    #[test]
    fn test_build_no_screenshot_when_absent() {
        let pb = PromptBuilder::new();
        let screen = sample_screen(); // no screenshot_b64
        let (_, messages) = pb.build(&screen, None, &[], "Hi");
        assert!(messages.last().unwrap().images_b64.is_empty());
    }

    #[test]
    fn test_build_history_messages_never_carry_images() {
        let pb = PromptBuilder::new();
        let mut screen = sample_screen();
        screen.screenshot_b64 = Some("img".to_string());
        let history = vec![ConversationTurn {
            role: "user".to_string(),
            content: "earlier".to_string(),
            timestamp_ms: 1,
            screen_summary: None,
        }];
        let (_, messages) = pb.build(&screen, None, &history, "now");
        // Only the last (current) message has images
        assert!(messages[0].images_b64.is_empty());
        assert_eq!(messages.last().unwrap().images_b64.len(), 1);
    }

    #[test]
    fn test_system_prompt_returned_separately() {
        let pb = PromptBuilder::new();
        let screen = sample_screen();
        let (system, messages) = pb.build(&screen, None, &[], "Hi");
        // System prompt should NOT be in the messages array
        assert!(system.contains("Zee"));
        for msg in &messages {
            assert_ne!(msg.role, "system");
        }
    }
}
