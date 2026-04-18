<script lang="ts">
  import type { Task, TaskState } from "$lib/types/task";

  // Note: prop is named `progress` (not `state`) to avoid colliding with
  // Svelte 5's `$state` rune naming heuristics.
  interface Props {
    task: Task;
    progress: TaskState;
    onadvance?: () => void;
    loading?: boolean;
  }

  let { task, progress, onadvance, loading = false }: Props = $props();

  let steps = $derived(task.steps ?? []);
  let currentIdx = $derived(progress.current_step_index);
  let done = $derived(progress.status === "Completed");

  function statusFor(i: number): "done" | "current" | "upcoming" {
    if (i < currentIdx || done) return "done";
    if (i === currentIdx) return "current";
    return "upcoming";
  }
</script>

<div class="timeline">
  <header class="head">
    <div>
      <h2 class="display title">{task.title}</h2>
      <p class="sub">
        Step {Math.min(currentIdx + 1, steps.length)} of {steps.length}
        · +{progress.xp_earned}/{task.xp_reward} XP
      </p>
    </div>
    <div class="progress-ring" aria-hidden="true">
      <svg width="48" height="48" viewBox="0 0 48 48">
        <circle
          cx="24" cy="24" r="20"
          fill="none" stroke="rgba(255,255,255,0.08)" stroke-width="4"
        />
        <circle
          cx="24" cy="24" r="20"
          fill="none" stroke="url(#grad)" stroke-width="4"
          stroke-linecap="round"
          stroke-dasharray={2 * Math.PI * 20}
          stroke-dashoffset={(1 - Math.min(1, currentIdx / Math.max(1, steps.length))) * 2 * Math.PI * 20}
          transform="rotate(-90 24 24)"
        />
        <defs>
          <linearGradient id="grad" x1="0" y1="0" x2="1" y2="1">
            <stop offset="0%" stop-color="#7c3aed" />
            <stop offset="100%" stop-color="#06b6d4" />
          </linearGradient>
        </defs>
      </svg>
      <span class="ring-pct">
        {Math.round((Math.min(currentIdx, steps.length) / Math.max(1, steps.length)) * 100)}%
      </span>
    </div>
  </header>

  <ol class="steps">
    {#each steps as step, i (step.id)}
      {@const s = statusFor(i)}
      <li class="step" data-status={s}>
        <!-- Marker: checkmark / number / ring -->
        <div class="marker">
          {#if s === "done"}
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none"
              stroke="white" stroke-width="3"
              stroke-linecap="round" stroke-linejoin="round">
              <path d="M20 6 9 17l-5-5" />
            </svg>
          {:else if s === "current"}
            <span class="dot-current"></span>
          {:else}
            <span class="num">{i + 1}</span>
          {/if}
        </div>

        {#if i < steps.length - 1}
          <div class="connector"></div>
        {/if}

        <div class="step-body">
          <div class="step-title">{step.instruction}</div>
          {#if s === "current" && step.hints?.length}
            <ul class="hints">
              {#each step.hints as hint}
                <li>{hint}</li>
              {/each}
            </ul>
          {/if}
          {#if s === "current" && !done}
            <button
              type="button"
              class="advance"
              onclick={() => onadvance?.()}
              disabled={loading}
            >
              {loading ? "Checking…" : "I've done this ✓"}
            </button>
          {/if}
          <div class="step-meta">
            <span class="xp-chip">+{step.xp} XP</span>
          </div>
        </div>
      </li>
    {/each}
  </ol>

  {#if done}
    <div class="done-banner">
      🎉 Task complete! Earned +{progress.xp_earned} XP. Head back to the
      dashboard to pick another.
    </div>
  {/if}
</div>

<style>
  .timeline {
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: 18px;
    padding: 24px;
    backdrop-filter: blur(6px);
  }

  .head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
    margin-bottom: 20px;
    padding-bottom: 18px;
    border-bottom: 1px solid var(--border);
  }
  .title { margin: 0; font-size: 20px; }
  .sub {
    margin: 6px 0 0;
    font-size: 12px;
    color: var(--fg-muted);
    letter-spacing: 0.02em;
  }

  .progress-ring {
    position: relative;
    width: 48px; height: 48px;
    flex-shrink: 0;
  }
  .ring-pct {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 10px;
    font-weight: 700;
    color: var(--fg-muted);
    font-family: var(--font-display);
  }

  /* Steps */
  .steps {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .step {
    position: relative;
    display: grid;
    grid-template-columns: 32px 1fr;
    column-gap: 14px;
    padding-bottom: 20px;
  }
  .step:last-child { padding-bottom: 0; }

  .marker {
    position: relative;
    z-index: 1;
    width: 32px; height: 32px;
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    font-family: var(--font-display);
    font-weight: 700;
    font-size: 13px;
    transition: background 200ms, color 200ms, box-shadow 200ms;
  }
  .step[data-status="done"] .marker {
    background: var(--accent);
    color: white;
    box-shadow: 0 4px 16px -4px rgba(124, 58, 237, 0.5);
  }
  .step[data-status="current"] .marker {
    background: rgba(124, 58, 237, 0.15);
    border: 1px solid var(--accent-from);
    color: var(--fg);
  }
  .step[data-status="upcoming"] .marker {
    background: rgba(255, 255, 255, 0.04);
    border: 1px solid var(--border);
    color: var(--fg-dim);
  }

  /* Pulsing dot on the current step */
  .dot-current {
    width: 10px; height: 10px;
    border-radius: 50%;
    background: var(--accent-from);
    box-shadow: 0 0 0 0 rgba(124, 58, 237, 0.6);
    animation: dot-pulse 1.6s infinite;
  }
  @keyframes dot-pulse {
    0%   { box-shadow: 0 0 0 0 rgba(124, 58, 237, 0.6); }
    70%  { box-shadow: 0 0 0 10px rgba(124, 58, 237, 0); }
    100% { box-shadow: 0 0 0 0 rgba(124, 58, 237, 0); }
  }

  /* Connector line between markers */
  .connector {
    position: absolute;
    left: 15px;
    top: 34px;
    bottom: -2px;
    width: 2px;
    background: var(--border);
  }
  .step[data-status="done"] .connector {
    background: linear-gradient(180deg, var(--accent-from), var(--accent-to));
  }

  /* Body */
  .step-body {
    padding-top: 4px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .step-title {
    font-size: 14px;
    line-height: 1.45;
    color: var(--fg);
    font-weight: 500;
  }
  .step[data-status="upcoming"] .step-title { color: var(--fg-dim); }
  .step[data-status="done"] .step-title {
    color: var(--fg-muted);
    text-decoration: line-through;
    text-decoration-color: rgba(255, 255, 255, 0.2);
  }

  .hints {
    margin: 0;
    padding: 10px 12px;
    list-style: none;
    background: rgba(124, 58, 237, 0.06);
    border: 1px solid rgba(124, 58, 237, 0.25);
    border-radius: 10px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .hints li {
    font-size: 12px;
    color: var(--fg-muted);
    line-height: 1.5;
    position: relative;
    padding-left: 14px;
  }
  .hints li::before {
    content: "→";
    position: absolute;
    left: 0;
    color: var(--accent-from);
  }

  .advance {
    all: unset;
    align-self: flex-start;
    background: var(--accent);
    color: white;
    padding: 7px 14px;
    border-radius: 10px;
    font-weight: 600;
    font-size: 13px;
    cursor: pointer;
    box-shadow: 0 4px 14px -4px rgba(124, 58, 237, 0.55);
    transition: transform 150ms, box-shadow 200ms;
  }
  .advance:hover:not(:disabled) {
    transform: translateY(-1px);
    box-shadow: 0 6px 20px -4px rgba(124, 58, 237, 0.7);
  }
  .advance:disabled { opacity: 0.6; cursor: wait; }

  .step-meta {
    display: flex;
    gap: 8px;
  }
  .xp-chip {
    font-size: 10px;
    letter-spacing: 0.04em;
    font-weight: 700;
    color: #fbbf24;
    background: rgba(251, 191, 36, 0.1);
    border: 1px solid rgba(251, 191, 36, 0.3);
    padding: 2px 8px;
    border-radius: 999px;
  }

  .done-banner {
    margin-top: 20px;
    padding: 14px 16px;
    border-radius: 12px;
    background: linear-gradient(135deg,
      rgba(16, 185, 129, 0.15),
      rgba(6, 182, 212, 0.15));
    border: 1px solid rgba(16, 185, 129, 0.4);
    font-size: 13px;
    color: #d1fae5;
  }
</style>
