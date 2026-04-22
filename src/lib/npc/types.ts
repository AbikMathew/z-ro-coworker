/** NPC lifecycle states — mirrors Rust `NpcState` enum. */
export type NpcState = "Idle" | "Listening" | "Thinking" | "Speaking" | { Error: string };

/** Snapshot of the NPC's current status. */
export interface NpcStatus {
  state: NpcState;
  active_task: string | null;
  current_step: string | null;
  model_name: string;
  conversation_turns: number;
}

/** A single message in the NPC chat panel. */
export interface ChatMessage {
  role: "user" | "npc";
  content: string;
}

/** Result of a voice turn (STT → LLM → TTS). */
export interface VoiceAskResult {
  user_transcript: string;
  assistant_text: string;
  /** Base64-encoded audio bytes; empty if TTS is disabled. */
  audio_b64: string;
  /** MIME type of `audio_b64` (e.g. "audio/mpeg"); empty when audio is empty. */
  audio_mime: string;
}

/**
 * One LLM option the user can pick from in Settings — mirrors Rust
 * `ModelInfo` from `src-tauri/src/npc/llm_providers/mod.rs`. Only models
 * whose API key is configured are returned by the backend.
 */
export interface ModelInfo {
  /** Provider id: "openai" | "gemini" | "anthropic". */
  provider: string;
  /** The API-level model id (e.g. "gpt-4o-mini", "claude-haiku-4-5"). */
  model: string;
  /** Human-readable name for the Settings UI. */
  display_name: string;
  /** Does this model support vision? (screenshots as input) */
  vision: boolean;
  /** "cheap" | "mid" | "premium" — used to render the cost chip. */
  cost_tier: string;
  /** Short hint explaining trade-offs ("best UI vision", "fastest", etc). */
  notes: string;
}
