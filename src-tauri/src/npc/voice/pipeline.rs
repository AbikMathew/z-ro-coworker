use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::npc::conversation::ConversationTurn;
use crate::npc::interrupt::InterruptHandle;
use crate::npc::prompt_builder::{LlmMessage, PromptBuilder};
use crate::npc::screen_reader::ScreenContext;
use crate::task_engine::types::TaskState;

use super::stt::SttProvider;
use super::tts::TtsProvider;

/// Sentinel error returned by pipeline methods when the turn was cancelled
/// mid-flight via `InterruptController::cancel()`. Callers should treat
/// this as a normal outcome, not a failure — the user asked us to stop.
pub const CANCELLED_ERR: &str = "cancelled";

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

/// The result of a voice-in/voice-out turn: a `TurnResult` plus the
/// synthesized TTS audio ready for the frontend to play.
#[derive(Debug, Clone)]
pub struct VoiceTurnResult {
    pub turn: TurnResult,
    /// MIME type of `audio_bytes` (e.g. `"audio/mpeg"`).
    pub audio_mime: String,
    /// Complete audio bytes from TTS (empty if no TTS configured).
    pub audio_bytes: Vec<u8>,
}

/// An overlay command embedded in the LLM response as a fenced code block.
///
/// Coordinates are **PIXEL COORDS** of the screenshot the LLM was shown —
/// the prompt tells the model the capture dimensions (e.g. 1440×900) and
/// asks for integer pixels in that space. `(0,0)` is top-left. The
/// coordinator's `translate_overlay_coords_pure` maps them into
/// monitor-local normalized space for the overlay renderer.
///
/// ```text
/// ```overlay
/// {"action":"arrow","x":1180,"y":820,"text":"Click Opus 4.7"}
/// ```
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlayCommand {
    /// `"arrow"` | `"box"` | `"tooltip"` | `"clear"`
    pub action: String,
    /// Pixel x of the target in the captured screenshot (center for
    /// arrow/tooltip, top-left for box).
    #[serde(default)]
    pub x: Option<f32>,
    /// Pixel y of the target in the captured screenshot.
    #[serde(default)]
    pub y: Option<f32>,
    /// Pixel width of the bounded region (only for `"box"`).
    #[serde(default)]
    pub width: Option<f32>,
    /// Pixel height of the bounded region (only for `"box"`).
    #[serde(default)]
    pub height: Option<f32>,
    /// Optional label/tooltip text drawn near the target.
    #[serde(default)]
    pub text: Option<String>,
    /// Optional color hint (CSS color string or named color).
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
    stt: Option<Box<dyn SttProvider>>,
    tts: Option<Box<dyn TtsProvider>>,
    /// The LLM lives behind an `RwLock<Arc<…>>` so the settings UI can
    /// hot-swap the model at runtime. Reads briefly clone the `Arc`, drop the
    /// lock, then await — the lock is never held across `.await`.
    llm: RwLock<Arc<dyn LlmProvider>>,
    prompt_builder: PromptBuilder,
}

impl VoicePipeline {
    /// Create a new pipeline.
    ///
    /// For Phase 1 (text-only), pass `None` for `stt` and `tts`.
    pub fn new(
        stt: Option<Box<dyn SttProvider>>,
        tts: Option<Box<dyn TtsProvider>>,
        llm: Arc<dyn LlmProvider>,
    ) -> Self {
        Self {
            stt,
            tts,
            llm: RwLock::new(llm),
            prompt_builder: PromptBuilder::new(),
        }
    }

    /// Hot-swap the active LLM provider. Subsequent turns will use `llm`.
    pub fn set_llm(&self, llm: Arc<dyn LlmProvider>) {
        if let Ok(mut slot) = self.llm.write() {
            *slot = llm;
        }
    }

    /// Briefly lock the slot, clone the Arc, and return it. The lock is never
    /// held across `.await`, so swaps can't deadlock the pipeline.
    fn current_llm(&self) -> Arc<dyn LlmProvider> {
        self.llm
            .read()
            .expect("VoicePipeline llm lock poisoned")
            .clone()
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
        cancel: &InterruptHandle,
    ) -> Result<TurnResult, String> {
        // Build prompt
        let (system, messages) = self.prompt_builder.build(screen, task, history, text);

        // Snapshot the current LLM once per turn — the settings UI may swap
        // it mid-session, but each turn gets a consistent provider.
        let llm = self.current_llm();

        // Concise per-turn digest so you can confirm at a glance what the
        // model received: which screen, whether a screenshot was attached,
        // whether a task is active, how much history was included.
        let ax_bytes = screen.ax_tree.as_ref().map(|t| t.len()).unwrap_or(0);
        let img_count: usize = messages.iter().map(|m| m.images_b64.len()).sum();
        println!(
            "[z-ro:npc] llm→{} | app=\"{}\" ax={}B images={} task={} history={}",
            llm.name(),
            screen.window.process_name,
            ax_bytes,
            img_count,
            task.map(|t| t.task_id.as_str()).unwrap_or("none"),
            history.len(),
        );

        // Race the LLM stream against the cancel signal. Arming the future
        // before the first chunk means a cancel fired during the first HTTP
        // round-trip still wakes us.
        let cancelled = cancel.cancelled();
        tokio::pin!(cancelled);

        let mut rx = llm.stream_chat(&system, &messages).await?;
        let mut full_response = String::new();
        loop {
            tokio::select! {
                biased;
                // biased: check cancellation first so a pile of buffered
                // chunks can't starve the cancel signal.
                () = &mut cancelled => {
                    // Dropping `rx` closes the channel; the provider's send
                    // task will see `SendError` on its next tx.send and
                    // abort the upstream HTTP stream.
                    drop(rx);
                    println!("[z-ro:npc] llm stream cancelled mid-turn");
                    return Err(CANCELLED_ERR.to_string());
                }
                chunk = rx.recv() => {
                    match chunk {
                        Some(c) => full_response.push_str(&c),
                        None => break,
                    }
                }
            }
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

    /// Get the name of the current LLM provider. Allocates a new `String`
    /// because the underlying `&str` is borrowed from whichever provider is
    /// behind the lock — the reference can't outlive the guard.
    pub fn llm_name(&self) -> String {
        self.current_llm().name().to_string()
    }

    /// Whether STT is configured (voice input is available).
    pub fn has_stt(&self) -> bool {
        self.stt.is_some()
    }

    /// Whether TTS is configured (voice output is available).
    pub fn has_tts(&self) -> bool {
        self.tts.is_some()
    }

    /// Process a proactive/system-triggered turn: text → LLM → TTS.
    ///
    /// Used by Phase 3 (milestone progress) and Phase 4 (WrongMove
    /// correction) where the "user input" is a tagged system prompt the
    /// coordinator authored, not something the user typed or spoke. Output
    /// shape matches a voice turn so the frontend can render a chat bubble
    /// + play the TTS audio with no separate code path.
    ///
    /// Intentionally reuses `process_text_turn` for the LLM work so prompt
    /// assembly, overlay parsing, and cancellation behave identically.
    pub async fn process_proactive_turn(
        &self,
        trigger_text: &str,
        screen: &ScreenContext,
        task: Option<&TaskState>,
        history: &[ConversationTurn],
        cancel: &InterruptHandle,
    ) -> Result<VoiceTurnResult, String> {
        let turn = self
            .process_text_turn(trigger_text, screen, task, history, cancel)
            .await?;

        let (audio_mime, audio_bytes) = match &self.tts {
            Some(tts) if !turn.assistant_text.trim().is_empty() => {
                let cancelled = cancel.cancelled();
                tokio::pin!(cancelled);
                tokio::select! {
                    biased;
                    () = &mut cancelled => {
                        println!("[z-ro:npc] proactive TTS cancelled mid-synthesis");
                        return Err(CANCELLED_ERR.to_string());
                    }
                    synth = tts.synthesize(&turn.assistant_text) => {
                        let synth = synth?;
                        (synth.mime, synth.bytes)
                    }
                }
            }
            _ => (String::new(), Vec::new()),
        };

        Ok(VoiceTurnResult {
            turn,
            audio_mime,
            audio_bytes,
        })
    }

    /// Process a full voice turn: audio → STT → LLM → TTS.
    ///
    /// 1. Transcribe audio to text via STT
    /// 2. Run the text turn through the LLM pipeline
    /// 3. Synthesize the clean (non-overlay) reply to audio via TTS
    ///
    /// If TTS is not configured, the returned audio is empty and `audio_mime`
    /// is `""` — the caller should fall back to displaying text only.
    pub async fn process_voice_turn(
        &self,
        audio_bytes: &[u8],
        audio_filename: &str,
        screen: &ScreenContext,
        task: Option<&TaskState>,
        history: &[ConversationTurn],
        cancel: &InterruptHandle,
    ) -> Result<VoiceTurnResult, String> {
        let stt = self
            .stt
            .as_ref()
            .ok_or_else(|| "STT provider not configured".to_string())?;

        // 1. Transcribe. STT providers are single-shot HTTP, so a cancel
        // just means "drop the future" — reqwest aborts the connection.
        let transcript = {
            let cancelled = cancel.cancelled();
            tokio::pin!(cancelled);
            tokio::select! {
                biased;
                () = &mut cancelled => {
                    println!("[z-ro:npc] STT cancelled before transcript");
                    return Err(CANCELLED_ERR.to_string());
                }
                result = stt.transcribe(audio_bytes, audio_filename) => result?,
            }
        };
        if transcript.trim().is_empty() {
            return Err("STT returned empty transcript (silence?)".to_string());
        }

        // 2. LLM turn (reuses text-turn logic — cancel threads through).
        let turn = self
            .process_text_turn(&transcript, screen, task, history, cancel)
            .await?;

        // 3. TTS — synthesize the displayed (overlay-stripped) text. Race
        // again so a cancel during synthesis doesn't burn TTS credits for
        // audio the user will never hear.
        let (audio_mime, audio_bytes) = match &self.tts {
            Some(tts) if !turn.assistant_text.trim().is_empty() => {
                let cancelled = cancel.cancelled();
                tokio::pin!(cancelled);
                tokio::select! {
                    biased;
                    () = &mut cancelled => {
                        println!("[z-ro:npc] TTS cancelled mid-synthesis");
                        return Err(CANCELLED_ERR.to_string());
                    }
                    synth = tts.synthesize(&turn.assistant_text) => {
                        let synth = synth?;
                        (synth.mime, synth.bytes)
                    }
                }
            }
            _ => (String::new(), Vec::new()),
        };

        Ok(VoiceTurnResult {
            turn,
            audio_mime,
            audio_bytes,
        })
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
{"action":"arrow","x":0.5,"y":0.75,"color":"blue","text":"Click here"}
```
Does that help?"#;

        let cmds = parse_overlay_commands(text);
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0].action, "arrow");
        assert!((cmds[0].x.unwrap() - 0.5).abs() < f32::EPSILON);
        assert!((cmds[0].y.unwrap() - 0.75).abs() < f32::EPSILON);
        assert_eq!(cmds[0].color.as_deref(), Some("blue"));
        assert_eq!(cmds[0].text.as_deref(), Some("Click here"));
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
{"action":"box","x":0.1,"y":0.2,"width":0.3,"height":0.05}
```
and also
```overlay
{"action":"tooltip","x":0.5,"y":0.4,"text":"Enter your name here"}
```
Got it?"#;

        let cmds = parse_overlay_commands(text);
        assert_eq!(cmds.len(), 2);
        assert_eq!(cmds[0].action, "box");
        assert_eq!(cmds[0].width, Some(0.3));
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
        assert!(cmds[0].x.is_none());
        assert!(cmds[0].y.is_none());
        assert!(cmds[0].width.is_none());
        assert!(cmds[0].height.is_none());
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
{"action":"arrow","x":0.1,"y":0.2}
This block is never closed."#;

        let cmds = parse_overlay_commands(text);
        assert!(cmds.is_empty()); // Unclosed block not parsed
    }

    // ── Strip overlay tests ────────────────────────────────────────────

    #[test]
    fn test_strip_overlay_blocks() {
        let text = r#"Check the button.
```overlay
{"action":"arrow","x":0.5,"y":0.5}
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
{"action":"arrow","x":0.1,"y":0.2}
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

    /// Mock that yields one chunk every `delay` and never completes on its
    /// own — useful for exercising the cancel path without racing a fast
    /// normal completion.
    struct SlowMockLlm {
        chunk_delay: std::time::Duration,
    }

    #[async_trait]
    impl LlmProvider for SlowMockLlm {
        async fn stream_chat(
            &self,
            _system: &str,
            _messages: &[LlmMessage],
        ) -> Result<mpsc::Receiver<String>, String> {
            let (tx, rx) = mpsc::channel(16);
            let delay = self.chunk_delay;
            tokio::spawn(async move {
                let mut i = 0u32;
                loop {
                    tokio::time::sleep(delay).await;
                    if tx.send(format!("chunk-{i} ")).await.is_err() {
                        // Receiver dropped (likely a cancel) — stop trying.
                        return;
                    }
                    i += 1;
                }
            });
            Ok(rx)
        }

        fn name(&self) -> &str {
            "slow-mock-llm"
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
            captured_at_ms: 1000,
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn test_pipeline_cancel_mid_stream_returns_cancelled_err() {
        use crate::npc::interrupt::InterruptController;
        use std::time::Duration;

        let llm = SlowMockLlm {
            chunk_delay: Duration::from_millis(30),
        };
        let pipeline = Arc::new(VoicePipeline::new(None, None, Arc::new(llm)));
        let screen = make_screen();
        let ctrl = Arc::new(InterruptController::new());
        let cancel = ctrl.new_turn();

        // Fire the cancel 80ms in — long enough for the stream to deliver
        // one or two chunks, short enough that the test finishes fast.
        let ctrl_for_canceler = ctrl.clone();
        let canceler = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(80)).await;
            ctrl_for_canceler.cancel();
        });

        let err = pipeline
            .process_text_turn("tell me a story", &screen, None, &[], &cancel)
            .await
            .expect_err("should be cancelled, not Ok");
        assert_eq!(err, CANCELLED_ERR);

        canceler.await.unwrap();
    }

    #[tokio::test]
    async fn test_pipeline_text_turn_basic() {
        let llm = MockLlm {
            response: "I can see you have VS Code open with main.rs!".to_string(),
        };
        let pipeline = VoicePipeline::new(None, None, Arc::new(llm));
        let screen = make_screen();

        let result = pipeline
            .process_text_turn("What do you see?", &screen, None, &[], &InterruptHandle::never_cancels())
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
{"action":"arrow","x":0.82,"y":0.91,"color":"green","text":"Save"}
```
That should work!"#;
        let llm = MockLlm {
            response: response.to_string(),
        };
        let pipeline = VoicePipeline::new(None, None, Arc::new(llm));
        let screen = make_screen();

        let result = pipeline
            .process_text_turn("How do I save?", &screen, None, &[], &InterruptHandle::never_cancels())
            .await
            .unwrap();

        // Overlay parsed
        assert_eq!(result.overlay_commands.len(), 1);
        assert_eq!(result.overlay_commands[0].action, "arrow");
        assert!((result.overlay_commands[0].x.unwrap() - 0.82).abs() < 0.001);
        assert!((result.overlay_commands[0].y.unwrap() - 0.91).abs() < 0.001);
        assert_eq!(result.overlay_commands[0].text.as_deref(), Some("Save"));

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
        let pipeline = VoicePipeline::new(None, None, Arc::new(llm));
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
            .process_text_turn("Where were we?", &screen, None, &history, &InterruptHandle::never_cancels())
            .await
            .unwrap();

        assert!(result.assistant_text.contains("continue"));
    }

    #[tokio::test]
    async fn test_pipeline_llm_name() {
        let llm = MockLlm {
            response: "test".to_string(),
        };
        let pipeline = VoicePipeline::new(None, None, Arc::new(llm));
        assert_eq!(pipeline.llm_name(), "mock-llm");
    }

    // ── Voice turn tests ───────────────────────────────────────────────

    struct MockStt {
        transcript: String,
    }

    #[async_trait]
    impl SttProvider for MockStt {
        async fn transcribe(
            &self,
            _audio: &[u8],
            _filename: &str,
        ) -> Result<String, String> {
            Ok(self.transcript.clone())
        }
        fn name(&self) -> &str {
            "mock-stt"
        }
    }

    struct MockTts {
        mime: String,
        bytes: Vec<u8>,
    }

    #[async_trait]
    impl TtsProvider for MockTts {
        async fn synthesize(
            &self,
            _text: &str,
        ) -> Result<crate::npc::voice::tts::TtsAudio, String> {
            Ok(crate::npc::voice::tts::TtsAudio {
                mime: self.mime.clone(),
                bytes: self.bytes.clone(),
            })
        }
        fn name(&self) -> &str {
            "mock-tts"
        }
    }

    #[tokio::test]
    async fn test_has_stt_has_tts_defaults_false() {
        let pipeline = VoicePipeline::new(
            None,
            None,
            Arc::new(MockLlm { response: "x".to_string() }),
        );
        assert!(!pipeline.has_stt());
        assert!(!pipeline.has_tts());
    }

    #[tokio::test]
    async fn test_process_voice_turn_full_pipeline() {
        let stt = Box::new(MockStt {
            transcript: "What am I looking at?".to_string(),
        });
        let tts = Box::new(MockTts {
            mime: "audio/mpeg".to_string(),
            bytes: vec![0xFF, 0xFB, 0x90, 0x00], // fake MP3 header bytes
        });
        let llm = Arc::new(MockLlm {
            response: "You're looking at VS Code.".to_string(),
        });
        let pipeline = VoicePipeline::new(Some(stt), Some(tts), llm);
        assert!(pipeline.has_stt());
        assert!(pipeline.has_tts());

        let screen = make_screen();
        let result = pipeline
            .process_voice_turn(b"audio-bytes", "audio.webm", &screen, None, &[], &InterruptHandle::never_cancels())
            .await
            .unwrap();

        assert_eq!(result.turn.user_transcript, "What am I looking at?");
        assert!(result.turn.assistant_text.contains("VS Code"));
        assert_eq!(result.audio_mime, "audio/mpeg");
        assert_eq!(result.audio_bytes, vec![0xFF, 0xFB, 0x90, 0x00]);
    }

    #[tokio::test]
    async fn test_process_voice_turn_errors_without_stt() {
        let pipeline = VoicePipeline::new(
            None,
            None,
            Arc::new(MockLlm { response: "x".to_string() }),
        );
        let screen = make_screen();
        let err = pipeline
            .process_voice_turn(b"x", "audio.webm", &screen, None, &[], &InterruptHandle::never_cancels())
            .await
            .unwrap_err();
        assert!(err.contains("STT"));
    }

    #[tokio::test]
    async fn test_process_voice_turn_empty_transcript_errors() {
        let stt = Box::new(MockStt {
            transcript: "   ".to_string(), // whitespace only
        });
        let pipeline = VoicePipeline::new(
            Some(stt),
            None,
            Arc::new(MockLlm { response: "x".to_string() }),
        );
        let screen = make_screen();
        let err = pipeline
            .process_voice_turn(b"silent", "audio.webm", &screen, None, &[], &InterruptHandle::never_cancels())
            .await
            .unwrap_err();
        assert!(err.to_lowercase().contains("empty") || err.to_lowercase().contains("silence"));
    }

    #[tokio::test]
    async fn test_process_voice_turn_without_tts_returns_empty_audio() {
        let stt = Box::new(MockStt {
            transcript: "Hello".to_string(),
        });
        let pipeline = VoicePipeline::new(
            Some(stt),
            None, // no TTS
            Arc::new(MockLlm { response: "Hi".to_string() }),
        );
        let screen = make_screen();
        let result = pipeline
            .process_voice_turn(b"x", "audio.webm", &screen, None, &[], &InterruptHandle::never_cancels())
            .await
            .unwrap();
        assert!(result.turn.assistant_text.contains("Hi"));
        assert_eq!(result.audio_mime, "");
        assert!(result.audio_bytes.is_empty());
    }
}
