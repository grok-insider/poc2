<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import type {
    AdvisorAction,
    Goal,
    Item,
    PriceRefreshMeta,
    Recommendation,
    RecommendArgs,
    RecommendResponse,
    RefreshPricesResponse,
  } from './types';

  type Props = {
    item: Item;
    goal: Goal;
    /** Optional callback so the App.svelte parent can lift the latest
     * recommendation list (used by the RecoveryPanel to display the
     * current top recommendation's step recovery hints). */
    onRecommendations?: (recs: Recommendation[]) => void;
  };

  let { item, goal, onRecommendations }: Props = $props();

  let recommendations = $state<Recommendation[]>([]);
  let meta = $state<{
    patch: string;
    rule_count: number;
    strategy_count: number;
    mod_count: number;
    bundle_path: string | null;
  } | null>(null);
  let priceMeta = $state<PriceRefreshMeta | null>(null);
  let loading = $state(false);
  let priceLoading = $state(false);
  let priceError = $state<string | null>(null);
  let error = $state<string | null>(null);
  let risk = $state(0.5);
  let depth = $state(2);

  async function refreshPrices() {
    priceLoading = true;
    priceError = null;
    try {
      const r = await invoke<RefreshPricesResponse>('refresh_prices', { args: {} });
      if (r.refreshed && r.meta) {
        priceMeta = r.meta;
        // Trigger a re-plan with the new prices.
        await refresh();
      } else if (r.error) {
        priceError = r.error;
      }
    } catch (err) {
      priceError = String(err);
    } finally {
      priceLoading = false;
    }
  }

  async function refresh() {
    loading = true;
    error = null;
    try {
      const args: RecommendArgs = {
        item,
        goal,
        stash: { unlimited: true },
        risk,
        top_n: 5,
        depth,
      };
      const response = await invoke<RecommendResponse>('recommend', { args });
      recommendations = response.recommendations;
      onRecommendations?.(recommendations);
      meta = {
        patch: response.patch,
        rule_count: response.rule_count,
        strategy_count: response.strategy_count,
        mod_count: response.mod_count,
        bundle_path: response.bundle_path,
      };
    } catch (err) {
      error = String(err);
      recommendations = [];
    } finally {
      loading = false;
    }
  }

  // Re-plan on any input change.
  $effect(() => {
    void item;
    void goal;
    void risk;
    void depth;
    refresh();
  });

  function describeAction(a: AdvisorAction): string {
    switch (a.kind) {
      case 'apply_currency':
        return a.omens.length
          ? `${a.currency} + omens [${a.omens.join(', ')}]`
          : a.currency;
      case 'activate_omen':
        return `Activate omen: ${a.omen}`;
      case 'apply_hinekoras_lock':
        return "Apply Hinekora's Lock";
      case 'reveal': {
        const prefer = a.prefer.join(', ') || 'any';
        const echoes = a.use_abyssal_echoes ? ' + Abyssal Echoes' : '';
        const floor = a.min_acceptable ? ` (require ${a.min_acceptable})` : '';
        const fail = a.abandon_if_no_match ? ' [abandon on no match]' : '';
        return `Reveal at Well of Souls (prefer: ${prefer})${echoes}${floor}${fail}`;
      }
      case 'recombine': {
        const omens = a.omens.length ? ` + omens [${a.omens.join(', ')}]` : '';
        return `Recombine with ${a.other_item_id}${omens}`;
      }
      case 'stop':
        return 'Stop — goal already met';
      case 'abandon':
        return `Abandon: ${a.reason}`;
      case 'guidance':
        return `Guidance: ${a.note}`;
    }
  }

  function describeSource(r: Recommendation): string {
    switch (r.source.kind) {
      case 'rule':
        return `rule ${r.source.id} (${r.source.confidence})`;
      case 'strategy':
        return `strategy ${r.source.id} :: ${r.source.step}`;
      case 'heuristic':
        return `heuristic ${r.source.name}`;
    }
  }

  function fmtDiv(c: { min: number; expected: number; max: number }): string {
    if (c.expected === 0) return 'free';
    return `${c.min.toFixed(2)} – ${c.max.toFixed(2)} div (≈${c.expected.toFixed(2)})`;
  }
</script>

<section class="advisor">
  <h2>Advisor</h2>

  <div class="controls">
    <label>
      Risk: {risk.toFixed(2)}
      <input type="range" min="0" max="1" step="0.05" bind:value={risk} />
    </label>
    <label>
      Depth: {depth}
      <input type="range" min="1" max="5" step="1" bind:value={depth} />
    </label>
    <button onclick={refresh} disabled={loading}>
      {loading ? 'planning…' : 'Re-plan'}
    </button>
    <button onclick={refreshPrices} disabled={priceLoading} class="secondary">
      {priceLoading ? 'fetching…' : 'Refresh prices'}
    </button>
  </div>

  {#if priceMeta}
    <p class="price-meta">
      live prices: {priceMeta.league} · {priceMeta.applied_count} of {priceMeta.total_entries}
      currencies priced @ {priceMeta.fetched_at}
    </p>
  {/if}
  {#if priceError}
    <pre class="error">price refresh failed: {priceError}</pre>
  {/if}

  {#if meta}
    <p class="meta">
      patch {meta.patch} · {meta.rule_count} rules · {meta.strategy_count} strategies ·
      {meta.mod_count} mods
      {#if meta.bundle_path}
        · bundle: {meta.bundle_path}
      {:else}
        · <span class="warn">no bundle loaded</span>
      {/if}
    </p>
  {/if}

  {#if error}
    <pre class="error">{error}</pre>
  {/if}

  {#if recommendations.length === 0 && !loading}
    <p class="muted">No recommendations.</p>
  {/if}

  <ol class="recommendations">
    {#each recommendations as r, i (i)}
      <li>
        <div class="row">
          <span class="action">{describeAction(r.action)}</span>
          <span class="score">score {r.score.toFixed(3)}</span>
        </div>
        <div class="meta-row">
          <span class="cost">{fmtDiv(r.expected_cost)}</span>
          <span>·</span>
          <span class="prob">P(reach) ≈ {(r.expected_prob * 100).toFixed(1)}%</span>
          <span>·</span>
          <span class="depth">depth {r.depth}</span>
        </div>
        <div class="rationale">{r.rationale}</div>
        <div class="source">{describeSource(r)}</div>
      </li>
    {/each}
  </ol>
</section>

<style>
  .controls {
    display: flex;
    gap: 1rem;
    align-items: center;
    margin-bottom: 0.75rem;
    flex-wrap: wrap;
  }

  .controls label {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    font-size: 0.85rem;
    color: var(--fg-muted);
  }

  .controls input[type='range'] {
    accent-color: var(--accent);
  }

  .meta {
    font-size: 0.8rem;
    color: var(--fg-muted);
    margin: 0 0 0.75rem;
    word-break: break-all;
  }

  .price-meta {
    font-size: 0.75rem;
    color: #a6d09a;
    margin: 0 0 0.5rem;
    word-break: break-all;
  }

  button.secondary {
    background: var(--bg);
    color: var(--fg-muted);
    border: 1px solid var(--border);
    font-weight: 400;
  }

  .warn {
    color: var(--accent);
  }

  .error {
    background: #2a1010;
    border-color: #5a2222;
    color: #ff8c8c;
  }

  .muted {
    color: var(--fg-muted);
  }

  .recommendations {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .recommendations li {
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 0.6rem 0.75rem;
  }

  .row {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: 1rem;
    margin-bottom: 0.25rem;
  }

  .action {
    font-weight: 600;
    color: var(--fg);
  }

  .score {
    font-family: ui-monospace, 'Fira Code', monospace;
    font-size: 0.8rem;
    color: var(--accent);
  }

  .meta-row {
    display: flex;
    gap: 0.4rem;
    font-size: 0.8rem;
    color: var(--fg-muted);
    font-family: ui-monospace, 'Fira Code', monospace;
    margin-bottom: 0.25rem;
  }

  .rationale {
    font-size: 0.85rem;
    color: var(--fg);
    margin-bottom: 0.25rem;
  }

  .source {
    font-size: 0.75rem;
    color: var(--fg-muted);
    font-family: ui-monospace, 'Fira Code', monospace;
  }
</style>
