<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import type {
    LeagueInfo,
    ReloadBundleResponse,
    RefreshPricesResponse,
  } from './types';

  type Props = {
    /** Triggered after a successful `reload_bundle` so the parent can
     * refresh derived state (concept lists, etc.). */
    onBundleReloaded?: (resp: ReloadBundleResponse) => void;
    /** Bound from the parent — Settings persists league choice for the
     * next refresh-prices call. */
    league: string;
    /** Two-way bound — Settings drives auto-refresh interval. */
    autoRefreshMinutes: 0 | 5 | 30 | 60;
    onLeagueChange: (next: string) => void;
    onAutoRefreshChange: (next: 0 | 5 | 30 | 60) => void;
  };

  let {
    onBundleReloaded,
    league,
    autoRefreshMinutes,
    onLeagueChange,
    onAutoRefreshChange,
  }: Props = $props();

  let bundlePathInput = $state('');
  let bundleReloading = $state(false);
  let bundleReloadError = $state<string | null>(null);
  let bundleReloadResult = $state<ReloadBundleResponse | null>(null);

  let leagues = $state<LeagueInfo[]>([]);
  let leaguesLoading = $state(false);
  let leaguesError = $state<string | null>(null);

  let pricesRefreshing = $state(false);
  let pricesError = $state<string | null>(null);
  let lastRefreshAt = $state<string | null>(null);

  // Load the leagues dropdown on mount.
  $effect.pre(() => {
    if (leagues.length > 0 || leaguesLoading) return;
    leaguesLoading = true;
    invoke<LeagueInfo[]>('list_leagues')
      .then((ls) => {
        leagues = ls;
      })
      .catch((e) => {
        leaguesError = String(e);
      })
      .finally(() => {
        leaguesLoading = false;
      });
  });

  // Auto-refresh prices on the configured cadence.
  $effect(() => {
    if (autoRefreshMinutes === 0) return;
    const interval = autoRefreshMinutes * 60 * 1000;
    const handle = setInterval(() => {
      void doRefreshPrices();
    }, interval);
    return () => clearInterval(handle);
  });

  async function reloadBundle() {
    bundleReloading = true;
    bundleReloadError = null;
    try {
      const args = bundlePathInput.trim()
        ? { path: bundlePathInput.trim() }
        : { path: null };
      const resp = await invoke<ReloadBundleResponse>('reload_bundle', { args });
      bundleReloadResult = resp;
      onBundleReloaded?.(resp);
    } catch (e) {
      bundleReloadError = String(e);
    } finally {
      bundleReloading = false;
    }
  }

  async function doRefreshPrices() {
    pricesRefreshing = true;
    pricesError = null;
    try {
      const resp = await invoke<RefreshPricesResponse>('refresh_prices', {
        args: { league },
      });
      if (resp.refreshed && resp.meta) {
        lastRefreshAt = resp.meta.fetched_at;
      } else if (resp.error) {
        pricesError = resp.error;
      }
    } catch (e) {
      pricesError = String(e);
    } finally {
      pricesRefreshing = false;
    }
  }
</script>

<section class="settings">
  <h2>Settings</h2>

  <!-- ============== Bundle reload ============== -->
  <div class="block">
    <h3>Data bundle</h3>
    <div class="bundle-row">
      <input
        type="text"
        placeholder="Optional path (leave empty for auto-discover)"
        bind:value={bundlePathInput}
      />
      <button onclick={reloadBundle} disabled={bundleReloading}>
        {bundleReloading ? 'reloading…' : 'Reload bundle'}
      </button>
    </div>
    {#if bundleReloadError}
      <pre class="error">{bundleReloadError}</pre>
    {/if}
    {#if bundleReloadResult}
      <p class="muted">
        loaded:
        {bundleReloadResult.bundle_path ?? '(no bundle found)'}
        {#if bundleReloadResult.patch}
          · patch {bundleReloadResult.patch}
        {/if}
        · {bundleReloadResult.mod_count} mods
        · {bundleReloadResult.strategy_count} strategies
      </p>
    {/if}
  </div>

  <!-- ============== League selection ============== -->
  <div class="block">
    <h3>League (poe2scout)</h3>
    {#if leaguesLoading}
      <p class="muted">loading league list…</p>
    {:else if leaguesError}
      <pre class="error">{leaguesError}</pre>
    {:else if leagues.length === 0}
      <p class="muted">No leagues returned.</p>
    {:else}
      <select
        value={league}
        onchange={(e) =>
          onLeagueChange((e.currentTarget as HTMLSelectElement).value)}
      >
        {#each leagues as l}
          <option value={l.value}>
            {l.value} (1div ≈ {l.divine_price_in_exalts.toFixed(0)}ex,
            {l.chaos_per_divine.toFixed(0)}c)
          </option>
        {/each}
      </select>
    {/if}
  </div>

  <!-- ============== Price refresh ============== -->
  <div class="block">
    <h3>Live prices</h3>
    <div class="prices-row">
      <button onclick={doRefreshPrices} disabled={pricesRefreshing}>
        {pricesRefreshing ? 'refreshing…' : 'Refresh now'}
      </button>
      <label class="auto-refresh">
        Auto-refresh:
        <select
          value={String(autoRefreshMinutes)}
          onchange={(e) =>
            onAutoRefreshChange(
              Number(
                (e.currentTarget as HTMLSelectElement).value,
              ) as 0 | 5 | 30 | 60,
            )}
        >
          <option value="0">off</option>
          <option value="5">every 5 min</option>
          <option value="30">every 30 min</option>
          <option value="60">hourly</option>
        </select>
      </label>
    </div>
    {#if pricesError}
      <pre class="error">{pricesError}</pre>
    {/if}
    {#if lastRefreshAt}
      <p class="muted">last refresh: {lastRefreshAt}</p>
    {/if}
  </div>

  <!-- ============== Plugin manager scaffold (Phase F.6) ============== -->
  <div class="block muted-block">
    <h3>Plugins (Phase F.6)</h3>
    <p class="muted">
      Plugin discovery and capability management ships in v1.0 Phase F.
      Plugin TOML lives in <code>~/.config/poc2/plugins/</code>.
    </p>
  </div>
</section>

<style>
  .settings {
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
    margin: 0 0 0.4rem;
    font-size: 0.85rem;
    color: var(--fg-muted);
  }

  .block {
    margin-bottom: 0.75rem;
  }

  .muted-block {
    opacity: 0.6;
  }

  .bundle-row {
    display: flex;
    gap: 0.4rem;
  }

  .bundle-row input {
    flex: 1;
    background: transparent;
    border: 1px solid var(--border);
    color: var(--fg);
    padding: 0.35rem 0.5rem;
    border-radius: 2px;
    font-family: ui-monospace, 'Fira Code', monospace;
    font-size: 0.8rem;
  }

  .prices-row {
    display: flex;
    gap: 1rem;
    align-items: center;
  }

  .auto-refresh {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    font-size: 0.8rem;
    color: var(--fg-muted);
  }

  select {
    background: var(--bg);
    color: var(--fg);
    border: 1px solid var(--border);
    border-radius: 2px;
    padding: 0.25rem 0.4rem;
    font-size: 0.8rem;
  }

  .error {
    background: #2a1010;
    border: 1px solid #5a2222;
    color: #ff8c8c;
    padding: 0.4rem;
    border-radius: 3px;
    font-size: 0.8rem;
    margin: 0.4rem 0 0;
  }

  .muted {
    color: var(--fg-muted);
    font-size: 0.8rem;
  }

  code {
    color: var(--accent);
    font-family: ui-monospace, 'Fira Code', monospace;
  }
</style>
