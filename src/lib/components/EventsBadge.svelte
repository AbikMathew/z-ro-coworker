<script lang="ts">
  /**
   * Small status chip showing whether the global OS event listener is
   * armed and has macOS Accessibility permission. When the listener is
   * blocked on permission, this is the only hint the user gets that
   * Zee has gone deaf — so we make it clickable and explain what to do.
   */
  import { invoke } from "@tauri-apps/api/core";
  import { onDestroy, onMount } from "svelte";

  interface EventsStatus {
    armed: boolean;
    accessibility_trusted: boolean;
  }

  let status: EventsStatus = $state({ armed: false, accessibility_trusted: false });
  let lastErrored = $state(false);
  let poll: ReturnType<typeof setInterval> | null = null;

  async function refresh() {
    try {
      status = await invoke<EventsStatus>("events_status");
      lastErrored = false;
    } catch (e) {
      // Browser-only preview mode — suppress noise.
      lastErrored = true;
    }
  }

  onMount(() => {
    refresh();
    poll = setInterval(refresh, 2000);
  });

  onDestroy(() => {
    if (poll) clearInterval(poll);
  });

  const label = $derived.by(() => {
    if (lastErrored) return "events: n/a";
    if (!status.accessibility_trusted) return "Grant Accessibility permission";
    if (!status.armed) return "Listener offline";
    return "Watching events";
  });

  const dotClass = $derived.by(() => {
    if (lastErrored) return "neutral";
    if (!status.accessibility_trusted || !status.armed) return "warn";
    return "ok";
  });

  function openSysSettings() {
    // Best-effort deep link to the Accessibility pane. If the user isn't
    // on macOS or the URL doesn't resolve, the click is a no-op — the
    // label still tells them where to go.
    try {
      window.open(
        "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility",
        "_blank",
      );
    } catch {
      /* ignore */
    }
  }
</script>

<button
  type="button"
  class="badge {dotClass}"
  onclick={openSysSettings}
  title={status.accessibility_trusted
    ? "Zee is listening for clicks and keystrokes to track task progress."
    : "Without Accessibility permission, macOS silently drops the events Zee needs to verify each step."}
>
  <span class="dot" aria-hidden="true"></span>
  <span class="text">{label}</span>
</button>

<style>
  .badge {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 4px 10px;
    border-radius: 999px;
    font-size: 11px;
    font-weight: 500;
    border: 1px solid rgba(255, 255, 255, 0.1);
    background: rgba(15, 23, 42, 0.55);
    color: var(--fg-muted, rgba(255, 255, 255, 0.7));
    cursor: pointer;
    transition: background 120ms ease;
  }
  .badge:hover {
    background: rgba(15, 23, 42, 0.75);
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    flex-shrink: 0;
  }
  .ok .dot {
    background: #22c55e;
    box-shadow: 0 0 6px rgba(34, 197, 94, 0.6);
  }
  .warn .dot {
    background: #f59e0b;
    box-shadow: 0 0 6px rgba(245, 158, 11, 0.7);
    animation: blink 2s ease-in-out infinite;
  }
  .neutral .dot {
    background: rgba(255, 255, 255, 0.3);
  }
  @keyframes blink {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.45; }
  }
  .text {
    white-space: nowrap;
  }
</style>
