use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::npc::conversation::ConversationTurn;
use crate::npc::prompt_builder::{LlmMessage, PromptBuilder};
use crate::npc::screen_reader::ScreenContext;
use crate::task_engine::types::TaskState;

use super::stt::SttProvider;
use super::tts::TtsProvider;

// ── LLM Provider trait ─────────────────────────────────────────────────

/// Trait for LLM providers — allows swapping between OpenAI, Groq, Gemini, local.
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Stream a chat completion.
    ///
    /// Returns a receiver that yields text chunks as they arrive from the
    /// model.  The channel is closed when the response is complete.
    async fn stream_chat(
        &self,
        system: &str,
        messages: &[LlmMessage],
    ) -> Result<mpsc::Receiver<String>, String>;

    /// Human-readable provider/model name for logging.
    fn name(&self) -> &str;
}

// ── Pipeline result types ──────────────────────────────────────────────

/// The result of a single NPC interaction turn.
#[derive(Debug, Clone)]
pub struct TurnResult {
    /// What the user said (transcript from STT or typed text).
    pub user_transcript: String,
    /// The assistant's full text response.
    pub assistant_text: String,
    /// Any overlay commands parsed from the response.
    pub overlay_commands: Vec<OverlayCommand>,
}

/// An overlay command embedded in the LLM response as a fenced code block.
///
/// ```text
/// ```overlay
/// {"action":"highlight","target":"button:Submit","color":"blue"}
/// ```
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlayCommand {
    pub action: String,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
}

// ── Voice Pipeline ─────────────────────────────────────────────────────

/// The voice pipeline processes a complete NPC interaction turn.
///
/// Full pipeline (Phase 2):
///   mic audio → STT → prompt builder → LLM stream → parse overlays → TTS → playback
///
/// Text-only pipeline (Phase 1):
///   typed text → prompt builder → LLM stream → parse overlays → return text
pub struct VoicePipeline {
    #[allow(dead_code)]
    stt: Option<Box<dyn SttProvider>>,
    #[allow(dead_code)]
    tts: Option<Box<dyn TtsProvider>>,
    llm: Box<dyn LlmProvider>,
    prompt_builder: PromptBuilder,
}

impl VoicePipeline {
    /// Create a new pipeline.
    ///
    /// For Phase 1 (text-only), pass `None` for `stt` and `tts`.
    pub fn new(
        stt: Option<Box<dyn SttProvider>>,
        tts: Option<Box<dyn TtsProvider>>,
        llm: Box<dyn LlmProvider>,
    ) -> Self {
        Self {
            stt,
            tts,
            llm,
            prompt_builder: PromptBuilder::new(),
        }
    }

    /// Process a text-only turn (no STT/TTS).
    ///
    /// 1. Build prompt with screen + task context
    /// 2. Stream LLM response, collecting full text
    /// 3. Parse overlay commands from the response
    /// 4. Return the result
    pub async fn process_text_turn(
        &self,
        text: &str,
        screen: &ScreenContext,
        task: Option<&TaskState>,
        history: &[ConversationTurn],
    ) -> Result<TurnResult, String> {
        // Build prompt
        let (system, messages) = self.prompt_builder.build(screen, task, history, text);

        // Stream LLM response
        let mut rx = self.llm.stream_chat(&system, &messages).await?;
        let mut full_response = String::new();
        while let Some(chunk) = rx.recv().await {
            full_response.push_str(&chunk);
        }

        // Parse overlay commands from the response
        let overlay_commands = parse_overlay_commands(&full_response);

        // Strip overlay code blocks from the spoken/displayed text
        let clean_text = strip_overlay_blocks(&full_response);

        Ok(TurnResult {
            user_transcript: text.to_string(),
            assistant_text: clean_text,
            overlay_commands,
        })
    }

    /// Get the name of the current LLM provider.
    pub fn llm_name(&self) -> &str {
        self.llm.name()
    }
}

// ── Overlay command parsing ────────────────────────────────────────────

/// Parse overlay commands from fenced code blocks in the LLM response.
///
/// Looks for blocks like:
/// ````text
/// ```overlay
/// {"action":"highlight","target":"button:Submit","color":"blue"}
/// ```
/// ````
fn parse_overlay_commands(text: &str) -> Vec<OverlayCommand> {
    let mut commands = Vec::new();
    let mut in_overlay_block = false;
    let mut block_content = String::new();

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed == "```overlay" {
            in_overlay_block = true;
            block_content.clear();
        } else if in_overlay_block && trimmed == "```" {
            in_overlay_block = false;
            if let Ok(cmd) = serde_json::from_str::<OverlayCommand>(&block_content) {
                commands.push(cmd);
            }
        } else if in_overlay_block {
            block_content.push_str(line);
            block_content.push('\n');
        }
    }

    commands
}

/// Remove overlay code blocks from text, leaving only the spoken/displayed parts.
fn strip_overlay_blocks(text: &str) -> String {
    let mut result = String::new();
    let mut in_overlay_block = false;

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed == "```overlay" {
            in_overlay_block = true;
        } else if in_overlay_block && trimmed == "```" {
            in_overlay_block = false;
        } else if !in_overlay_block {
            result.push_str(line);
            result.push('\n');
        }
    }

    result.trim().to_string()
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::WindowInfo;
    use crate::npc::screen_reader::ScreenContext;

    // ── Overlay parsing tests ──────────────────────────────────────────

    #[test]
    fn test_parse_overlay_commands() {
        let text = r#"Look at the submit button.
```overlay
{"action":"highlight","target":"button:Submit","color":"blue"}
```
Does that help?"#;

        let cmds = parse_overlay_commands(text);
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0].action, "highlight");
        assert_eq!(cmds[0].target.as_deref(), Some("button:Submit"));
        assert_eq!(cmds[0].color.as_deref(), Some("blue"));
    }

    #[test]
    fn test_parse_no_overlay() {
        let cmds = parse_overlay_commands("Just a normal response.");
        assert!(cmds.is_empty());
    }

    #[test]
    fn test_parse_multiple_overlays() {
        let text = r#"Check these:
```overlay
{"action":"highlight","target":"button:OK"}
```
and also
```overlay
{"action":"tooltip","target":"field:Name","text":"Enter your name here"}
```
Got it?"#;

        let cmds = parse_overlay_commands(text);
        assert_eq!(cmds.len(), 2);
        assert_eq!(cmds[0].action, "highlight");
        assert_eq!(cmds[1].action, "tooltip");
        assert_eq!(cmds[1].text.as_deref(), Some("Enter your name here"));
    }

    #[test]
    fn test_parse_overlay_invalid_json_skipped() {
        let text = r#"Hmm:
```overlay
{this is not valid json}
```
Keep going."#;

        let cmds = parse_overlay_commands(text);
        assert!(cmds.is_empty()); // Invalid JSON silently skipped
    }

    #[test]
    fn test_parse_overlay_missing_optional_fields() {
        let text = r#"Here:
```overlay
{"action":"clear"}
```
Done."#;

        let cmds = parse_overlay_commands(text);
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0].action, "clear");
        assert!(cmds[0].target.is_none());
        assert!(cmds[0].text.is_none());
        assert!(cmds[0].color.is_none());
    }

    #[test]
    fn test_parse_overlay_ignores_non_overlay_code_blocks() {
        let text = r#"Here's some code:
```rust
fn main() { println!("hello"); }
```
Not an overlay."#;

        let cmds = parse_overlay_commands(text);
        assert!(cmds.is_empty());
    }

    #[test]
    fn test_parse_overlay_unclosed_block() {
        let text = r#"Oops:
```overlay
{"action":"highlight","target":"x"}
This block is never closed."#;

        let cmds = parse_overlay_commands(text);
        assert!(cmds.is_empty()); // Unclosed block not parsed
    }

    // ── Strip overlay tests ────────────────────────────────────────────

    #[test]
    fn test_strip_overlay_blocks() {
        let text = r#"Check the button.
```overlay
{"action":"highlight","target":"x"}
```
Got it?"#;

        let clean = strip_overlay_blocks(text);
        assert_eq!(clean, "Check the button.\nGot it?");
    }

    #[test]
    fn test_strip_no_overlay() {
        let clean = strip_overlay_blocks("Hello world");
        assert_eq!(clean, "Hello world");
    }

    #[test]
    fn test_strip_preserves_non_overlay_code_blocks() {
        let text = r#"Try this:
```rust
fn main() {}
```
Easy right?"#;

        let clean = strip_overlay_blocks(text);
        assert!(clean.contains("fn main()"));
        assert!(clean.contains("Easy right?"));
    }

    #[test]
    fn test_strip_multiple_overlays() {
        let text = r#"First:
```overlay
{"action":"highlight"}
```
Then:
```overlay
{"action":"clear"}
```
Done."#;

        let clean = strip_overlay_blocks(text);
        assert_eq!(clean, "First:\nThen:\nDone.");
    }

    // ── Mock LLM integration test ──────────────────────────────────────

    /// A mock LLM provider that returns a fixed response for testing.
    /// Sends the full response as a single chunk (simulates fast model).
    struct MockLlm {
        response: String,
    }

    #[async_trait]
    impl LlmProvider for MockLlm {
        async fn stream_chat(
            &self,
            _system: &str,
            _messages: &[LlmMessage],
        ) -> Result<mpsc::Receiver<String>, String> {
            let (tx, rx) = mpsc::channel(16);
            let response = self.response.clone();
            tokio::spawn(async move {
                let _ = tx.send(response).await;
            });
            Ok(rx)
        }

        fn name(&self) -> &str {
            "mock-llm"
        }
    }

    fn make_screen() -> ScreenContext {
        ScreenContext {
            window: WindowInfo {
                title: "main.rs".to_string(),
                process_name: "Code".to_string(),
                bundle_id: Some("com.microsoft.VSCode".to_string()),
                pid: Some(9999),
            },
            ax_tree: Some("window \"main.rs\"\n  editor\n    text \"fn main()\"\n".to_string()),
            screenshot_b64: None,
            captured_at_ms: 1000,
        }
    }

    #[tokio::test]
    async fn test_pipeline_text_turn_basic() {
        let llm = MockLlm {
            response: "I can see you have VS Code open with main.rs!".to_string(),
        };
        let pipeline = VoicePipeline::new(None, None, Box::new(llm));
        let screen = make_screen();

        let result = pipeline
            .process_text_turn("What do you see?", &screen, None, &[])
            .await
            .unwrap();

        assert_eq!(result.user_transcript, "What do you see?");
        assert!(result.assistant_text.contains("VS Code"));
        assert!(result.overlay_commands.is_empty());
    }

    #[tokio::test]
    async fn test_pipeline_text_turn_with_overlay() {
        let response = r#"Click the save button.
```overlay
{"action":"highlight","target":"button:Save","color":"green"}
```
That should work!"#;
        let llm = MockLlm {
            response: response.to_string(),
        };
        let pipeline = VoicePipeline::new(None, None, Box::new(llm));
        let screen = make_screen();

        let result = pipeline
            .process_text_turn("How do I save?", &screen, None, &[])
            .await
            .unwrap();

        // Overlay parsed
        assert_eq!(result.overlay_commands.len(), 1);
        assert_eq!(result.overlay_commands[0].action, "highlight");
        assert_eq!(
            result.overlay_commands[0].target.as_deref(),
            Some("button:Save")
        );

        // Overlay block stripped from displayed text
        assert!(!result.assistant_text.contains("```overlay"));
        assert!(result.assistant_text.contains("Click the save button."));
        assert!(result.assistant_text.contains("That should work!"));
    }

    #[tokio::test]
    async fn test_pipeline_text_turn_with_history() {
        let llm = MockLlm {
            response: "Yes, continue from where we left off.".to_string(),
        };
        let pipeline = VoicePipeline::new(None, None, Box::new(llm));
        let screen = make_screen();
        let history = vec![
            crate::npc::conversation::ConversationTurn {
                role: "user".to_string(),
                content: "Help me with Rust".to_string(),
                timestamp_ms: 1,
                screen_summary: None,
            },
            crate::npc::conversation::ConversationTurn {
                role: "assistant".to_string(),
                content: "Sure, what are you stuck on?".to_string(),
                timestamp_ms: 2,
                screen_summary: None,
            },
        ];

        let result = pipeline
            .process_text_turn("Where were we?", &screen, None, &history)
            .await
            .unwrap();

        assert!(result.assistant_text.contains("continue"));
    }

    #[tokio::test]
    async fn test_pipeline_llm_name() {
        let llm = MockLlm {
            response: "test".to_string(),
        };
        let pipeline = VoicePipeline::new(None, None, Box::new(llm));
        assert_eq!(pipeline.llm_name(), "mock-llm");
    }
}
