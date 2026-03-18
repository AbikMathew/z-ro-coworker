<script lang="ts">
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";

  interface OverlayElement {
    element_type: string;
    x: number;
    y: number;
    width?: number;
    height?: number;
    text?: string;
    color?: string;
  }

  let elements: OverlayElement[] = $state([]);

  onMount(() => {
    const unlisten = listen<{ elements: OverlayElement[] }>("overlay-update", (event) => {
      elements = event.payload.elements;
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  });
</script>

<div class="fixed inset-0 pointer-events-none">
  <svg class="absolute inset-0 w-full h-full">
    {#each elements as el}
      {#if el.element_type === "highlight"}
        <!-- Pulsing highlight box -->
        <rect
          x={el.x}
          y={el.y}
          width={el.width || 200}
          height={el.height || 100}
          fill="none"
          stroke={el.color || "#3b82f6"}
          stroke-width="3"
          rx="8"
          class="animate-pulse"
        />
        <!-- Corner markers -->
        <circle cx={el.x} cy={el.y} r="5" fill={el.color || "#3b82f6"} />
        <circle cx={el.x + (el.width || 200)} cy={el.y} r="5" fill={el.color || "#3b82f6"} />
        <circle cx={el.x} cy={el.y + (el.height || 100)} r="5" fill={el.color || "#3b82f6"} />
        <circle cx={el.x + (el.width || 200)} cy={el.y + (el.height || 100)} r="5" fill={el.color || "#3b82f6"} />
      {/if}

      {#if el.element_type === "arrow"}
        <!-- Down arrow pointing at target -->
        <line
          x1={el.x}
          y1={el.y}
          x2={el.x}
          y2={el.y + 50}
          stroke={el.color || "#ef4444"}
          stroke-width="3"
          marker-end="url(#arrowhead)"
        />
      {/if}
    {/each}

    <!-- Arrow marker definition -->
    <defs>
      <marker id="arrowhead" markerWidth="10" markerHeight="7" refX="10" refY="3.5" orient="auto">
        <polygon points="0 0, 10 3.5, 0 7" fill="#ef4444" />
      </marker>
    </defs>
  </svg>

  <!-- HTML tooltips (positioned absolutely) -->
  {#each elements as el}
    {#if el.element_type === "tooltip" && el.text}
      <div
        class="absolute pointer-events-auto"
        style="left: {el.x}px; top: {el.y}px;"
      >
        <div class="bg-gray-900 text-white text-sm px-4 py-2 rounded-xl shadow-lg border border-gray-700 max-w-xs">
          <div class="flex items-center gap-2 mb-1">
            <div class="w-5 h-5 rounded-full bg-blue-600 flex items-center justify-center text-[10px] font-bold">Z</div>
            <span class="text-gray-400 text-xs">Zee says:</span>
          </div>
          {el.text}
        </div>
        <!-- Arrow pointer -->
        <div class="w-3 h-3 bg-gray-900 border-l border-t border-gray-700 rotate-45 -mt-1.5 ml-6"></div>
      </div>
    {/if}
  {/each}

  {#if elements.length === 0}
    <p class="text-white text-xs opacity-30 absolute bottom-4 right-4">Z-RO Overlay Active</p>
  {/if}
</div>

<style>
  :global(html),
  :global(body) {
    background: transparent !important;
    margin: 0;
    padding: 0;
    overflow: hidden;
  }

  @keyframes pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.5; }
  }

  .animate-pulse {
    animation: pulse 2s ease-in-out infinite;
  }
</style>
