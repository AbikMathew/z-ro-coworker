<script lang="ts">
  import { taskState, advanceStep } from "$lib/stores/taskStore";
  import type { TaskState } from "$lib/types/task";

  let currentTask: TaskState | null = $state(null);
  taskState.subscribe((v) => (currentTask = v));
</script>

{#if currentTask}
  <div class="bg-gray-900 rounded-2xl p-6 border border-gray-800">
    <div class="flex items-center justify-between mb-4">
      <h2 class="text-lg font-semibold">Current Task</h2>
      <span class="text-sm text-gray-500">
        Step {currentTask.current_step_index + 1} / {currentTask.total_steps}
      </span>
    </div>

    <!-- Progress bar -->
    <div class="w-full bg-gray-800 rounded-full h-2 mb-4">
      <div
        class="bg-blue-600 h-2 rounded-full transition-all duration-500"
        style="width: {((currentTask.current_step_index + 1) / currentTask.total_steps) * 100}%"
      ></div>
    </div>

    <!-- Current step -->
    <div class="mb-4">
      <p class="text-white font-medium mb-1">{currentTask.current_step.instruction}</p>
      {#if currentTask.current_step.hints.length > 0}
        <p class="text-gray-500 text-sm">{currentTask.current_step.hints[0]}</p>
      {/if}
    </div>

    <!-- XP -->
    <div class="flex items-center justify-between">
      <span class="text-sm text-yellow-500">+{currentTask.xp_earned} XP</span>
      {#if currentTask.status === "InProgress"}
        <button
          onclick={advanceStep}
          class="bg-green-600 hover:bg-green-700 text-white px-4 py-2 rounded-lg text-sm transition-colors"
        >
          I've done this
        </button>
      {:else}
        <span class="text-green-500 font-medium">Task Complete!</span>
      {/if}
    </div>
  </div>
{/if}
