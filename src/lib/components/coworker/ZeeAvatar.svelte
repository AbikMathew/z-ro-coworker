<script lang="ts">
  /**
   * Zee's visual representation.
   *
   * Pure CSS — no image assets. The avatar has three animation states:
   *   - `idle`     → gentle breathe (scale 1 → 1.02) on a 3s loop
   *   - `thinking` → a conic-gradient ring spins around the face
   *   - `speaking` → three sound-wave bars at the bottom, staggered
   *
   * The state prop comes straight from the backend `NpcState` discriminator
   * so the avatar stays in sync with what the pipeline is actually doing
   * without any extra plumbing.
   */
  interface Props {
    /** Current NPC lifecycle state. Unrecognized values fall back to `idle`. */
    state?: "idle" | "listening" | "thinking" | "speaking" | "error";
    /** Rendered size (px). The inner face + rings scale proportionally. */
    size?: number;
  }

  let { state = "idle", size = 160 }: Props = $props();

  // Pre-derived so the template stays declarative.
  const thinking = $derived(state === "thinking");
  const speaking = $derived(state === "speaking");
  const listening = $derived(state === "listening");
  const errored = $derived(state === "error");
</script>

<div
  class="avatar-wrap"
  style="--size: {size}px;"
  data-state={state}
>
  <!-- Thinking ring (only visible when state = 'thinking') -->
  {#if thinking}
    <div class="ring ring-thinking" aria-hidden="true"></div>
  {/if}

  <!-- Listening glow (red-ish so it feels like "mic hot") -->
  {#if listening}
    <div class="ring ring-listening" aria-hidden="true"></div>
  {/if}

  <!-- Error pulse -->
  {#if errored}
    <div class="ring ring-error" aria-hidden="true"></div>
  {/if}

  <!-- The face itself. Breathes when idle, steady otherwise. -->
  <div class="face" class:breathe={state === "idle"}>
    <div class="face-inner">
      <span class="letter">Z</span>
    </div>
  </div>

  <!-- Speaking sound-waves — three bars bouncing with staggered delays -->
  {#if speaking}
    <div class="waves" aria-hidden="true">
      <span></span>
      <span></span>
      <span></span>
    </div>
  {/if}

  <!-- State label underneath — keeps the user oriented without a tooltip -->
  <div class="state-label">
    {#if state === "idle"}Idle
    {:else if state === "listening"}Listening
    {:else if state === "thinking"}Thinking…
    {:else if state === "speaking"}Speaking
    {:else if state === "error"}Error
    {/if}
  </div>
</div>

<style>
  .avatar-wrap {
    position: relative;
    width: var(--size);
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    isolation: isolate; /* ring z-index scoped to this component */
  }

  /* ── Face ──────────────────────────────────────────────────────────── */
  .face {
    width: var(--size);
    height: var(--size);
    border-radius: 28%;
    background: linear-gradient(135deg, #7c3aed, #06b6d4);
    box-shadow:
      0 20px 50px -15px rgba(124, 58, 237, 0.5),
      inset 0 1px 0 rgba(255, 255, 255, 0.2);
    display: flex;
    align-items: center;
    justify-content: center;
    position: relative;
    z-index: 2;
  }
  .face-inner {
    width: 78%;
    height: 78%;
    border-radius: 24%;
    background: rgba(15, 23, 42, 0.55);
    display: flex;
    align-items: center;
    justify-content: center;
    backdrop-filter: blur(4px);
    border: 1px solid rgba(255, 255, 255, 0.1);
  }
  .letter {
    font-family: "Plus Jakarta Sans Variable", system-ui, sans-serif;
    font-size: calc(var(--size) * 0.38);
    font-weight: 800;
    color: #f8fafc;
    letter-spacing: -0.02em;
    background: linear-gradient(135deg, #ffffff, #a5f3fc);
    -webkit-background-clip: text;
    background-clip: text;
    color: transparent;
  }

  /* Idle breathe */
  @keyframes breathe {
    0%, 100% { transform: scale(1); }
    50%      { transform: scale(1.025); }
  }
  .breathe {
    animation: breathe 3s ease-in-out infinite;
  }

  /* ── Rings ─────────────────────────────────────────────────────────── */
  .ring {
    position: absolute;
    inset: -12px;
    border-radius: 32%;
    z-index: 1;
    pointer-events: none;
  }

  /* Thinking: rotating conic gradient */
  @keyframes ring-rotate {
    to { transform: rotate(360deg); }
  }
  .ring-thinking {
    background: conic-gradient(
      from 0deg,
      rgba(124, 58, 237, 0),
      rgba(124, 58, 237, 0.9),
      rgba(6, 182, 212, 0.9),
      rgba(124, 58, 237, 0)
    );
    animation: ring-rotate 2.4s linear infinite;
    filter: blur(6px);
  }

  /* Listening: soft red pulse */
  @keyframes listening-pulse {
    0%, 100% { opacity: 0.55; transform: scale(1); }
    50%      { opacity: 0.9;  transform: scale(1.03); }
  }
  .ring-listening {
    background: radial-gradient(
      closest-side,
      rgba(244, 63, 94, 0.35),
      rgba(244, 63, 94, 0) 75%
    );
    animation: listening-pulse 1.2s ease-in-out infinite;
  }

  /* Error: static red glow */
  .ring-error {
    background: radial-gradient(
      closest-side,
      rgba(239, 68, 68, 0.5),
      rgba(239, 68, 68, 0) 75%
    );
  }

  /* ── Speaking waves ────────────────────────────────────────────────── */
  .waves {
    position: absolute;
    bottom: -6px;
    left: 50%;
    transform: translateX(-50%);
    display: flex;
    align-items: flex-end;
    gap: 5px;
    height: calc(var(--size) * 0.18);
    z-index: 3;
  }
  .waves span {
    display: block;
    width: calc(var(--size) * 0.055);
    background: linear-gradient(180deg, #a5f3fc, #06b6d4);
    border-radius: 4px;
    box-shadow: 0 0 12px rgba(6, 182, 212, 0.5);
    animation: wave-bounce 0.9s ease-in-out infinite;
  }
  .waves span:nth-child(1) { animation-delay: 0s;    height: 40%; }
  .waves span:nth-child(2) { animation-delay: 0.15s; height: 75%; }
  .waves span:nth-child(3) { animation-delay: 0.3s;  height: 55%; }

  @keyframes wave-bounce {
    0%, 100% { transform: scaleY(0.4); }
    50%      { transform: scaleY(1);   }
  }

  /* ── State label ───────────────────────────────────────────────────── */
  .state-label {
    margin-top: 22px;
    font-size: 11px;
    letter-spacing: 0.15em;
    text-transform: uppercase;
    color: var(--fg-muted, #94a3b8);
    font-weight: 600;
    font-family: "Inter Variable", system-ui, sans-serif;
  }
</style>
