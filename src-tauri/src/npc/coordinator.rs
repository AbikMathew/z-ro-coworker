use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::{watch, Mutex};

use crate::task_engine::machine::TaskMachine;

use super::conversation::{ConversationMemory, ConversationTurn};
use super::screen_reader::ScreenReader;
use super::voice::pipeline::VoicePipeline;

// ── Public types ───────────────────────────────────────────────────────

/// NPC lifecycle states.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NpcState {
    /// Waiting in tray, not listening.
    Idle,
    /// Mic is hot, capturing audio (Phase 2).
    Listening,
    /// Processing user input through the pipeline.
    Thinking,
    /// TTS audio playing back (Phase 2).
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
}

impl NpcCoordinator {
    /// Create a new coordinator.
    ///
    /// `task_machine` is shared with the Tauri command layer so both the NPC
    /// and the task commands can read/write the current task state.
    pub fn new(pipeline: VoicePipeline, task_machine: Arc<std::sync::Mutex<TaskMachine>>) -> Self {
        let (state_tx, state_rx) = watch::channel(NpcState::Idle);
        Self {
            state_tx,
            state_rx,
            screen_reader: Arc::new(ScreenReader::new()),
            conversation: Arc::new(Mutex::new(ConversationMemory::new(20))),
            pipeline: Arc::new(pipeline),
            task_machine,
        }
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
        let model_name = self.pipeline.llm_name().to_string();

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

    /// Interrupt the NPC (Phase 2: stop TTS playback).
    pub async fn interrupt(&self) -> Result<(), String> {
        self.set_state(NpcState::Idle);
        Ok(())
    }

    /// Clear conversation history (new session).
    pub async fn clear_conversation(&self) {
        self.conversation.lock().await.clear();
    }

    // ── Phase 2 stubs ──────────────────────────────────────────────────

    /// Push-to-talk: start capturing mic audio (Phase 2).
    pub async fn start_listening(&self) -> Result<(), String> {
        Err("Voice input not yet implemented (Phase 2)".to_string())
    }

    /// Push-to-talk: stop capturing, trigger pipeline (Phase 2).
    pub async fn stop_listening(&self) -> Result<(), String> {
        Err("Voice input not yet implemented (Phase 2)".to_string())
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
        let pipeline = VoicePipeline::new(None, None, Box::new(llm));
        // Use an empty dir — no tasks loaded, that's fine
        let tm = Arc::new(std::sync::Mutex::new(TaskMachine::new(PathBuf::from(
            "/nonexistent",
        ))));
        NpcCoordinator::new(pipeline, tm)
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
    async fn test_start_listening_returns_phase2_error() {
        let coord = make_coordinator("hello");
        let result = coord.start_listening().await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Phase 2"));
    }

    #[tokio::test]
    async fn test_stop_listening_returns_phase2_error() {
        let coord = make_coordinator("hello");
        let result = coord.stop_listening().await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Phase 2"));
    }
}
