import { writable } from "svelte/store";
import { invoke } from "@tauri-apps/api/core";
import type { TaskState } from "$lib/types/task";

export const taskState = writable<TaskState | null>(null);
export const isLoading = writable(false);

export async function startTask(taskId: string) {
  isLoading.set(true);
  try {
    const state = await invoke<TaskState>("start_task", { taskId });
    taskState.set(state);
  } catch (e) {
    console.error("Failed to start task:", e);
  } finally {
    isLoading.set(false);
  }
}

export async function advanceStep() {
  isLoading.set(true);
  try {
    const state = await invoke<TaskState>("advance_step");
    taskState.set(state);
  } catch (e) {
    console.error("Failed to advance step:", e);
  } finally {
    isLoading.set(false);
  }
}

export async function refreshTaskState() {
  try {
    const state = await invoke<TaskState | null>("get_task_state");
    taskState.set(state);
  } catch (e) {
    console.error("Failed to get task state:", e);
  }
}
