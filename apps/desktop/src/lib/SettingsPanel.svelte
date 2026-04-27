<script lang="ts">
  import { invoke, listen } from './tauri';
  import type { UnlistenFn } from '@tauri-apps/api/event';
  import {
    CLIENT_LOG_EVENT,
    type ClientLogEvent,
    type ClientLogStatus,
    type LeagueInfo,
    type MetaResponse,
    type PluginInfo,
    type ReloadBundleResponse,
    type RefreshPricesResponse,
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
  let leaguesLoaded = $state(false);
  let leaguesError = $state<string | null>(null);

  let pricesRefreshing = $state(false);
  let pricesError = $state<string | null>(null);
  let lastRefreshAt = $state<string | null>(null);

  // ---- Phase F — plugin manager ----
  let plugins = $state<PluginInfo[]>([]);
  let pluginsLoading = $state(false);
  let pluginsLoaded = $state(false);
  let pluginsError = $state<string | null>(null);

  async function refreshPlugins() {
    pluginsLoading = true;
    pluginsError = null;
    try {
      plugins = await invoke<PluginInfo[]>('list_plugins');
      pluginsLoaded = true;
    } catch (e) {
      pluginsError = String(e);
    } finally {
      pluginsLoading = false;
    }
  }

  async function reloadPluginsFromDisk() {
    pluginsLoading = true;
    pluginsError = null;
    try {
      const count = await invoke<number>('reload_plugins');
      pluginsError = null;
      plugins = await invoke<PluginInfo[]>('list_plugins');
      if (count === 0 && plugins.length === 0) {
        pluginsError = 'No plugins found in ~/.config/poc2/plugins/';
      }
    } catch (e) {
      pluginsError = String(e);
    } finally {
      pluginsLoading = false;
    }
  }

  // Auto-refresh once on mount.
  $effect.pre(() => {
    if (!pluginsLoaded && !pluginsLoading) {
      void refreshPlugins();
    }
  });

  // ---- Phase E — meta builds + off-meta finder ----
  let metaResult = $state<MetaResponse | null>(null);
  let metaLoading = $state(false);
  let metaError = $state<string | null>(null);

  async function fetchMeta() {
    metaLoading = true;
    metaError = null;
    try {
      metaResult = await invoke<MetaResponse>('fetch_meta_builds', {
        args: { league },
      });
    } catch (e) {
      metaError = String(e);
    } finally {
      metaLoading = false;
    }
  }

  // ---- Phase D.1 Client.txt watcher ----
  let clientLogPath = $state('');
  let clientLogStatus = $state<ClientLogStatus | null>(null);
  let clientLogError = $state<string | null>(null);
  let clientLogEvents = $state<ClientLogEvent[]>([]);
  let unlistenClientLog: UnlistenFn | null = null;

  $effect(() => {
    let cancelled = false;
    listen<ClientLogEvent>(CLIENT_LOG_EVENT, (ev) => {
      if (cancelled) return;
      // Keep last 5 entries.
      clientLogEvents = [ev.payload, ...clientLogEvents].slice(0, 5);
    }).then((u) => {
      if (cancelled) {
        u();
        return;
      }
      unlistenClientLog = u;
    });
    return () => {
      cancelled = true;
      if (unlistenClientLog) {
        unlistenClientLog();
        unlistenClientLog = null;
      }
    };
  });

  $effect.pre(() => {
    invoke<ClientLogStatus>('client_log_status')
      .then((status) => {
        clientLogStatus = status;
      })
      .catch(() => {
        /* watcher status is best-effort */
      });
  });

  async function startClientLog() {
    if (!clientLogPath.trim()) {
      clientLogError = 'Set the Client.txt path first.';
      return;
    }
    clientLogError = null;
    try {
      clientLogStatus = await invoke<ClientLogStatus>('start_client_log', {
        args: { path: clientLogPath.trim() },
      });
    } catch (e) {
      clientLogError = String(e);
    }
  }

  async function stopClientLog() {
    try {
      clientLogStatus = await invoke<ClientLogStatus>('stop_client_log');
    } catch (e) {
      clientLogError = String(e);
    }
  }

  function describeClientLogEvent(ev: ClientLogEvent): string {
    switch (ev.kind) {
      case 'area_entered':
        return `entered: ${ev.area}`;
      case 'player_joined':
        return `joined: ${ev.player}`;
      case 'death':
        return ev.killer
          ? `${ev.victim} died to ${ev.killer}`
          : `${ev.victim} died`;
      case 'whisper':
        return `@${ev.from}: ${ev.message}`;
      case 'other':
        return ev.line.slice(0, 60);
    }
  }

  // Load the leagues dropdown on mount.
  $effect.pre(() => {
    if (leaguesLoaded || leaguesLoading) return;
    leaguesLoading = true;
    invoke<LeagueInfo[]>('list_leagues')
      .then((ls) => {
        leagues = ls;
        leaguesLoaded = true;
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

  <!-- ============== What to craft right now (Phase E) ============== -->
  <div class="block">
    <h3>What to craft right now</h3>
    <button onclick={fetchMeta} disabled={metaLoading} class="secondary">
      {metaLoading ? 'fetching…' : 'Fetch meta builds'}
    </button>
    {#if metaError}
      <pre class="error">{metaError}</pre>
    {/if}
    {#if metaResult}
      {#if metaResult.n_builds === 0}
        <p class="muted">
          poe.ninja PoE2 builds endpoint returned no data for league
          <code>{metaResult.league}</code>. The endpoint may not be
          live yet for this patch (Phase E.1 ships with a permissive
          deserializer).
        </p>
      {:else}
        <p class="muted">
          {metaResult.n_builds} builds sampled from
          <code>{metaResult.league}</code> @ {metaResult.fetched_at}
        </p>
        <ul class="niche-list">
          {#each metaResult.niches as n, i (i)}
            <li>
              <span class="niche-concept"><code>{n.concept}</code></span>
              <span class="niche-meta">
                demand {n.demand} · {n.competition} crafters · score {n.score.toFixed(2)}
              </span>
            </li>
          {/each}
        </ul>
      {/if}
    {/if}
  </div>

  <!-- ============== Client.txt watcher (Phase D.1) ============== -->
  <div class="block">
    <h3>Client.txt watcher</h3>
    <div class="bundle-row">
      <input
        type="text"
        placeholder="Absolute path to PoE2 Client.txt"
        bind:value={clientLogPath}
      />
      {#if clientLogStatus?.watching}
        <button onclick={stopClientLog} class="secondary">Stop</button>
      {:else}
        <button onclick={startClientLog}>Watch</button>
      {/if}
    </div>
    {#if clientLogError}
      <pre class="error">{clientLogError}</pre>
    {/if}
    {#if clientLogStatus?.watching}
      <p class="muted">watching: {clientLogStatus.path}</p>
    {/if}
    {#if clientLogEvents.length > 0}
      <ul class="log-feed">
        {#each clientLogEvents as ev, i (i)}
          <li>{describeClientLogEvent(ev)}</li>
        {/each}
      </ul>
    {/if}
  </div>

  <!-- ============== Plugin manager (Phase F.6) ============== -->
  <div class="block">
    <h3>Plugins</h3>
    <div class="bundle-row">
      <button onclick={reloadPluginsFromDisk} disabled={pluginsLoading}>
        {pluginsLoading ? 'reloading…' : 'Reload from disk'}
      </button>
    </div>
    <p class="muted">
      Plugins live in <code>~/.config/poc2/plugins/&lt;id&gt;/</code> with
      a <code>poc2-plugin.toml</code> manifest + a <code>*.wasm</code>
      file. See <code>examples/plugins/</code> for templates.
    </p>
    {#if pluginsError}
      <pre class="error">{pluginsError}</pre>
    {/if}
    {#if plugins.length === 0 && !pluginsLoading}
      <p class="muted">No plugins loaded.</p>
    {:else}
      <ul class="plugin-list">
        {#each plugins as p (p.id)}
          <li>
            <div class="plugin-header">
              <span class="plugin-name"><code>{p.id}</code> v{p.version}</span>
              <span class="plugin-status">
                {p.enabled ? 'enabled' : 'disabled'}
              </span>
            </div>
            <div class="plugin-meta">
              caps: [{p.capabilities.join(', ')}]
              {#if p.n_strategies > 0}· {p.n_strategies} strategies{/if}
              {#if p.n_rules > 0}· {p.n_rules} rules{/if}
            </div>
            {#if p.description}
              <div class="plugin-desc">{p.description}</div>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
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

  button.secondary {
    background: var(--bg);
    color: var(--fg-muted);
    border: 1px solid var(--border);
    font-weight: 400;
  }

  .log-feed,
  .niche-list {
    list-style: none;
    padding: 0;
    margin: 0.4rem 0 0;
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
  }

  .log-feed li,
  .niche-list li {
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 2px;
    padding: 0.25rem 0.4rem;
    font-size: 0.75rem;
    color: var(--fg);
    font-family: ui-monospace, 'Fira Code', monospace;
  }

  .niche-list li {
    display: flex;
    justify-content: space-between;
    gap: 0.5rem;
  }

  .niche-meta {
    color: var(--fg-muted);
  }

  .plugin-list {
    list-style: none;
    padding: 0;
    margin: 0.4rem 0 0;
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
  }

  .plugin-list li {
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 3px;
    padding: 0.4rem 0.6rem;
  }

  .plugin-header {
    display: flex;
    justify-content: space-between;
    font-size: 0.8rem;
    color: var(--fg);
    margin-bottom: 0.2rem;
  }

  .plugin-status {
    color: var(--fg-muted);
    font-size: 0.7rem;
  }

  .plugin-meta {
    color: var(--fg-muted);
    font-size: 0.7rem;
    font-family: ui-monospace, 'Fira Code', monospace;
  }

  .plugin-desc {
    color: var(--fg);
    font-size: 0.75rem;
    margin-top: 0.25rem;
  }
</style>
