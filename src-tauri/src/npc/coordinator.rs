use serde::{Deserialize, Serialize};
use std::sync::{Arc, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::AppHandle;
use tokio::sync::{watch, Mutex};

use crate::task_engine::machine::TaskMachine;

use super::conversation::{ConversationMemory, ConversationTurn};
use super::llm_providers::{self, ApiKeys, ModelInfo};
use super::overlay_driver::{self, AutoHideState};
use super::screen_reader::ScreenReader;
use super::voice::pipeline::{OverlayCommand, VoicePipeline};

// ── Voice result type ──────────────────────────────────────────────────

/// Result of a voice turn returned to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceAskResult {
    /// What the user said (STT transcript).
    pub user_transcript: String,
    /// Zee's text reply.
    pub assistant_text: String,
    /// Base64-encoded audio bytes ready to play.  Empty if no TTS provider.
    pub audio_b64: String,
    /// MIME type of `audio_b64` (e.g. `"audio/mpeg"`).  Empty when audio is empty.
    pub audio_mime: String,
}

// ── Public types ───────────────────────────────────────────────────────

/// NPC lifecycle states.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NpcState {
    /// Waiting, not actively processing.
    Idle,
    /// Frontend is capturing mic audio (push-to-talk active).
    Listening,
    /// Processing user input through the pipeline.
    Thinking,
    /// TTS audio is being played back on the frontend.
    Speaking,
    /// Something went wrong.
    Error(String),
}

/// Snapshot of the NPC's current status (sent to frontend).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NpcStatus {
    pub state: NpcState,
    pub active_task: Option<String>,
    pub current_step: Option<String>,
    pub model_name: String,
    pub conversation_turns: usize,
}

/// Configuration for which providers the NPC should use.
#[derive(Debug, Clone, Deserialize)]
pub struct NpcConfig {
    pub llm_provider: String,
    pub stt_provider: String,
    pub tts_provider: String,
    pub voice_id: Option<String>,
    pub model_name: Option<String>,
}

impl Default for NpcConfig {
    fn default() -> Self {
        Self {
            llm_provider: "openai".to_string(),
            stt_provider: "none".to_string(),
            tts_provider: "none".to_string(),
            voice_id: None,
            model_name: Some("gpt-4o-mini".to_string()),
        }
    }
}

// ── NPC Coordinator ────────────────────────────────────────────────────

/// Central orchestrator for the NPC coworker.
///
/// Manages the lifecycle state machine and wires together the screen reader,
/// prompt builder, conversation memory, and voice pipeline.
///
/// Phase 1 supports text-only interaction via `ask_text()`.
/// Phase 2 adds voice via `start_listening()` / `stop_listening()`.
pub struct NpcCoordinator {
    state_tx: watch::Sender<NpcState>,
    state_rx: watch::Receiver<NpcState>,
    screen_reader: Arc<ScreenReader>,
    conversation: Arc<Mutex<ConversationMemory>>,
    pipeline: Arc<VoicePipeline>,
    task_machine: Arc<std::sync::Mutex<TaskMachine>>,
    /// API keys used to build providers at runtime (model-swap path).
    api_keys: Arc<ApiKeys>,
    /// Tauri AppHandle — installed after Tauri finishes `.setup()` via
    /// `set_app_handle`. Needed by the overlay driver to emit events and
    /// show/hide the overlay window.
    app_handle: OnceLock<AppHandle>,
    /// Handle to the pending overlay auto-hide timer. Each new overlay
    /// command batch cancels the prior timer so the 15 s countdown restarts.
    auto_hide: AutoHideState,
}

impl NpcCoordinator {
    /// Create a new coordinator.
    ///
    /// `task_machine` is shared with the Tauri command layer so both the NPC
    /// and the task commands can read/write the current task state.
    pub fn new(
        pipeline: VoicePipeline,
        task_machine: Arc<std::sync::Mutex<TaskMachine>>,
        api_keys: Arc<ApiKeys>,
    ) -> Self {
        let (state_tx, state_rx) = watch::channel(NpcState::Idle);
        Self {
            state_tx,
            state_rx,
            screen_reader: Arc::new(ScreenReader::new()),
            conversation: Arc::new(Mutex::new(ConversationMemory::new(20))),
            pipeline: Arc::new(pipeline),
            task_machine,
            api_keys,
            app_handle: OnceLock::new(),
            auto_hide: overlay_driver::new_auto_hide_state(),
        }
    }

    /// Install the Tauri `AppHandle` so the overlay driver can emit events
    /// and show/hide the overlay window, and the screen reader can push
    /// `context-update` events to the frontend. Called once from `lib.rs`
    /// inside `.setup(...)`. Subsequent calls are no-ops.
    pub fn set_app_handle(&self, app: AppHandle) {
        self.screen_reader.set_app_handle(app.clone());
        let _ = self.app_handle.set(app);
    }

    /// Internal: dispatch a batch of overlay commands if the AppHandle is
    /// installed. Logs and swallows errors — overlay failures must never
    /// break a turn.
    fn dispatch_overlay(&self, commands: &[OverlayCommand]) {
        if commands.is_empty() {
            return;
        }
        let Some(app) = self.app_handle.get() else {
            eprintln!(
                "[z-ro:npc] overlay: {} command(s) dropped — AppHandle not yet installed",
                commands.len()
            );
            return;
        };
        if let Err(e) = overlay_driver::apply(app, commands, &self.auto_hide) {
            eprintln!("[z-ro:npc] overlay apply failed: {}", e);
        }
    }

    /// List all LLM models that can be selected (filtered by configured API keys).
    pub fn list_available_models(&self) -> Vec<ModelInfo> {
        llm_providers::available_models(&self.api_keys)
    }

    /// Hot-swap the active LLM provider. The next turn will use it.
    pub fn set_llm(&self, provider: &str, model: &str) -> Result<(), String> {
        let llm = llm_providers::build_provider(provider, model, &self.api_keys)?;
        self.pipeline.set_llm(llm);
        println!(
            "[z-ro:npc] LLM swapped → {}:{}",
            provider, model
        );
        Ok(())
    }

    // ── Public API ─────────────────────────────────────────────────────

    /// Start a conversation session (called when user activates NPC).
    pub async fn activate(&self) -> Result<(), String> {
        self.set_state(NpcState::Idle);
        println!("[z-ro:npc] Activated (model: {})", self.pipeline.llm_name());
        Ok(())
    }

    /// Stop listening and reset to idle.
    pub async fn deactivate(&self) -> Result<(), String> {
        self.set_state(NpcState::Idle);
        println!("[z-ro:npc] Deactivated");
        Ok(())
    }

    /// Process a text question (Phase 1 primary interaction path).
    ///
    /// 1. Capture screen context
    /// 2. Read task state
    /// 3. Stream through LLM pipeline
    /// 4. Store conversation turn
    /// 5. Return assistant response
    pub async fn ask_text(&self, question: String) -> Result<String, String> {
        self.set_state(NpcState::Thinking);
        let start = std::time::Instant::now();

        // Capture screen context
        let screen = self.screen_reader.capture().unwrap_or_else(|e| {
            eprintln!("[z-ro:npc] Screen capture failed: {}", e);
            super::screen_reader::ScreenContext {
                window: crate::context::WindowInfo {
                    title: "Unknown".to_string(),
                    process_name: "Unknown".to_string(),
                    bundle_id: None,
                    pid: None,
                },
                ax_tree: None,
                screenshot_b64: None,
                captured_at_ms: 0,
            }
        });

        // Read task state
        let task_state = self
            .task_machine
            .lock()
            .map_err(|e| format!("TaskMachine lock failed: {}", e))?
            .get_state();

        // Get conversation history
        let history: Vec<ConversationTurn>;
        {
            let mem = self.conversation.lock().await;
            history = mem.recent(10).to_vec();
        }

        // Process through pipeline
        let result = self
            .pipeline
            .process_text_turn(
                &question,
                &screen,
                task_state.as_ref(),
                &history,
            )
            .await;

        match result {
            Ok(turn_result) => {
                let elapsed = start.elapsed();
                println!(
                    "[z-ro:npc] pipeline | total={}ms",
                    elapsed.as_millis()
                );

                // Store the conversation turns
                let now_ms = now_millis();
                let screen_summary = format!(
                    "{} - {}",
                    screen.window.process_name, screen.window.title
                );
                {
                    let mut mem = self.conversation.lock().await;
                    mem.push(ConversationTurn {
                        role: "user".to_string(),
                        content: question,
                        timestamp_ms: now_ms,
                        screen_summary: Some(screen_summary.clone()),
                    });
                    mem.push(ConversationTurn {
                        role: "assistant".to_string(),
                        content: turn_result.assistant_text.clone(),
                        timestamp_ms: now_ms + 1,
                        screen_summary: Some(screen_summary),
                    });
                }

                // Push any overlay commands the LLM emitted through to the
                // overlay window. Safe to call on an empty Vec — it's a no-op.
                self.dispatch_overlay(&turn_result.overlay_commands);

                self.set_state(NpcState::Idle);
                Ok(turn_result.assistant_text)
            }
            Err(e) => {
                eprintln!("[z-ro:npc] Pipeline error: {}", e);
                self.set_state(NpcState::Error(e.clone()));
                Err(e)
            }
        }
    }

    /// Get current NPC status for frontend.
    pub fn get_status(&self) -> NpcStatus {
        let state = self.state_rx.borrow().clone();
        let model_name = self.pipeline.llm_name();

        // Read task info without panicking on lock failure
        let (active_task, current_step) = self
            .task_machine
            .lock()
            .ok()
            .and_then(|tm| {
                tm.get_state().map(|ts| {
                    (
                        Some(ts.task_id),
                        Some(ts.current_step.instruction),
                    )
                })
            })
            .unwrap_or((None, None));

        // Conversation turn count — use try_lock to avoid blocking
        let conversation_turns = self
            .conversation
            .try_lock()
            .map(|mem| mem.len())
            .unwrap_or(0);

        NpcStatus {
            state,
            active_task,
            current_step,
            model_name,
            conversation_turns,
        }
    }

    /// Process a voice question: audio in → transcript → LLM → audio out.
    ///
    /// Mirrors `ask_text` but:
    /// - Transitions through `Listening` → `Thinking` → `Speaking`
    /// - Takes raw audio bytes (any format supported by STT provider)
    /// - Returns both the assistant text and the synthesized reply audio
    pub async fn ask_voice(
        &self,
        audio_bytes: Vec<u8>,
        audio_filename: String,
    ) -> Result<VoiceAskResult, String> {
        use base64::Engine;

        self.set_state(NpcState::Thinking);
        let start = std::time::Instant::now();

        // Capture screen context
        let screen = self.screen_reader.capture().unwrap_or_else(|e| {
            eprintln!("[z-ro:npc] Screen capture failed: {}", e);
            super::screen_reader::ScreenContext {
                window: crate::context::WindowInfo {
                    title: "Unknown".to_string(),
                    process_name: "Unknown".to_string(),
                    bundle_id: None,
                    pid: None,
                },
                ax_tree: None,
                screenshot_b64: None,
                captured_at_ms: 0,
            }
        });

        // Read task state
        let task_state = self
            .task_machine
            .lock()
            .map_err(|e| format!("TaskMachine lock failed: {}", e))?
            .get_state();

        // Get conversation history
        let history: Vec<ConversationTurn> = {
            let mem = self.conversation.lock().await;
            mem.recent(10).to_vec()
        };

        // Run the full voice pipeline
        let voice_result = self
            .pipeline
            .process_voice_turn(
                &audio_bytes,
                &audio_filename,
                &screen,
                task_state.as_ref(),
                &history,
            )
            .await;

        match voice_result {
            Ok(vt) => {
                let elapsed = start.elapsed();
                println!(
                    "[z-ro:npc] voice pipeline | total={}ms transcript=\"{}\" audio={}B",
                    elapsed.as_millis(),
                    vt.turn.user_transcript,
                    vt.audio_bytes.len()
                );

                // Store conversation turns
                let now_ms = now_millis();
                let screen_summary =
                    format!("{} - {}", screen.window.process_name, screen.window.title);
                {
                    let mut mem = self.conversation.lock().await;
                    mem.push(ConversationTurn {
                        role: "user".to_string(),
                        content: vt.turn.user_transcript.clone(),
                        timestamp_ms: now_ms,
                        screen_summary: Some(screen_summary.clone()),
                    });
                    mem.push(ConversationTurn {
                        role: "assistant".to_string(),
                        content: vt.turn.assistant_text.clone(),
                        timestamp_ms: now_ms + 1,
                        screen_summary: Some(screen_summary),
                    });
                }

                // Push any overlay commands the LLM emitted through to the
                // overlay window. Safe to call on an empty Vec — it's a no-op.
                self.dispatch_overlay(&vt.turn.overlay_commands);

                // Briefly transition through Speaking so the frontend can
                // show the audio indicator, then back to Idle.
                if !vt.audio_bytes.is_empty() {
                    self.set_state(NpcState::Speaking);
                }
                let audio_b64 = if vt.audio_bytes.is_empty() {
                    String::new()
                } else {
                    base64::engine::general_purpose::STANDARD.encode(&vt.audio_bytes)
                };
                self.set_state(NpcState::Idle);

                Ok(VoiceAskResult {
                    user_transcript: vt.turn.user_transcript,
                    assistant_text: vt.turn.assistant_text,
                    audio_b64,
                    audio_mime: vt.audio_mime,
                })
            }
            Err(e) => {
                eprintln!("[z-ro:npc] Voice pipeline error: {}", e);
                self.set_state(NpcState::Error(e.clone()));
                Err(e)
            }
        }
    }

    /// Interrupt the NPC.  Resets state to Idle — the frontend is
    /// responsible for stopping any in-flight audio playback on its side.
    pub async fn interrupt(&self) -> Result<(), String> {
        self.set_state(NpcState::Idle);
        Ok(())
    }

    /// Clear conversation history (new session).
    pub async fn clear_conversation(&self) {
        self.conversation.lock().await.clear();
    }

    // ── Listening state hooks ──────────────────────────────────────────
    //
    // Actual mic capture happens on the frontend via MediaRecorder — these
    // commands just update the state so the UI can show the right indicator.
    // The audio bytes are delivered later through `ask_voice`.

    /// Frontend started capturing mic audio.  Updates state to Listening.
    pub async fn start_listening(&self) -> Result<(), String> {
        if !self.pipeline.has_stt() {
            return Err("STT provider not configured (set GROQ_API_KEY)".to_string());
        }
        self.set_state(NpcState::Listening);
        Ok(())
    }

    /// Frontend stopped capturing (before bytes arrive).  Returns to Idle;
    /// `ask_voice` will drive the next state transitions.
    pub async fn stop_listening(&self) -> Result<(), String> {
        self.set_state(NpcState::Idle);
        Ok(())
    }

    // ── Internal ───────────────────────────────────────────────────────

    fn set_state(&self, state: NpcState) {
        let _ = self.state_tx.send(state);
    }
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::npc::prompt_builder::LlmMessage;
    use crate::npc::voice::pipeline::{LlmProvider, VoicePipeline};
    use async_trait::async_trait;
    use std::path::PathBuf;
    use tokio::sync::mpsc;

    /// Mock LLM that returns a fixed response.
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
            let resp = self.response.clone();
            tokio::spawn(async move {
                let _ = tx.send(resp).await;
            });
            Ok(rx)
        }
        fn name(&self) -> &str {
            "mock"
        }
    }

    fn make_coordinator(response: &str) -> NpcCoordinator {
        let llm = MockLlm {
            response: response.to_string(),
        };
        let pipeline = VoicePipeline::new(None, None, Arc::new(llm));
        // Use an empty dir — no tasks loaded, that's fine
        let tm = Arc::new(std::sync::Mutex::new(TaskMachine::new(PathBuf::from(
            "/nonexistent",
        ))));
        NpcCoordinator::new(pipeline, tm, Arc::new(ApiKeys::default()))
    }

    #[test]
    fn test_initial_state_is_idle() {
        let coord = make_coordinator("hello");
        let status = coord.get_status();
        assert_eq!(status.state, NpcState::Idle);
        assert_eq!(status.model_name, "mock");
        assert_eq!(status.conversation_turns, 0);
        assert!(status.active_task.is_none());
    }

    #[tokio::test]
    async fn test_activate_and_deactivate() {
        let coord = make_coordinator("hello");
        coord.activate().await.unwrap();
        assert_eq!(coord.get_status().state, NpcState::Idle);
        coord.deactivate().await.unwrap();
        assert_eq!(coord.get_status().state, NpcState::Idle);
    }

    #[tokio::test]
    async fn test_ask_text_returns_response() {
        let coord = make_coordinator("I can see your screen!");
        coord.activate().await.unwrap();

        let response = coord
            .ask_text("What do you see?".to_string())
            .await
            .unwrap();

        assert!(response.contains("I can see your screen!"));
    }

    #[tokio::test]
    async fn test_ask_text_increments_conversation_turns() {
        let coord = make_coordinator("Sure thing.");
        coord.activate().await.unwrap();

        assert_eq!(coord.get_status().conversation_turns, 0);
        coord.ask_text("Hello".to_string()).await.unwrap();
        // Each ask_text adds 2 turns: user + assistant
        assert_eq!(coord.get_status().conversation_turns, 2);

        coord.ask_text("Again".to_string()).await.unwrap();
        assert_eq!(coord.get_status().conversation_turns, 4);
    }

    #[tokio::test]
    async fn test_ask_text_state_returns_to_idle() {
        let coord = make_coordinator("Done.");
        coord.ask_text("Test".to_string()).await.unwrap();
        assert_eq!(coord.get_status().state, NpcState::Idle);
    }

    #[tokio::test]
    async fn test_interrupt_resets_to_idle() {
        let coord = make_coordinator("hello");
        coord.interrupt().await.unwrap();
        assert_eq!(coord.get_status().state, NpcState::Idle);
    }

    #[tokio::test]
    async fn test_clear_conversation() {
        let coord = make_coordinator("Hello.");
        coord.ask_text("Hi".to_string()).await.unwrap();
        assert_eq!(coord.get_status().conversation_turns, 2);

        coord.clear_conversation().await;
        assert_eq!(coord.get_status().conversation_turns, 0);
    }

    #[tokio::test]
    async fn test_start_listening_errors_without_stt() {
        // Coordinator built with MockLlm but no STT — should refuse to
        // transition to Listening.
        let coord = make_coordinator("hello");
        let result = coord.start_listening().await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("STT"));
        // State should remain Idle
        assert_eq!(coord.get_status().state, NpcState::Idle);
    }

    #[tokio::test]
    async fn test_stop_listening_resets_to_idle() {
        let coord = make_coordinator("hello");
        coord.stop_listening().await.unwrap();
        assert_eq!(coord.get_status().state, NpcState::Idle);
    }

    #[tokio::test]
    async fn test_ask_voice_errors_without_stt() {
        let coord = make_coordinator("hello");
        let err = coord
            .ask_voice(vec![1, 2, 3], "audio.webm".to_string())
            .await
            .unwrap_err();
        assert!(err.contains("STT"));
    }
}
