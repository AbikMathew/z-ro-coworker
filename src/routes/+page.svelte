<script lang="ts">
  import TaskPanel from "$lib/components/TaskPanel.svelte";
  import NPCChat from "$lib/components/NPCChat.svelte";
  import ProgressBar from "$lib/components/ProgressBar.svelte";
  import { taskState, startTask } from "$lib/stores/taskStore";
  import type { TaskState } from "$lib/types/task";

  let state: TaskState | null = $state(null);
  taskState.subscribe((v) => (state = v));

  async function handleStartTask() {
    await startTask("navigate_filesystem");
  }
</script>

<main class="min-h-screen bg-gray-950 text-white">
  <div class="max-w-4xl mx-auto p-6">
    <!-- Header -->
    <div class="text-center mb-8">
      <h1 class="text-4xl font-bold tracking-tight">Z-RO Cowork</h1>
      <p class="text-gray-400 mt-1">Your AI-powered co-worker</p>
    </div>

    {#if !state}
      <!-- No active task — show welcome + start -->
      <div class="max-w-md mx-auto text-center">
        <div class="bg-gray-900 rounded-2xl p-6 border border-gray-800 mb-6">
          <div class="flex items-center gap-3 mb-4 justify-center">
            <div class="w-10 h-10 rounded-full bg-blue-600 flex items-center justify-center text-sm font-bold">Z</div>
            <div class="text-left">
              <p class="font-medium">Zee</p>
              <p class="text-gray-500 text-sm">Ready to help</p>
            </div>
          </div>
          <p class="text-gray-300 text-sm">
            Hey! I'm Zee, your virtual co-worker. Start a task and I'll guide you through it — just like a real colleague would.
          </p>
        </div>

        <button
          onclick={handleStartTask}
          class="bg-blue-600 hover:bg-blue-700 text-white px-6 py-3 rounded-xl font-medium transition-colors"
        >
          Start: Navigate File System
        </button>
      </div>
    {:else}
      <!-- Active task view -->
      <div class="grid grid-cols-1 lg:grid-cols-2 gap-6">
        <div class="space-y-4">
          <TaskPanel />
          <ProgressBar xp={state.xp_earned} />
        </div>
        <div>
          <NPCChat />
        </div>
      </div>
    {/if}
  </div>
</main>
