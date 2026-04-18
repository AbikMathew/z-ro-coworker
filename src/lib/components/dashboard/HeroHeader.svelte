<script lang="ts">
  /**
   * Dashboard greeting + at-a-glance stats.
   *
   * This is intentionally lightweight — the numbers here are display-only
   * (XP + streak) until we wire persistent progress. The greeting rotates
   * with the time of day so the first pixel of the app feels alive.
   */
  interface Props {
    /** Lifetime XP across all completed tasks. */
    totalXp?: number;
    /** Day-streak placeholder. Swap for real tracking in a later pass. */
    streak?: number;
    /** Display name of the user. */
    userName?: string;
  }

  let { totalXp = 0, streak = 1, userName = "Friend" }: Props = $props();

  // Greeting based on local time — keeps the dashboard feeling personal.
  let greeting = $derived.by(() => {
    const h = new Date().getHours();
    if (h < 5) return "Still up";
    if (h < 12) return "Good morning";
    if (h < 17) return "Good afternoon";
    if (h < 22) return "Good evening";
    return "Burning the midnight oil";
  });

  // Bucket XP into a level for the chip.  Every 100 XP is one level —
  // simple enough to reason about, motivating enough to keep clicking.
  let level = $derived(Math.floor(totalXp / 100) + 1);
  let xpInLevel = $derived(totalXp % 100);
</script>

<header class="hero">
  <div class="greeting">
    <p class="eyebrow">Your desk at Z-RO</p>
    <h1 class="display">
      {greeting}, <span class="name">{userName}</span> 👋
    </h1>
    <p class="sub">
      Pick a task to practice. Zee will watch your screen and nudge you
      when you get stuck.
    </p>
  </div>

  <div class="stats">
    <div class="stat">
      <div class="stat-value">{level}</div>
      <div class="stat-label">Level</div>
    </div>
    <div class="stat">
      <div class="stat-value">{totalXp}</div>
      <div class="stat-label">Total XP</div>
      <div class="xp-bar" aria-hidden="true">
        <div class="xp-fill" style="width: {xpInLevel}%"></div>
      </div>
    </div>
    <div class="stat">
      <div class="stat-value">🔥 {streak}</div>
      <div class="stat-label">Day streak</div>
    </div>
  </div>
</header>

<style>
  .hero {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: 32px;
    padding: 40px 0 28px;
    flex-wrap: wrap;
  }

  .greeting {
    min-width: 280px;
    flex: 1;
  }
  .eyebrow {
    text-transform: uppercase;
    letter-spacing: 0.16em;
    font-size: 11px;
    font-weight: 600;
    color: var(--fg-muted);
    margin-bottom: 10px;
  }
  h1 {
    font-size: clamp(28px, 3.2vw, 42px);
    font-weight: 700;
    line-height: 1.15;
    margin: 0 0 10px;
  }
  .name {
    background: var(--accent);
    -webkit-background-clip: text;
    background-clip: text;
    color: transparent;
  }
  .sub {
    color: var(--fg-muted);
    max-width: 540px;
    font-size: 14px;
    line-height: 1.5;
  }

  .stats {
    display: flex;
    gap: 16px;
    flex-wrap: wrap;
  }
  .stat {
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: 14px;
    padding: 14px 18px;
    min-width: 120px;
    backdrop-filter: blur(6px);
  }
  .stat-value {
    font-family: var(--font-display);
    font-size: 22px;
    font-weight: 700;
    letter-spacing: -0.01em;
  }
  .stat-label {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.12em;
    color: var(--fg-muted);
    margin-top: 2px;
  }
  .xp-bar {
    margin-top: 8px;
    height: 4px;
    border-radius: 4px;
    background: rgba(255, 255, 255, 0.06);
    overflow: hidden;
  }
  .xp-fill {
    height: 100%;
    background: var(--accent);
    transition: width 400ms cubic-bezier(0.22, 1, 0.36, 1);
  }
</style>
