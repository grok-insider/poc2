<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';

  let pingResponse = $state<string>('');
  let loading = $state<boolean>(false);

  async function ping() {
    loading = true;
    try {
      pingResponse = await invoke<string>('ping');
    } catch (err) {
      pingResponse = `error: ${String(err)}`;
    } finally {
      loading = false;
    }
  }
</script>

<main class="container">
  <header>
    <h1>Path of Crafting 2</h1>
    <p class="tagline">PoE2 crafting advisor — M1 skeleton</p>
  </header>

  <section class="status">
    <h2>Health check</h2>
    <button onclick={ping} disabled={loading}>
      {loading ? 'pinging...' : 'Ping Tauri backend'}
    </button>
    {#if pingResponse}
      <pre class="response">{pingResponse}</pre>
    {/if}
  </section>

  <section class="roadmap">
    <h2>Build status</h2>
    <ul>
      <li>M1 — Foundation (in progress)</li>
      <li class="muted">M2 — Engine core + data pipeline</li>
      <li class="muted">M3 — Strategy library + rule engine</li>
      <li class="muted">M4 — Advisor / beam-search planner</li>
      <li class="muted">M5 — Probability + market awareness</li>
      <li class="muted">M6 — UI v1</li>
      <li class="muted">M7 — Live integration / overlay</li>
      <li class="muted">M8 — Polish + release</li>
    </ul>
  </section>
</main>
