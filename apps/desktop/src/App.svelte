<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import AdvisorPanel from './lib/AdvisorPanel.svelte';
  import ClipboardImport from './lib/ClipboardImport.svelte';
  import ItemBuilder from './lib/ItemBuilder.svelte';
  import RecoveryPanel from './lib/RecoveryPanel.svelte';
  import TargetPanel from './lib/TargetPanel.svelte';
  import { FRESH_BODY_ARMOUR, WORKED_EXAMPLE_GOAL } from './lib/fixtures';
  import type { Goal, Item, PersistedState, Recommendation } from './lib/types';

  let pingResponse = $state<string>('');
  let item = $state<Item>(structuredClone(FRESH_BODY_ARMOUR));
  let goal = $state<Goal>(structuredClone(WORKED_EXAMPLE_GOAL));
  let stateLoaded = $state(false);
  let recommendations = $state<Recommendation[]>([]);
  let lastFailed = $state(false);

  // Load persisted state on mount.
  $effect.pre(() => {
    if (stateLoaded) return;
    invoke<PersistedState>('load_state')
      .then((s) => {
        if (s.goal_json) {
          try {
            const parsed = JSON.parse(s.goal_json) as Goal;
            // Sanity-check the shape — fall back to default on malformed.
            if (parsed?.target && parsed.budget) {
              goal = parsed;
            }
          } catch {
            /* ignore — keep default */
          }
        }
      })
      .catch(() => {
        /* nothing persisted yet — keep defaults */
      })
      .finally(() => {
        stateLoaded = true;
      });
  });

  // Auto-save Goal on change (after initial load).
  $effect(() => {
    if (!stateLoaded) return;
    const goalSnapshot = JSON.stringify(goal);
    invoke('save_state', {
      state: {
        goal_json: goalSnapshot,
      } satisfies PersistedState,
    }).catch(() => {
      /* swallow — persistence is best-effort */
    });
  });

  async function ping() {
    try {
      pingResponse = await invoke<string>('ping');
    } catch (err) {
      pingResponse = `error: ${String(err)}`;
    }
  }

  function resetItem() {
    item = structuredClone(FRESH_BODY_ARMOUR);
  }

  function resetGoal() {
    goal = structuredClone(WORKED_EXAMPLE_GOAL);
  }
</script>

<main class="container">
  <header>
    <h1>Path of Crafting 2</h1>
    <p class="tagline">PoE2 crafting advisor — M6 advisor IPC</p>
  </header>

  <div class="layout">
    <div class="left">
      <ClipboardImport onItem={(next) => (item = next)} />
      <ItemBuilder {item} onUpdate={(next) => (item = next)} />
      <button class="reset" onclick={resetItem}>Reset to fresh BodyArmour</button>
      <TargetPanel {goal} onUpdate={(next) => (goal = next)} />
      <button class="reset" onclick={resetGoal}>Reset goal to worked example</button>
    </div>
    <div class="right">
      <AdvisorPanel
        {item}
        {goal}
        onRecommendations={(recs) => {
          recommendations = recs;
        }}
      />
      <label class="last-failed-toggle">
        <input type="checkbox" bind:checked={lastFailed} />
        Last action failed (highlights the recovery panel)
      </label>
      <RecoveryPanel
        recommendation={recommendations[0] ?? null}
        {lastFailed}
      />
    </div>
  </div>

  <section class="status">
    <h2>Health check</h2>
    <button onclick={ping}>Ping Tauri backend</button>
    {#if pingResponse}
      <pre class="response">{pingResponse}</pre>
    {/if}
  </section>
</main>

<style>
  .layout {
    display: grid;
    grid-template-columns: minmax(280px, 1fr) minmax(380px, 1.4fr);
    gap: 1rem;
    margin-bottom: 1rem;
  }

  .left,
  .right {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }

  .reset {
    background: var(--bg);
    color: var(--fg-muted);
    border: 1px solid var(--border);
    font-weight: 400;
  }

  .last-failed-toggle {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    font-size: 0.8rem;
    color: var(--fg-muted);
    padding: 0.25rem 0;
  }

  @media (max-width: 720px) {
    .layout {
      grid-template-columns: 1fr;
    }
  }
</style>
