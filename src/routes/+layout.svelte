<script lang="ts">
  import "../app.css";
  import { page } from "$app/stores";
  import { taskState } from "$lib/stores/taskStore";
  import type { TaskState } from "$lib/types/task";

  let { children } = $props();

  // Current active task — drives the XP badge in the top nav.
  let current: TaskState | null = $state(null);
  taskState.subscribe((v) => (current = v));

  // Route → highlights the nav link for better wayfinding.
  let path = $derived($page.url.pathname);
  let isOverlayRoute = $derived(path.startsWith("/overlay"));
</script>

{#if isOverlayRoute}
  <!-- Overlay window uses its own transparent layout — no chrome. -->
  {@render children()}
{:else}
  <div class="app-shell">
    <nav class="top-nav">
      <a class="brand" href="/">
        <span class="logo">
          <span class="logo-z">Z</span>
        </span>
        <span class="brand-text">
          <span class="brand-name">Z-RO</span>
          <span class="brand-sub">Cowork</span>
        </span>
      </a>

      <div class="nav-links">
        <a href="/" class:active={path === "/"}>Dashboard</a>
        <a href="/task" class:active={path.startsWith("/task")}>
          Active Task
          {#if current}
            <span class="badge-dot"></span>
          {/if}
        </a>
        <a href="/settings" class:active={path.startsWith("/settings")}>
          Settings
        </a>
      </div>

      <div class="nav-right">
        {#if current}
          <div class="xp-pill" title="XP earned in current task">
            <span class="xp-icon">⚡</span>
            <span class="xp-num">{current.xp_earned}</span>
            <span class="xp-label">XP</span>
          </div>
        {/if}
        <a class="gear" href="/settings" aria-label="Open settings">
          <svg
            width="18" height="18" viewBox="0 0 24 24" fill="none"
            stroke="currentColor" stroke-width="2"
            stroke-linecap="round" stroke-linejoin="round"
          >
            <circle cx="12" cy="12" r="3" />
            <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09a1.65 1.65 0 0 0-1-1.51 1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09a1.65 1.65 0 0 0 1.51-1 1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33h0a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51h0a1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82v0a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" />
          </svg>
        </a>
      </div>
    </nav>

    <main class="app-main">
      {@render children()}
    </main>
  </div>
{/if}

<style>
  .app-shell {
    min-height: 100vh;
    display: flex;
    flex-direction: column;
  }

  /* Top nav */
  .top-nav {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 24px;
    padding: 14px 32px;
    border-bottom: 1px solid var(--border);
    background: rgba(11, 13, 18, 0.72);
    backdrop-filter: blur(14px);
    position: sticky;
    top: 0;
    z-index: 50;
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 12px;
    text-decoration: none;
    color: var(--fg);
  }
  .logo {
    width: 34px;
    height: 34px;
    border-radius: 10px;
    background: var(--accent);
    display: flex;
    align-items: center;
    justify-content: center;
    box-shadow: 0 6px 18px -6px rgba(124, 58, 237, 0.6);
  }
  .logo-z {
    font-family: var(--font-display);
    font-weight: 800;
    color: white;
    font-size: 18px;
    letter-spacing: -0.02em;
  }
  .brand-text {
    display: flex;
    flex-direction: column;
    line-height: 1;
  }
  .brand-name {
    font-family: var(--font-display);
    font-size: 15px;
    font-weight: 700;
    letter-spacing: -0.01em;
  }
  .brand-sub {
    font-size: 10px;
    letter-spacing: 0.18em;
    color: var(--fg-muted);
    text-transform: uppercase;
    margin-top: 2px;
  }

  .nav-links {
    display: flex;
    align-items: center;
    gap: 4px;
    flex: 1;
    justify-content: center;
  }
  .nav-links a {
    position: relative;
    padding: 8px 14px;
    border-radius: 10px;
    font-size: 13px;
    font-weight: 500;
    color: var(--fg-muted);
    text-decoration: none;
    transition: background 150ms, color 150ms;
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .nav-links a:hover {
    background: rgba(255, 255, 255, 0.04);
    color: var(--fg);
  }
  .nav-links a.active {
    background: rgba(255, 255, 255, 0.06);
    color: var(--fg);
  }
  .badge-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--dif-beginner);
    box-shadow: 0 0 0 3px rgba(16, 185, 129, 0.2);
  }

  .nav-right {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .xp-pill {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 6px 12px;
    border-radius: 999px;
    background: rgba(251, 191, 36, 0.1);
    border: 1px solid rgba(251, 191, 36, 0.3);
    font-size: 12px;
    font-weight: 700;
    color: #fde68a;
  }
  .xp-icon { font-size: 12px; }
  .xp-num { color: #fbbf24; font-family: var(--font-display); }
  .xp-label {
    font-size: 10px;
    letter-spacing: 0.1em;
    opacity: 0.8;
  }
  .gear {
    color: var(--fg-muted);
    padding: 8px;
    border-radius: 10px;
    display: inline-flex;
    transition: background 150ms, color 150ms;
  }
  .gear:hover {
    background: rgba(255, 255, 255, 0.06);
    color: var(--fg);
  }

  .app-main {
    flex: 1;
    padding: 0 32px 48px;
    max-width: 1200px;
    width: 100%;
    margin: 0 auto;
  }

  /* Smaller screens */
  @media (max-width: 720px) {
    .top-nav {
      padding: 12px 16px;
      gap: 12px;
    }
    .app-main { padding: 0 16px 32px; }
    .brand-text { display: none; }
    .nav-links { gap: 0; }
    .nav-links a { padding: 6px 10px; font-size: 12px; }
  }
</style>
