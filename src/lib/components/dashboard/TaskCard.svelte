<script lang="ts">
  import type { Task } from "$lib/types/task";

  interface Props {
    task: Task;
    /** Click handler — usually kicks off `startTask(task.id)` + navigation. */
    onstart?: (taskId: string) => void;
  }

  let { task, onstart }: Props = $props();

  // Fallbacks so cards always render cleanly even for tasks that haven't
  // been updated to the new presentation schema yet.
  let icon = $derived(task.icon ?? "✨");
  let category = $derived(task.category ?? "default");
  let difficulty = $derived(task.difficulty ?? "beginner");
  let stepsCount = $derived(task.steps?.length ?? 0);

  let diffColor = $derived.by(() => {
    switch (difficulty) {
      case "beginner": return "var(--dif-beginner)";
      case "intermediate": return "var(--dif-intermediate)";
      case "advanced": return "var(--dif-advanced)";
      default: return "var(--fg-muted)";
    }
  });

  let diffLabel = $derived.by(() => {
    if (difficulty === "beginner") return "Beginner";
    if (difficulty === "intermediate") return "Intermediate";
    if (difficulty === "advanced") return "Advanced";
    return "All levels";
  });

  function handleClick() {
    onstart?.(task.id);
  }
  function handleKey(e: KeyboardEvent) {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      handleClick();
    }
  }
</script>

<!--
  Using a real <button> instead of role="button" on a div — satisfies a11y
  without needing an override and gives us free keyboard support. All of
  the card styling still works because we reset default button chrome.
-->
<button
  type="button"
  class="card"
  onclick={handleClick}
  onkeydown={handleKey}
  aria-label="Start task: {task.title}"
>
  <!-- Gradient thumbnail — category drives the color, icon sits center -->
  <div class="thumb cat-bg" data-category={category}>
    <span class="thumb-icon">{icon}</span>
    <!-- Difficulty chip lives on the thumbnail corner -->
    <span
      class="diff-chip"
      style="--dif: {diffColor};"
    >
      <span class="dot" style="background: {diffColor};"></span>
      {diffLabel}
    </span>
    {#if task.est_minutes}
      <span class="time-chip">⏱ {task.est_minutes} min</span>
    {/if}
  </div>

  <!-- Body -->
  <div class="body">
    <h3 class="title display">{task.title}</h3>
    <p class="desc">{task.description}</p>

    <div class="footer">
      <span class="xp">+{task.xp_reward} XP</span>
      <span class="meta">{stepsCount} step{stepsCount === 1 ? "" : "s"}</span>
      <span class="start">
        Start
        <svg
          width="14" height="14" viewBox="0 0 24 24"
          fill="none" stroke="currentColor" stroke-width="2.5"
          stroke-linecap="round" stroke-linejoin="round"
        >
          <path d="M5 12h14M13 5l7 7-7 7" />
        </svg>
      </span>
    </div>
  </div>
</button>

<style>
  .card {
    /* Reset default <button> chrome so the card styling fully applies. */
    all: unset;
    box-sizing: border-box;
    font: inherit;
    color: inherit;
    width: 100%;
    text-align: left;

    position: relative;
    display: flex;
    flex-direction: column;
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: 18px;
    overflow: hidden;
    cursor: pointer;
    transition:
      transform 200ms cubic-bezier(0.22, 1, 0.36, 1),
      border-color 200ms,
      box-shadow 250ms;
  }
  .card:hover,
  .card:focus-visible {
    transform: translateY(-3px);
    border-color: var(--border-strong);
    box-shadow: 0 20px 50px -20px rgba(124, 58, 237, 0.35);
  }
  .card:hover .start {
    transform: translateX(4px);
  }
  .card:hover .thumb {
    filter: brightness(1.08) saturate(1.1);
  }
  .card:hover .thumb-icon {
    transform: scale(1.08) rotate(-3deg);
  }

  /* Thumbnail */
  .thumb {
    position: relative;
    aspect-ratio: 16 / 9;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: filter 220ms;
  }
  .thumb-icon {
    font-size: 58px;
    filter: drop-shadow(0 6px 18px rgba(0, 0, 0, 0.35));
    transition: transform 260ms cubic-bezier(0.22, 1, 0.36, 1);
  }
  .diff-chip {
    position: absolute;
    top: 12px;
    left: 12px;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.04em;
    padding: 4px 10px 4px 8px;
    border-radius: 999px;
    background: rgba(15, 23, 42, 0.72);
    color: #f8fafc;
    backdrop-filter: blur(6px);
    border: 1px solid rgba(255, 255, 255, 0.12);
  }
  .dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    display: inline-block;
  }
  .time-chip {
    position: absolute;
    top: 12px;
    right: 12px;
    font-size: 11px;
    font-weight: 600;
    padding: 4px 10px;
    border-radius: 999px;
    background: rgba(15, 23, 42, 0.72);
    color: #f8fafc;
    backdrop-filter: blur(6px);
    border: 1px solid rgba(255, 255, 255, 0.12);
  }

  /* Body */
  .body {
    padding: 18px 18px 16px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    flex: 1;
  }
  .title {
    font-size: 17px;
    font-weight: 700;
    letter-spacing: -0.015em;
    margin: 0;
    color: var(--fg);
  }
  .desc {
    font-size: 13px;
    line-height: 1.5;
    color: var(--fg-muted);
    margin: 0;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }

  .footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    margin-top: 10px;
    padding-top: 12px;
    border-top: 1px solid var(--border);
  }
  .xp {
    color: #fbbf24;
    font-weight: 700;
    font-size: 13px;
  }
  .meta {
    color: var(--fg-dim);
    font-size: 12px;
  }
  .start {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--fg);
    font-weight: 600;
    font-size: 13px;
    transition: transform 200ms cubic-bezier(0.22, 1, 0.36, 1);
  }
</style>
