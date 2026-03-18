<script lang="ts">
  let { xp = 0, label = "XP" }: { xp: number; label?: string } = $props();
  let level = $derived(Math.floor(xp / 100) + 1);
  let progressInLevel = $derived(xp % 100);
</script>

<div class="bg-gray-900 rounded-xl p-4 border border-gray-800">
  <div class="flex items-center justify-between mb-2">
    <span class="text-sm text-gray-400">Level {level}</span>
    <span class="text-sm text-yellow-500">{xp} {label}</span>
  </div>
  <div class="w-full bg-gray-800 rounded-full h-2">
    <div
      class="bg-yellow-500 h-2 rounded-full transition-all duration-700"
      style="width: {progressInLevel}%"
    ></div>
  </div>
</div>
