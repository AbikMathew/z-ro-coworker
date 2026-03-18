<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import TaskPanel from "$lib/components/TaskPanel.svelte";
  import NPCChat from "$lib/components/NPCChat.svelte";
  import ProgressBar from "$lib/components/ProgressBar.svelte";
  import { taskState, startTask } from "$lib/stores/taskStore";
  import type { TaskState } from "$lib/types/task";
  import type { WindowInfo } from "$lib/types/context";

  let state: TaskState | null = $state(null);
  taskState.subscribe((v) => (state = v));

  // Debug state
  let windowInfo: WindowInfo | null = $state(null);
  let contextPolling = $state(false);
  let contextInterval: ReturnType<typeof setInterval> | null = $state(null);
  let availableTasks: any[] = $state([]);
  let debugLog: string[] = $state([]);

  function log(msg: string) {
    debugLog = [...debugLog.slice(-19), `${new Date().toLocaleTimeString()} ${msg}`];
  }

  async function loadTasks() {
    try {
      availableTasks = await invoke("list_tasks");
      log(`Loaded ${availableTasks.length} tasks`);
    } catch (e) {
      log(`Failed to load tasks: ${e}`);
    }
  }

  async function handleStartTask(taskId: string) {
    await startTask(taskId);
    log(`Started task: ${taskId}`);
  }

  // Context detection
  async function pollContext() {
    try {
      windowInfo = await invoke<WindowInfo>("get_active_window");
    } catch (e) {
      log(`Context error: ${e}`);
    }
  }

  function toggleContextPolling() {
    if (contextPolling) {
      if (contextInterval) clearInterval(contextInterval);
      contextInterval = null;
      contextPolling = false;
      log("Context polling stopped");
    } else {
      pollContext();
      contextInterval = setInterval(pollContext, 2000);
      contextPolling = true;
      log("Context polling started (every 2s)");
    }
  }

  // Overlay
  async function testOverlay() {
    try {
      await invoke("show_overlay");
      await invoke("update_overlay", {
        data: {
          elements: [
            {
              element_type: "highlight",
              x: 100,
              y: 100,
              width: 300,
              height: 200,
              text: null,
              color: "#3b82f6",
            },
            {
              element_type: "tooltip",
              x: 150,
              y: 320,
              width: null,
              height: null,
              text: "This is where you should click!",
              color: null,
            },
            {
              element_type: "arrow",
              x: 250,
              y: 80,
              width: null,
              height: null,
              text: null,
              color: "#ef4444",
            },
          ],
        },
      });
      log("Overlay shown with highlight + tooltip + arrow");
    } catch (e) {
      log(`Overlay error: ${e}`);
    }
  }

  async function hideOverlay() {
    try {
      await invoke("hide_overlay");
      log("Overlay hidden");
    } catch (e) {
      log(`Hide overlay error: ${e}`);
    }
  }

  // Load tasks on mount
  loadTasks();
</script>

<main class="min-h-screen bg-gray-950 text-white">
  <div class="max-w-5xl mx-auto p-6">
    <!-- Header -->
    <div class="flex items-center justify-between mb-6">
      <div>
        <h1 class="text-3xl font-bold tracking-tight">Z-RO Cowork</h1>
        <p class="text-gray-400 text-sm mt-1">AI-powered co-worker assistant</p>
      </div>
      {#if state}
        <ProgressBar xp={state.xp_earned} />
      {/if}
    </div>

    {#if !state}
      <!-- Dashboard: Task selection + Feature testing -->
      <div class="grid grid-cols-1 lg:grid-cols-2 gap-6">
        <!-- Available Tasks -->
        <div class="space-y-4">
          <h2 class="text-lg font-semibold">Available Tasks</h2>
          {#if availableTasks.length === 0}
            <div class="bg-gray-900 rounded-xl p-4 border border-gray-800 text-gray-500 text-sm">
              No tasks loaded. Check the tasks/ directory.
            </div>
          {/if}
          {#each availableTasks as task}
            <div class="bg-gray-900 rounded-xl p-4 border border-gray-800">
              <h3 class="font-medium">{task.title}</h3>
              <p class="text-gray-400 text-sm mt-1">{task.description}</p>
              <div class="flex items-center justify-between mt-3">
                <span class="text-yellow-500 text-sm">+{task.xp_reward} XP bonus</span>
                <button
                  onclick={() => handleStartTask(task.id)}
                  class="bg-blue-600 hover:bg-blue-700 text-white px-4 py-2 rounded-lg text-sm transition-colors"
                >
                  Start Task
                </button>
              </div>
            </div>
          {/each}

          <!-- Screen Context Detection -->
          <h2 class="text-lg font-semibold mt-6">Screen Context Detection</h2>
          <div class="bg-gray-900 rounded-xl p-4 border border-gray-800">
            <div class="flex items-center justify-between mb-3">
              <span class="text-sm text-gray-400">Active Window Detection</span>
              <button
                onclick={toggleContextPolling}
                class="px-3 py-1 rounded-lg text-sm transition-colors {contextPolling
                  ? 'bg-red-600 hover:bg-red-700'
                  : 'bg-green-600 hover:bg-green-700'} text-white"
              >
                {contextPolling ? "Stop Polling" : "Start Polling"}
              </button>
            </div>
            {#if windowInfo}
              <div class="space-y-1 text-sm font-mono">
                <p><span class="text-gray-500">Process:</span> <span class="text-green-400">{windowInfo.process_name}</span></p>
                <p><span class="text-gray-500">Window:</span> <span class="text-blue-400">{windowInfo.title}</span></p>
                <p><span class="text-gray-500">Bundle:</span> <span class="text-yellow-400">{windowInfo.bundle_id || "N/A"}</span></p>
              </div>
            {:else}
              <p class="text-gray-500 text-sm">Click "Start Polling" to detect active windows</p>
            {/if}
          </div>

          <!-- Overlay Testing -->
          <h2 class="text-lg font-semibold mt-6">Overlay System</h2>
          <div class="bg-gray-900 rounded-xl p-4 border border-gray-800">
            <p class="text-gray-400 text-sm mb-3">Test the visual overlay (highlights, tooltips, arrows)</p>
            <div class="flex gap-2">
              <button
                onclick={testOverlay}
                class="bg-purple-600 hover:bg-purple-700 text-white px-4 py-2 rounded-lg text-sm transition-colors"
              >
                Show Overlay
              </button>
              <button
                onclick={hideOverlay}
                class="bg-gray-700 hover:bg-gray-600 text-white px-4 py-2 rounded-lg text-sm transition-colors"
              >
                Hide Overlay
              </button>
            </div>
          </div>
        </div>

        <!-- NPC Chat + Debug Log -->
        <div class="space-y-4">
          <h2 class="text-lg font-semibold">Ask Zee (AI Co-worker)</h2>
          <NPCChat />

          <!-- Debug Log -->
          <h2 class="text-lg font-semibold mt-4">Debug Log</h2>
          <div class="bg-gray-900 rounded-xl p-4 border border-gray-800 h-48 overflow-y-auto font-mono text-xs">
            {#each debugLog as entry}
              <p class="text-gray-400">{entry}</p>
            {/each}
            {#if debugLog.length === 0}
              <p class="text-gray-600">Events will appear here...</p>
            {/if}
          </div>
        </div>
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
