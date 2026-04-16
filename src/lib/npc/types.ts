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
