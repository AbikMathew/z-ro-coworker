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
