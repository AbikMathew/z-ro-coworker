<script lang="ts">
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";

  /**
   * Overlay element emitted by the backend (`src-tauri/src/commands/overlay.rs`).
   *
   * Coordinates are NORMALIZED 0.0–1.0 — the overlay window covers the
   * entire primary monitor (see `src-tauri/src/overlay/mod.rs`), so we
   * multiply by our own viewport to land on the student's visible pixel.
   *
   * `element_type` is one of: `"arrow"` | `"box"` | `"tooltip"`. The legacy
   * `"highlight"` alias is still accepted and rendered like `"box"` for
   * backward compatibility.
   */
  interface OverlayElement {
    element_type: string;
    x: number;
    y: number;
    width?: number;
    height?: number;
    text?: string;
    color?: string;
  }

  // ── Reactive state ─────────────────────────────────────────────────────
  //
  // `elements` mirrors the last `overlay-update` event. `renderNonce` bumps
  // every update so the `{#key}` block remounts and the CSS fade-in runs
  // fresh — otherwise a re-emit with the same coords would render instantly.
  let elements: OverlayElement[] = $state([]);
  let renderNonce = $state(0);

  // Viewport — the overlay window is sized to cover the primary monitor,
  // but we still read from window.inner* so the renderer is DPI-/resize-safe.
  let vw = $state(typeof window !== "undefined" ? window.innerWidth : 1440);
  let vh = $state(typeof window !== "undefined" ? window.innerHeight : 900);

  onMount(() => {
    const onResize = () => {
      vw = window.innerWidth;
      vh = window.innerHeight;
    };
    window.addEventListener("resize", onResize);

    const unlisten = listen<{ elements: OverlayElement[] }>(
      "overlay-update",
      (event) => {
        elements = event.payload.elements;
        renderNonce += 1;
      },
    );

    return () => {
      window.removeEventListener("resize", onResize);
      unlisten.then((fn) => fn());
    };
  });

  // ── Geometry helpers ───────────────────────────────────────────────────

  /** Normalized [0,1] → pixel. Defensively clamps in case the LLM drifts. */
  function nx(x: number): number {
    return Math.max(0, Math.min(1, x)) * vw;
  }
  function ny(y: number): number {
    return Math.max(0, Math.min(1, y)) * vh;
  }

  /**
   * Pick an arrow entry point — where the arrow tail starts — so it visibly
   * "flies in" from the nearest screen edge toward the target. We cap the
   * flight distance so arrows stay short and readable on cluttered UIs.
   */
  function arrowTail(targetXpx: number, targetYpx: number) {
    const MAX_FLIGHT = 160; // px
    // Distance from the target to each of the four screen edges
    const dLeft = targetXpx;
    const dRight = vw - targetXpx;
    const dTop = targetYpx;
    const dBottom = vh - targetYpx;
    const min = Math.min(dLeft, dRight, dTop, dBottom);

    // Flight comes from the nearest edge — keeps arrows out of the target
    // region itself.
    let tx = targetXpx;
    let ty = targetYpx;
    if (min === dLeft) tx = Math.max(0, targetXpx - MAX_FLIGHT);
    else if (min === dRight) tx = Math.min(vw, targetXpx + MAX_FLIGHT);
    else if (min === dTop) ty = Math.max(0, targetYpx - MAX_FLIGHT);
    else ty = Math.min(vh, targetYpx + MAX_FLIGHT);
    return { x: tx, y: ty };
  }

  /**
   * Decide where to place the floating label relative to the arrow tip so
   * it stays on-screen. If the tip is near the right edge we flip the label
   * to the left, etc.
   */
  function labelAnchor(tipX: number, tipY: number) {
    const LABEL_W = 200;
    const LABEL_H = 40;
    const PAD = 14;
    // Prefer placing below-right of the tip.
    let x = tipX + PAD;
    let y = tipY + PAD;
    if (x + LABEL_W > vw) x = tipX - LABEL_W - PAD;
    if (y + LABEL_H > vh) y = tipY - LABEL_H - PAD;
    return { x: Math.max(8, x), y: Math.max(8, y) };
  }
</script>

<div class="fixed inset-0 pointer-events-none">
  {#key renderNonce}
    <svg
      class="absolute inset-0 w-full h-full overlay-fade"
      style="pointer-events: none;"
    >
      <defs>
        <marker
          id="arrowhead"
          markerWidth="12"
          markerHeight="12"
          refX="10"
          refY="6"
          orient="auto"
        >
          <polygon points="0 0, 12 6, 0 12" fill="currentColor" />
        </marker>
      </defs>

      {#each elements as el, i}
        {@const color = el.color || "#38bdf8"}

        <!-- Box / highlight: rounded rect around a region -->
        {#if el.element_type === "box" || el.element_type === "highlight"}
          {@const rx = nx(el.x)}
          {@const ry = ny(el.y)}
          {@const rw = (el.width ?? 0.2) * vw}
          {@const rh = (el.height ?? 0.08) * vh}
          <g class="overlay-pop" style="color: {color};">
            <rect
              x={rx}
              y={ry}
              width={rw}
              height={rh}
              fill="none"
              stroke={color}
              stroke-width="3"
              rx="10"
              class="box-pulse"
            />
            {#if el.text}
              {@const labelX = rx + rw / 2}
              {@const labelY = ry - 10}
              <rect
                x={labelX - 90}
                y={labelY - 22}
                width="180"
                height="28"
                rx="8"
                fill="rgba(15, 23, 42, 0.92)"
                stroke={color}
                stroke-width="1"
              />
              <text
                x={labelX}
                y={labelY - 4}
                text-anchor="middle"
                fill="#f8fafc"
                font-size="13"
                font-weight="600"
                font-family="ui-sans-serif, system-ui, sans-serif"
              >
                {el.text}
              </text>
            {/if}
          </g>
        {/if}

        <!-- Arrow: tail from nearest edge → tip at (x, y) -->
        {#if el.element_type === "arrow"}
          {@const tipX = nx(el.x)}
          {@const tipY = ny(el.y)}
          {@const tail = arrowTail(tipX, tipY)}
          <g class="overlay-pop" style="color: {color};">
            <line
              x1={tail.x}
              y1={tail.y}
              x2={tipX}
              y2={tipY}
              stroke={color}
              stroke-width="4"
              stroke-linecap="round"
              marker-end="url(#arrowhead)"
            />
            <!-- Target dot — a small ring around the tip -->
            <circle
              cx={tipX}
              cy={tipY}
              r="10"
              fill="none"
              stroke={color}
              stroke-width="2"
              class="ring-ping"
            />
          </g>
        {/if}
      {/each}
    </svg>

    <!-- HTML labels layer: floating labels for arrows + standalone tooltips -->
    {#each elements as el}
      {@const color = el.color || "#38bdf8"}

      {#if el.element_type === "arrow" && el.text}
        {@const tipX = nx(el.x)}
        {@const tipY = ny(el.y)}
        {@const anchor = labelAnchor(tipX, tipY)}
        <div
          class="absolute overlay-pop"
          style="left: {anchor.x}px; top: {anchor.y}px; pointer-events: none;"
        >
          <div
            class="flex items-center gap-2 px-3 py-2 rounded-xl shadow-lg backdrop-blur-md"
            style="background: rgba(15, 23, 42, 0.92); border: 1px solid {color};"
          >
            <div
              class="w-5 h-5 rounded-full flex items-center justify-center text-[10px] font-bold text-white"
              style="background: {color};"
            >
              Z
            </div>
            <span class="text-white text-sm font-medium">{el.text}</span>
          </div>
        </div>
      {/if}

      {#if el.element_type === "tooltip" && el.text}
        {@const tx = nx(el.x)}
        {@const ty = ny(el.y)}
        <div
          class="absolute overlay-pop max-w-xs"
          style="left: {tx}px; top: {ty}px; pointer-events: none;"
        >
          <div
            class="px-4 py-2 rounded-xl shadow-lg backdrop-blur-md"
            style="background: rgba(15, 23, 42, 0.92); border: 1px solid {color};"
          >
            <div class="flex items-center gap-2 mb-1">
              <div
                class="w-5 h-5 rounded-full flex items-center justify-center text-[10px] font-bold text-white"
                style="background: {color};"
              >
                Z
              </div>
              <span class="text-slate-400 text-xs">Zee says:</span>
            </div>
            <div class="text-white text-sm">{el.text}</div>
          </div>
        </div>
      {/if}
    {/each}
  {/key}

  {#if elements.length === 0}
    <p
      class="text-white text-xs opacity-20 absolute bottom-4 right-4"
      style="pointer-events: none;"
    >
      Z-RO Overlay
    </p>
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

  /* Whole-batch fade-in. Re-keying via {#key renderNonce} replays this. */
  @keyframes overlay-fade-in {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }
  .overlay-fade {
    animation: overlay-fade-in 180ms ease-out both;
  }

  /* Per-element pop: small scale + slight rise, so arrows and labels
     visibly "land" on the screen instead of appearing instantly. */
  @keyframes overlay-pop {
    0% {
      opacity: 0;
      transform: scale(0.92) translateY(4px);
    }
    60% {
      opacity: 1;
      transform: scale(1.02) translateY(0);
    }
    100% {
      opacity: 1;
      transform: scale(1) translateY(0);
    }
  }
  .overlay-pop {
    animation: overlay-pop 320ms cubic-bezier(0.22, 1, 0.36, 1) both;
    transform-box: fill-box;
    transform-origin: center;
  }

  /* Pulsing box border */
  @keyframes box-pulse {
    0%,
    100% {
      opacity: 1;
    }
    50% {
      opacity: 0.55;
    }
  }
  .box-pulse {
    animation: box-pulse 1.8s ease-in-out infinite;
  }

  /* Ping ring around the arrow tip — helps the eye find the target */
  @keyframes ring-ping {
    0% {
      r: 6;
      opacity: 0.9;
    }
    100% {
      r: 22;
      opacity: 0;
    }
  }
  .ring-ping {
    animation: ring-ping 1.4s ease-out infinite;
  }
</style>
