<script lang="ts">
  import { invoke } from './tauri';
  import type { AdvisorAction, Item, TrialDistribution } from './types';

  type Props = {
    /** Item to simulate against. */
    item: Item;
    /** Action to run repeatedly. v1 surfaces the current top
     * recommendation's action by default. */
    action: AdvisorAction | null;
  };

  let { item, action }: Props = $props();
  let nTrials = $state(1000);
  let result = $state<TrialDistribution | null>(null);
  let running = $state(false);
  let error = $state<string | null>(null);

  async function run() {
    if (!action) return;
    running = true;
    error = null;
    try {
      result = await invoke<TrialDistribution>('run_n_trials', {
        args: { item, action, n_trials: nTrials },
      });
    } catch (e) {
      error = String(e);
      result = null;
    } finally {
      running = false;
    }
  }

  function describeAction(a: AdvisorAction): string {
    switch (a.kind) {
      case 'apply_currency':
        return a.omens.length
          ? `${a.currency} + [${a.omens.join(', ')}]`
          : a.currency;
      case 'activate_omen':
        return `Activate omen: ${a.omen}`;
      case 'apply_hinekoras_lock':
        return "Hinekora's Lock";
      case 'reveal':
        return 'Reveal at Well of Souls';
      case 'recombine':
        return 'Recombine';
      case 'stop':
        return 'Stop';
      case 'abandon':
        return 'Abandon';
      case 'guidance':
        return 'Guidance';
      case 'recurring':
        return `Loop · ${a.inner.length} step${a.inner.length === 1 ? '' : 's'}`;
    }
  }

  function histogramBars(h: Record<number, number>): {
    bucket: number;
    count: number;
    fraction: number;
  }[] {
    const entries = Object.entries(h)
      .map(([k, v]) => ({ bucket: Number(k), count: Number(v) }))
      .sort((a, b) => a.bucket - b.bucket);
    const total = entries.reduce((s, e) => s + e.count, 0);
    return entries.map((e) => ({
      ...e,
      fraction: total === 0 ? 0 : e.count / total,
    }));
  }
</script>

<section class="sim-runner">
  <h2>Simulation runner</h2>

  {#if !action}
    <p class="muted">No action selected. Run the advisor first; this panel
      simulates the top recommendation N times.</p>
  {:else}
    <div class="controls">
      <span class="action-label">action: <code>{describeAction(action)}</code></span>
      <label>
        N
        <input type="number" min="1" max="10000" step="100" bind:value={nTrials} />
      </label>
      <button onclick={run} disabled={running}>
        {running ? 'simulating…' : `Run ${nTrials} trials`}
      </button>
    </div>

    {#if error}
      <pre class="error">{error}</pre>
    {/if}

    {#if result}
      <!-- Summary stats -->
      <dl class="summary">
        <dt>success rate</dt>
        <dd>
          {(result.success_rate * 100).toFixed(2)}%
          {#if result.success_rate_stderr > 0}
            ± {(result.success_rate_stderr * 100).toFixed(2)}%
          {/if}
        </dd>
        <dt>mean change count</dt>
        <dd>{result.mean_change_count.toFixed(2)}</dd>
        <dt>cost per trial</dt>
        <dd>{result.cost_per_trial_div.toFixed(3)} div</dd>
        <dt>expected total cost</dt>
        <dd>{result.total_cost_div_expected.toFixed(2)} div ({result.n_trials} trials)</dd>
      </dl>

      <!-- Lightweight inline-SVG histogram -->
      {#if Object.keys(result.change_count_histogram).length > 0}
        <h3>Change-count distribution</h3>
        <div class="histogram">
          {#each histogramBars(result.change_count_histogram) as bar (bar.bucket)}
            <div class="bar-row" title="{bar.count} trials at change_count={bar.bucket}">
              <span class="bucket">{bar.bucket}</span>
              <div class="bar-track">
                <div
                  class="bar-fill"
                  style="width: {(bar.fraction * 100).toFixed(1)}%"
                ></div>
              </div>
              <span class="count">{bar.count}</span>
            </div>
          {/each}
        </div>
      {/if}
    {/if}
  {/if}
</section>

<style>
  .sim-runner {
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 0.75rem;
  }

  h2 {
    margin: 0 0 0.5rem;
    font-size: 1rem;
  }

  h3 {
    margin: 0.75rem 0 0.4rem;
    font-size: 0.8rem;
    color: var(--fg-muted);
  }

  .controls {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    margin-bottom: 0.5rem;
    flex-wrap: wrap;
  }

  .action-label {
    font-size: 0.8rem;
    color: var(--fg-muted);
  }

  code {
    color: var(--accent);
    font-family: ui-monospace, 'Fira Code', monospace;
  }

  .controls input {
    width: 5rem;
    background: transparent;
    border: 1px solid var(--border);
    color: var(--fg);
    padding: 0.2rem 0.4rem;
    border-radius: 2px;
    font-family: ui-monospace, 'Fira Code', monospace;
    font-size: 0.8rem;
  }

  .controls label {
    display: flex;
    align-items: center;
    gap: 0.3rem;
    font-size: 0.75rem;
    color: var(--fg-muted);
  }

  .summary {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 0.2rem 1rem;
    font-size: 0.85rem;
    margin: 0;
    padding: 0.5rem;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 3px;
  }

  .summary dt {
    color: var(--fg-muted);
  }

  .summary dd {
    margin: 0;
    color: var(--fg);
    font-family: ui-monospace, 'Fira Code', monospace;
  }

  .histogram {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
  }

  .bar-row {
    display: grid;
    grid-template-columns: 2.5rem 1fr 3rem;
    gap: 0.5rem;
    align-items: center;
    font-size: 0.75rem;
    font-family: ui-monospace, 'Fira Code', monospace;
  }

  .bucket {
    color: var(--fg-muted);
    text-align: right;
  }

  .bar-track {
    background: var(--bg);
    border: 1px solid var(--border);
    height: 0.9rem;
    border-radius: 2px;
    overflow: hidden;
  }

  .bar-fill {
    height: 100%;
    background: var(--accent);
  }

  .count {
    color: var(--fg);
    text-align: right;
  }

  .error {
    background: #2a1010;
    border: 1px solid #5a2222;
    color: #ff8c8c;
    padding: 0.4rem;
    border-radius: 3px;
    font-size: 0.8rem;
  }

  .muted {
    color: var(--fg-muted);
    font-size: 0.85rem;
  }
</style>
