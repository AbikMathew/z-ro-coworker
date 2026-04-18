<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { goto } from "$app/navigation";
  import { onMount } from "svelte";
  import { fly } from "svelte/transition";
  import HeroHeader from "$lib/components/dashboard/HeroHeader.svelte";
  import TaskCard from "$lib/components/dashboard/TaskCard.svelte";
  import { startTask, taskState } from "$lib/stores/taskStore";
  import type { Task, TaskState } from "$lib/types/task";

  // Catalog of all tasks loaded from the `tasks/` directory on startup.
  let tasks: Task[] = $state([]);
  let loading = $state(true);
  let error = $state<string | null>(null);

  // Mirror the active-task store so we can deep-link to /task if there is
  // one in progress (rather than silently re-starting the same task).
  let current: TaskState | null = $state(null);
  taskState.subscribe((v) => (current = v));

  onMount(async () => {
    try {
      tasks = await invoke<Task[]>("list_tasks");
    } catch (e) {
      error = `Failed to load tasks: ${e}`;
      console.error(error);
    } finally {
      loading = false;
    }
  });

  async function handleStart(taskId: string) {
    // If this same task is already active, just jump to it. If a
    // DIFFERENT one is active, still start the new one — the backend
    // replaces the current machine state.
    if (!current || current.task_id !== taskId) {
      await startTask(taskId);
    }
    await goto("/task");
  }

  // Group tasks by category for the section headers. Keeps the grid
  // scannable — kids can scan "Git" or "Coding" without reading titles.
  let grouped = $derived.by(() => {
    const groups = new Map<string, Task[]>();
    const order = [
      "git",
      "coding",
      "jira",
      "communication",
      "productivity",
      "other",
    ];
    for (const t of tasks) {
      const key = t.category ?? "other";
      if (!groups.has(key)) groups.set(key, []);
      groups.get(key)!.push(t);
    }
    return order
      .map((k) => ({ category: k, tasks: groups.get(k) ?? [] }))
      .filter((g) => g.tasks.length > 0);
  });

  function categoryTitle(c: string): string {
    switch (c) {
      case "git": return "Version control";
      case "coding": return "Coding practice";
      case "jira": return "Project management";
      case "communication": return "Team communication";
      case "productivity": return "Productivity basics";
      default: return "More tasks";
    }
  }
</script>

<HeroHeader totalXp={current?.xp_earned ?? 0} streak={1} userName="Friend" />

{#if loading}
  <div class="status-box">
    <div class="skeleton"></div>
    <div class="skeleton"></div>
    <div class="skeleton"></div>
  </div>
{:else if error}
  <div class="status-box error">{error}</div>
{:else if tasks.length === 0}
  <div class="status-box">
    No tasks found. Make sure the <code>tasks/</code> directory has JSON files.
  </div>
{:else}
  {#each grouped as group, gi (group.category)}
    <section
      class="task-section"
      in:fly={{ y: 18, duration: 320, delay: gi * 60 }}
    >
      <div class="section-head">
        <h2 class="display section-title">{categoryTitle(group.category)}</h2>
        <span class="section-count">
          {group.tasks.length} task{group.tasks.length === 1 ? "" : "s"}
        </span>
      </div>
      <div class="grid">
        {#each group.tasks as task, ti (task.id)}
          <div in:fly={{ y: 20, duration: 320, delay: gi * 60 + ti * 40 }}>
            <TaskCard {task} onstart={handleStart} />
          </div>
        {/each}
      </div>
    </section>
  {/each}
{/if}

<style>
  .task-section {
    margin-top: 36px;
  }
  .section-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    margin-bottom: 18px;
  }
  .section-title {
    font-size: 20px;
    font-weight: 700;
    margin: 0;
  }
  .section-count {
    font-size: 12px;
    color: var(--fg-dim);
    letter-spacing: 0.04em;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
    gap: 20px;
  }

  .status-box {
    margin-top: 32px;
    padding: 20px;
    border-radius: 16px;
    border: 1px solid var(--border);
    background: var(--bg-card);
    color: var(--fg-muted);
    font-size: 14px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .status-box.error { color: #fca5a5; border-color: rgba(239, 68, 68, 0.4); }

  .skeleton {
    height: 64px;
    border-radius: 12px;
    background: linear-gradient(
      90deg,
      rgba(255, 255, 255, 0.03),
      rgba(255, 255, 255, 0.06),
      rgba(255, 255, 255, 0.03)
    );
    background-size: 200% 100%;
    animation: skele 1.6s ease-in-out infinite;
  }
  @keyframes skele {
    0% { background-position: 0% 50%; }
    100% { background-position: -200% 50%; }
  }
</style>
