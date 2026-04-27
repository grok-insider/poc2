<script lang="ts">
  import { invoke } from './lib/tauri';
  import AdvisorPanel from './lib/AdvisorPanel.svelte';
  import ClipboardImport from './lib/ClipboardImport.svelte';
  import ItemBuilder from './lib/ItemBuilder.svelte';
  import RecipeLibrary from './lib/RecipeLibrary.svelte';
  import RecoveryPanel from './lib/RecoveryPanel.svelte';
  import SettingsPanel from './lib/SettingsPanel.svelte';
  import SimulationRunner from './lib/SimulationRunner.svelte';
  import TargetBuilder from './lib/TargetBuilder.svelte';
  import { assetUrl, buildAssetIndex, displayId, initials, itemAssetId } from './lib/assets';
  import { baseIconUrl, loadBaseIconManifest } from './lib/baseIcons';
  import BasePicker from './lib/BasePicker.svelte';
  import EligiblePanel from './lib/EligiblePanel.svelte';
  import HistoryPanel from './lib/HistoryPanel.svelte';
  import type { BaseIconManifest, BaseSummary, HistoryEntry } from './lib/types';
  import { FRESH_BODY_ARMOUR, WORKED_EXAMPLE_GOAL } from './lib/fixtures';
  import type { AssetManifest, Goal, Item, PersistedState, Recommendation } from './lib/types';



  type DrawerView = 'item' | 'target' | 'tools' | null;

  let pingResponse = $state<string>('');
  let item = $state<Item>(structuredClone(FRESH_BODY_ARMOUR));
  let goal = $state<Goal>(structuredClone(WORKED_EXAMPLE_GOAL));
  let stateLoaded = $state(false);
  let recommendations = $state<Recommendation[]>([]);
  let lastFailed = $state(false);
  let league = $state<string>('Fate of the Vaal');
  let autoRefreshMinutes = $state<0 | 5 | 30 | 60>(0);
  let assetManifest = $state<AssetManifest | null>(null);
  let assetsLoading = $state(false);
  let assetError = $state<string | null>(null);
  let failedImageIds = $state<string[]>([]);
  let drawer = $state<DrawerView>(null);
  let baseIconManifest = $state<BaseIconManifest | null>(null);
  let basePickerOpen = $state(false);
  let basePickerInitialClass = $state<string | null>(null);
  let rightTab = $state<'preview' | 'eligible' | 'history'>('preview');
  let history = $state<HistoryEntry[]>([]);
  /** Phase D.3 — animate the affected mod row in Item Preview when a
   * `record_outcome` event lands. Cleared after the animation duration
   * (1500 ms). The id matches the mod row's `mod_id`; if the change
   * was a rarity flip without mod movement, this stays `null` and the
   * item header subtly flashes instead. */
  let recentlyChangedModId = $state<string | null>(null);
  /** Set to `true` for ~1500 ms after a rarity-only change. */
  let recentRarityFlash = $state(false);
  let highlightTimer: ReturnType<typeof setTimeout> | null = null;

  function flashChange(before: typeof item, after: typeof item, change: string) {
    if (highlightTimer) clearTimeout(highlightTimer);
    if (change === 'rarity') {
      recentlyChangedModId = null;
      recentRarityFlash = true;
    } else {
      // Diff prefixes + suffixes to find the moved mod. For 'added'
      // we look at the new mod_ids; for 'removed' we point at the
      // affix slot the now-empty spot used to hold; for 'replaced'
      // we surface the newly-added id.
      const beforeIds = new Set(
        [...before.prefixes, ...before.suffixes].map((m) => m.mod_id),
      );
      const afterIds = [...after.prefixes, ...after.suffixes];
      const added = afterIds.find((m) => !beforeIds.has(m.mod_id));
      recentlyChangedModId = added?.mod_id ?? null;
      recentRarityFlash = false;
    }
    highlightTimer = setTimeout(() => {
      recentlyChangedModId = null;
      recentRarityFlash = false;
      highlightTimer = null;
    }, 1500);
  }

  const assetIndex = $derived(buildAssetIndex(assetManifest?.entries ?? []));
  const itemImage = $derived(
    baseIconUrl(baseIconManifest, item.base_type_id ?? null) ??
      assetUrl(assetIndex, itemAssetId(item)),
  );
  const topRecommendation = $derived(recommendations[0] ?? null);
  const costCards = $derived(buildCostCards(recommendations));
  const successChance = $derived(topRecommendation ? topRecommendation.expected_prob * 100 : 0);
  const totalCost = $derived(topRecommendation?.expected_cost.expected ?? 0);
  const cheapestStep = $derived(
    [...recommendations].sort(
      (a, b) => a.expected_cost.expected - b.expected_cost.expected,
    )[0] ?? null,
  );
  const safestStep = $derived(
    [...recommendations]
      .filter((r) => r.action.kind !== 'apply_currency' || !r.action.currency.includes('Annul'))
      .sort((a, b) => b.expected_prob - a.expected_prob)[0] ?? null,
  );
  const goalMet = $derived(targetSatisfied(item, goal));
  const targetConcepts = $derived(extractTargetConcepts(goal));
  const prefixCapacity = $derived(rarityCapacity(item.rarity));
  const suffixCapacity = $derived(rarityCapacity(item.rarity));
  const remoteArtCount = $derived(
    (assetManifest?.entries ?? []).filter((entry) => entry.source_url?.startsWith('http')).length,
  );
  const itemTitle = $derived(itemDisplayTitle(item));

  $effect.pre(() => {
    if (stateLoaded) return;
    invoke<PersistedState>('load_state')
      .then((s) => {
        if (s.goal_json) {
          try {
            const parsed = JSON.parse(s.goal_json) as Goal;
            if (parsed?.target && parsed.budget) goal = parsed;
          } catch {
            /* keep default */
          }
        }
      })
      .catch(() => {
        /* nothing persisted yet */
      })
      .finally(() => {
        stateLoaded = true;
      });
    void loadAssets();
    void loadBaseIconManifest().then((m) => {
      baseIconManifest = m;
    });
  });

  $effect(() => {
    if (!stateLoaded) return;
    invoke('save_state', {
      state: { goal_json: JSON.stringify(goal) } satisfies PersistedState,
    }).catch(() => {
      /* persistence is best-effort */
    });
  });

  async function loadAssets() {
    assetsLoading = true;
    assetError = null;
    try {
      assetManifest = await invoke<AssetManifest>('asset_manifest');
    } catch (err) {
      assetError = String(err);
    } finally {
      assetsLoading = false;
    }
  }

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

  function modLabel(id: string): string {
    return displayId(id).replace(/^Local /, '');
  }

  function imageAvailable(id: string | null, url: string | null): boolean {
    return Boolean(id && url && !failedImageIds.includes(id));
  }

  function markImageFailed(id: string | null) {
    if (id && !failedImageIds.includes(id)) failedImageIds = [...failedImageIds, id];
  }

  function itemDisplayTitle(it: Item): string {
    const rarityWord = it.rarity === 'rare' ? 'Expert' : it.rarity;
    const r = rarityWord.charAt(0).toUpperCase() + rarityWord.slice(1);
    const name = it.base_display_name ?? displayId(it.base);
    return `${r} ${name}`;
  }

  function openBasePicker(initialClass: string | null = null) {
    basePickerInitialClass = initialClass;
    basePickerOpen = true;
  }

  function applyBase(b: BaseSummary) {
    item = {
      ...item,
      base: b.class_pascal,
      base_type_id: b.id,
      base_display_name: b.name,
      ilvl: Math.max(item.ilvl, b.drop_level),
      rarity: 'normal',
      prefixes: [],
      suffixes: [],
      implicits: [],
      enchantments: [],
      hidden_desecrated: null,
      sockets: [],
      hinekora_lock: null,
      corrupted: false,
      sanctified: false,
      mirrored: false,
    };
  }

  function extractTargetConcepts(g: Goal): string[] {
    const out: string[] = [];
    const collect = (specs: { concept?: string | null; concept_any?: string[] }[] | undefined) => {
      for (const s of specs ?? []) {
        if (s.concept) out.push(s.concept);
        if (s.concept_any) out.push(...s.concept_any);
      }
    };
    collect(g.target.prefixes);
    collect(g.target.suffixes);
    return [...new Set(out)];
  }

  function rarityCapacity(rarity: string): number {
    if (rarity === 'normal') return 0;
    if (rarity === 'magic') return 1;
    return 3;
  }

  function modSatisfiesTarget(modId: string, concepts: string[]): boolean {
    if (concepts.length === 0) return false;
    // Heuristic: match concept fragments inside mod_id (engine-style PascalCase ids).
    const lower = modId.toLowerCase();
    return concepts.some((c) => lower.includes(c.toLowerCase()));
  }

  function targetSatisfied(it: Item, g: Goal): boolean {
    const concepts = extractTargetConcepts(g);
    if (concepts.length === 0) return false;
    const have = [...it.prefixes, ...it.suffixes];
    return concepts.every((c) =>
      have.some((m) => modSatisfiesTarget(m.mod_id, [c])),
    );
  }

  function actionLabel(a: Recommendation['action']): string {
    if (a.kind === 'apply_currency') return displayId(a.currency);
    if (a.kind === 'activate_omen') return `Activate ${displayId(a.omen)}`;
    if (a.kind === 'apply_hinekoras_lock') return "Hinekora's Lock";
    if (a.kind === 'reveal') return 'Reveal at Well of Souls';
    if (a.kind === 'recombine') return 'Recombine';
    if (a.kind === 'guidance') return 'Guidance';
    if (a.kind === 'stop') return 'Stop';
    if (a.kind === 'abandon') return 'Abandon';
    return 'Action';
  }

  function buildCostCards(recs: Recommendation[]) {
    const seen = new Set<string>();
    return recs
      .filter((rec) => rec.action.kind === 'apply_currency')
      .map((rec) => ({
        id: rec.action.kind === 'apply_currency' ? rec.action.currency : '',
        cost: rec.expected_cost.expected,
        probability: rec.expected_prob,
      }))
      .filter((card) => {
        if (!card.id || seen.has(card.id)) return false;
        seen.add(card.id);
        return true;
      })
      .slice(0, 6);
  }

  function openDrawer(view: DrawerView) {
    drawer = drawer === view ? null : view;
  }

  function drawerTitle(view: DrawerView): string {
    switch (view) {
      case 'item':
        return 'Item Builder';
      case 'target':
        return 'Target Planner';
      case 'tools':
        return 'Tools and Recipes';
      default:
        return '';
    }
  }
</script>

<main class="app-shell">
  <aside class="sidebar">
    <div class="brand-mark">
      <img src="/poc2.svg" alt="" />
      <div>
        <strong>PoC 2</strong>
        <span>Crafting Advisor</span>
      </div>
    </div>
    <nav>
      <button class:active={drawer === null} onclick={() => (drawer = null)}>
        <span class="nav-icon">⚒</span> Crafting Guide
      </button>
      <button class:active={drawer === 'item'} onclick={() => openDrawer('item')}>
        <span class="nav-icon">◆</span> Item Builder
      </button>
      <button class:active={drawer === 'target'} onclick={() => openDrawer('target')}>
        <span class="nav-icon">◎</span> Target Planner
      </button>
      <button class:active={drawer === 'tools'} onclick={() => openDrawer('tools')}>
        <span class="nav-icon">⚙</span> Tools &amp; Recipes
      </button>
      <button onclick={() => openBasePicker(null)}>
        <span class="nav-icon">▦</span> Item Database
      </button>
    </nav>
    <div class="sidebar-footer">
      <div class="sidebar-card">
        <span class="card-eyebrow">Remote Art</span>
        <strong>{remoteArtCount}/{assetManifest?.entries.length ?? 0}</strong>
        <small>Using poe2db / poecdn URLs.</small>
        <button class="ghost compact" onclick={loadAssets} disabled={assetsLoading}>
          {assetsLoading ? 'Loading…' : 'Reload art list'}
        </button>
      </div>
      <div class="brand-footer">PoC 2 · v0.4</div>
    </div>
  </aside>

  <section class="workspace">
    <header class="topbar">
      <div class="topbar-left">
        <div class="title-block">
          <span class="eyebrow">Crafting Project</span>
          <h1>{itemTitle}</h1>
        </div>
        <div class="title-pills">
          <span class="pill">Level {item.ilvl}+</span>
          <span class="pill">{displayId(item.base)} Base</span>
          <span class="pill emphasis">Energy Shield Goal</span>
        </div>
      </div>
      <div class="top-actions">
        <button class="ghost" onclick={ping}>⚙ Settings</button>
        <button class="ghost" onclick={loadAssets} disabled={assetsLoading}>? Help</button>
        <span class="patch-pill">PoE 2 v0.4</span>
      </div>
    </header>

    {#if pingResponse || assetError}
      <div class="notice">
        {#if pingResponse}<span>{pingResponse}</span>{/if}
        {#if assetError}<span class="danger">{assetError}</span>{/if}
      </div>
    {/if}

    <div class="content">
      <section class="col panel base-panel">
        <div class="panel-title">Base Item</div>
        <div class="base-scroll">
          <div class="item-art-frame">
            {#if imageAvailable(itemAssetId(item), itemImage)}
              <img
                src={itemImage ?? ''}
                alt={item.base}
                onerror={() => markImageFailed(itemAssetId(item))}
              />
            {:else}
              <div class="fallback-art">{initials(item.base)}</div>
            {/if}
            <span class="ilvl-badge">ilvl {item.ilvl}</span>
          </div>
          <h2 class="base-name">{displayId(item.base)}</h2>
          <dl class="stat-list">
            <div><dt>Item Level</dt><dd>{item.ilvl}</dd></div>
            <div><dt>Rarity</dt><dd class="cap">{item.rarity}</dd></div>
            <div><dt>Quality</dt><dd>{item.quality}%</dd></div>
          </dl>
          <p class="implicit-text">
            Implicit:
            <span>
              {item.implicits[0]?.mod_id ? modLabel(item.implicits[0].mod_id) : 'none'}
            </span>
          </p>
          <div class="flag-row">
            <span class:lit={item.corrupted}>Corrupted</span>
            <span class:lit={item.sanctified}>Sanctified</span>
            <span class:lit={item.hinekora_lock !== null}>Lock</span>
          </div>
          <button class="ghost wide" onclick={() => openBasePicker(item.base)}>Change Base</button>
        </div>
      </section>

      <div class="col advisor-col">
        <AdvisorPanel
          {item}
          {goal}
          {assetIndex}
          onRecommendations={(recs) => {
            recommendations = recs;
          }}
          onItemUpdate={(next, change, explanation) => {
            const before = item;
            item = next;
            flashChange(before, next, change);
            history = [
              {
                id: `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
                timestamp: new Date().toLocaleTimeString(),
                change,
                explanation,
                before,
              },
              ...history,
            ].slice(0, 50);
            rightTab = 'history';
          }}
        />
      </div>

      <aside class="col right-stack">
        <section class="panel summary-card">
          <div class="panel-title">Crafting Summary</div>
          <div class="summary-tile">
            <span>Total Steps</span>
            <strong>{recommendations.length}</strong>
          </div>
          <div class="summary-tile">
            <span>Estimated Cost</span>
            <strong>~{totalCost.toFixed(1)} div</strong>
          </div>
          <div class="summary-tile">
            <span>Risk Level</span>
            <strong class="risk">
              {successChance >= 60 ? 'Low' : successChance >= 30 ? 'Medium' : 'High'}
            </strong>
          </div>
          {#if goalMet}
            <div class="goal-met">
              <strong>Goal met</strong>
              <span>Divine to perfect rolls or stop.</span>
            </div>
          {/if}
          {#if cheapestStep || safestStep}
            <div class="suggestions">
              {#if cheapestStep}
                <button
                  type="button"
                  class="sugg cheap"
                  onclick={() => (rightTab = 'preview')}
                  title="Cheapest suggestion"
                >
                  <span class="sugg-label">Cheapest</span>
                  <strong>{actionLabel(cheapestStep.action)}</strong>
                  <small>~{cheapestStep.expected_cost.expected.toFixed(2)} div</small>
                </button>
              {/if}
              {#if safestStep && safestStep !== cheapestStep}
                <button
                  type="button"
                  class="sugg safe"
                  onclick={() => (rightTab = 'preview')}
                  title="Safest suggestion"
                >
                  <span class="sugg-label">Safest</span>
                  <strong>{actionLabel(safestStep.action)}</strong>
                  <small>{(safestStep.expected_prob * 100).toFixed(1)}%</small>
                </button>
              {/if}
            </div>
          {/if}
          <label class="failure-toggle">
            <input type="checkbox" bind:checked={lastFailed} /> Last action failed
          </label>
          <RecoveryPanel recommendation={topRecommendation} {lastFailed} />
        </section>

        <section class="panel tabs-panel">
          <div class="tabstrip" role="tablist">
            <button
              type="button"
              class:active={rightTab === 'preview'}
              onclick={() => (rightTab = 'preview')}
            >Preview</button>
            <button
              type="button"
              class:active={rightTab === 'eligible'}
              onclick={() => (rightTab = 'eligible')}
            >Eligible</button>
            <button
              type="button"
              class:active={rightTab === 'history'}
              onclick={() => (rightTab = 'history')}
            >History {history.length > 0 ? `· ${history.length}` : ''}</button>
          </div>

          <div class="tab-body">
            {#if rightTab === 'preview'}
              <div class="rare-title">{itemTitle}</div>
              <div class="preview-body">
                {#if imageAvailable(itemAssetId(item), itemImage)}
                  <img
                    src={itemImage ?? ''}
                    alt=""
                    onerror={() => markImageFailed(itemAssetId(item))}
                  />
                {:else}
                  <div class="fallback-art small">{initials(item.base)}</div>
                {/if}
                <span class="muted">{item.base_display_name ?? displayId(item.base)}</span>
                <strong>Item Level: {item.ilvl}</strong>
              </div>
              <div class="mod-lines" class:rarity-flash={recentRarityFlash}>
                {#each item.implicits as mod, i (i)}
                  <p class="implicit">{modLabel(mod.mod_id)}</p>
                {/each}
                {#each item.prefixes as mod, i (i)}
                  <p
                    class:satisfies={modSatisfiesTarget(mod.mod_id, targetConcepts)}
                    class:recently-changed={recentlyChangedModId === mod.mod_id}
                  >
                    {modLabel(mod.mod_id)}{mod.is_fractured ? ' (fractured)' : ''}
                  </p>
                {/each}
                {#each item.suffixes as mod, i (i)}
                  <p
                    class:satisfies={modSatisfiesTarget(mod.mod_id, targetConcepts)}
                    class:recently-changed={recentlyChangedModId === mod.mod_id}
                  >
                    {modLabel(mod.mod_id)}{mod.is_fractured ? ' (fractured)' : ''}
                  </p>
                {/each}
                {#if item.prefixes.length + item.suffixes.length + item.implicits.length === 0}
                  <p class="muted">Fresh base. Add a target and follow the guide.</p>
                {/if}
              </div>
              <div class="breakdown-grid compact">
                <div>
                  <header>
                    <span>Prefixes</span>
                    <span class="count">{item.prefixes.length}/{prefixCapacity}</span>
                  </header>
                  {#each item.prefixes as mod, i (i)}
                    <span
                      class="mod-pill"
                      class:satisfies={modSatisfiesTarget(mod.mod_id, targetConcepts)}
                      class:recently-changed={recentlyChangedModId === mod.mod_id}
                    >
                      {modLabel(mod.mod_id)}
                    </span>
                  {/each}
                  {#if item.prefixes.length === 0}<span class="muted">empty</span>{/if}
                </div>
                <div>
                  <header>
                    <span>Suffixes</span>
                    <span class="count">{item.suffixes.length}/{suffixCapacity}</span>
                  </header>
                  {#each item.suffixes as mod, i (i)}
                    <span
                      class="mod-pill"
                      class:satisfies={modSatisfiesTarget(mod.mod_id, targetConcepts)}
                      class:recently-changed={recentlyChangedModId === mod.mod_id}
                    >
                      {modLabel(mod.mod_id)}
                    </span>
                  {/each}
                  {#if item.suffixes.length === 0}<span class="muted">empty</span>{/if}
                </div>
              </div>
              <div class="prediction">
                <span>Predicted Success</span>
                <strong>{successChance.toFixed(1)}%</strong>
              </div>
            {:else if rightTab === 'eligible'}
              <EligiblePanel {item} {targetConcepts} />
            {:else if rightTab === 'history'}
              <HistoryPanel
                entries={history}
                onUndo={(idx) => {
                  const entry = history[idx];
                  if (!entry) return;
                  item = entry.before;
                  history = history.filter((_, i) => i !== idx);
                }}
              />
            {/if}
          </div>
        </section>
      </aside>
    </div>

    <section class="cost-dock">
      <div class="dock-header">
        <span class="dock-title">Currency Cost Estimate</span>
        <span class="dock-meta">
          {costCards.length} step{costCards.length === 1 ? '' : 's'} ·
          ~{totalCost.toFixed(1)} div total
        </span>
      </div>
      <div class="cost-cards">
        {#each costCards as card (card.id)}
          <article class="cost-card">
            {#if imageAvailable(card.id, assetUrl(assetIndex, card.id))}
              <img
                src={assetUrl(assetIndex, card.id) ?? ''}
                alt=""
                onerror={() => markImageFailed(card.id)}
              />
            {:else}
              <div class="mini-fallback">{initials(displayId(card.id))}</div>
            {/if}
            <span>{displayId(card.id)}</span>
            <strong>× 1</strong>
            <small>~{card.cost.toFixed(2)} div · {(card.probability * 100).toFixed(1)}%</small>
          </article>
        {/each}
        {#if costCards.length === 0}
          <article class="cost-card empty">
            <div class="mini-fallback">?</div>
            <span class="muted">No currency steps yet</span>
            <small>Start the advisor to see cost cards.</small>
          </article>
        {/if}
        <article class="cost-total">
          <span>Total</span>
          <strong>~{totalCost.toFixed(1)}</strong>
          <small>Divine Orbs</small>
        </article>
      </div>
    </section>

    {#if drawer !== null}
      <button
        class="drawer-scrim"
        type="button"
        aria-label="Close panel"
        onclick={() => (drawer = null)}
      ></button>
      <aside class="drawer">
        <header class="drawer-head">
          <h3>{drawerTitle(drawer)}</h3>
          <button class="ghost compact" onclick={() => (drawer = null)}>Close</button>
        </header>
        <div class="drawer-body">
          {#if drawer === 'item'}
            <button
              class="ghost wide"
              onclick={() => {
                drawer = null;
                openBasePicker(null);
              }}
            >
              Pick a base from the database
            </button>
            <ClipboardImport onItem={(next) => (item = next)} />
            <ItemBuilder {item} onUpdate={(next) => (item = next)} />
            <button class="ghost wide" onclick={resetItem}>Reset to fresh base</button>
          {:else if drawer === 'target'}
            <TargetBuilder
              {goal}
              {item}
              onUpdate={(next) => (goal = next)}
              onReset={resetGoal}
            />
          {:else if drawer === 'tools'}
            <SettingsPanel
              {league}
              {autoRefreshMinutes}
              onLeagueChange={(next) => (league = next)}
              onAutoRefreshChange={(next) => (autoRefreshMinutes = next)}
            />
            <SimulationRunner {item} action={topRecommendation?.action ?? null} />
            <RecipeLibrary
              {item}
              {goal}
              onLoadRecipe={(loadedItem, loadedGoal) => {
                item = loadedItem;
                goal = loadedGoal;
              }}
            />
          {/if}
        </div>
      </aside>
    {/if}
  </section>

  <BasePicker
    open={basePickerOpen}
    initialClass={basePickerInitialClass}
    onClose={() => (basePickerOpen = false)}
    onPick={applyBase}
  />
</main>

<style>
  :global(html, body, #app) {
    height: 100%;
    overflow: hidden;
  }

  .app-shell {
    height: 100vh;
    display: grid;
    grid-template-columns: 220px minmax(0, 1fr);
    overflow: hidden;
    background:
      radial-gradient(circle at 50% -25%, rgba(160, 105, 32, 0.18), transparent 38rem),
      linear-gradient(135deg, #06080a 0%, #0d1115 52%, #06080a 100%);
  }

  .sidebar {
    border-right: 1px solid var(--border-strong);
    background: linear-gradient(180deg, rgba(7, 11, 14, 0.96), rgba(3, 5, 7, 0.96));
    padding: 0.7rem;
    display: flex;
    flex-direction: column;
    gap: 0.7rem;
    overflow: hidden;
    min-height: 0;
  }

  .brand-mark {
    display: flex;
    align-items: center;
    gap: 0.55rem;
    padding: 0.4rem 0.5rem;
    border: 1px solid var(--border-strong);
    border-radius: 4px;
    background: linear-gradient(90deg, rgba(197, 143, 61, 0.18), rgba(20, 14, 5, 0.7));
    color: var(--gold-bright);
    text-transform: uppercase;
    letter-spacing: 0.08em;
    flex-shrink: 0;
  }

  .brand-mark img {
    width: 32px;
    height: 32px;
  }

  .brand-mark strong {
    display: block;
    font-size: 0.9rem;
  }

  .brand-mark span,
  .sidebar-card small,
  .eyebrow,
  .card-eyebrow,
  .muted {
    color: var(--fg-muted);
  }

  nav {
    display: grid;
    gap: 0.25rem;
    overflow-y: auto;
    min-height: 0;
  }

  nav button {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    border: 1px solid transparent;
    border-radius: 4px;
    padding: 0.5rem 0.65rem;
    color: var(--fg-soft);
    background: rgba(255, 255, 255, 0.02);
    text-align: left;
    font-weight: 500;
    cursor: pointer;
    transition: background 120ms ease, border-color 120ms ease, color 120ms ease;
    font-size: 0.85rem;
  }

  nav button:hover {
    background: rgba(197, 143, 61, 0.08);
    color: var(--gold);
  }

  nav button.active {
    color: var(--gold-bright);
    border-color: rgba(197, 143, 61, 0.65);
    background: linear-gradient(90deg, rgba(197, 143, 61, 0.28), rgba(197, 143, 61, 0.04));
    box-shadow: inset 0 0 0 1px rgba(255, 211, 122, 0.18);
  }

  .nav-icon {
    color: var(--gold);
    font-size: 0.95rem;
    width: 1.1rem;
    text-align: center;
  }

  .sidebar-footer {
    margin-top: auto;
    display: grid;
    gap: 0.5rem;
    flex-shrink: 0;
  }

  .sidebar-card {
    border: 1px solid var(--border-strong);
    border-radius: 4px;
    padding: 0.55rem;
    display: grid;
    gap: 0.25rem;
    background: rgba(0, 0, 0, 0.4);
    font-size: 0.78rem;
  }

  .sidebar-card strong {
    color: var(--gold-bright);
    font-size: 1.2rem;
    font-family: Georgia, 'Times New Roman', serif;
  }

  .brand-footer {
    color: var(--fg-muted);
    font-size: 0.65rem;
    text-align: center;
    text-transform: uppercase;
    letter-spacing: 0.12em;
  }

  .workspace {
    display: grid;
    grid-template-rows: auto auto 1fr auto;
    overflow: hidden;
    min-height: 0;
    position: relative;
  }

  .topbar {
    border-bottom: 1px solid var(--border-strong);
    background:
      linear-gradient(90deg, rgba(20, 13, 4, 0.85), rgba(8, 12, 16, 0.92)),
      var(--bg-elevated);
    box-shadow: inset 0 0 0 1px rgba(247, 187, 90, 0.06);
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.55rem 0.85rem;
    gap: 0.75rem;
    flex-wrap: wrap;
  }

  .topbar-left {
    display: flex;
    align-items: center;
    gap: 0.85rem;
    min-width: 0;
    flex-wrap: wrap;
  }

  .title-block {
    display: grid;
    gap: 0.05rem;
  }

  .title-pills {
    display: flex;
    gap: 0.35rem;
    flex-wrap: wrap;
  }

  .pill,
  .patch-pill,
  .flag-row span {
    border: 1px solid var(--border-gold);
    color: var(--gold);
    padding: 0.2rem 0.5rem;
    border-radius: 999px;
    font-size: 0.7rem;
    background: rgba(0, 0, 0, 0.4);
    text-transform: uppercase;
    letter-spacing: 0.08em;
  }

  .pill.emphasis {
    color: var(--gold-bright);
    background: linear-gradient(90deg, rgba(197, 143, 61, 0.35), rgba(70, 45, 12, 0.5));
  }

  .flag-row span.lit {
    color: #72ff58;
    border-color: rgba(114, 255, 88, 0.55);
  }

  h1,
  h2 {
    margin: 0;
    font-family: Georgia, 'Times New Roman', serif;
    color: var(--gold-bright);
    font-weight: 500;
    letter-spacing: 0.04em;
  }

  h1 {
    font-size: 1.25rem;
  }

  .top-actions {
    display: flex;
    gap: 0.45rem;
    align-items: center;
  }

  .notice {
    border-bottom: 1px solid var(--border-gold);
    background: rgba(16, 10, 4, 0.78);
    color: var(--gold);
    padding: 0.4rem 0.85rem;
    display: flex;
    gap: 1rem;
    font-size: 0.8rem;
  }

  .danger {
    color: #ff8c66;
  }

  .content {
    display: grid;
    grid-template-columns: 220px minmax(0, 1fr) 320px;
    gap: 0.6rem;
    padding: 0.6rem;
    min-height: 0;
    overflow: hidden;
  }

  .col {
    min-height: 0;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }

  .col.advisor-col {
    overflow: hidden;
  }

  .right-stack {
    overflow-y: auto;
    overflow-x: hidden;
    padding-right: 2px;
    gap: 0.6rem;
  }

  .panel,
  :global(section.panel) {
    border: 1px solid var(--border-strong);
    background:
      linear-gradient(180deg, rgba(15, 19, 22, 0.94), rgba(5, 8, 11, 0.94)),
      var(--bg-elevated);
    box-shadow: inset 0 0 0 1px rgba(247, 187, 90, 0.08);
    padding: 0.7rem;
    border-radius: 4px;
  }

  .base-panel {
    overflow: hidden;
  }

  .base-scroll {
    overflow-y: auto;
    overflow-x: hidden;
    display: flex;
    flex-direction: column;
    gap: 0.55rem;
    padding-right: 2px;
    min-height: 0;
  }

  .panel-title,
  .dock-title {
    color: var(--gold);
    text-align: center;
    text-transform: uppercase;
    letter-spacing: 0.1em;
    font-family: Georgia, 'Times New Roman', serif;
    margin-bottom: 0.5rem;
    font-size: 0.78rem;
    flex-shrink: 0;
  }

  .item-art-frame {
    position: relative;
    min-height: 150px;
    display: grid;
    place-items: center;
    background:
      radial-gradient(circle at 50% 40%, rgba(31, 54, 61, 0.65), rgba(0, 0, 0, 0.55) 70%),
      repeating-linear-gradient(135deg, rgba(255, 255, 255, 0.02) 0 6px, transparent 6px 12px);
    border: 1px solid rgba(197, 143, 61, 0.4);
    border-radius: 4px;
    overflow: hidden;
  }

  .item-art-frame img,
  .preview-body img {
    max-width: 70%;
    max-height: 130px;
    object-fit: contain;
    filter: drop-shadow(0 12px 16px rgba(0, 0, 0, 0.7));
  }

  .ilvl-badge {
    position: absolute;
    top: 0.35rem;
    right: 0.35rem;
    background: rgba(0, 0, 0, 0.6);
    border: 1px solid var(--border-gold);
    color: var(--gold-bright);
    font-size: 0.65rem;
    padding: 0.1rem 0.4rem;
    border-radius: 999px;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  .fallback-art,
  .mini-fallback {
    display: grid;
    place-items: center;
    border: 1px solid rgba(0, 202, 255, 0.4);
    color: #00d7ff;
    background: rgba(0, 35, 50, 0.45);
  }

  .fallback-art {
    width: 96px;
    height: 96px;
    border-radius: 50%;
    font-size: 1.6rem;
    letter-spacing: 0.08em;
  }

  .fallback-art.small {
    width: 64px;
    height: 64px;
    font-size: 0.95rem;
  }

  .base-name {
    text-align: center;
    font-size: 1rem;
    margin-top: 0.1rem;
  }

  .stat-list {
    display: grid;
    gap: 0.25rem;
    margin: 0;
  }

  .stat-list div {
    display: flex;
    justify-content: space-between;
    border-bottom: 1px solid rgba(255, 255, 255, 0.06);
    padding-bottom: 0.15rem;
  }

  dt {
    color: var(--fg-muted);
    text-transform: uppercase;
    letter-spacing: 0.08em;
    font-size: 0.7rem;
  }

  dd {
    margin: 0;
    color: var(--fg);
    font-size: 0.85rem;
  }

  .cap {
    text-transform: capitalize;
  }

  .implicit-text {
    margin: 0;
    color: var(--fg-muted);
    font-size: 0.78rem;
  }

  .implicit-text span {
    color: #a98dff;
  }

  .flag-row {
    display: flex;
    flex-wrap: wrap;
    gap: 0.3rem;
  }

  .flag-row span {
    text-transform: capitalize;
    font-size: 0.65rem;
  }

  .preview-card,
  .breakdown-card,
  .summary-card {
    flex-shrink: 0;
  }

  .preview-card {
    display: grid;
    gap: 0.4rem;
  }

  .rare-title {
    border: 1px solid var(--border-gold);
    background: linear-gradient(90deg, rgba(112, 70, 13, 0.55), rgba(11, 9, 6, 0.85));
    color: var(--gold-bright);
    text-align: center;
    padding: 0.45rem;
    font-family: Georgia, 'Times New Roman', serif;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    border-radius: 4px;
    font-size: 0.85rem;
  }

  .preview-body {
    display: grid;
    place-items: center;
    gap: 0.25rem;
    padding: 0.4rem 0.3rem;
    font-size: 0.85rem;
  }

  .preview-body strong {
    color: var(--gold);
    font-family: Georgia, 'Times New Roman', serif;
  }

  .mod-lines {
    border-top: 1px solid rgba(197, 143, 61, 0.3);
    border-bottom: 1px solid rgba(197, 143, 61, 0.3);
    padding: 0.5rem 0;
    text-align: center;
    font-size: 0.85rem;
  }

  .mod-lines p {
    margin: 0.2rem 0;
    color: #00c8ff;
  }

  .mod-lines .implicit {
    color: #a98dff;
  }

  .prediction {
    margin-top: 0.3rem;
    display: grid;
    place-items: center;
    color: #72ff58;
    border: 1px solid rgba(114, 255, 88, 0.45);
    border-radius: 4px;
    padding: 0.5rem;
    background: radial-gradient(circle, rgba(47, 123, 18, 0.28), rgba(0, 0, 0, 0.4));
  }

  .prediction span {
    text-transform: uppercase;
    letter-spacing: 0.1em;
    font-size: 0.7rem;
    color: rgba(178, 255, 162, 0.8);
  }

  .prediction strong {
    font-size: 1.9rem;
    font-family: Georgia, 'Times New Roman', serif;
  }

  .breakdown-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0.4rem;
  }

  .breakdown-grid div {
    border: 1px solid rgba(255, 255, 255, 0.08);
    padding: 0.4rem;
    display: grid;
    gap: 0.3rem;
    align-content: start;
    background: rgba(0, 0, 0, 0.25);
    border-radius: 4px;
  }

  .breakdown-grid header {
    display: flex;
    justify-content: space-between;
    color: var(--gold);
    text-transform: uppercase;
    letter-spacing: 0.08em;
    font-size: 0.7rem;
    border-bottom: 1px solid rgba(197, 143, 61, 0.25);
    padding-bottom: 0.2rem;
  }

  .breakdown-grid .count {
    color: var(--fg-muted);
  }

  .mod-pill {
    color: #00c8ff;
    font-size: 0.72rem;
    background: rgba(0, 80, 110, 0.25);
    border: 1px solid rgba(0, 200, 255, 0.25);
    padding: 0.15rem 0.35rem;
    border-radius: 999px;
    width: fit-content;
  }

  .mod-pill.satisfies,
  .mod-lines p.satisfies {
    color: #72ff58;
    background: rgba(40, 90, 25, 0.32);
    border-color: rgba(114, 255, 88, 0.45);
  }

  .summary-card {
    display: grid;
    gap: 0.45rem;
  }

  .summary-tile {
    display: flex;
    align-items: center;
    justify-content: space-between;
    border: 1px solid rgba(197, 143, 61, 0.32);
    border-radius: 4px;
    padding: 0.45rem 0.6rem;
    background: rgba(0, 0, 0, 0.35);
  }

  .summary-tile span {
    color: var(--fg-muted);
    text-transform: uppercase;
    letter-spacing: 0.08em;
    font-size: 0.7rem;
  }

  .summary-tile strong {
    color: var(--gold-bright);
    font-family: Georgia, 'Times New Roman', serif;
    font-size: 1rem;
  }

  .summary-tile strong.risk {
    color: #ffb96b;
  }

  .failure-toggle {
    display: flex;
    gap: 0.4rem;
    align-items: center;
    font-size: 0.78rem;
    color: var(--fg-muted);
  }

  .cost-dock {
    border-top: 1px solid var(--border-strong);
    background:
      linear-gradient(90deg, rgba(15, 11, 4, 0.85), rgba(5, 8, 10, 0.92)),
      rgba(5, 8, 10, 0.92);
    padding: 0.5rem 0.6rem;
    display: grid;
    gap: 0.4rem;
    min-height: 0;
  }

  .dock-header {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: 0.5rem;
  }

  .dock-title {
    color: var(--gold);
    text-transform: uppercase;
    letter-spacing: 0.1em;
    font-family: Georgia, 'Times New Roman', serif;
    font-size: 0.78rem;
  }

  .dock-meta {
    color: var(--fg-muted);
    font-size: 0.75rem;
  }

  .cost-cards {
    display: flex;
    gap: 0.5rem;
    overflow-x: auto;
    overflow-y: hidden;
    padding-bottom: 0.2rem;
  }

  .cost-card,
  .cost-total {
    min-width: 130px;
    border: 1px solid rgba(197, 143, 61, 0.4);
    background: linear-gradient(180deg, rgba(20, 24, 27, 0.92), rgba(6, 8, 9, 0.92));
    padding: 0.5rem;
    display: grid;
    place-items: center;
    gap: 0.18rem;
    color: var(--fg-soft);
    border-radius: 4px;
    text-align: center;
    flex-shrink: 0;
  }

  .cost-card.empty {
    color: var(--fg-muted);
    border-style: dashed;
  }

  .cost-card img,
  .mini-fallback {
    width: 38px;
    height: 38px;
    object-fit: contain;
  }

  .mini-fallback {
    border-radius: 999px;
    font-size: 0.85rem;
  }

  .cost-card span {
    font-size: 0.7rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
  }

  .cost-card strong,
  .cost-total strong {
    color: var(--gold-bright);
    font-size: 1.1rem;
    font-family: Georgia, 'Times New Roman', serif;
  }

  .cost-card small,
  .cost-total small {
    color: var(--fg-muted);
    font-size: 0.68rem;
  }

  .cost-total {
    background: linear-gradient(180deg, rgba(50, 30, 8, 0.85), rgba(15, 11, 4, 0.92));
    border-color: var(--gold);
    color: var(--gold-bright);
  }

  .ghost {
    background: rgba(0, 0, 0, 0.35);
    color: var(--gold);
    border: 1px solid var(--border-gold);
    border-radius: 4px;
    padding: 0.4rem 0.65rem;
    cursor: pointer;
    font-weight: 500;
    font-size: 0.8rem;
  }

  .ghost.compact {
    padding: 0.3rem 0.55rem;
    font-size: 0.78rem;
  }

  .ghost:hover:not(:disabled) {
    background: rgba(197, 143, 61, 0.12);
    color: var(--gold-bright);
  }

  .wide {
    width: 100%;
  }

  .drawer-scrim {
    position: absolute;
    inset: 0;
    background: rgba(0, 0, 0, 0.55);
    border: 0;
    cursor: pointer;
    z-index: 5;
  }

  .drawer {
    position: absolute;
    top: 0;
    right: 0;
    bottom: 0;
    width: min(560px, 70vw);
    border-left: 1px solid var(--border-strong);
    background: linear-gradient(180deg, rgba(8, 12, 16, 0.98), rgba(3, 5, 7, 0.98));
    z-index: 6;
    display: grid;
    grid-template-rows: auto 1fr;
    box-shadow: -10px 0 40px rgba(0, 0, 0, 0.55);
  }

  .drawer-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.7rem 0.85rem;
    border-bottom: 1px solid var(--border-strong);
    background: rgba(20, 13, 4, 0.85);
  }

  .drawer-head h3 {
    margin: 0;
    color: var(--gold-bright);
    font-family: Georgia, 'Times New Roman', serif;
    font-weight: 500;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    font-size: 0.95rem;
  }

  .drawer-body {
    overflow-y: auto;
    padding: 0.75rem;
    display: grid;
    gap: 0.7rem;
  }

  @media (max-width: 1180px) {
    .content {
      grid-template-columns: 200px minmax(0, 1fr) 280px;
    }
  }

  @media (max-width: 980px) {
    .content {
      grid-template-columns: 200px minmax(0, 1fr);
    }
    .right-stack {
      display: none;
    }
  }

  @media (max-width: 820px) {
    .app-shell {
      grid-template-columns: 1fr;
    }
    .sidebar {
      display: none;
    }
    .content {
      grid-template-columns: 1fr;
    }
  }

  .tabs-panel {
    display: flex;
    flex-direction: column;
    min-height: 0;
    flex: 1 1 auto;
    overflow: hidden;
    padding: 0;
  }

  .tabstrip {
    display: flex;
    gap: 0.25rem;
    padding: 0.4rem 0.5rem 0;
    border-bottom: 1px solid var(--border-strong);
    background: rgba(0, 0, 0, 0.3);
  }

  .tabstrip button {
    background: transparent;
    color: var(--fg-muted);
    border: 1px solid transparent;
    border-bottom: 0;
    border-top-left-radius: 4px;
    border-top-right-radius: 4px;
    padding: 0.35rem 0.7rem;
    cursor: pointer;
    font-size: 0.78rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    font-family: Georgia, 'Times New Roman', serif;
  }

  .tabstrip button:hover {
    color: var(--gold);
  }

  .tabstrip button.active {
    color: var(--gold-bright);
    border-color: var(--border-strong);
    background: linear-gradient(180deg, rgba(20, 13, 4, 0.85), rgba(8, 12, 16, 0.92));
  }

  .tab-body {
    overflow-y: auto;
    overflow-x: hidden;
    padding: 0.65rem 0.7rem;
    flex: 1 1 auto;
    min-height: 0;
  }

  .breakdown-grid.compact div {
    padding: 0.4rem;
  }

  .goal-met {
    border: 1px solid rgba(114, 255, 88, 0.55);
    background: radial-gradient(circle, rgba(47, 123, 18, 0.32), rgba(0, 0, 0, 0.4));
    color: #b2ffa2;
    border-radius: 4px;
    padding: 0.55rem 0.7rem;
    text-align: center;
    display: grid;
    gap: 0.2rem;
  }

  .goal-met strong {
    color: #72ff58;
    font-family: Georgia, 'Times New Roman', serif;
    font-size: 1.1rem;
  }

  .goal-met span {
    font-size: 0.78rem;
    color: rgba(178, 255, 162, 0.8);
  }

  .suggestions {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0.45rem;
  }

  .sugg {
    text-align: left;
    background: rgba(0, 0, 0, 0.4);
    border: 1px solid var(--border-strong);
    border-radius: 4px;
    padding: 0.45rem 0.55rem;
    color: var(--fg-soft);
    cursor: pointer;
    display: grid;
    gap: 0.15rem;
  }

  .sugg-label {
    color: var(--fg-muted);
    font-size: 0.7rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
  }

  .sugg strong {
    color: var(--gold-bright);
    font-family: Georgia, 'Times New Roman', serif;
    font-size: 0.95rem;
  }

  .sugg small {
    color: var(--fg-muted);
    font-size: 0.72rem;
    font-family: ui-monospace, 'Fira Code', monospace;
  }

  .sugg.cheap {
    border-color: rgba(114, 255, 88, 0.45);
  }

  .sugg.safe {
    border-color: rgba(0, 200, 255, 0.45);
  }

  /* Phase D.3 — Item Preview row highlight on record_outcome.
   *
   * `recently-changed` is added to the affected `<p>` and `<span>` rows
   * for ~1500 ms after a record_outcome event lands. The animation
   * fades a gold inset glow in, holds, and fades out, drawing the
   * user's attention to what just changed without obstructing reads.
   *
   * `rarity-flash` (on the surrounding mod-lines container) handles
   * the rarity-only change case where no specific mod row exists to
   * highlight — typically a Transmute on a Normal item promoting to
   * Magic before the new mod renders. */
  @keyframes poc2-row-flash {
    0% {
      background: rgba(255, 211, 122, 0);
      box-shadow: inset 0 0 0 1px rgba(255, 211, 122, 0);
    }
    18% {
      background: rgba(255, 211, 122, 0.28);
      box-shadow: inset 0 0 0 1px rgba(255, 211, 122, 0.85);
    }
    100% {
      background: rgba(255, 211, 122, 0);
      box-shadow: inset 0 0 0 1px rgba(255, 211, 122, 0);
    }
  }

  .recently-changed {
    animation: poc2-row-flash 1.5s ease-out;
    border-radius: 3px;
  }

  @keyframes poc2-rarity-flash {
    0%,
    100% {
      background: transparent;
    }
    20% {
      background: rgba(220, 165, 70, 0.18);
    }
  }

  .rarity-flash {
    animation: poc2-rarity-flash 1.5s ease-out;
    border-radius: 4px;
  }
</style>
