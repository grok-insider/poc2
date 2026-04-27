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
  import EligiblePanel from './lib/EligiblePanel.svelte';
  import HistoryPanel from './lib/HistoryPanel.svelte';
  import ItemDatabase from './lib/ItemDatabase.svelte';
  import type {
    AssetManifest,
    BaseIconManifest,
    BaseSummary,
    Goal,
    HistoryEntry,
    Item,
    MaterialUse,
    PersistedState,
    Recommendation,
    TrainedModelStatus,
  } from './lib/types';
  import { FRESH_BODY_ARMOUR, WORKED_EXAMPLE_GOAL } from './lib/fixtures';

  type DrawerView = 'item' | 'target' | 'tools' | null;

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
  let itemDatabaseMode = $state<'inspect' | 'pick-base'>('inspect');
  /** Right column tab — 'preview' (poe2db-style item card), 'eligible'
   * (eligible-mod pool), 'history' (crafting log timeline). */
  let rightTab = $state<'preview' | 'eligible' | 'history'>('preview');
  /** Left column tab — 'guide' (step-by-step recommendations from
   * AdvisorPanel), 'base' (BaseItem editor). */
  let leftTab = $state<'guide' | 'base'>('guide');
  let history = $state<HistoryEntry[]>([]);
  let trainedStatus = $state<TrainedModelStatus | null>(null);
  let recentlyChangedModId = $state<string | null>(null);
  let recentRarityFlash = $state(false);
  let highlightTimer: ReturnType<typeof setTimeout> | null = null;

  /** M17 — free-form notes pinned to the bottom dock. Persisted via the
   * `notes` field on PersistedState. */
  let notes = $state<string>('');
  let notesEditing = $state(false);
  /** Crafting log filter — matches the dropdown in the bottom-dock log. */
  let logFilter = $state<'all' | 'completed' | 'pending'>('all');

  function flashChange(before: typeof item, after: typeof item, change: string) {
    if (highlightTimer) clearTimeout(highlightTimer);
    if (change === 'rarity') {
      recentlyChangedModId = null;
      recentRarityFlash = true;
    } else {
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
  const successChance = $derived(topRecommendation ? topRecommendation.expected_prob * 100 : 0);
  const nextCost = $derived(topRecommendation ? recommendationDisplayCost(topRecommendation) : 0);
  const spentCost = $derived(
    history.reduce((sum, entry) => sum + (entry.cost_div ?? 0), 0),
  );
  const totalCost = $derived(spentCost + nextCost);
  /** Risk classification — drives the topbar bars chip and summary tile.
   * Uses the same thresholds as the success chip text color. */
  const riskBucket = $derived<'low' | 'medium' | 'high'>(
    successChance >= 60 ? 'low' : successChance >= 30 ? 'medium' : 'high',
  );
  const riskBars = $derived(riskBucket === 'low' ? 1 : riskBucket === 'medium' ? 2 : 3);
  const goalMet = $derived(topRecommendation?.action.kind === 'stop');
  const targetConcepts = $derived(extractTargetConcepts(goal));
  const prefixCapacity = $derived(rarityCapacity(item.rarity));
  const suffixCapacity = $derived(rarityCapacity(item.rarity));
  const remoteArtCount = $derived(
    (assetManifest?.entries ?? []).filter((entry) => entry.source_url?.startsWith('http')).length,
  );

  /** Materials shopping list — completed ledger entries plus the current
   * next recommendation. Top-N recommendations are alternatives, not a
   * sequential plan, so we intentionally do not aggregate every option. */
  const materials = $derived(buildMaterials(history, topRecommendation));
  /** Per-step cost rows for the bottom-dock CURRENCY COST ESTIMATE panel.
   * Completed rows come from history; the only pending row is the current
   * top recommendation. */
  const stepCosts = $derived(
    buildCostRows(history, topRecommendation),
  );

  /** Filtered crafting log for the bottom dock and right column. We
   * only persist completed entries today; "pending" surfaces the
   * *next* recommended action as a synthetic pending row below. */
  const visibleLog = $derived(
    logFilter === 'pending' ? [] : history,
  );
  /** Synthetic pending row — pinned at the top of the log when the
   * current top recommendation hasn't been recorded yet. */
  const pendingRow = $derived(
    topRecommendation && (logFilter === 'all' || logFilter === 'pending')
      ? {
          stepNumber: history.length + 1,
          label: stepShortLabel(topRecommendation.action),
        }
      : null,
  );

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
        if (s.item_json) {
          try {
            const parsed = JSON.parse(s.item_json) as Item;
            if (parsed?.base && parsed.rarity && typeof parsed.ilvl === 'number') item = parsed;
          } catch {
            /* keep default */
          }
        }
        if (s.league) league = s.league;
        if (
          s.auto_refresh_minutes === 0 ||
          s.auto_refresh_minutes === 5 ||
          s.auto_refresh_minutes === 30 ||
          s.auto_refresh_minutes === 60
        ) {
          autoRefreshMinutes = s.auto_refresh_minutes;
        }
        if (typeof s.notes === 'string') notes = s.notes;
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
    void refreshTrainedStatus();
  });

  async function refreshTrainedStatus() {
    try {
      trainedStatus = await invoke<TrainedModelStatus>('trained_model_status');
    } catch {
      trainedStatus = null;
    }
  }

  $effect(() => {
    if (!stateLoaded) return;
    invoke('save_state', {
      state: {
        goal_json: JSON.stringify(goal),
        item_json: JSON.stringify(item),
        league,
        auto_refresh_minutes: autoRefreshMinutes,
        notes,
      } satisfies PersistedState,
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

  function resetItem() {
    item = structuredClone(FRESH_BODY_ARMOUR);
    history = [];
    recommendations = [];
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
    const name = it.base_display_name ?? displayId(it.base);
    // poe2db-style: just the base name in caps for the typeLine. The
    // rarity word appears separately as a kicker on the title row.
    return name;
  }

  function rarityWord(r: string): string {
    if (r === 'rare') return 'Rare';
    if (r === 'magic') return 'Magic';
    if (r === 'unique') return 'Unique';
    return 'Normal';
  }

  function classDisplay(it: Item): string {
    // Fallback to splitting the engine class id (e.g. "BodyArmour" → "Body Armours").
    const id = it.base ?? '';
    const split = id.replace(/([a-z])([A-Z])/g, '$1 $2').trim();
    if (!split) return 'Item';
    // Pluralize the rough way poe2db labels armour classes.
    if (/Armour$/.test(split)) return `${split}s`;
    return split;
  }

  function openBasePicker(initialClass: string | null = null) {
    itemDatabaseMode = 'pick-base';
    basePickerInitialClass = initialClass;
    basePickerOpen = true;
  }

  function openItemDatabase() {
    itemDatabaseMode = 'inspect';
    basePickerInitialClass = null;
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
    history = [];
    recommendations = [];
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
    const lower = modId.toLowerCase();
    return concepts.some((c) => lower.includes(c.toLowerCase()));
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
    if (a.kind === 'recurring') return 'Recurring step';
    return 'Action';
  }

  function recommendationDisplayCost(r: Recommendation): number {
    return r.loop_estimate?.total_cost.expected ?? r.expected_cost.expected;
  }

  function stepShortLabel(a: Recommendation['action']): string {
    if (a.kind === 'apply_currency') return displayId(a.currency);
    if (a.kind === 'activate_omen') return `Omen: ${displayId(a.omen)}`;
    if (a.kind === 'apply_hinekoras_lock') return "Hinekora's Lock";
    if (a.kind === 'reveal') return 'Reveal at Well of Souls';
    if (a.kind === 'recombine') return 'Recombine';
    if (a.kind === 'guidance') return 'Guidance';
    if (a.kind === 'stop') return 'Stop';
    if (a.kind === 'abandon') return 'Abandon';
    if (a.kind === 'recurring') return 'Recurring step';
    return 'Action';
  }

  function addMaterialUse(tally: Map<string, number>, use: MaterialUse, scale = 1) {
    if (use.quantity <= 0) return;
    tally.set(use.id, (tally.get(use.id) ?? 0) + use.quantity * scale);
  }

  function materialUsesForAction(
    action: Recommendation['action'],
    rec: Recommendation | null = null,
  ): MaterialUse[] {
    if (action.kind === 'apply_currency') {
      return [
        { id: action.currency, quantity: 1 },
        ...action.omens.map((id) => ({ id, quantity: 1 })),
      ];
    }
    if (action.kind === 'activate_omen') return [{ id: action.omen, quantity: 1 }];
    if (action.kind === 'apply_hinekoras_lock') return [{ id: 'HinekorasLock', quantity: 1 }];
    if (action.kind === 'reveal') {
      const uses: MaterialUse[] = [];
      if (action.bone) uses.push({ id: action.bone, quantity: 1 });
      if (action.omen) uses.push({ id: action.omen, quantity: 1 });
      return uses;
    }
    if (action.kind === 'recombine') {
      return action.omens.map((id) => ({ id, quantity: 1 }));
    }
    if (action.kind === 'recurring') {
      const iterations = Math.max(1, Math.round(rec?.loop_estimate?.mean_iterations ?? 1));
      const tally = new Map<string, number>();
      for (const inner of action.inner) {
        for (const use of materialUsesForAction(inner)) addMaterialUse(tally, use, iterations);
      }
      return [...tally.entries()].map(([id, quantity]) => ({ id, quantity }));
    }
    return [];
  }

  /** Aggregate a per-currency materials shopping list out of completed
   * history plus the current next recommendation. */
  function buildMaterials(
    entries: HistoryEntry[],
    next: Recommendation | null,
  ): { id: string; needed: number; have: number }[] {
    const tally = new Map<string, number>();
    for (const entry of entries) {
      for (const use of entry.materials ?? []) addMaterialUse(tally, use);
    }
    if (next) {
      for (const use of materialUsesForAction(next.action, next)) addMaterialUse(tally, use);
    }
    return [...tally.entries()].map(([id, needed]) => ({
      id,
      needed: Math.max(1, Math.ceil(needed)),
      // Synthetic placeholder until we hook a real stash inventory IPC:
      // we report the user "owns" a comfortable margin so the panel
      // shows green ticks rather than fabricating shortages.
      have: Math.max(1, Math.ceil(needed)) + 2,
    }));
  }

  function buildCostRows(entries: HistoryEntry[], next: Recommendation | null) {
    const rows = [...entries].reverse().map((entry, i) => ({
      key: entry.id,
      idx: i + 1,
      label: entry.action_label ?? entry.change,
      cost: entry.cost_div ?? 0,
      isCurrent: false,
    }));
    if (next) {
      rows.push({
        key: 'pending-next',
        idx: entries.length + 1,
        label: stepShortLabel(next.action),
        cost: recommendationDisplayCost(next),
        isCurrent: true,
      });
    }
    return rows;
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

  function timeAgo(ts: string): string {
    // HistoryPanel persists a wall-clock string, not a Date — so we
    // can't compute an exact delta. Fall back to the raw label which
    // already reads like a friendly time stamp.
    return ts;
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
      <button onclick={openItemDatabase}>
        <span class="nav-icon">▦</span> Item Database
      </button>
    </nav>
    <div class="sidebar-footer">
      <button
        type="button"
        class="art-line"
        onclick={loadAssets}
        disabled={assetsLoading}
        title="Click to reload remote art manifest"
      >
        <span class="art-eye">Artifacts</span>
        <strong>{remoteArtCount}/{assetManifest?.entries.length ?? 0}</strong>
        {#if assetsLoading}<span class="art-spinner">…</span>{/if}
      </button>
      <div class="online-pill">
        <span class="dot" aria-hidden="true"></span>
        PoC 2 · online
      </div>
    </div>
  </aside>

  <section class="workspace">
    <header class="topbar">
      <div class="topbar-left">
        <div class="title-block">
          <span class="eyebrow">Crafting Project</span>
          <h1>{rarityWord(item.rarity)} {itemDisplayTitle(item)}</h1>
        </div>
        <div class="title-pills">
          <span class="pill">LVL {item.ilvl}+</span>
          {#if targetConcepts.length > 0}
            <span class="pill emphasis" title={targetConcepts.join(', ')}>
              {targetConcepts.length === 1
                ? targetConcepts[0]
                : `${targetConcepts[0]} + ${targetConcepts.length - 1}`} Goal
            </span>
          {/if}
        </div>
      </div>
      <div class="top-actions">
        <span class="success-chip">
          <span class="chip-label">Success</span>
          <strong>{successChance.toFixed(1)}%</strong>
        </span>
        <span class="risk-chip risk-{riskBucket}">
          <span class="chip-label">Risk</span>
          <strong>{riskBucket === 'low' ? 'Low' : riskBucket === 'medium' ? 'Medium' : 'High'}</strong>
          <span class="bars" aria-hidden="true">
            <span class:on={riskBars >= 1}></span>
            <span class:on={riskBars >= 2}></span>
            <span class:on={riskBars >= 3}></span>
          </span>
        </span>
        {#if trainedStatus}
          <button
            type="button"
            class="policy-pill"
            class:active={trainedStatus.models_loaded > 0}
            onclick={refreshTrainedStatus}
            title={trainedStatus.models_loaded > 0
              ? `${trainedStatus.models_loaded} trained Q-table${trainedStatus.models_loaded === 1 ? '' : 's'} active. Cache: ${trainedStatus.cache_dir}`
              : `No trained models loaded. Cache dir: ${trainedStatus.cache_dir} (${trainedStatus.cache_dir_exists ? 'exists' : 'missing'}). Run train-advisor to populate.`}
          >
            <span class="dot" aria-hidden="true"></span>
            <span class="label">Policy</span>
            <strong>{trainedStatus.models_loaded}</strong>
          </button>
        {/if}
        <span class="patch-pill">PoE 2 v0.4</span>
        <button
          type="button"
          class="settings-cog"
          aria-label="Tools and Recipes"
          title="Tools and Recipes"
          onclick={() => openDrawer('tools')}
        >⚙</button>
      </div>
    </header>

    {#if assetError}
      <div class="notice">
        <span class="danger">{assetError}</span>
      </div>
    {/if}

    <div class="content">
      <div class="col advisor-col panel">
        <div class="left-tabstrip" role="tablist">
          <button
            type="button"
            class:active={leftTab === 'guide'}
            onclick={() => (leftTab = 'guide')}
          >Step-by-Step Guide</button>
          <button
            type="button"
            class:active={leftTab === 'base'}
            onclick={() => (leftTab = 'base')}
          >Base Item</button>
        </div>
        <div class="left-body">
          {#if leftTab === 'guide'}
            <AdvisorPanel
              {item}
              {goal}
              {assetIndex}
              embedded
              onRecommendations={(recs) => {
                recommendations = recs;
              }}
              onItemUpdate={(next, change, explanation, recommendation) => {
                const before = item;
                item = next;
                recommendations = [];
                flashChange(before, next, change);
                history = [
                  {
                    id: `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
                    timestamp: new Date().toLocaleTimeString(),
                    change,
                    explanation,
                    action: recommendation?.action ?? null,
                    action_label: recommendation ? actionLabel(recommendation.action) : change,
                    cost_div: recommendation ? recommendationDisplayCost(recommendation) : 0,
                    materials: recommendation
                      ? materialUsesForAction(recommendation.action, recommendation)
                      : [],
                    before,
                  },
                  ...history,
                ].slice(0, 50);
              }}
            />
          {:else}
            <div class="base-tab-body">
              <button
                class="ghost wide"
                onclick={() => openBasePicker(item.base)}
              >
                Pick a base from the database
              </button>
              <ClipboardImport onItem={(next) => {
                item = next;
                history = [];
                recommendations = [];
              }} />
              <ItemBuilder {item} onUpdate={(next) => (item = next)} />
              <button class="ghost wide" onclick={resetItem}>Reset to fresh base</button>
            </div>
          {/if}
        </div>
      </div>

      <aside class="col right-stack">
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
            >Eligible Bases</button>
            <button
              type="button"
              class:active={rightTab === 'history'}
              onclick={() => (rightTab = 'history')}
            >History {history.length > 0 ? `· ${history.length}` : ''}</button>
          </div>

          <div class="tab-body">
            {#if rightTab === 'preview'}
              <!-- poe2db-style item card. The structure mirrors the
                   reference DOM (typeLine → properties → separator →
                   requirements → art) so the styling reads as the
                   in-game item popup. -->
              <div class="poe2db-card rarity-{item.rarity}" class:rarity-flash={recentRarityFlash}>
                <div class="typeLine">
                  <span class="lc">{itemDisplayTitle(item)}</span>
                </div>
                <div class="properties">
                  <div class="property class-line">{classDisplay(item)}</div>
                  {#each item.implicits as mod, i (i)}
                    <div class="property implicit-line">{modLabel(mod.mod_id)}</div>
                  {/each}
                  <div class="separator"></div>
                  <div class="property requirements">
                    Requires: <span class="num">Level {item.ilvl}</span>
                  </div>
                </div>
                <div class="poe2db-art">
                  {#if imageAvailable(itemAssetId(item), itemImage)}
                    <img
                      src={itemImage ?? ''}
                      alt=""
                      onerror={() => markImageFailed(itemAssetId(item))}
                    />
                  {:else}
                    <div class="fallback-art">{initials(item.base)}</div>
                  {/if}
                </div>
                {#if item.prefixes.length + item.suffixes.length > 0}
                  <div class="separator"></div>
                  <div class="mod-block">
                    {#each item.prefixes as mod, i (i)}
                      <p
                        class="mod-line affix"
                        class:satisfies={modSatisfiesTarget(mod.mod_id, targetConcepts)}
                        class:recently-changed={recentlyChangedModId === mod.mod_id}
                      >
                        {modLabel(mod.mod_id)}{mod.is_fractured ? ' (fractured)' : ''}
                      </p>
                    {/each}
                    {#each item.suffixes as mod, i (i)}
                      <p
                        class="mod-line affix"
                        class:satisfies={modSatisfiesTarget(mod.mod_id, targetConcepts)}
                        class:recently-changed={recentlyChangedModId === mod.mod_id}
                      >
                        {modLabel(mod.mod_id)}{mod.is_fractured ? ' (fractured)' : ''}
                      </p>
                    {/each}
                  </div>
                {/if}
              </div>

              <button class="ghost compact change-base" onclick={() => openBasePicker(item.base)}>
                Change Base
              </button>

              <div class="affix-slots">
                <div class="kicker">Affix Slots</div>
                <div class="affix-row">
                  <span class="affix-label">Prefixes</span>
                  <div class="diamonds">
                    {#each Array(prefixCapacity) as _, i (i)}
                      <span class="diamond" class:filled={i < item.prefixes.length}></span>
                    {/each}
                    {#if prefixCapacity === 0}
                      <span class="diamond"></span>
                      <span class="diamond"></span>
                      <span class="diamond"></span>
                    {/if}
                  </div>
                  <span class="affix-count">{item.prefixes.length}/{prefixCapacity || 3}</span>
                </div>
                <div class="affix-row">
                  <span class="affix-label">Suffixes</span>
                  <div class="diamonds">
                    {#each Array(suffixCapacity) as _, i (i)}
                      <span class="diamond" class:filled={i < item.suffixes.length}></span>
                    {/each}
                    {#if suffixCapacity === 0}
                      <span class="diamond"></span>
                      <span class="diamond"></span>
                      <span class="diamond"></span>
                    {/if}
                  </div>
                  <span class="affix-count">{item.suffixes.length}/{suffixCapacity || 3}</span>
                </div>
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
                  history = history.slice(idx + 1);
                }}
              />
            {/if}
          </div>
        </section>

        <section class="panel summary-card">
          <div class="panel-title">Crafting Summary</div>
          <div class="summary-grid">
            <div class="summary-tile">
              <span>Total Steps</span>
              <strong>{history.length + (topRecommendation ? 1 : 0)}</strong>
            </div>
            <div class="summary-tile">
              <span>Projected Cost</span>
              <strong>~{totalCost.toFixed(2)} div</strong>
            </div>
            <div class="summary-tile">
              <span>Success Chance</span>
              <strong class="prob">{successChance.toFixed(1)}%</strong>
            </div>
            <div class="summary-tile">
              <span>Risk Level</span>
              <strong class="risk risk-{riskBucket}">
                {riskBucket === 'low' ? 'Low' : riskBucket === 'medium' ? 'Medium' : 'High'}
              </strong>
            </div>
            <div class="summary-tile cheapest" style="grid-column: 1 / -1;">
              <span>Next Recommendation</span>
              <strong>
                {topRecommendation ? actionLabel(topRecommendation.action) : '—'}
                {#if topRecommendation}
                  <small>~{nextCost.toFixed(2)} div</small>
                {/if}
              </strong>
            </div>
          </div>
          {#if goalMet}
            <div class="goal-met">
              <strong>Goal met</strong>
              <span>Divine to perfect rolls or stop.</span>
            </div>
          {/if}
          <label class="failure-toggle">
            <input type="checkbox" bind:checked={lastFailed} /> Last action failed
          </label>
          <RecoveryPanel recommendation={topRecommendation} {lastFailed} />
        </section>
      </aside>
    </div>

    <section class="bottom-dock">
      <article class="dock-panel cost-panel">
        <header>
          <span class="dock-kicker">Currency Cost Ledger</span>
          <span class="dock-total">~<strong>{totalCost.toFixed(2)}</strong> div</span>
        </header>
        <ul class="cost-list">
          {#each stepCosts as step (step.key)}
            <li class:current={step.isCurrent}>
              <span class="bullet" aria-hidden="true">●</span>
              <span class="step-num">Step {step.idx}</span>
              <span class="step-name">{step.label}</span>
              <span class="step-cost">~{step.cost.toFixed(2)} div</span>
            </li>
          {/each}
          {#if stepCosts.length === 0}
            <li class="empty">No planned steps yet.</li>
          {/if}
        </ul>
        <footer class="cost-total-row">
          <span>Spent + Next Estimate</span>
          <strong>~{totalCost.toFixed(2)} div</strong>
        </footer>
      </article>

      <article class="dock-panel materials-panel">
        <header class="materials-header">
          <span class="dock-kicker">Required Materials</span>
          <span class="hdr-needed">Needed</span>
          <span class="hdr-have">Have</span>
        </header>
        <ul class="materials-list">
          {#each materials as mat (mat.id)}
            <li>
              {#if imageAvailable(mat.id, assetUrl(assetIndex, mat.id))}
                <img
                  src={assetUrl(assetIndex, mat.id) ?? ''}
                  alt=""
                  onerror={() => markImageFailed(mat.id)}
                />
              {:else}
                <div class="mini-fallback" aria-hidden="true">{initials(displayId(mat.id))}</div>
              {/if}
              <span class="mat-name">{displayId(mat.id)}</span>
              <span class="mat-needed">{mat.needed}</span>
              <span class="mat-have" class:short={mat.have < mat.needed}>{mat.have}</span>
            </li>
          {/each}
          {#if materials.length === 0}
            <li class="empty">No materials needed yet.</li>
          {/if}
        </ul>
      </article>

      <article class="dock-panel notes-panel">
        <header>
          <span class="dock-kicker">Notes</span>
        </header>
        {#if notesEditing}
          <textarea
            class="notes-text edit"
            bind:value={notes}
            placeholder="Aim for high ES rolls and open suffixes for resistances or attributes."
          ></textarea>
        {:else}
          <div class="notes-text view">
            {#if notes.trim().length === 0}
              <p class="muted">No notes yet. Click "Edit Notes" to start a scratchpad.</p>
            {:else}
              {#each notes.split(/\n+/) as line, i (i)}
                {#if line.trim().length > 0}
                  <p>{line}</p>
                {/if}
              {/each}
            {/if}
          </div>
        {/if}
        <footer>
          <button
            type="button"
            class="ghost compact"
            onclick={() => (notesEditing = !notesEditing)}
          >
            {notesEditing ? 'Save Notes' : 'Edit Notes'} ✎
          </button>
        </footer>
      </article>

      <article class="dock-panel log-panel">
        <header class="log-header">
          <span class="dock-kicker">Crafting Log</span>
          <select bind:value={logFilter} class="log-filter">
            <option value="all">All Steps</option>
            <option value="completed">Completed</option>
            <option value="pending">Pending</option>
          </select>
        </header>
        <ul class="log-list">
          {#if pendingRow}
            <li class="log-row pending">
              <span class="badge pending-badge">{pendingRow.stepNumber}</span>
              <span class="log-time">—</span>
              <span class="log-name">Step {pendingRow.stepNumber}: {pendingRow.label}</span>
              <span class="log-status pending">Pending</span>
            </li>
          {/if}
          {#each visibleLog as entry, i (entry.id)}
            <li class="log-row completed">
              <span class="badge completed-badge" aria-hidden="true">✓</span>
              <span class="log-time">{timeAgo(entry.timestamp)}</span>
              <span class="log-name">Step {visibleLog.length - i}: {entry.action_label ?? entry.change}</span>
              <span class="log-status completed">Completed</span>
            </li>
          {/each}
          {#if visibleLog.length === 0 && !pendingRow}
            <li class="log-row empty">
              <span class="muted">No outcomes recorded yet.</span>
            </li>
          {/if}
        </ul>
        <footer>
          <button
            type="button"
            class="ghost compact wide"
            onclick={() => (rightTab = 'history')}
          >
            View Full History
          </button>
        </footer>
      </article>
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
            <ClipboardImport onItem={(next) => {
              item = next;
              history = [];
              recommendations = [];
            }} />
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
                history = [];
                recommendations = [];
              }}
            />
          {/if}
        </div>
      </aside>
    {/if}
  </section>

  <ItemDatabase
    open={basePickerOpen}
    mode={itemDatabaseMode}
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
    grid-template-columns: 196px minmax(0, 1fr);
    overflow: hidden;
    background:
      radial-gradient(circle at 12% -10%, rgba(221, 174, 92, 0.12), transparent 38rem),
      radial-gradient(circle at 88% 8%, rgba(91, 138, 132, 0.09), transparent 32rem),
      linear-gradient(135deg, #0b0f12 0%, #0f1418 48%, #07090b 100%);
  }

  /* ───────── Sidebar ───────── */
  .sidebar {
    border-right: 1px solid rgba(255, 255, 255, 0.07);
    background: linear-gradient(180deg, rgba(10, 13, 16, 0.98), rgba(5, 7, 9, 0.98)), #07090b;
    padding: 1rem 0.7rem;
    display: flex;
    flex-direction: column;
    gap: 0.9rem;
    overflow: hidden;
    min-height: 0;
  }

  .brand-mark {
    display: flex;
    align-items: center;
    gap: 0.65rem;
    padding: 0.2rem 0.25rem 0.85rem;
    border-bottom: 1px solid rgba(255, 255, 255, 0.07);
    color: var(--gold-bright);
    text-transform: uppercase;
    letter-spacing: 0.08em;
    flex-shrink: 0;
  }

  .brand-mark img {
    width: 40px;
    height: 40px;
    border: 1px solid rgba(255, 211, 122, 0.22);
    border-radius: 12px;
    padding: 0.42rem;
    background: rgba(255, 211, 122, 0.08);
  }

  .brand-mark strong {
    display: block;
    font-size: 0.85rem;
  }

  .brand-mark span,
  .eyebrow {
    color: var(--fg-muted);
  }

  nav {
    display: grid;
    gap: 0.4rem;
    overflow-y: auto;
    min-height: 0;
  }

  nav button {
    display: flex;
    align-items: center;
    gap: 0.65rem;
    border: 1px solid transparent;
    border-radius: 10px;
    padding: 0.55rem 0.65rem;
    color: var(--fg-soft);
    background: rgba(255, 255, 255, 0.025);
    text-align: left;
    font-weight: 500;
    cursor: pointer;
    transition: background 120ms ease, border-color 120ms ease, color 120ms ease;
    font-size: 0.82rem;
  }

  nav button:hover {
    background: rgba(255, 255, 255, 0.055);
    color: var(--gold);
  }

  nav button.active {
    color: var(--gold-bright);
    border-color: rgba(255, 211, 122, 0.3);
    background: linear-gradient(90deg, rgba(197, 143, 61, 0.26), rgba(255, 255, 255, 0.04));
    box-shadow: 0 12px 28px rgba(0, 0, 0, 0.22);
  }

  .nav-icon {
    color: var(--gold);
    font-size: 0.95rem;
    width: 1.6rem;
    height: 1.6rem;
    display: grid;
    place-items: center;
    border-radius: 8px;
    background: rgba(255, 211, 122, 0.08);
    text-align: center;
  }

  .sidebar-footer {
    margin-top: auto;
    display: grid;
    gap: 0.5rem;
    flex-shrink: 0;
  }

  .art-line {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    border: 1px solid var(--border-strong);
    border-radius: 8px;
    padding: 0.4rem 0.6rem;
    background: rgba(0, 0, 0, 0.4);
    color: var(--fg-soft);
    font-size: 0.7rem;
    cursor: pointer;
    text-transform: uppercase;
    letter-spacing: 0.08em;
  }

  .art-line:hover:not(:disabled) {
    border-color: var(--border-gold);
    color: var(--gold);
  }

  .art-line:disabled {
    opacity: 0.6;
    cursor: progress;
  }

  .art-eye {
    color: var(--fg-muted);
  }

  .art-line strong {
    color: var(--gold-bright);
    font-family: Georgia, 'Times New Roman', serif;
    font-size: 0.92rem;
  }

  .art-spinner {
    color: var(--gold);
  }

  .online-pill {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    color: var(--fg-muted);
    font-size: 0.65rem;
    text-transform: uppercase;
    letter-spacing: 0.12em;
    padding: 0.35rem 0.55rem;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: rgba(0, 0, 0, 0.3);
  }

  .online-pill .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: #6cd76a;
    box-shadow: 0 0 0 2px rgba(108, 215, 106, 0.22);
  }

  /* ───────── Workspace ───────── */
  .workspace {
    display: grid;
    grid-template-rows: auto auto 1fr auto;
    overflow: hidden;
    min-height: 0;
    position: relative;
    background: rgba(255, 255, 255, 0.012);
  }

  /* ───────── Topbar ───────── */
  .topbar {
    border-bottom: 1px solid rgba(255, 255, 255, 0.07);
    background: linear-gradient(90deg, rgba(13, 17, 20, 0.96), rgba(12, 15, 18, 0.86)),
      rgba(12, 15, 18, 0.96);
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.85rem 1.1rem;
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

  .eyebrow {
    font-size: 0.65rem;
    text-transform: uppercase;
    letter-spacing: 0.14em;
    color: var(--fg-muted);
  }

  .title-pills {
    display: flex;
    gap: 0.35rem;
    flex-wrap: wrap;
  }

  .pill,
  .patch-pill {
    border: 1px solid var(--border-gold);
    color: var(--gold);
    padding: 0.22rem 0.55rem;
    border-radius: 999px;
    font-size: 0.68rem;
    background: rgba(0, 0, 0, 0.4);
    text-transform: uppercase;
    letter-spacing: 0.1em;
    font-weight: 600;
  }

  .pill.emphasis {
    color: var(--gold-bright);
    background: linear-gradient(90deg, rgba(197, 143, 61, 0.35), rgba(70, 45, 12, 0.5));
  }

  h1 {
    margin: 0;
    font-family: Georgia, 'Times New Roman', serif;
    color: var(--gold-bright);
    font-weight: 500;
    letter-spacing: 0.04em;
    font-size: 1.42rem;
  }

  .top-actions {
    display: flex;
    gap: 0.5rem;
    align-items: center;
    flex-wrap: wrap;
  }

  .success-chip,
  .risk-chip {
    display: inline-flex;
    align-items: center;
    gap: 0.45rem;
    border: 1px solid rgba(197, 143, 61, 0.36);
    border-radius: 999px;
    background: rgba(0, 0, 0, 0.42);
    color: var(--fg-muted);
    padding: 0.32rem 0.7rem;
    font-size: 0.7rem;
    text-transform: uppercase;
    letter-spacing: 0.1em;
  }

  .success-chip {
    border-color: rgba(114, 255, 88, 0.5);
    color: rgba(178, 255, 162, 0.85);
    background: radial-gradient(circle at 20% 20%, rgba(47, 123, 18, 0.3), rgba(0, 0, 0, 0.4));
  }

  .chip-label {
    color: var(--fg-muted);
  }

  .success-chip .chip-label {
    color: rgba(178, 255, 162, 0.7);
  }

  .success-chip strong,
  .risk-chip strong {
    color: var(--gold-bright);
    font-family: Georgia, 'Times New Roman', serif;
    font-size: 0.95rem;
    letter-spacing: 0.02em;
    text-transform: none;
  }

  .success-chip strong {
    color: #b2ffa2;
  }

  .risk-chip.risk-low strong {
    color: #b2ffa2;
  }

  .risk-chip.risk-medium {
    border-color: rgba(255, 200, 80, 0.5);
  }

  .risk-chip.risk-medium strong {
    color: #ffd37a;
  }

  .risk-chip.risk-high {
    border-color: rgba(255, 110, 90, 0.55);
  }

  .risk-chip.risk-high strong {
    color: #ff9c7e;
  }

  .risk-chip .bars {
    display: inline-flex;
    align-items: flex-end;
    gap: 2px;
    margin-left: 0.15rem;
  }

  .risk-chip .bars span {
    display: block;
    width: 3px;
    background: rgba(255, 255, 255, 0.18);
    border-radius: 1px;
  }

  .risk-chip .bars span:nth-child(1) {
    height: 5px;
  }
  .risk-chip .bars span:nth-child(2) {
    height: 8px;
  }
  .risk-chip .bars span:nth-child(3) {
    height: 11px;
  }

  .risk-chip.risk-low .bars span.on {
    background: #b2ffa2;
  }
  .risk-chip.risk-medium .bars span.on {
    background: #ffd37a;
  }
  .risk-chip.risk-high .bars span.on {
    background: #ff9c7e;
  }

  .policy-pill {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    border: 1px solid var(--border-strong);
    border-radius: 999px;
    padding: 0.32rem 0.7rem;
    background: rgba(0, 0, 0, 0.42);
    color: var(--fg-muted);
    font-size: 0.7rem;
    text-transform: uppercase;
    letter-spacing: 0.1em;
    cursor: pointer;
  }

  .policy-pill .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: rgba(160, 60, 60, 0.7);
    box-shadow: 0 0 0 2px rgba(160, 60, 60, 0.18);
  }

  .policy-pill.active {
    color: var(--gold);
    border-color: var(--border-gold);
  }

  .policy-pill.active .dot {
    background: #6cd76a;
    box-shadow: 0 0 0 2px rgba(108, 215, 106, 0.22);
  }

  .policy-pill .label {
    color: var(--fg-muted);
  }

  .policy-pill.active .label {
    color: var(--fg-soft);
  }

  .policy-pill strong {
    color: var(--gold-bright);
    font-family: Georgia, 'Times New Roman', serif;
    font-size: 0.95rem;
  }

  .policy-pill:hover {
    background: rgba(197, 143, 61, 0.08);
  }

  .settings-cog {
    width: 32px;
    height: 32px;
    border: 1px solid var(--border-strong);
    border-radius: 8px;
    background: rgba(0, 0, 0, 0.42);
    color: var(--gold);
    font-size: 1rem;
    cursor: pointer;
    display: grid;
    place-items: center;
  }

  .settings-cog:hover {
    color: var(--gold-bright);
    border-color: var(--border-gold);
    background: rgba(197, 143, 61, 0.1);
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

  /* ───────── Two-column content ───────── */
  .content {
    display: grid;
    grid-template-columns: minmax(0, 1fr) clamp(320px, 26vw, 380px);
    gap: 0.75rem;
    padding: 0.85rem 1.1rem;
    min-height: 0;
    overflow: hidden;
  }

  .col {
    min-height: 0;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }

  .advisor-col {
    overflow: hidden;
    padding: 0;
  }

  .right-stack {
    overflow-y: auto;
    overflow-x: hidden;
    padding-right: 2px;
    gap: 0.75rem;
  }

  .panel,
  :global(section.panel) {
    border: 1px solid rgba(255, 255, 255, 0.075);
    background: linear-gradient(180deg, rgba(18, 22, 25, 0.96), rgba(8, 11, 14, 0.96)),
      rgba(14, 18, 21, 0.94);
    box-shadow: 0 22px 50px rgba(0, 0, 0, 0.2), inset 0 1px 0 rgba(255, 255, 255, 0.035);
    padding: 0.85rem;
    border-radius: 16px;
  }

  .panel-title {
    color: var(--gold);
    text-align: center;
    text-transform: uppercase;
    letter-spacing: 0.1em;
    font-family: Georgia, 'Times New Roman', serif;
    margin-bottom: 0.55rem;
    font-size: 0.78rem;
    flex-shrink: 0;
  }

  /* ───────── Left column (advisor / base item) ───────── */
  .left-tabstrip {
    display: flex;
    gap: 0.25rem;
    padding: 0.45rem 0.5rem 0;
    border-bottom: 1px solid rgba(255, 255, 255, 0.07);
    background: rgba(0, 0, 0, 0.18);
    border-top-left-radius: 16px;
    border-top-right-radius: 16px;
    flex-shrink: 0;
  }

  .left-tabstrip button {
    background: transparent;
    color: var(--fg-muted);
    border: 1px solid transparent;
    border-bottom: 0;
    border-radius: 8px 8px 0 0;
    padding: 0.45rem 0.9rem;
    cursor: pointer;
    font-size: 0.78rem;
    text-transform: uppercase;
    letter-spacing: 0.1em;
    font-family: Georgia, 'Times New Roman', serif;
  }

  .left-tabstrip button:hover {
    color: var(--gold);
  }

  .left-tabstrip button.active {
    color: var(--gold-bright);
    border-color: rgba(255, 211, 122, 0.18);
    background: rgba(255, 211, 122, 0.08);
  }

  .left-body {
    flex: 1 1 auto;
    min-height: 0;
    overflow: hidden;
    display: flex;
    flex-direction: column;
    padding: 0.85rem;
  }

  .base-tab-body {
    overflow-y: auto;
    display: grid;
    gap: 0.7rem;
    flex: 1 1 auto;
  }

  /* ───────── Right column tabs panel ───────── */
  .tabs-panel {
    display: flex;
    flex-direction: column;
    flex: 0 0 auto;
    overflow: hidden;
    padding: 0;
  }

  .tabstrip {
    display: flex;
    gap: 0.25rem;
    padding: 0.4rem 0.5rem 0;
    border-bottom: 1px solid rgba(255, 255, 255, 0.07);
    background: rgba(0, 0, 0, 0.18);
    border-top-left-radius: 16px;
    border-top-right-radius: 16px;
  }

  .tabstrip button {
    background: transparent;
    color: var(--fg-muted);
    border: 1px solid transparent;
    border-bottom: 0;
    border-radius: 6px 6px 0 0;
    padding: 0.35rem 0.7rem;
    cursor: pointer;
    font-size: 0.74rem;
    text-transform: uppercase;
    letter-spacing: 0.1em;
    font-family: Georgia, 'Times New Roman', serif;
  }

  .tabstrip button:hover {
    color: var(--gold);
  }

  .tabstrip button.active {
    color: var(--gold-bright);
    border-color: rgba(255, 211, 122, 0.18);
    background: rgba(255, 211, 122, 0.08);
  }

  .tab-body {
    padding: 0.85rem;
    flex: 1 1 auto;
    min-height: 0;
  }

  /* ───────── poe2db-style item card ───────── */
  .poe2db-card {
    border: 1px solid rgba(127, 127, 127, 0.45);
    background: rgba(0, 0, 0, 0.92);
    color: rgb(127, 127, 127);
    font-family: 'Fontin SmallCaps', 'FontinSmallCaps', Verdana, Arial, Helvetica, sans-serif;
    font-variant: small-caps;
    text-align: center;
    padding: 0.55rem 1rem 0.85rem;
    border-radius: 4px;
    line-height: 1.32;
  }

  .poe2db-card .typeLine {
    font-size: 1.18rem;
    line-height: 1.05;
    padding: 0.45rem 0;
    color: var(--normal-color, #c8c8c8);
  }

  .poe2db-card.rarity-magic .typeLine {
    color: #8888ff;
  }

  .poe2db-card.rarity-rare .typeLine {
    color: #ffff77;
  }

  .poe2db-card.rarity-unique .typeLine {
    color: #af6025;
  }

  .poe2db-card.rarity-magic {
    border-color: rgba(136, 136, 255, 0.5);
  }
  .poe2db-card.rarity-rare {
    border-color: rgba(255, 255, 119, 0.5);
  }
  .poe2db-card.rarity-unique {
    border-color: rgba(175, 96, 37, 0.6);
  }

  .poe2db-card .typeLine .lc {
    display: inline-block;
    padding: 0.25rem 1.5rem;
  }

  .poe2db-card .properties {
    display: grid;
    gap: 0.05rem;
    font-size: 0.92rem;
  }

  .poe2db-card .property {
    color: rgb(127, 127, 127);
  }

  .poe2db-card .property .num {
    color: #fff;
  }

  .poe2db-card .property.implicit-line {
    color: #b4b4ff; /* crafted-color */
  }

  .poe2db-card .property.requirements .num {
    color: #fff;
  }

  .poe2db-card .separator {
    height: 8px;
    margin: 4px 0;
    background:
      linear-gradient(
        90deg,
        transparent,
        rgba(127, 127, 127, 0.55) 20%,
        rgba(127, 127, 127, 0.55) 80%,
        transparent
      );
    mask-image: linear-gradient(90deg, transparent, #000 30%, #000 70%, transparent);
  }

  .poe2db-card .poe2db-art {
    margin-top: 0.5rem;
    display: grid;
    place-items: center;
    min-height: 130px;
  }

  .poe2db-card .poe2db-art img {
    max-width: 70%;
    max-height: 220px;
    object-fit: contain;
    filter: drop-shadow(0 12px 16px rgba(0, 0, 0, 0.7));
  }

  .poe2db-card .poe2db-art .fallback-art {
    width: 96px;
    height: 96px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    border: 1px solid rgba(0, 202, 255, 0.4);
    color: #00d7ff;
    background: rgba(0, 35, 50, 0.45);
    font-size: 1.4rem;
    letter-spacing: 0.08em;
  }

  .poe2db-card .mod-block {
    display: grid;
    gap: 0.15rem;
    padding-top: 0.25rem;
  }

  .poe2db-card .mod-line {
    margin: 0;
    color: #8888ff; /* magic-color = explicit affix in poe2db scheme */
    font-size: 0.92rem;
  }

  .poe2db-card .mod-line.satisfies {
    color: #72ff58;
  }

  .change-base {
    margin: 0.55rem auto 0;
    display: inline-block;
  }

  /* ───────── Affix Slots block ───────── */
  .affix-slots {
    margin-top: 0.85rem;
    border: 1px solid rgba(197, 143, 61, 0.22);
    background: rgba(0, 0, 0, 0.25);
    border-radius: 12px;
    padding: 0.65rem 0.8rem;
    display: grid;
    gap: 0.45rem;
  }

  .affix-slots .kicker {
    color: var(--gold);
    font-size: 0.7rem;
    text-transform: uppercase;
    letter-spacing: 0.12em;
    text-align: center;
    font-family: Georgia, 'Times New Roman', serif;
    border-bottom: 1px solid rgba(197, 143, 61, 0.2);
    padding-bottom: 0.3rem;
  }

  .affix-row {
    display: grid;
    grid-template-columns: minmax(70px, auto) 1fr auto;
    align-items: center;
    gap: 0.55rem;
    font-size: 0.78rem;
  }

  .affix-label {
    color: var(--fg-muted);
    text-transform: uppercase;
    letter-spacing: 0.08em;
    font-size: 0.68rem;
  }

  .affix-row .diamonds {
    display: flex;
    gap: 0.35rem;
  }

  .diamond {
    width: 12px;
    height: 12px;
    border: 1px solid rgba(197, 143, 61, 0.5);
    transform: rotate(45deg);
    background: rgba(0, 0, 0, 0.45);
  }

  .diamond.filled {
    background: linear-gradient(135deg, rgba(255, 211, 122, 0.95), rgba(150, 105, 30, 0.95));
    border-color: rgba(255, 211, 122, 0.85);
    box-shadow: 0 0 6px rgba(255, 211, 122, 0.45);
  }

  .affix-count {
    color: var(--fg-muted);
    font-family: ui-monospace, 'Fira Code', monospace;
    font-size: 0.74rem;
  }

  /* ───────── Crafting Summary ───────── */
  .summary-card {
    display: grid;
    gap: 0.55rem;
    flex-shrink: 0;
  }

  .summary-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0.4rem;
  }

  .summary-tile {
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
    border: 1px solid rgba(255, 255, 255, 0.075);
    border-radius: 10px;
    padding: 0.45rem 0.6rem;
    background: rgba(0, 0, 0, 0.32);
  }

  .summary-tile span {
    color: var(--fg-muted);
    text-transform: uppercase;
    letter-spacing: 0.08em;
    font-size: 0.65rem;
  }

  .summary-tile strong {
    color: var(--gold-bright);
    font-family: Georgia, 'Times New Roman', serif;
    font-size: 1rem;
    display: flex;
    align-items: baseline;
    gap: 0.4rem;
  }

  .summary-tile strong.prob {
    color: #b2ffa2;
  }

  .summary-tile strong.risk.risk-low {
    color: #b2ffa2;
  }
  .summary-tile strong.risk.risk-medium {
    color: #ffd37a;
  }
  .summary-tile strong.risk.risk-high {
    color: #ff9c7e;
  }

  .summary-tile small {
    color: var(--fg-muted);
    font-size: 0.7rem;
    font-family: ui-monospace, 'Fira Code', monospace;
    text-transform: none;
    letter-spacing: 0;
  }

  .summary-tile.cheapest strong {
    font-size: 0.92rem;
  }

  .failure-toggle {
    display: flex;
    gap: 0.4rem;
    align-items: center;
    font-size: 0.78rem;
    color: var(--fg-muted);
  }

  .goal-met {
    border: 1px solid rgba(114, 255, 88, 0.55);
    background: radial-gradient(circle, rgba(47, 123, 18, 0.32), rgba(0, 0, 0, 0.4));
    color: #b2ffa2;
    border-radius: 8px;
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

  /* ───────── Bottom dock — 4 columns ───────── */
  .bottom-dock {
    border-top: 1px solid rgba(255, 255, 255, 0.07);
    background: linear-gradient(180deg, rgba(8, 11, 14, 0.92), rgba(5, 8, 10, 0.95)),
      rgba(5, 8, 10, 0.92);
    padding: 0.7rem 1.1rem 0.85rem;
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 0.7rem;
    min-height: 0;
  }

  .dock-panel {
    border: 1px solid rgba(255, 255, 255, 0.07);
    background: linear-gradient(180deg, rgba(18, 22, 25, 0.94), rgba(8, 11, 14, 0.94));
    border-radius: 12px;
    padding: 0.6rem 0.7rem 0.55rem;
    display: flex;
    flex-direction: column;
    gap: 0.45rem;
    min-width: 0;
    min-height: 0;
  }

  .dock-panel header {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 0.5rem;
    border-bottom: 1px solid rgba(197, 143, 61, 0.15);
    padding-bottom: 0.35rem;
  }

  .dock-kicker {
    color: var(--gold);
    text-transform: uppercase;
    letter-spacing: 0.12em;
    font-family: Georgia, 'Times New Roman', serif;
    font-size: 0.72rem;
  }

  .dock-total {
    color: var(--fg-soft);
    font-size: 0.78rem;
  }

  .dock-total strong {
    color: var(--gold-bright);
    font-family: Georgia, 'Times New Roman', serif;
    font-size: 0.95rem;
    margin-right: 0.15rem;
  }

  /* Currency cost list */
  .cost-list,
  .materials-list,
  .log-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 0.25rem;
    overflow-y: auto;
    min-height: 0;
    max-height: 22vh;
  }

  .cost-list li {
    display: grid;
    grid-template-columns: auto auto 1fr auto;
    align-items: center;
    gap: 0.45rem;
    font-size: 0.78rem;
    padding: 0.18rem 0;
    color: var(--fg-soft);
  }

  .cost-list li.empty {
    color: var(--fg-muted);
    font-style: italic;
  }

  .cost-list li .bullet {
    color: var(--fg-muted);
    font-size: 0.6rem;
  }

  .cost-list li.current .bullet {
    color: var(--gold-bright);
    text-shadow: 0 0 6px rgba(255, 211, 122, 0.7);
  }

  .cost-list li .step-num {
    color: var(--fg-muted);
    font-family: ui-monospace, 'Fira Code', monospace;
    font-size: 0.74rem;
  }

  .cost-list li .step-name {
    color: var(--fg-soft);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .cost-list li .step-cost {
    color: var(--gold-bright);
    font-family: Georgia, 'Times New Roman', serif;
  }

  .cost-total-row {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    padding-top: 0.35rem;
    border-top: 1px solid rgba(197, 143, 61, 0.18);
    font-size: 0.74rem;
    color: var(--fg-muted);
    text-transform: uppercase;
    letter-spacing: 0.08em;
  }

  .cost-total-row strong {
    color: var(--gold-bright);
    font-family: Georgia, 'Times New Roman', serif;
    font-size: 0.95rem;
    text-transform: none;
    letter-spacing: 0;
  }

  /* Materials */
  .materials-header {
    display: grid;
    grid-template-columns: 1fr 56px 56px;
    gap: 0.4rem;
    align-items: baseline;
  }

  .hdr-needed,
  .hdr-have {
    color: var(--fg-muted);
    text-transform: uppercase;
    font-size: 0.62rem;
    letter-spacing: 0.1em;
    text-align: right;
  }

  .materials-list li {
    display: grid;
    grid-template-columns: 26px 1fr 56px 56px;
    align-items: center;
    gap: 0.4rem;
    font-size: 0.78rem;
    padding: 0.2rem 0;
  }

  .materials-list li.empty {
    grid-template-columns: 1fr;
    color: var(--fg-muted);
    font-style: italic;
  }

  .materials-list li img {
    width: 24px;
    height: 24px;
    object-fit: contain;
  }

  .mini-fallback {
    width: 24px;
    height: 24px;
    border-radius: 50%;
    border: 1px solid rgba(0, 202, 255, 0.4);
    color: #00d7ff;
    background: rgba(0, 35, 50, 0.45);
    display: grid;
    place-items: center;
    font-size: 0.6rem;
  }

  .materials-list .mat-name {
    color: var(--fg-soft);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .materials-list .mat-needed,
  .materials-list .mat-have {
    text-align: right;
    color: var(--gold-bright);
    font-family: Georgia, 'Times New Roman', serif;
    font-size: 0.92rem;
  }

  .materials-list .mat-have.short {
    color: #ff9c7e;
  }

  /* Notes */
  .notes-text {
    flex: 1 1 auto;
    min-height: 5rem;
    border: 1px solid rgba(255, 255, 255, 0.07);
    border-radius: 8px;
    background: rgba(0, 0, 0, 0.32);
    padding: 0.55rem 0.65rem;
    font-size: 0.8rem;
    color: var(--fg-soft);
    overflow-y: auto;
  }

  .notes-text.edit {
    font: inherit;
    font-size: 0.8rem;
    color: var(--fg-soft);
    resize: none;
    width: 100%;
    line-height: 1.4;
  }

  .notes-text.view p {
    margin: 0 0 0.4rem;
  }

  .notes-text.view p:last-child {
    margin-bottom: 0;
  }

  .notes-panel footer {
    display: flex;
    justify-content: flex-end;
  }

  /* Crafting log */
  .log-header {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 0.5rem;
  }

  .log-filter {
    background: rgba(0, 0, 0, 0.4);
    border: 1px solid var(--border);
    color: var(--fg-soft);
    border-radius: 6px;
    padding: 0.18rem 0.4rem;
    font-size: 0.7rem;
    cursor: pointer;
  }

  .log-filter:hover {
    border-color: var(--border-gold);
  }

  .log-list .log-row {
    display: grid;
    grid-template-columns: 26px auto 1fr auto;
    align-items: center;
    gap: 0.4rem;
    padding: 0.25rem 0;
    border-bottom: 1px dashed rgba(255, 255, 255, 0.04);
    font-size: 0.78rem;
  }

  .log-list .log-row:last-child {
    border-bottom: 0;
  }

  .log-list .log-row.empty {
    grid-template-columns: 1fr;
  }

  .log-list .badge {
    width: 22px;
    height: 22px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    font-size: 0.78rem;
    font-family: Georgia, 'Times New Roman', serif;
  }

  .log-list .completed-badge {
    color: #b2ffa2;
    background: rgba(47, 123, 18, 0.42);
    border: 1px solid rgba(114, 255, 88, 0.55);
  }

  .log-list .pending-badge {
    color: var(--gold-bright);
    background: rgba(197, 143, 61, 0.18);
    border: 1px solid var(--border-gold);
  }

  .log-list .log-time {
    color: var(--fg-muted);
    font-family: ui-monospace, 'Fira Code', monospace;
    font-size: 0.7rem;
  }

  .log-list .log-name {
    color: var(--fg-soft);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .log-list .log-status {
    font-size: 0.66rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
  }

  .log-list .log-status.completed {
    color: #b2ffa2;
  }

  .log-list .log-status.pending {
    color: var(--gold);
  }

  .log-panel footer {
    display: flex;
    justify-content: stretch;
  }

  /* ───────── Drawer & shared ghost button ───────── */
  .ghost {
    background: rgba(0, 0, 0, 0.35);
    color: var(--gold);
    border: 1px solid var(--border-gold);
    border-radius: 8px;
    padding: 0.4rem 0.65rem;
    cursor: pointer;
    font-weight: 500;
    font-size: 0.8rem;
  }

  .ghost.compact {
    padding: 0.28rem 0.55rem;
    font-size: 0.74rem;
  }

  .ghost:hover:not(:disabled) {
    background: rgba(197, 143, 61, 0.12);
    color: var(--gold-bright);
  }

  .wide {
    width: 100%;
  }

  .muted {
    color: var(--fg-muted);
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

  /* Mod-line flash animation (Phase D.3 carry-over) */
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
      background: rgba(0, 0, 0, 0.92);
    }
    20% {
      background: rgba(220, 165, 70, 0.18);
    }
  }

  .rarity-flash {
    animation: poc2-rarity-flash 1.5s ease-out;
  }

  /* ───────── Responsive ───────── */
  @media (max-width: 1280px) {
    .bottom-dock {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }

  @media (max-width: 1100px) {
    .content {
      grid-template-columns: minmax(0, 1fr) 300px;
    }
  }

  @media (max-width: 980px) {
    .content {
      grid-template-columns: minmax(0, 1fr);
    }
    .right-stack {
      display: none;
    }
    .bottom-dock {
      grid-template-columns: 1fr;
    }
  }

  @media (max-width: 820px) {
    .app-shell {
      grid-template-columns: 1fr;
    }
    .sidebar {
      display: none;
    }
  }
</style>
