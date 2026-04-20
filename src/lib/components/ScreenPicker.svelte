<script lang="ts">
  /**
   * "Share your screen" control. Triggers the native macOS
   * `SCContentSharingPicker` (same dialog Zoom / Meet use), then shows a live
   * "Zee is watching" indicator while the SCK stream is running.
   *
   * We deliberately don't snapshot frames in the browser — the native picker
   * returns a filter to the Rust backend, which starts an `SCStream` and
   * pushes frames into the app-wide `FrameBuffer`. The frontend only owns the
   * UX; the backend owns the capture pipeline.
   */
  import { invoke } from "@tauri-apps/api/core";
  import { onDestroy, onMount } from "svelte";

  interface PickerResult {
    width: number;
    height: number;
    source_kind: "window" | "display" | "application";
  }

  interface CaptureStatus {
    active: boolean;
    frames_in_ring: number;
    latest_width: number | null;
    latest_height: number | null;
    latest_age_ms: number | null;
  }

  let status: CaptureStatus = $state({
    active: false,
    frames_in_ring: 0,
    latest_width: null,
    latest_height: null,
    latest_age_ms: null,
  });
  let pickerError = $state<string | null>(null);
  let pickerPending = $state(false);
  let lastPick: PickerResult | null = $state(null);

  /** Poll backend status once per second so the indicator stays honest. */
  let pollHandle: ReturnType<typeof setInterval> | null = null;

  async function refreshStatus() {
    try {
      status = await invoke<CaptureStatus>("capture_status");
    } catch (e) {
      // IPC not available yet — expected in browser-only dev mode.
      status = {
        active: false,
        frames_in_ring: 0,
        latest_width: null,
        latest_height: null,
        latest_age_ms: null,
      };
    }
  }

  async function share() {
    pickerError = null;
    pickerPending = true;
    try {
      lastPick = await invoke<PickerResult>("capture_request_picker");
      await refreshStatus();
    } catch (e) {
      // "the user cancelled the picker" is not an error worth showing —
      // swallow it so the UI doesn't flash red when someone hits Esc.
      const msg = String(e);
      if (msg.toLowerCase().includes("cancel")) {
        pickerError = null;
      } else {
        pickerError = msg;
      }
    } finally {
      pickerPending = false;
    }
  }

  async function stop() {
    try {
      await invoke("capture_stop");
      lastPick = null;
      await refreshStatus();
    } catch (e) {
      pickerError = String(e);
    }
  }

  onMount(() => {
    refreshStatus();
    pollHandle = setInterval(refreshStatus, 1000);
  });

  onDestroy(() => {
    if (pollHandle) clearInterval(pollHandle);
  });

  let ageLabel = $derived.by(() => {
    if (status.latest_age_ms === null) return "—";
    if (status.latest_age_ms < 1000) return `${status.latest_age_ms}ms`;
    return `${(status.latest_age_ms / 1000).toFixed(1)}s`;
  });
</script>

<div class="picker">
  {#if status.active}
    <div class="active">
      <span class="dot" aria-hidden="true"></span>
      <div class="meta">
        <div class="title">Zee is watching</div>
        <div class="sub">
          {#if lastPick}
            {lastPick.source_kind} · {status.latest_width ?? "?"}×{status.latest_height ?? "?"}
          {:else}
            Streaming frames
          {/if}
          · latest {ageLabel} · ring {status.frames_in_ring}
        </div>
      </div>
      <button class="stop" onclick={stop} disabled={pickerPending}>
        Stop sharing
      </button>
    </div>
  {:else}
    <button class="share" onclick={share} disabled={pickerPending}>
      {pickerPending ? "Opening picker…" : "Share your screen with Zee"}
    </button>
  {/if}

  {#if pickerError}
    <p class="err">{pickerError}</p>
  {/if}
</div>

<style>
  .picker {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .share,
  .stop {
    background: var(--accent, linear-gradient(135deg, #7c3aed, #38bdf8));
    color: #fff;
    border: 0;
    border-radius: 12px;
    padding: 10px 16px;
    font-size: 14px;
    font-weight: 600;
    cursor: pointer;
    transition: transform 120ms ease;
  }
  .share:disabled,
  .stop:disabled {
    opacity: 0.55;
    cursor: progress;
  }
  .share:hover:not(:disabled),
  .stop:hover:not(:disabled) {
    transform: translateY(-1px);
  }
  .active {
    display: flex;
    align-items: center;
    gap: 12px;
    background: rgba(15, 23, 42, 0.55);
    border: 1px solid rgba(56, 189, 248, 0.3);
    border-radius: 14px;
    padding: 10px 14px;
  }
  .dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: #38bdf8;
    box-shadow: 0 0 10px rgba(56, 189, 248, 0.8);
    animation: pulse 1.6s ease-in-out infinite;
    flex-shrink: 0;
  }
  @keyframes pulse {
    0%, 100% { opacity: 1; transform: scale(1); }
    50%      { opacity: 0.55; transform: scale(0.9); }
  }
  .meta {
    flex: 1;
    min-width: 0;
  }
  .title {
    font-weight: 600;
    font-size: 13px;
  }
  .sub {
    font-size: 11px;
    color: var(--fg-muted, rgba(255, 255, 255, 0.6));
    margin-top: 2px;
  }
  .stop {
    background: rgba(239, 68, 68, 0.2);
    border: 1px solid rgba(239, 68, 68, 0.5);
    padding: 6px 12px;
    font-size: 12px;
  }
  .err {
    font-size: 12px;
    color: rgba(248, 113, 113, 0.9);
    margin: 0;
  }
</style>
