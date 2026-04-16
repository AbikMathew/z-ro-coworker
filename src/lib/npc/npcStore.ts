import { writable, derived } from "svelte/store";
import { invoke } from "@tauri-apps/api/core";
import type { NpcStatus, NpcState } from "./types";

/** Reactive NPC status store. */
export const npcStatus = writable<NpcStatus>({
  state: "Idle",
  active_task: null,
  current_step: null,
  model_name: "unknown",
  conversation_turns: 0,
});

/** Convenience: just the state string. */
export const npcState = derived(npcStatus, ($s) => $s.state);

// ── Tauri invoke wrappers ──────────────────────────────────────────

export async function activateNpc(): Promise<void> {
  await invoke("npc_activate");
  await refreshStatus();
}

export async function deactivateNpc(): Promise<void> {
  await invoke("npc_deactivate");
  await refreshStatus();
}

export async function askText(question: string): Promise<string> {
  npcStatus.update((s) => ({ ...s, state: "Thinking" }));
  try {
    const response = await invoke<string>("npc_ask_text", { question });
    await refreshStatus();
    return response;
  } catch (e) {
    npcStatus.update((s) => ({ ...s, state: { Error: String(e) } }));
    throw e;
  }
}

export async function interruptNpc(): Promise<void> {
  await invoke("npc_interrupt");
  await refreshStatus();
}

export async function refreshStatus(): Promise<void> {
  try {
    const status = await invoke<NpcStatus>("npc_get_status");
    npcStatus.set(status);
  } catch (e) {
    console.error("[npc] Failed to refresh status:", e);
  }
}
