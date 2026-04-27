<script lang="ts">
  import { invoke } from './tauri';
  import type { Recommendation, RecoveryStepView } from './types';

  type Props = {
    /** The recommendation whose source step we may want to inspect.
     * Visible only when source.kind === 'strategy' AND lastFailed === true. */
    recommendation: Recommendation | null;
    /** Whether the previous attempted action was a failure. The panel is
     * always visible when true; advisory-only otherwise. */
    lastFailed: boolean;
  };

  let { recommendation, lastFailed }: Props = $props();
  let view = $state<RecoveryStepView | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);

  $effect(() => {
    view = null;
    error = null;
    if (!recommendation || recommendation.source.kind !== 'strategy') return;
    const { id: strategy_id, step: step_id } = recommendation.source;
    loading = true;
    invoke<RecoveryStepView>('recovery_hints', { strategyId: strategy_id, stepId: step_id })
      .then((v) => {
        view = v;
      })
      .catch((e) => {
        error = String(e);
      })
      .finally(() => {
        loading = false;
      });
  });

  function fmtCost(div: number | null): string {
    if (div === null || div === undefined) return 'cost n/a';
    if (div === 0) return 'free';
    return `+${div} div`;
  }
</script>

<section class="recovery" class:emphasized={lastFailed}>
  <h2>Recovery</h2>

  {#if !recommendation}
    <p class="muted">Select a strategy-sourced recommendation to see its recovery options.</p>
  {:else if recommendation.source.kind !== 'strategy'}
    <p class="muted">
      The current top recommendation is from a {recommendation.source.kind}, not a strategy.
      Strategy-sourced recommendations carry recovery hints; rules and heuristics do not.
    </p>
  {:else if loading}
    <p class="muted">loading…</p>
  {:else if error}
    <pre class="error">{error}</pre>
  {:else if view}
    <div class="meta">
      strategy <code>{recommendation.source.id}</code> :: step
      <code>{view.step_id}</code>
    </div>

    {#if view.next_action_summary}
      <div class="default-failure">
        <span class="label">Default failure path:</span>
        <span class="action">{view.next_action_summary}</span>
      </div>
    {/if}

    {#if view.hints.length === 0}
      <p class="muted">No recovery hints attached to this step.</p>
    {:else}
      <ol class="hints">
        {#each view.hints as h, i (i)}
          <li>
            <div class="hint-row">
              <span class="message">{h.message}</span>
              <span class="cost">{fmtCost(h.added_cost_div)}</span>
            </div>
            {#if h.goto_step_id}
              <div class="goto">
                → step <code>{h.goto_step_id}</code>
              </div>
            {/if}
          </li>
        {/each}
      </ol>
    {/if}
  {/if}
</section>

<style>
  .recovery {
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 0.75rem;
  }

  .recovery.emphasized {
    border-color: #ff8c40;
    box-shadow: 0 0 0 1px #ff8c4044;
  }

  h2 {
    margin: 0 0 0.5rem;
    font-size: 1rem;
  }

  .meta {
    font-size: 0.75rem;
    color: var(--fg-muted);
    margin-bottom: 0.5rem;
    font-family: ui-monospace, 'Fira Code', monospace;
  }

  code {
    color: var(--accent);
  }

  .default-failure {
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 3px;
    padding: 0.4rem 0.6rem;
    margin-bottom: 0.5rem;
    font-size: 0.85rem;
  }

  .default-failure .label {
    color: var(--fg-muted);
    margin-right: 0.4rem;
  }

  .default-failure .action {
    color: var(--fg);
    font-family: ui-monospace, 'Fira Code', monospace;
  }

  .hints {
    list-style: decimal inside;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
  }

  .hints li {
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 3px;
    padding: 0.5rem 0.6rem;
  }

  .hint-row {
    display: flex;
    justify-content: space-between;
    gap: 0.5rem;
    align-items: baseline;
  }

  .message {
    color: var(--fg);
    font-size: 0.85rem;
  }

  .cost {
    color: var(--fg-muted);
    font-size: 0.75rem;
    font-family: ui-monospace, 'Fira Code', monospace;
    white-space: nowrap;
  }

  .goto {
    color: var(--fg-muted);
    font-size: 0.75rem;
    margin-top: 0.25rem;
  }

  .muted {
    color: var(--fg-muted);
    font-size: 0.85rem;
  }

  .error {
    background: #2a1010;
    border: 1px solid #5a2222;
    color: #ff8c8c;
    padding: 0.4rem;
    border-radius: 3px;
    font-size: 0.8rem;
  }
</style>
