import { invoke } from "@tauri-apps/api/core";
import type { NpcStatus, NpcState, VoiceAskResult } from "./types";

/**
 * Reactive NPC store built on Svelte 5 runes.
 *
 * Why a class + single module-level instance?
 * - `$state` can't be exported directly from a module (ES binding is
 *   immutable), so we wrap it in a class whose fields are reactive.
 * - Components that import `npc` get deep reactivity automatically, no
 *   `subscribe()` call or manual unsubscribe needed — this fixes the
 *   subscription leak from the previous writable-store design.
 */
class NpcStore {
  /** Current NPC status snapshot. Mutated in place for deep reactivity. */
  status: NpcStatus = $state({
    state: "Idle",
    active_task: null,
    current_step: null,
    model_name: "unknown",
    conversation_turns: 0,
  });

  /** Convenience accessor for just the state variant. */
  get state(): NpcState {
    return this.status.state;
  }

  /** Pull the latest status from Rust and update in place. */
  async refresh(): Promise<void> {
    try {
      const next = await invoke<NpcStatus>("npc_get_status");
      this.status.state = next.state;
      this.status.active_task = next.active_task;
      this.status.current_step = next.current_step;
      this.status.model_name = next.model_name;
      this.status.conversation_turns = next.conversation_turns;
    } catch (e) {
      console.error("[npc] Failed to refresh status:", e);
    }
  }

  async activate(): Promise<void> {
    await invoke("npc_activate");
    await this.refresh();
  }

  async deactivate(): Promise<void> {
    await invoke("npc_deactivate");
    await this.refresh();
  }

  async interrupt(): Promise<void> {
    await invoke("npc_interrupt");
    await this.refresh();
  }

  async startListening(): Promise<void> {
    await invoke("npc_start_listening");
    await this.refresh();
  }

  async stopListening(): Promise<void> {
    await invoke("npc_stop_listening");
    await this.refresh();
  }

  async askText(question: string): Promise<string> {
    this.status.state = "Thinking";
    try {
      const response = await invoke<string>("npc_ask_text", { question });
      await this.refresh();
      return response;
    } catch (e) {
      this.status.state = { Error: String(e) };
      throw e;
    }
  }

  /**
   * Process a voice turn: send recorded audio (base64) to Rust, receive
   * transcript + assistant text + TTS audio (base64 MP3).
   *
   * `filename` gives the STT provider a hint at the container/codec
   * (e.g. `"audio.webm"` for MediaRecorder output).
   */
  async askVoice(audioB64: string, filename: string): Promise<VoiceAskResult> {
    this.status.state = "Thinking";
    try {
      const result = await invoke<VoiceAskResult>("npc_ask_voice", {
        audioB64,
        filename,
      });
      await this.refresh();
      return result;
    } catch (e) {
      this.status.state = { Error: String(e) };
      throw e;
    }
  }
}

/** Singleton store instance. Import this in components. */
export const npc = new NpcStore();
