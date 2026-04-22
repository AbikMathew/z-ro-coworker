<script lang="ts">
  import { onMount } from "svelte";
  import { fade, fly } from "svelte/transition";
  import ModelPicker from "$lib/components/settings/ModelPicker.svelte";
  import { npc } from "$lib/npc/npcStore.svelte";

  // Local mirrors of the store state so the UI stays reactive even if the
  // store object itself is reassigned in the future. Svelte 5 runes make
  // the reactive tracking automatic as long as we read `npc.x` in the
  // component body.
  let loading = $state(true);
  let error = $state<string | null>(null);
  let saving = $state(false);
  let lastSavedAt = $state<number | null>(null);

  // The current selection the user has picked in the UI. Initialized from
  // localStorage, then reconciled to the model catalog once it loads.
  let selected = $state<{ provider: string; model: string } | null>(null);

  onMount(async () => {
    try {
      await npc.listModels();
      // Seed selection: prefer localStorage, otherwise whatever the backend
      // reports as the current model_name (mapped back to a catalog entry).
      const persisted = npc.getPersistedModel();
      if (persisted && npc.availableModels.some(
        (m) => m.provider === persisted.provider && m.model === persisted.model,
      )) {
        selected = persisted;
      } else {
        // Best-effort: find a catalog entry whose `model` matches the
        // backend's current `model_name`. If there's no match (e.g. the
        // backend booted with an env-configured provider that's not in the
        // catalog), leave `selected = null` so no radio is checked.
        await npc.refresh();
        const match = npc.availableModels.find(
          (m) => m.model === npc.status.model_name,
        );
        selected = match ? { provider: match.provider, model: match.model } : null;
      }
    } catch (e) {
      error = `Failed to load models: ${e}`;
      console.error(error);
    } finally {
      loading = false;
    }
  });

  // ── Settings state for non-model knobs (persisted to localStorage) ───
  //
  // Kept separate from the model picker because it doesn't round-trip to
  // the backend — these only affect the frontend's own behaviour.
  const LS_OVERLAY = "zro:settings:overlayEnabled";
  const LS_OVERLAY_AUTOHIDE = "zro:settings:overlayAutoHide";

  let overlayEnabled = $state(true);
  let overlayAutoHide = $state(true);

  onMount(() => {
    try {
      const o = localStorage.getItem(LS_OVERLAY);
      if (o !== null) overlayEnabled = o === "true";
      const a = localStorage.getItem(LS_OVERLAY_AUTOHIDE);
      if (a !== null) overlayAutoHide = a === "true";
    } catch {
      /* localStorage unavailable — use defaults */
    }
  });

  $effect(() => {
    try {
      localStorage.setItem(LS_OVERLAY, String(overlayEnabled));
      localStorage.setItem(LS_OVERLAY_AUTOHIDE, String(overlayAutoHide));
    } catch {
      /* ignore */
    }
  });

  async function handleSelect(provider: string, model: string) {
    if (saving) return;
    saving = true;
    error = null;
    try {
      await npc.setModel(provider, model);
      selected = { provider, model };
      lastSavedAt = Date.now();
    } catch (e) {
      error = `Failed to set model: ${e}`;
      console.error(error);
    } finally {
      saving = false;
    }
  }

  // Format "just now / 5s ago / …" for the status pill without pulling in a
  // dependency. Re-evaluated every second via a ticking counter.
  let now = $state(Date.now());
  onMount(() => {
    const h = setInterval(() => (now = Date.now()), 1000);
    return () => clearInterval(h);
  });
  let savedAgo = $derived.by(() => {
    if (!lastSavedAt) return null;
    const sec = Math.max(0, Math.floor((now - lastSavedAt) / 1000));
    if (sec < 2) return "just now";
    if (sec < 60) return `${sec}s ago`;
    return `${Math.floor(sec / 60)}m ago`;
  });
</script>

<div class="page" in:fade={{ duration: 220 }}>
  <header class="head">
    <h1 class="display title">Settings</h1>
    <p class="sub">
      Customize Zee's brain, voice, and guidance behavior.
      Changes apply instantly — no restart needed.
    </p>
  </header>

  {#if error}
    <div class="banner error" in:fade>{error}</div>
  {/if}

  <!-- ── Brain (LLM) ─────────────────────────────────────────── -->
  <section class="card" in:fly={{ y: 12, duration: 300, delay: 40 }}>
    <div class="card-head">
      <div>
        <h2 class="display section-title">Brain</h2>
        <p class="section-sub">
          Which model Zee uses to read your screen and decide what to say.
          Cheaper models are faster; premium ones read dense UIs more
          accurately.
        </p>
      </div>
      {#if saving}
        <span class="status-pill busy">Saving…</span>
      {:else if savedAgo}
        <span class="status-pill ok">Saved {savedAgo}</span>
      {/if}
    </div>

    {#if loading}
      <div class="skeleton-stack">
        <div class="skeleton"></div>
        <div class="skeleton"></div>
        <div class="skeleton"></div>
      </div>
    {:else}
      <ModelPicker
        models={npc.availableModels}
        {selected}
        onselect={handleSelect}
        busy={saving}
      />
    {/if}
  </section>

  <!-- ── Visual guidance ─────────────────────────────────────── -->
  <section class="card" in:fly={{ y: 12, duration: 300, delay: 100 }}>
    <div class="card-head">
      <div>
        <h2 class="display section-title">Visual guidance</h2>
        <p class="section-sub">
          When Zee wants to point at a button, an arrow flies in from the
          edge of your screen. Turn it off if overlays feel intrusive.
        </p>
      </div>
    </div>

    <div class="toggle-row">
      <label class="toggle">
        <input type="checkbox" bind:checked={overlayEnabled} />
        <span class="switch" aria-hidden="true"></span>
        <span class="label-text">
          <span class="label-main">Show overlay arrows</span>
          <span class="label-sub">Zee draws arrows/boxes over your other apps.</span>
        </span>
      </label>
    </div>

    <div class="toggle-row">
      <label class="toggle" class:disabled={!overlayEnabled}>
        <input
          type="checkbox"
          bind:checked={overlayAutoHide}
          disabled={!overlayEnabled}
        />
        <span class="switch" aria-hidden="true"></span>
        <span class="label-text">
          <span class="label-main">Auto-hide after 15 seconds</span>
          <span class="label-sub">Clear the overlay on its own so it doesn't linger.</span>
        </span>
      </label>
    </div>
  </section>

  <!-- ── About / Debug ──────────────────────────────────────── -->
  <section class="card about" in:fly={{ y: 12, duration: 300, delay: 160 }}>
    <h2 class="display section-title">About</h2>
    <dl class="kv">
      <dt>Current model</dt>
      <dd>{npc.status.model_name || "unknown"}</dd>
      <dt>Conversation turns</dt>
      <dd>{npc.status.conversation_turns}</dd>
      <dt>Active task</dt>
      <dd>{npc.status.active_task ?? "none"}</dd>
    </dl>
    <p class="footnote">
      Z-RO Cowork · Phase 3 — you're running on Tauri + SvelteKit.
      API keys are read from <code>.env</code> at startup.
    </p>
  </section>
</div>

<style>
  .page {
    max-width: 760px;
    margin: 16px auto 64px;
    display: flex;
    flex-direction: column;
    gap: 20px;
  }
  .head {
    padding: 8px 0 4px;
  }
  .title {
    font-size: 28px;
    margin: 0 0 8px;
    letter-spacing: -0.02em;
  }
  .sub {
    margin: 0;
    color: var(--fg-muted);
    font-size: 14px;
    line-height: 1.55;
    max-width: 560px;
  }

  .banner {
    padding: 12px 16px;
    border-radius: 12px;
    font-size: 13px;
  }
  .banner.error {
    background: rgba(239, 68, 68, 0.08);
    border: 1px solid rgba(239, 68, 68, 0.3);
    color: #fca5a5;
  }

  .card {
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: 18px;
    padding: 22px 24px 24px;
    backdrop-filter: blur(6px);
  }
  .card-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
    margin-bottom: 18px;
  }
  .section-title {
    font-size: 18px;
    margin: 0 0 6px;
    font-weight: 700;
  }
  .section-sub {
    margin: 0;
    color: var(--fg-muted);
    font-size: 13px;
    line-height: 1.55;
    max-width: 520px;
  }

  .status-pill {
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    padding: 5px 10px;
    border-radius: 999px;
    flex-shrink: 0;
  }
  .status-pill.ok {
    background: rgba(16, 185, 129, 0.12);
    border: 1px solid rgba(16, 185, 129, 0.35);
    color: #86efac;
  }
  .status-pill.busy {
    background: rgba(124, 58, 237, 0.12);
    border: 1px solid rgba(124, 58, 237, 0.35);
    color: #c4b5fd;
  }

  /* Skeletons while loading */
  .skeleton-stack {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .skeleton {
    height: 58px;
    border-radius: 12px;
    background: linear-gradient(
      90deg,
      rgba(255, 255, 255, 0.03),
      rgba(255, 255, 255, 0.06),
      rgba(255, 255, 255, 0.03)
    );
    background-size: 200% 100%;
    animation: skele 1.6s ease-in-out infinite;
  }
  @keyframes skele {
    0%   { background-position: 0% 50%; }
    100% { background-position: -200% 50%; }
  }

  /* Toggles */
  .toggle-row + .toggle-row { margin-top: 14px; }
  .toggle {
    display: flex;
    align-items: flex-start;
    gap: 14px;
    cursor: pointer;
    padding: 6px 0;
  }
  .toggle.disabled { cursor: not-allowed; opacity: 0.5; }
  .toggle input[type="checkbox"] {
    position: absolute;
    opacity: 0;
    pointer-events: none;
  }
  .switch {
    flex: 0 0 auto;
    width: 40px;
    height: 22px;
    background: rgba(255, 255, 255, 0.1);
    border: 1px solid var(--border-strong);
    border-radius: 999px;
    position: relative;
    transition: background 150ms, border-color 150ms;
    margin-top: 2px;
  }
  .switch::after {
    content: "";
    position: absolute;
    top: 2px;
    left: 2px;
    width: 16px;
    height: 16px;
    background: var(--fg);
    border-radius: 50%;
    transition: transform 180ms cubic-bezier(0.22, 1, 0.36, 1);
  }
  .toggle input:checked + .switch {
    background: var(--accent);
    border-color: transparent;
  }
  .toggle input:checked + .switch::after {
    transform: translateX(18px);
  }
  .label-text {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .label-main {
    font-size: 14px;
    font-weight: 600;
    color: var(--fg);
  }
  .label-sub {
    font-size: 12px;
    color: var(--fg-muted);
    line-height: 1.5;
  }

  /* About card */
  .about .kv {
    display: grid;
    grid-template-columns: 160px 1fr;
    gap: 4px 16px;
    margin: 10px 0 14px;
  }
  .about dt {
    font-size: 11px;
    color: var(--fg-dim);
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }
  .about dd {
    margin: 0;
    font-size: 13px;
    color: var(--fg);
    font-family: var(--font-display);
  }
  .footnote {
    margin: 0;
    padding-top: 12px;
    border-top: 1px solid var(--border);
    font-size: 12px;
    color: var(--fg-muted);
    line-height: 1.55;
  }
  .footnote code {
    background: rgba(255, 255, 255, 0.06);
    padding: 1px 6px;
    border-radius: 6px;
    font-size: 11px;
  }
</style>
