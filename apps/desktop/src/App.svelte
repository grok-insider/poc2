<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import AdvisorPanel from './lib/AdvisorPanel.svelte';
  import ItemBuilder from './lib/ItemBuilder.svelte';
  import { FRESH_BODY_ARMOUR, WORKED_EXAMPLE_GOAL } from './lib/fixtures';
  import type { Item } from './lib/types';

  let pingResponse = $state<string>('');
  let item = $state<Item>(structuredClone(FRESH_BODY_ARMOUR));
  let goal = $state(structuredClone(WORKED_EXAMPLE_GOAL));

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
</script>

<main class="container">
  <header>
    <h1>Path of Crafting 2</h1>
    <p class="tagline">PoE2 crafting advisor — M6 advisor IPC</p>
  </header>

  <div class="layout">
    <div class="left">
      <ItemBuilder {item} onUpdate={(next) => (item = next)} />
      <button class="reset" onclick={resetItem}>Reset to fresh BodyArmour</button>
    </div>
    <div class="right">
      <AdvisorPanel {item} {goal} />
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

  @media (max-width: 720px) {
    .layout {
      grid-template-columns: 1fr;
    }
  }
</style>
