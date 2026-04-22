<script lang="ts">
  import { listen } from "@tauri-apps/api/event";
  import { onMount, onDestroy } from "svelte";
  import ZeeAvatar from "$lib/components/coworker/ZeeAvatar.svelte";
  import NpcPanel from "$lib/components/NpcPanel.svelte";
  import StepTimeline from "$lib/components/task/StepTimeline.svelte";
  import { npc } from "$lib/npc/npcStore.svelte";
  import type { Task, TaskState } from "$lib/types/task";
  import type { WindowInfo } from "$lib/types/context";

  // Prop renamed from `state` → `progress` to avoid colliding with the
  // Svelte 5 `$state` rune parser (see StepTimeline for the same reason).
  interface Props {
    task: Task;
    progress: TaskState;
    onadvance?: () => void;
    loading?: boolean;
  }

  let { task, progress, onadvance, loading = false }: Props = $props();

  // ── Context sidebar: what Zee currently sees ───────────────────────
  //
  // Event-driven: the backend screen poller emits `context-update` events
  // whenever the focused window changes. No IPC polling — scroll stays
  // buttery smooth.
  let windowInfo = $state<WindowInfo | null>(null);
  let unlistenFn: (() => void) | null = null;

  onMount(async () => {
    const unlisten = await listen<WindowInfo>("context-update", (event) => {
      windowInfo = event.payload;
    });
    unlistenFn = unlisten;
  });
  onDestroy(() => {
    if (unlistenFn) unlistenFn();
  });

  // Translate backend NpcState discriminator → ZeeAvatar prop.
  type AvatarState = "idle" | "listening" | "thinking" | "speaking" | "error";
  let avatarState = $derived.by<AvatarState>(() => {
    const s = npc.status.state;
    if (s === "Idle") return "idle";
    if (s === "Listening") return "listening";
    if (s === "Thinking") return "thinking";
    if (s === "Speaking") return "speaking";
    return "error";
  });
</script>

<div class="layout">
  <!-- LEFT: step timeline -->
  <div class="col col-steps">
    <StepTimeline {task} {progress} {onadvance} {loading} />
  </div>

  <!-- CENTER: Zee + chat -->
  <div class="col col-zee">
    <div class="zee-card">
      <ZeeAvatar state={avatarState} size={140} />
      <p class="zee-tag">Your coworker · {npc.status.model_name}</p>
    </div>
    <div class="chat-wrap">
      <NpcPanel />
    </div>
  </div>

  <!-- RIGHT: context sidebar -->
  <aside class="col col-context">
    <div class="context-card">
      <h3 class="c-head">What Zee sees</h3>
      {#if windowInfo}
        <dl class="ctx">
          <dt>App</dt>
          <dd>{windowInfo.process_name}</dd>
          <dt>Window</dt>
          <dd class="truncate">{windowInfo.title || "—"}</dd>
          {#if windowInfo.bundle_id}
            <dt>Bundle</dt>
            <dd class="truncate">{windowInfo.bundle_id}</dd>
          {/if}
        </dl>
      {:else}
        <p class="ctx-empty">Detecting your active window…</p>
      {/if}
    </div>

    <div class="context-card">
      <h3 class="c-head">Task progress</h3>
      <dl class="ctx">
        <dt>Total XP</dt>
        <dd>{progress.xp_earned} / {task.xp_reward}</dd>
        <dt>Step</dt>
        <dd>{progress.current_step_index + 1} of {progress.total_steps}</dd>
        <dt>Status</dt>
        <dd class:status-done={progress.status === "Completed"}>
          {progress.status === "Completed" ? "Completed 🎉" : "In progress"}
        </dd>
      </dl>
    </div>

    <div class="context-card tip">
      <h3 class="c-head">Tip</h3>
      <p>
        Hold the mic button in the chat to ask Zee a question out loud. Zee
        can see the step you're on and the app you're looking at.
      </p>
    </div>
  </aside>
</div>

<style>
  .layout {
    display: grid;
    grid-template-columns: minmax(0, 1.1fr) minmax(0, 1.3fr) minmax(260px, 0.9fr);
    gap: 20px;
    margin-top: 24px;
  }

  .col { min-width: 0; }

  /* Center column: avatar on top, chat below */
  .col-zee {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .zee-card {
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: 18px;
    padding: 28px 20px 24px;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 14px;
    backdrop-filter: blur(6px);
  }
  .zee-tag {
    color: var(--fg-muted);
    font-size: 11px;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    margin: 0;
  }
  /* NpcPanel is styled with Tailwind internally — leave its chrome alone
     and just let it fill its column. No rules needed here for now. */

  /* Right column: stacked info cards */
  .col-context {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .context-card {
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: 16px;
    padding: 16px 18px;
  }
  .c-head {
    font-family: var(--font-display);
    font-size: 12px;
    text-transform: uppercase;
    letter-spacing: 0.12em;
    color: var(--fg-muted);
    margin: 0 0 10px;
    font-weight: 700;
  }
  .ctx {
    display: grid;
    grid-template-columns: 70px 1fr;
    gap: 6px 12px;
    margin: 0;
  }
  .ctx dt {
    font-size: 11px;
    color: var(--fg-dim);
    letter-spacing: 0.04em;
  }
  .ctx dd {
    margin: 0;
    font-size: 13px;
    color: var(--fg);
  }
  .truncate {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .status-done { color: #86efac; font-weight: 600; }
  .ctx-empty {
    color: var(--fg-muted);
    font-size: 12px;
    margin: 0;
  }
  .tip p {
    color: var(--fg-muted);
    font-size: 12px;
    line-height: 1.55;
    margin: 0;
  }

  /* Responsive: collapse to single column on narrow viewports */
  @media (max-width: 1080px) {
    .layout {
      grid-template-columns: 1fr;
    }
  }
</style>
