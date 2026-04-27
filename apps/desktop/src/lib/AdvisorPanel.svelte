<script lang="ts">
  import { invoke, listen } from './tauri';
  import type { UnlistenFn } from '@tauri-apps/api/event';
  import { actionAssetId, assetUrl, initials, type AssetIndex } from './assets';
  import { checkCanApply, formatCannotApply } from './cannotApply';
  import OutcomeDialog from './OutcomeDialog.svelte';
  import {
    ADVISOR_PROGRESS_EVENT,
    type AdvisorAction,
    type Goal,
    type Item,
    type PriceRefreshMeta,
    type Recommendation,
    type RecommendArgs,
    type RecommendResponse,
    type RefreshPricesResponse,
    type StreamingProgressEvent,
  } from './types';

  type Props = {
    item: Item;
    goal: Goal;
    assetIndex?: AssetIndex;
    /** When true, the parent (App.svelte) is supplying the panel
     * chrome (outer card, tab strip, "Step-by-Step Guide" title) so we
     * skip our own `<section class="panel">` wrapper and the redundant
     * h2 header. The toolbar, success chip, and recommendations list
     * still render — embedded mode is purely a layout flag. */
    embedded?: boolean;
    /** Optional callback so the App.svelte parent can lift the latest
     * recommendation list (used by the RecoveryPanel to display the
     * current top recommendation's step recovery hints). */
    onRecommendations?: (recs: Recommendation[]) => void;
    /** Called when a step outcome updates the item state. */
    onItemUpdate?: (
      item: Item,
      change: string,
      explanation: string,
      recommendation: Recommendation | null,
    ) => void;
  };

  let {
    item,
    goal,
    assetIndex = new Map(),
    embedded = false,
    onRecommendations,
    onItemUpdate,
  }: Props = $props();
  let outcomeAction = $state<AdvisorAction | null>(null);
  let outcomeRecommendation = $state<Recommendation | null>(null);

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
  let streamingDepth = $state<number | null>(null);
  let priceLoading = $state(false);
  let priceError = $state<string | null>(null);
  let error = $state<string | null>(null);
  let risk = $state(0.5);
  let depth = $state(2);
  let useStreaming = $state(true);
  let failedAssetIds = $state<string[]>([]);
  let unlistenStream: UnlistenFn | null = null;
  let activeRequestId = $state(0);

  // Subscribe to the streaming-planner event channel exactly once.
  $effect(() => {
    let cancelled = false;
    listen<StreamingProgressEvent>(ADVISOR_PROGRESS_EVENT, (ev) => {
      if (cancelled) return;
      const p = ev.payload;
      if (p.request_id !== activeRequestId) return;
      recommendations = p.recommendations;
      onRecommendations?.(recommendations);
      streamingDepth = p.depth;
      meta = meta
        ? { ...meta, patch: p.patch }
        : {
            patch: p.patch,
            rule_count: 0,
            strategy_count: 0,
            mod_count: 0,
            bundle_path: null,
          };
      if (p.is_final) {
        loading = false;
        streamingDepth = null;
      }
    }).then((u) => {
      if (cancelled) {
        u();
        return;
      }
      unlistenStream = u;
    });
    return () => {
      cancelled = true;
      if (unlistenStream) {
        unlistenStream();
        unlistenStream = null;
      }
    };
  });

  async function searchTrade() {
    try {
      await invoke('trade_search', { args: { item, open: true } });
    } catch (err) {
      error = String(err);
    }
  }

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
    const requestId = activeRequestId + 1;
    activeRequestId = requestId;
    loading = true;
    error = null;
    recommendations = [];
    onRecommendations?.(recommendations);
    try {
      const args: RecommendArgs = {
        item,
        goal,
        stash: { unlimited: true },
        risk,
        top_n: 5,
        depth,
        request_id: requestId,
      };
      if (useStreaming) {
        // Streaming: emits via the advisor://progress event listener
        // above. The terminal event clears `loading`.
        await invoke('recommend_streaming', { args });
        // Always also fire one synchronous call so meta counts are
        // populated even when streaming hasn't completed yet.
        const response = await invoke<RecommendResponse>('recommend', { args });
        if (requestId !== activeRequestId) return;
        if (recommendations.length === 0) {
          recommendations = response.recommendations;
          onRecommendations?.(recommendations);
        }
        meta = {
          patch: response.patch,
          rule_count: response.rule_count,
          strategy_count: response.strategy_count,
          mod_count: response.mod_count,
          bundle_path: response.bundle_path,
        };
        loading = false;
        streamingDepth = null;
        return; // loading cleared by the final stream event
      }
      const response = await invoke<RecommendResponse>('recommend', { args });
      if (requestId !== activeRequestId) return;
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
      if (requestId !== activeRequestId) return;
      error = String(err);
      recommendations = [];
    } finally {
      if (requestId === activeRequestId && !useStreaming) loading = false;
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
      case 'recurring': {
        // Phase B.4 — describe the loop body in one line.
        const inner = a.inner.map((x) => describeAction(x)).join(' → ');
        return `Loop: ${inner}`;
      }
    }
  }

  function actionTitle(a: AdvisorAction): string {
    switch (a.kind) {
      case 'apply_currency':
        return prettifyId(a.currency);
      case 'activate_omen':
        return `Activate ${prettifyId(a.omen)}`;
      case 'apply_hinekoras_lock':
        return "Hinekora's Lock";
      case 'reveal':
        return 'Reveal at Well of Souls';
      case 'recombine':
        return 'Recombine';
      case 'stop':
        return 'Stop · goal met';
      case 'abandon':
        return 'Abandon';
      case 'guidance':
        return 'Guidance';
      case 'recurring':
        return 'Recurring step';
    }
  }

  /** Phase A.2 — IPC-backed cannot-apply cache, keyed by currency id.
   * Refreshed when the item or recommendations change. Falls back to
   * `null` (no badge) when the IPC hasn't returned yet, so the UI
   * never shows a stale or fabricated reason. */
  let cannotApplyByCurrency = $state<Record<string, string | null>>({});

  $effect(() => {
    const currencies = uniqueCurrenciesFromRecommendations(recommendations);
    const itemSnapshot = item;
    let cancelled = false;
    cannotApplyByCurrency = {};
    Promise.all(
      currencies.map(async (c) => {
        const view = await checkCanApply(itemSnapshot, c);
        return [c, formatCannotApply(view)] as const;
      }),
    )
      .then((pairs) => {
        if (cancelled) return;
        const next: Record<string, string | null> = {};
        for (const [c, reason] of pairs) next[c] = reason;
        cannotApplyByCurrency = next;
      })
      .catch(() => {
        // Soft-fail: leave the cache empty so badges don't appear with
        // stale text after an IPC error.
        if (!cancelled) cannotApplyByCurrency = {};
      });
    return () => {
      cancelled = true;
    };
  });

  function uniqueCurrenciesFromRecommendations(recs: Recommendation[]): string[] {
    const set = new Set<string>();
    for (const r of recs) {
      if (r.action.kind === 'apply_currency') set.add(r.action.currency);
      if (r.action.kind === 'recurring') {
        for (const inner of r.action.inner) {
          if (inner.kind === 'apply_currency') set.add(inner.currency);
        }
      }
    }
    return [...set];
  }

  /** Phase D.1 — read the cached IPC verdict for a recommendation's
   * action. Returns `null` when the action isn't a currency apply (no
   * gate to surface) or when the cache hasn't loaded yet. */
  function cannotApplyReason(a: AdvisorAction): string | null {
    if (a.kind === 'apply_currency') {
      return cannotApplyByCurrency[a.currency] ?? null;
    }
    if (a.kind === 'recurring' && a.inner.length > 0) {
      const first = a.inner[0];
      if (first.kind === 'apply_currency') {
        return cannotApplyByCurrency[first.currency] ?? null;
      }
    }
    return null;
  }

  function recordUnsupportedReason(a: AdvisorAction): string | null {
    if (a.kind === 'apply_currency') {
      const id = a.currency;
      const supported = new Set([
        'OrbOfAnnulment',
        'OrbOfTransmutation',
        'GreaterOrbOfTransmutation',
        'PerfectOrbOfTransmutation',
        'OrbOfAugmentation',
        'GreaterOrbOfAugmentation',
        'PerfectOrbOfAugmentation',
        'ExaltedOrb',
        'GreaterExaltedOrb',
        'PerfectExaltedOrb',
        'RegalOrb',
        'GreaterRegalOrb',
        'PerfectRegalOrb',
        'ChaosOrb',
        'GreaterChaosOrb',
        'PerfectChaosOrb',
        'DivineOrb',
      ]);
      if (supported.has(id) || id.includes('Essence')) return null;
      if (id === 'FracturingOrb') return 'fracture recording is not implemented yet';
      if (id === 'VaalOrb') return 'corruption outcome recording is not implemented yet';
      return `${prettifyId(id)} outcome recording is not implemented yet`;
    }
    if (a.kind === 'guidance') return 'guidance is informational';
    if (a.kind === 'stop') return 'nothing to record';
    if (a.kind === 'abandon') return 'abandon does not change the item';
    if (a.kind === 'reveal') return 'reveal outcome recording is not implemented yet';
    if (a.kind === 'recurring') return 'record the concrete inner action instead';
    if (a.kind === 'apply_hinekoras_lock') return "Hinekora's Lock recording is not implemented yet";
    if (a.kind === 'activate_omen') return 'omen-only recording is not implemented yet';
    if (a.kind === 'recombine') return 'recombine recording is not implemented yet';
    return null;
  }

  function displayCost(r: Recommendation): { min: number; expected: number; max: number } {
    return r.loop_estimate?.total_cost ?? r.expected_cost;
  }

  /** Phase D.1 — render the StopPredicate as a friendly string. */
  function stopPredicateSummary(stop: import('./types').StopPredicate): string {
    const parts: string[] = [];
    for (const c of stop.concepts ?? []) {
      const affixSuffix = c.affix ? ` on ${c.affix}` : '';
      parts.push(`T${c.min_tier}+ ${c.concept}${affixSuffix}`);
    }
    if (stop.max_mods != null) {
      parts.push(`≤ ${stop.max_mods} mods`);
    }
    return parts.length > 0 ? `Stop when: ${parts.join(' AND ')}` : 'Stop when: never';
  }

  function actionSubtitle(a: AdvisorAction): string {
    switch (a.kind) {
      case 'apply_currency':
        return a.omens.length ? `+ omens: ${a.omens.map(prettifyId).join(', ')}` : '';
      case 'reveal':
        return a.prefer.length ? `prefer ${a.prefer.join(', ')}` : 'any concept';
      case 'recombine':
        return `with ${a.other_item_id}`;
      case 'guidance':
        return a.note;
      case 'abandon':
        return a.reason;
      default:
        return '';
    }
  }

  function prettifyId(id: string): string {
    return id
      .replace(/([a-z0-9])([A-Z])/g, '$1 $2')
      .replace(/_/g, ' ')
      .trim();
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

  function actionIcon(a: AdvisorAction): string | null {
    const id = actionAssetId(a);
    if (id && failedAssetIds.includes(id)) return null;
    return assetUrl(assetIndex, id);
  }

  function markAssetFailed(a: AdvisorAction) {
    const id = actionAssetId(a);
    if (id && !failedAssetIds.includes(id)) failedAssetIds = [...failedAssetIds, id];
  }
</script>

<svelte:element this={embedded ? 'div' : 'section'} class={embedded ? 'advisor advisor-embedded' : 'panel advisor'}>
  {#if !embedded}
    <header class="advisor-head">
      <h2>Step-by-Step Guide</h2>
      <div class="success-box" title="Top recommendation's MC-aggregated success probability">
        <span>Success</span>
        <strong>
          {recommendations[0]
            ? (recommendations[0].expected_prob * 100).toFixed(1)
            : '—'}{recommendations[0] ? '%' : ''}
        </strong>
      </div>
    </header>
  {/if}

  <div class="controls">
    <label class="slider">
      <span>Risk <em>{risk.toFixed(2)}</em></span>
      <input type="range" min="0" max="1" step="0.05" bind:value={risk} />
    </label>
    <label class="slider">
      <span>Depth <em>{depth}</em></span>
      <input type="range" min="1" max="5" step="1" bind:value={depth} />
    </label>
    <button class="primary" onclick={refresh} disabled={loading}>
      {loading
        ? streamingDepth !== null
          ? `streaming d${streamingDepth}…`
          : 'planning…'
        : 'Re-plan'}
    </button>
    <label class="streaming-toggle">
      <input type="checkbox" bind:checked={useStreaming} /> streaming
    </label>
    <button onclick={refreshPrices} disabled={priceLoading} class="secondary">
      {priceLoading ? 'fetching…' : 'Refresh prices'}
    </button>
    <button onclick={searchTrade} class="secondary">Search trade</button>
  </div>

  {#if priceMeta}
    <p class="price-meta">
      live prices: {priceMeta.league} · {priceMeta.applied_count} of
      {priceMeta.total_entries} currencies priced @ {priceMeta.fetched_at}
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
        · bundle <span class="bundle-name" title={meta.bundle_path}>
          {meta.bundle_path.split('/').pop() ?? meta.bundle_path}
        </span>
      {:else}
        · <span class="warn">no bundle loaded</span>
      {/if}
    </p>
  {/if}

  {#if error}
    <pre class="error">{error}</pre>
  {/if}

  {#if recommendations.length === 0 && !loading}
    <p class="muted">No recommendations yet.</p>
  {/if}

  <ol class="recommendations">
    {#each recommendations as r, i (i)}
      <li class:current={i === 0}>
        <div class="step-rail">
          <div class="step-index">{i + 1}</div>
          {#if i < recommendations.length - 1}<span class="step-line"></span>{/if}
        </div>
        <div class="step-icon">
          {#if actionIcon(r.action)}
            <img
              src={actionIcon(r.action) ?? ''}
              alt=""
              onerror={() => markAssetFailed(r.action)}
            />
          {:else}
            <span>{initials(describeAction(r.action))}</span>
          {/if}
        </div>
        <div class="step-content">
          <div class="row">
            <div>
              <span class="action">{actionTitle(r.action)}</span>
              {#if actionSubtitle(r.action)}
                <span class="action-sub">{actionSubtitle(r.action)}</span>
              {/if}
            </div>
            <div class="step-prob">
              <strong>{(r.expected_prob * 100).toFixed(1)}%</strong>
              {#if r.prob_stderr > 0}<small>± {(r.prob_stderr * 100).toFixed(1)}%</small>{/if}
            </div>
          </div>
          <div class="prob-bar" style:--p="{Math.min(100, Math.max(0, r.expected_prob * 100)).toFixed(1)}%">
            <span></span>
          </div>
          <div class="meta-row">
            <span class="cost">{fmtDiv(displayCost(r))}</span>
            <span class="dot">·</span>
            <span class="depth">d{r.depth}</span>
            <span class="dot">·</span>
            <span class="score" title="Planner score (Q-uplift × heuristic)">
              {r.score.toFixed(2)}
            </span>
            {#if cannotApplyReason(r.action)}
              <span class="cannot-badge">cannot apply · {cannotApplyReason(r.action)}</span>
            {/if}
            {#if recordUnsupportedReason(r.action)}
              <span class="cannot-badge">not recordable · {recordUnsupportedReason(r.action)}</span>
            {/if}
          </div>
          {#if r.action.kind === 'recurring'}
            <!-- Phase D.1 — recurring step card surface -->
            <div class="recurring-card">
              <div class="recurring-row">
                <span class="recurring-stop">{stopPredicateSummary(r.action.stop)}</span>
                {#if r.loop_estimate}
                  <span class="recurring-iter">
                    ≈ {r.loop_estimate.mean_iterations.toFixed(1)} ±
                    {r.loop_estimate.iter_stderr.toFixed(1)} iterations
                  </span>
                {/if}
              </div>
              {#if r.loop_estimate}
                <div class="recurring-row">
                  <span class="recurring-cost">
                    Total cost: {fmtDiv(r.loop_estimate.total_cost)}
                  </span>
                </div>
              {/if}
              <details class="recurring-inner">
                <summary>Show inner sequence</summary>
                <ol class="recurring-list">
                  {#each r.action.inner as step, i (i)}
                    <li>{describeAction(step)}</li>
                  {/each}
                </ol>
              </details>
            </div>
          {/if}
          {#if r.rationale}
            <div class="rationale">{r.rationale}</div>
          {/if}
          <details class="why">
            <summary>Why this step?</summary>
            <p>
              <strong>Source:</strong> {describeSource(r)}
            </p>
            <p>
              <strong>Probability:</strong>
              {(r.expected_prob * 100).toFixed(1)}%
              {#if r.prob_stderr > 0}± {(r.prob_stderr * 100).toFixed(1)}%{/if}
            </p>
            <p>
              <strong>Cost:</strong>
              {fmtDiv(displayCost(r))}
            </p>
            <p class="hint">
              Click "I just used this" to record what actually rolled. The advisor
              re-plans from the new item state.
            </p>
          </details>
          <div class="step-footer">
            <span class="source" title={describeSource(r)}>{describeSource(r)}</span>
            <button
              class="step-cta"
              type="button"
              disabled={cannotApplyReason(r.action) !== null || recordUnsupportedReason(r.action) !== null}
              title={recordUnsupportedReason(r.action) ?? cannotApplyReason(r.action) ?? 'Record this outcome'}
              onclick={() => {
                outcomeAction = r.action;
                outcomeRecommendation = r;
              }}
            >
              I just used this →
            </button>
          </div>
        </div>
      </li>
    {/each}
  </ol>
</svelte:element>

<OutcomeDialog
  {item}
  action={outcomeAction}
  onApply={(updated, change, explanation) => {
    onItemUpdate?.(updated, change, explanation, outcomeRecommendation);
  }}
  onClose={() => {
    outcomeAction = null;
    outcomeRecommendation = null;
  }}
/>

<style>
  .advisor {
    display: flex;
    flex-direction: column;
    gap: 0.8rem;
    height: 100%;
    min-height: 0;
    overflow: hidden;
  }

  /* When the parent renders the panel chrome (App.svelte left column
   * with its own tab strip), strip the outer padding and let the
   * recommendations list fill the remaining space. */
  .advisor.advisor-embedded {
    gap: 0.6rem;
    height: 100%;
  }

  .advisor-head {
    display: flex;
    justify-content: space-between;
    gap: 0.8rem;
    align-items: center;
    border-bottom: 1px solid rgba(255, 255, 255, 0.07);
    padding-bottom: 0.7rem;
  }

  h2 {
    margin: 0;
    color: var(--gold-bright);
    font-family: Georgia, 'Times New Roman', serif;
    font-weight: 500;
    letter-spacing: 0.04em;
    font-size: 1.05rem;
    text-transform: uppercase;
  }

  .success-box {
    min-width: 110px;
    border: 1px solid rgba(114, 255, 88, 0.32);
    background: radial-gradient(circle, rgba(47, 123, 18, 0.2), rgba(0, 0, 0, 0.22));
    color: #72ff58;
    padding: 0.3rem 0.7rem;
    display: inline-flex;
    align-items: baseline;
    gap: 0.45rem;
    border-radius: 999px;
  }

  .success-box span {
    font-size: 0.68rem;
    text-transform: uppercase;
    letter-spacing: 0.1em;
    color: rgba(178, 255, 162, 0.75);
  }

  .success-box strong {
    font-size: 1.15rem;
    font-family: Georgia, 'Times New Roman', serif;
    line-height: 1;
  }

  .controls {
    display: flex;
    gap: 0.7rem;
    align-items: center;
    flex-wrap: wrap;
    background: rgba(255, 255, 255, 0.025);
    border: 1px solid rgba(255, 255, 255, 0.07);
    padding: 0.55rem 0.7rem;
    border-radius: 12px;
    flex-shrink: 0;
  }

  .controls label.slider {
    display: grid;
    grid-template-columns: auto 1fr;
    align-items: center;
    gap: 0.5rem;
    font-size: 0.7rem;
    color: var(--fg-muted);
    text-transform: uppercase;
    letter-spacing: 0.08em;
    min-width: 150px;
  }

  .controls label.slider input[type='range'] {
    width: 100%;
    min-width: 70px;
  }

  .controls label.slider em {
    font-style: normal;
    color: var(--gold-bright);
    margin-left: 0.4rem;
  }

  .controls input[type='range'] {
    accent-color: var(--gold);
  }

  .controls button.primary {
    background: linear-gradient(180deg, rgba(220, 165, 70, 0.95), rgba(150, 105, 30, 0.95));
    color: #1a1100;
    border: 1px solid rgba(255, 211, 122, 0.85);
    border-radius: 10px;
    padding: 0.45rem 0.85rem;
    font-weight: 700;
    cursor: pointer;
  }

  .controls button.primary:disabled {
    opacity: 0.55;
    cursor: progress;
  }

  .meta {
    font-size: 0.74rem;
    color: var(--fg-muted);
    margin: 0;
    display: flex;
    flex-wrap: wrap;
    gap: 0.25rem;
    align-items: baseline;
  }

  .bundle-name {
    color: var(--fg-soft);
    font-family: ui-monospace, 'Fira Code', monospace;
    font-size: 0.72rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 18rem;
  }

  .price-meta {
    font-size: 0.75rem;
    color: #a6d09a;
    margin: 0;
    word-break: break-all;
  }

  button.secondary {
    background: rgba(0, 0, 0, 0.28);
    color: var(--fg-muted);
    border: 1px solid var(--border);
    font-weight: 400;
    padding: 0.4rem 0.75rem;
    border-radius: 10px;
    cursor: pointer;
  }

  button.secondary:hover {
    color: var(--gold);
    border-color: var(--border-gold);
  }

  .streaming-toggle {
    display: flex;
    align-items: center;
    gap: 0.3rem;
    font-size: 0.75rem;
    color: var(--fg-muted);
  }

  .warn {
    color: var(--accent);
  }

  .error {
    background: #2a1010;
    border-color: #5a2222;
    color: #ff8c8c;
    padding: 0.5rem;
    border-radius: 4px;
  }

  .muted {
    color: var(--fg-muted);
  }

  .recommendations {
    list-style: none;
    padding: 0 2px 0 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 0.65rem;
    overflow-y: auto;
    overflow-x: hidden;
    flex: 1 1 auto;
    min-height: 0;
  }

  .recommendations li {
    position: relative;
    display: grid;
    grid-template-columns: 30px 48px minmax(0, 1fr);
    gap: 0.6rem;
    align-items: stretch;
    background: linear-gradient(90deg, rgba(18, 23, 24, 0.98), rgba(8, 11, 13, 0.98));
    border: 1px solid rgba(255, 255, 255, 0.075);
    border-radius: 16px;
    padding: 0.72rem 0.82rem;
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.035);
  }

  .recommendations li.current {
    border-color: rgba(114, 255, 88, 0.38);
    background: linear-gradient(90deg, rgba(16, 31, 20, 0.98), rgba(8, 11, 13, 0.98));
    box-shadow: inset 0 1px 0 rgba(114, 255, 88, 0.12), 0 18px 32px rgba(0, 0, 0, 0.16);
  }

  .step-rail {
    display: grid;
    grid-template-rows: auto 1fr;
    gap: 0.25rem;
    align-items: start;
    justify-items: center;
  }

  .step-index {
    width: 26px;
    height: 26px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    color: var(--gold-bright);
    border: 1px solid var(--border-gold);
    background: #050505;
    font-family: Georgia, 'Times New Roman', serif;
    font-size: 0.85rem;
  }

  .recommendations li.current .step-index {
    color: #1a1100;
    background: linear-gradient(180deg, #ffd37a, #c58f3d);
    border-color: #ffd37a;
  }

  .step-line {
    width: 1px;
    height: 100%;
    background: rgba(197, 143, 61, 0.3);
  }

  .step-icon {
    width: 48px;
    height: 48px;
    display: grid;
    place-items: center;
    border: 1px solid rgba(197, 143, 61, 0.28);
    background: radial-gradient(circle, rgba(197, 143, 61, 0.15), rgba(0, 0, 0, 0.5));
    border-radius: 12px;
  }

  .step-icon img {
    max-width: 40px;
    max-height: 40px;
    object-fit: contain;
  }

  .step-icon span {
    color: #00c8ff;
    font-weight: 700;
  }

  .step-content {
    min-width: 0;
    display: grid;
    gap: 0.35rem;
  }

  .row {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: 1rem;
  }

  .row > div:first-child {
    display: grid;
    gap: 0.15rem;
  }

  .action {
    font-weight: 600;
    color: var(--fg);
  }

  .action-sub {
    font-size: 0.8rem;
    color: var(--fg-muted);
  }

  .step-prob {
    text-align: right;
    color: #72ff58;
    font-family: Georgia, 'Times New Roman', serif;
    flex-shrink: 0;
  }

  .step-prob strong {
    font-size: 1.05rem;
  }

  .step-prob small {
    display: block;
    color: var(--fg-muted);
    font-family: ui-monospace, 'Fira Code', monospace;
  }

  .prob-bar {
    height: 6px;
    background: rgba(255, 255, 255, 0.06);
    border-radius: 999px;
    overflow: hidden;
    border: 1px solid rgba(197, 143, 61, 0.18);
  }

  .prob-bar span {
    display: block;
    height: 100%;
    width: var(--p, 0%);
    background: linear-gradient(90deg, rgba(34, 200, 70, 0.95), rgba(120, 235, 110, 0.95));
  }

  .recommendations li:not(.current) .prob-bar span {
    background: linear-gradient(90deg, rgba(197, 143, 61, 0.85), rgba(255, 211, 122, 0.85));
  }

  .meta-row {
    display: flex;
    gap: 0.45rem;
    font-size: 0.78rem;
    color: var(--fg-muted);
    font-family: ui-monospace, 'Fira Code', monospace;
  }

  .meta-row .dot {
    opacity: 0.5;
  }

  .score {
    color: var(--gold);
  }

  .rationale {
    font-size: 0.85rem;
    color: var(--fg);
  }

  .source {
    font-size: 0.7rem;
    color: var(--fg-muted);
    font-family: ui-monospace, 'Fira Code', monospace;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 22rem;
    flex: 1 1 auto;
    min-width: 0;
  }

  .step-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.45rem;
    margin-top: 0.15rem;
  }

  .step-cta {
    background: rgba(0, 0, 0, 0.28);
    color: var(--gold);
    border: 1px solid var(--border-gold);
    border-radius: 999px;
    padding: 0.25rem 0.7rem;
    cursor: pointer;
    font-size: 0.74rem;
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }

  .step-cta:hover {
    background: rgba(197, 143, 61, 0.15);
    color: var(--gold-bright);
  }

  .step-cta:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  /* Phase D.1 — cannot-apply badge */
  .cannot-badge {
    background: rgba(80, 25, 25, 0.55);
    color: #ff8c8c;
    border: 1px solid #5a2222;
    border-radius: 999px;
    padding: 0 0.45rem;
    font-size: 0.7rem;
  }

  /* Phase D.1 — recurring step card */
  .recurring-card {
    border: 1px solid var(--border-strong);
    background: rgba(40, 25, 70, 0.18);
    border-radius: 4px;
    padding: 0.4rem 0.55rem;
    display: grid;
    gap: 0.25rem;
  }

  .recurring-row {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 0.5rem;
    font-size: 0.78rem;
  }

  .recurring-stop {
    color: #a98dff;
    font-family: ui-monospace, 'Fira Code', monospace;
  }

  .recurring-iter {
    color: var(--gold);
    font-family: ui-monospace, 'Fira Code', monospace;
    font-size: 0.74rem;
  }

  .recurring-cost {
    color: var(--fg-soft);
    font-family: ui-monospace, 'Fira Code', monospace;
    font-size: 0.74rem;
  }

  .recurring-inner summary {
    color: var(--gold);
    cursor: pointer;
    font-size: 0.74rem;
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }

  .recurring-list {
    margin: 0.25rem 0 0;
    padding-left: 1.1rem;
    color: var(--fg-soft);
    font-size: 0.78rem;
    font-family: ui-monospace, 'Fira Code', monospace;
  }

  .recommendations li.current .step-cta {
    background: linear-gradient(180deg, rgba(220, 165, 70, 0.95), rgba(150, 105, 30, 0.95));
    color: #1a1100;
    border-color: rgba(255, 211, 122, 0.85);
  }

  .why {
    border: 1px solid rgba(197, 143, 61, 0.22);
    border-radius: 10px;
    background: rgba(0, 0, 0, 0.25);
    padding: 0.35rem 0.5rem;
  }

  .why summary {
    cursor: pointer;
    color: var(--gold);
    font-size: 0.78rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    font-family: Georgia, 'Times New Roman', serif;
  }

  .why p {
    margin: 0.3rem 0 0;
    font-size: 0.8rem;
    color: var(--fg-soft);
  }

  .why p strong {
    color: var(--gold-bright);
    font-weight: 600;
    margin-right: 0.35rem;
  }

  .why .hint {
    color: var(--fg-muted);
    font-style: italic;
  }
</style>
