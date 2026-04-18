<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { goto } from "$app/navigation";
  import { onMount } from "svelte";
  import { fade } from "svelte/transition";
  import ActiveTaskLayout from "$lib/components/task/ActiveTaskLayout.svelte";
  import {
    taskState,
    advanceStep,
    refreshTaskState,
    isLoading,
  } from "$lib/stores/taskStore";
  import type { Task, TaskState } from "$lib/types/task";

  // Route contract: if no active task, bounce home. The dashboard owns
  // the "start a task" flow — this page just renders whatever is active.
  //
  // Variable is named `progress` (not `state`) to avoid colliding with
  // Svelte 5's `$state` rune naming heuristics.
  let progress = $state<TaskState | null>(null);
  let loading = $state(false);
  let allTasks = $state<Task[]>([]);
  let bootstrapped = $state(false);

  taskState.subscribe((v) => (progress = v));
  isLoading.subscribe((v) => (loading = v));

  onMount(async () => {
    // Load the full task catalog so we can match `progress.task_id` →
    // Task (the backend returns just the TaskState, not the full Task).
    try {
      allTasks = await invoke<Task[]>("list_tasks");
    } catch (e) {
      console.warn("[task] list_tasks failed:", e);
    }
    // Ensure the store reflects the *backend* truth (in case user
    // navigated here directly after a reload).
    await refreshTaskState();

    bootstrapped = true;

    if (!progress) {
      // Nothing in progress — kick user back to the dashboard.
      goto("/", { replaceState: true });
    }
  });

  // Resolve the full Task from the in-memory catalog. `progress.task_id`
  // is authoritative, but we need the full Task object to render steps
  // with hints, xp, validation rules, etc.
  let currentTask = $derived<Task | null>(
    progress ? (allTasks.find((t) => t.id === progress!.task_id) ?? null) : null,
  );

  async function handleAdvance() {
    await advanceStep();
  }
</script>

{#if !bootstrapped}
  <div class="placeholder" in:fade={{ duration: 150 }}>
    <p>Loading your task…</p>
  </div>
{:else if !progress || !currentTask}
  <div class="placeholder" in:fade={{ duration: 150 }}>
    <h2 class="display">No active task</h2>
    <p>Pick a task from the dashboard to get started.</p>
    <a class="btn" href="/">← Back to dashboard</a>
  </div>
{:else}
  <div in:fade={{ duration: 220 }}>
    <ActiveTaskLayout
      task={currentTask}
      progress={progress}
      onadvance={handleAdvance}
      {loading}
    />
  </div>
{/if}

<style>
  .placeholder {
    margin: 64px auto;
    max-width: 520px;
    text-align: center;
    padding: 40px;
    border-radius: 18px;
    border: 1px dashed var(--border-strong);
    background: var(--bg-card);
  }
  .placeholder h2 {
    font-size: 22px;
    margin: 0 0 8px;
  }
  .placeholder p {
    color: var(--fg-muted);
    margin: 0 0 20px;
    font-size: 14px;
  }
  .btn {
    display: inline-block;
    padding: 10px 18px;
    border-radius: 10px;
    background: var(--accent);
    color: white;
    text-decoration: none;
    font-weight: 600;
    font-size: 14px;
    box-shadow: 0 6px 18px -6px rgba(124, 58, 237, 0.6);
  }
</style>
