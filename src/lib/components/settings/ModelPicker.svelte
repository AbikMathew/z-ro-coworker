<script lang="ts">
  import type { ModelInfo } from "$lib/npc/types";

  interface Props {
    models: ModelInfo[];
    /** Currently-selected (provider, model) pair, or `null` if nothing chosen yet. */
    selected: { provider: string; model: string } | null;
    /** Raised when the user picks a different card. */
    onselect?: (provider: string, model: string) => void;
    /** Disables all radios (while a request is in flight). */
    busy?: boolean;
  }

  let { models, selected, onselect, busy = false }: Props = $props();

  // Group by provider so the UI reads "OpenAI: [...] / Anthropic: [...]"
  // instead of one long list where the provider is buried in each card.
  let grouped = $derived.by(() => {
    const order = ["anthropic", "gemini", "openai"];
    const map = new Map<string, ModelInfo[]>();
    for (const m of models) {
      if (!map.has(m.provider)) map.set(m.provider, []);
      map.get(m.provider)!.push(m);
    }
    return order
      .map((p) => ({ provider: p, items: map.get(p) ?? [] }))
      .filter((g) => g.items.length > 0);
  });

  function providerLabel(p: string): string {
    switch (p) {
      case "openai": return "OpenAI";
      case "gemini": return "Google Gemini";
      case "anthropic": return "Anthropic Claude";
      default: return p;
    }
  }

  function costSymbol(tier: string): string {
    switch (tier) {
      case "cheap": return "$";
      case "mid": return "$$";
      case "premium": return "$$$";
      default: return "";
    }
  }

  function isSelected(m: ModelInfo): boolean {
    return (
      !!selected && selected.provider === m.provider && selected.model === m.model
    );
  }
</script>

{#if models.length === 0}
  <div class="empty">
    No models available. Set at least one of <code>OPENAI_API_KEY</code>,
    <code>GEMINI_API_KEY</code>, or <code>ANTHROPIC_API_KEY</code> in your
    <code>.env</code> and restart.
  </div>
{:else}
  {#each grouped as group (group.provider)}
    <div class="group">
      <div class="group-head">{providerLabel(group.provider)}</div>
      <div class="rows">
        {#each group.items as m (m.provider + ":" + m.model)}
          <label class="row" class:selected={isSelected(m)} class:busy>
            <input
              type="radio"
              name="llm-model"
              value={m.provider + ":" + m.model}
              checked={isSelected(m)}
              disabled={busy}
              onchange={() => onselect?.(m.provider, m.model)}
            />
            <span class="radio-dot" aria-hidden="true"></span>
            <span class="body">
              <span class="line1">
                <span class="name">{m.display_name}</span>
                <span class="cost" data-tier={m.cost_tier}>{costSymbol(m.cost_tier)}</span>
                {#if m.vision}
                  <span class="chip vision">vision</span>
                {/if}
              </span>
              <span class="notes">{m.notes}</span>
            </span>
          </label>
        {/each}
      </div>
    </div>
  {/each}
{/if}

<style>
  .empty {
    padding: 16px 18px;
    border: 1px dashed var(--border-strong);
    border-radius: 12px;
    color: var(--fg-muted);
    font-size: 13px;
    line-height: 1.55;
  }
  .empty code {
    background: rgba(255, 255, 255, 0.06);
    padding: 1px 6px;
    border-radius: 6px;
    font-size: 12px;
  }

  .group + .group { margin-top: 18px; }
  .group-head {
    font-family: var(--font-display);
    font-size: 11px;
    letter-spacing: 0.12em;
    text-transform: uppercase;
    color: var(--fg-dim);
    font-weight: 700;
    margin-bottom: 10px;
  }

  .rows {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .row {
    display: flex;
    align-items: flex-start;
    gap: 12px;
    padding: 12px 14px;
    border: 1px solid var(--border);
    border-radius: 12px;
    background: rgba(255, 255, 255, 0.02);
    cursor: pointer;
    transition: border-color 150ms, background 150ms, transform 150ms;
  }
  .row:hover:not(.busy) {
    border-color: var(--border-strong);
    background: rgba(255, 255, 255, 0.04);
  }
  .row.selected {
    border-color: var(--accent-from);
    background: rgba(124, 58, 237, 0.08);
    box-shadow: 0 6px 24px -10px rgba(124, 58, 237, 0.4);
  }
  .row.busy { cursor: wait; opacity: 0.7; }
  .row input[type="radio"] {
    position: absolute;
    opacity: 0;
    pointer-events: none;
  }
  .radio-dot {
    flex: 0 0 auto;
    width: 16px; height: 16px;
    border-radius: 50%;
    border: 2px solid var(--border-strong);
    margin-top: 3px;
    position: relative;
    transition: border-color 150ms;
  }
  .row.selected .radio-dot {
    border-color: var(--accent-from);
  }
  .row.selected .radio-dot::after {
    content: "";
    position: absolute;
    inset: 2px;
    border-radius: 50%;
    background: var(--accent);
  }

  .body {
    display: flex;
    flex-direction: column;
    gap: 4px;
    flex: 1;
    min-width: 0;
  }
  .line1 {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
  }
  .name {
    font-size: 14px;
    font-weight: 600;
    color: var(--fg);
  }
  .cost {
    font-family: var(--font-display);
    font-size: 12px;
    font-weight: 700;
    letter-spacing: 0.04em;
  }
  .cost[data-tier="cheap"] { color: #86efac; }
  .cost[data-tier="mid"] { color: #fbbf24; }
  .cost[data-tier="premium"] { color: #f472b6; }

  .chip {
    font-size: 10px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    padding: 2px 8px;
    border-radius: 999px;
  }
  .chip.vision {
    background: rgba(6, 182, 212, 0.12);
    border: 1px solid rgba(6, 182, 212, 0.35);
    color: #67e8f9;
  }

  .notes {
    font-size: 12px;
    color: var(--fg-muted);
    line-height: 1.5;
  }
</style>
