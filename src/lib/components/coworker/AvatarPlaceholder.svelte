<script lang="ts">
  interface Props {
    speaking?: boolean;
  }
  let { speaking = false }: Props = $props();
</script>

<!--
  Placeholder for the future game-asset avatar. Keeping it dead-simple on
  purpose — no animations, no art — so nobody mistakes it for final UI.
-->
<div class="flex flex-col items-center">
  <div
    class="relative w-48 h-48 rounded-3xl bg-gradient-to-br from-gray-800 to-gray-900 border border-gray-700 flex items-center justify-center shadow-2xl"
    class:ring-2={speaking}
    class:ring-emerald-500={speaking}
  >
    <div class="absolute inset-2 rounded-2xl border border-dashed border-gray-600"></div>
    <div class="text-center px-4">
      <div class="text-4xl mb-1">Z</div>
      <div class="text-[10px] tracking-widest uppercase text-gray-500">
        Coworker avatar<br />placeholder
      </div>
    </div>
    {#if speaking}
      <div class="absolute bottom-2 right-2 flex items-center gap-1">
        <span class="w-1.5 h-1.5 bg-emerald-400 rounded-full animate-pulse"></span>
        <span class="text-[10px] text-emerald-400">speaking</span>
      </div>
    {/if}
  </div>
  <p class="mt-3 text-xs text-gray-600">(future game asset)</p>
</div>
