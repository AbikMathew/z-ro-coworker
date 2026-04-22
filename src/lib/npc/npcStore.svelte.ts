import { invoke } from "@tauri-apps/api/core";
import type { NpcStatus, NpcState, VoiceAskResult, ModelInfo } from "./types";

/** localStorage key for the user's selected provider/model. */
const LS_MODEL_KEY = "zro:npc:model";

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

  /**
   * Tell the backend whether continuous on-air voice mode is active. The
   * frontend owns the mic + VAD; the backend just remembers the state so
   * later phases (proactive speech, WrongMove) can condition on it.
   * Fire-and-forget — a failed IPC call shouldn't block the UI toggle.
   */
  async setOnAir(active: boolean): Promise<void> {
    try {
      await invoke("npc_set_on_air", { active });
    } catch (e) {
      console.warn("[npc] setOnAir failed:", e);
    }
  }

  /**
   * Mute Zee's proactive (milestone/WrongMove) speech on the backend.
   * Manual asks are unaffected. Fire-and-forget.
   */
  async setProactiveMuted(muted: boolean): Promise<void> {
    try {
      await invoke("npc_set_proactive_muted", { muted });
    } catch (e) {
      console.warn("[npc] setProactiveMuted failed:", e);
    }
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

  // ── Model management ─────────────────────────────────────────────────
  //
  // Populated lazily from the backend (filtered by which API keys are
  // configured). The Settings page reads `availableModels` and calls
  // `setModel()` on change; selection is persisted to localStorage so the
  // choice survives restarts even though the Rust side defaults back to
  // its env-configured provider on boot.

  /** Cached list of selectable models (filled by `listModels()`). */
  availableModels: ModelInfo[] = $state([]);

  /** Fetch the selectable model list from the backend. */
  async listModels(): Promise<ModelInfo[]> {
    try {
      this.availableModels = await invoke<ModelInfo[]>("npc_list_models");
      return this.availableModels;
    } catch (e) {
      console.error("[npc] Failed to list models:", e);
      return [];
    }
  }

  /**
   * Hot-swap the active LLM. Persists the choice to localStorage so the
   * Settings UI can re-select it after a reload.
   */
  async setModel(provider: string, model: string): Promise<void> {
    await invoke("npc_set_model", { provider, model });
    await this.refresh();
    try {
      localStorage.setItem(
        LS_MODEL_KEY,
        JSON.stringify({ provider, model }),
      );
    } catch {
      /* localStorage may be unavailable — non-fatal */
    }
  }

  /**
   * Read the persisted `{provider, model}` selection, or `null` if the
   * user hasn't changed the default. Called by Settings on mount.
   */
  getPersistedModel(): { provider: string; model: string } | null {
    try {
      const raw = localStorage.getItem(LS_MODEL_KEY);
      if (!raw) return null;
      const parsed = JSON.parse(raw);
      if (typeof parsed?.provider === "string" && typeof parsed?.model === "string") {
        return parsed;
      }
    } catch {
      /* corrupted entry — ignore */
    }
    return null;
  }
}

/** Singleton store instance. Import this in components. */
export const npc = new NpcStore();
