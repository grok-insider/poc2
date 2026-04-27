<script lang="ts">
  import { invoke } from './tauri';
  import { displayId } from './assets';
  import { checkCanApply, formatCannotApply } from './cannotApply';
  import type {
    AdvisorAction,
    CannotApplyView,
    EligibleModView,
    EligibleModsResponse,
    Item,
    RecordOutcome,
    RecordOutcomeResponse,
  } from './types';

  type Props = {
    item: Item;
    action: AdvisorAction | null;
    onApply: (item: Item, change: string, explanation: string) => void;
    onClose: () => void;
    /** Optional list of target-concept ids the goal cares about. Used by
     * the Phase C.3 "Pool view: target / non-target" filter. */
    targetConcepts?: string[];
  };

  let { item, action, onApply, onClose, targetConcepts = [] }: Props = $props();

  type Mode = 'add' | 'remove' | 'replace' | 'reveal' | 'none';

  /** Phase C.3 filter chips. */
  type RollSource = 'all' | 'currency' | 'essence' | 'desecrated' | 'vaal';
  type TierBand = 'all' | 't1' | 't1_t2' | 't1_t3';
  type PoolView = 'all' | 'target' | 'non_target';

  let busy = $state(false);
  let error = $state<string | null>(null);
  let response = $state<EligibleModsResponse | null>(null);
  let search = $state('');
  let conceptFilter = $state<string | null>(null);
  let pickedModId = $state<string | null>(null);
  let removeAffix = $state<'prefix' | 'suffix'>('prefix');
  let removeIndex = $state<number>(0);
  /** Per-stat normalized roll (0..1) keyed by stat_id. */
  let rolls = $state<Record<string, number>>({});

  /** Phase C.3 filter chips state. */
  let rollSource = $state<RollSource>('all');
  let tierBand = $state<TierBand>('all');
  let poolView = $state<PoolView>('all');
  /** Phase B.6 — selected omen for bone reveal (mirrors action.omen). */
  let selectedOmen = $state<string | null>(null);
  /** Phase A.2 — engine's structured cannot-apply verdict for the
   * current `(item, currency)` pair. Refreshed via IPC on each
   * action / item change so the badge text matches the engine. */
  let cannotApplyState = $state<CannotApplyView | null>(null);

  const mode = $derived(detectMode(action));
  const minRequiredLevel = $derived(minRequiredLevelForCurrency(currencyOf(action)));
  const eligibleAffix = $derived(determineAffix(action));
  const removeOptions = $derived(buildRemoveOptions(item));
  const isFracturing = $derived(currencyOf(action) === 'FracturingOrb');
  const concepts = $derived(uniqueConcepts(response?.mods ?? []));
  const filtered = $derived(
    filterMods(
      response?.mods ?? [],
      search,
      conceptFilter,
      rollSource,
      tierBand,
      poolView,
      targetConcepts,
    ),
  );
  const pickedMod = $derived(
    response?.mods.find((m) => m.mod_id === pickedModId) ?? null,
  );
  const cannotApplyReason = $derived(
    cannotApplyState ? formatCannotApply(cannotApplyState) : null,
  );

  // Phase C.3 — default the roll-source chip based on the action kind so
  // a Currency apply opens with Currency selected, an Essence apply
  // opens with Essence, etc.
  $effect(() => {
    const c = currencyOf(action);
    if (!c) {
      rollSource = 'all';
      return;
    }
    if (c.includes('EssenceOf')) rollSource = 'essence';
    else if (c === 'VaalOrb') rollSource = 'vaal';
    else if (action?.kind === 'reveal') rollSource = 'desecrated';
    else rollSource = 'currency';
  });

  // Phase B.6 — initial omen for the reveal action.
  $effect(() => {
    if (action?.kind === 'reveal') {
      selectedOmen = action.omen ?? null;
    }
  });

  // Phase A.2 — refresh the engine's structured CannotApply verdict
  // whenever the action or item changes. Falls back to `ok` when no
  // currency is selected (Stop / Guidance / Reveal-only paths).
  $effect(() => {
    const c = currencyOf(action);
    if (!c) {
      cannotApplyState = { kind: 'ok' };
      return;
    }
    const itemSnapshot = item;
    let cancelled = false;
    checkCanApply(itemSnapshot, c)
      .then((view) => {
        if (!cancelled) cannotApplyState = view;
      })
      .catch((e) => {
        // Surface the failure as a generic "other" reason so the badge
        // never silently disappears on IPC errors.
        if (!cancelled) {
          cannotApplyState = {
            kind: 'other',
            message: `precondition check failed: ${String(e)}`,
          };
        }
      });
    return () => {
      cancelled = true;
    };
  });

  $effect(() => {
    if (!pickedMod) return;
    // Default each stat to midpoint (0.5) when the user picks a different mod.
    // For Fracturing, default each stat to max (1.0) per UX recommendation.
    const next: Record<string, number> = {};
    const def = isFracturing ? 1.0 : 0.5;
    for (const s of pickedMod.stats) next[s.stat_id] = def;
    rolls = next;
  });

  $effect(() => {
    if (!action) {
      response = null;
      return;
    }
    if (mode === 'remove' || mode === 'none') {
      response = null;
      return;
    }
    busy = true;
    error = null;
    invoke<EligibleModsResponse>('eligible_mods', {
      args: {
        item,
        affix: eligibleAffix,
        min_required_level: minRequiredLevel,
      },
    })
      .then((r) => {
        response = r;
      })
      .catch((e) => {
        error = String(e);
      })
      .finally(() => {
        busy = false;
      });
  });

  function detectMode(a: AdvisorAction | null): Mode {
    if (!a) return 'none';
    if (a.kind === 'reveal') return 'reveal';
    if (a.kind === 'apply_currency') {
      const id = a.currency;
      if (
        id === 'OrbOfAnnulment' ||
        id === 'OrbOfTransmutation' ||
        id === 'GreaterOrbOfTransmutation' ||
        id === 'PerfectOrbOfTransmutation'
      ) {
        // Annul removes; transmute adds
        if (id === 'OrbOfAnnulment') return 'remove';
        return 'add';
      }
      if (
        id === 'OrbOfAugmentation' ||
        id === 'GreaterOrbOfAugmentation' ||
        id === 'PerfectOrbOfAugmentation' ||
        id === 'ExaltedOrb' ||
        id === 'GreaterExaltedOrb' ||
        id === 'PerfectExaltedOrb' ||
        id === 'RegalOrb' ||
        id === 'GreaterRegalOrb' ||
        id === 'PerfectRegalOrb'
      ) {
        return 'add';
      }
      if (
        id === 'ChaosOrb' ||
        id === 'GreaterChaosOrb' ||
        id === 'PerfectChaosOrb'
      ) {
        return 'replace';
      }
    }
    return 'add';
  }

  /** Phase C.5 — list of omens applicable to the current bone reveal.
   * Reads from a static catalogue of the omens that materially shift
   * the desecrated reveal pool. */
  function omensForReveal(): { id: string; label: string }[] {
    return [
      { id: 'OmenOfSinistralNecromancy', label: 'Sinistral Necromancy (force prefix)' },
      { id: 'OmenOfDextralNecromancy', label: 'Dextral Necromancy (force suffix)' },
      { id: 'OmenOfTheBlackblooded', label: 'Blackblooded (Amanamu pool)' },
      { id: 'OmenOfTheLiege', label: 'Liege (Kurgal pool)' },
      { id: 'OmenOfTheSovereign', label: 'Sovereign (Ulaman pool)' },
      { id: 'OmenOfEchoesOfTheAbyss', label: 'Echoes of the Abyss (double reveal)' },
    ];
  }

  function determineAffix(a: AdvisorAction | null): 'prefix' | 'suffix' | 'either' {
    if (!a || a.kind !== 'apply_currency') return 'either';
    const omens = a.omens.map((o) => o.toLowerCase());
    if (omens.some((o) => o.includes('sinistral'))) return 'prefix';
    if (omens.some((o) => o.includes('dextral'))) return 'suffix';
    return 'either';
  }

  function currencyOf(a: AdvisorAction | null): string | null {
    return a?.kind === 'apply_currency' ? a.currency : null;
  }

  function minRequiredLevelForCurrency(currency: string | null): number {
    if (!currency) return 0;
    if (currency.startsWith('Perfect')) return 70;
    if (currency.startsWith('Greater')) return 35;
    return 0;
  }

  function filterMods(
    mods: EligibleModView[],
    term: string,
    concept: string | null,
    source: RollSource,
    tier: TierBand,
    pool: PoolView,
    targets: string[],
  ) {
    const t = term.trim().toLowerCase();
    return mods.filter((m) => {
      // Concept dropdown filter (legacy).
      if (concept && !m.concepts.includes(concept)) return false;
      // Phase C.3 — roll-source chip filter.
      if (source === 'currency') {
        if (m.kind !== 'explicit' || m.is_essence_only || m.is_desecrated_only) {
          return false;
        }
      } else if (source === 'essence') {
        if (!m.is_essence_only) return false;
      } else if (source === 'desecrated') {
        if (m.kind !== 'desecrated' && !m.is_desecrated_only) return false;
      } else if (source === 'vaal') {
        if (m.kind !== 'corrupted') return false;
      }
      // Phase C.3 — tier band chip filter.
      if (tier === 't1' && m.tier_index !== 1) return false;
      if (tier === 't1_t2' && m.tier_index > 2) return false;
      if (tier === 't1_t3' && m.tier_index > 3) return false;
      // Phase C.3 — pool view (target / non-target).
      if (pool !== 'all' && targets.length > 0) {
        const hits = m.concepts.some((c) => targets.includes(c));
        if (pool === 'target' && !hits) return false;
        if (pool === 'non_target' && hits) return false;
      }
      // Search filter (legacy).
      if (!t) return true;
      return (
        m.mod_id.toLowerCase().includes(t) ||
        (m.name?.toLowerCase().includes(t) ?? false) ||
        (m.text_template?.toLowerCase().includes(t) ?? false)
      );
    });
  }

  function uniqueConcepts(mods: EligibleModView[]): string[] {
    const set = new Set<string>();
    for (const m of mods) for (const c of m.concepts) set.add(c);
    return [...set].sort();
  }

  function buildRemoveOptions(it: Item) {
    const out: { affix: 'prefix' | 'suffix'; index: number; mod_id: string }[] = [];
    it.prefixes.forEach((m, i) => out.push({ affix: 'prefix', index: i, mod_id: m.mod_id }));
    it.suffixes.forEach((m, i) => out.push({ affix: 'suffix', index: i, mod_id: m.mod_id }));
    return out;
  }

  async function confirm() {
    if (!action) return;
    busy = true;
    error = null;
    try {
      let outcome: RecordOutcome;
      const rollAvg = averageRoll(rolls);
      if (mode === 'add') {
        if (!pickedModId) {
          error = 'Pick a modifier first.';
          busy = false;
          return;
        }
        outcome = {
          kind: 'add_mod',
          mod_id: pickedModId,
          roll: rollAvg,
          currency: currencyOf(action) ?? undefined,
        };
      } else if (mode === 'remove') {
        outcome = { kind: 'remove_mod', affix: removeAffix, index: removeIndex };
      } else if (mode === 'replace') {
        if (!pickedModId) {
          error = 'Pick a replacement modifier first.';
          busy = false;
          return;
        }
        outcome = {
          kind: 'replace_mod',
          remove_affix: removeAffix,
          remove_index: removeIndex,
          add_mod_id: pickedModId,
          roll: rollAvg,
        };
      } else {
        onClose();
        return;
      }
      const r = await invoke<RecordOutcomeResponse>('record_outcome', {
        args: { item, outcome },
      });
      onApply(r.item, r.change, r.explanation);
      onClose();
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  function averageRoll(map: Record<string, number>): number | undefined {
    const vals = Object.values(map);
    if (vals.length === 0) return undefined;
    return vals.reduce((a, b) => a + b, 0) / vals.length;
  }

  function setAllStatsToMax() {
    if (!pickedMod) return;
    const next: Record<string, number> = {};
    for (const s of pickedMod.stats) next[s.stat_id] = 1.0;
    rolls = next;
  }

  function statRolledValue(statId: string, min: number, max: number): number {
    const t = rolls[statId] ?? 0.5;
    return min + t * (max - min);
  }

  function actionTitle(a: AdvisorAction | null): string {
    if (!a) return '';
    if (a.kind === 'apply_currency') return displayId(a.currency);
    if (a.kind === 'activate_omen') return `Activate ${displayId(a.omen)}`;
    if (a.kind === 'apply_hinekoras_lock') return "Hinekora's Lock";
    if (a.kind === 'reveal') return 'Reveal at Well of Souls';
    if (a.kind === 'recombine') return 'Recombine';
    return a.kind;
  }
</script>

{#if action}
  <button class="scrim" type="button" aria-label="Close" onclick={onClose}></button>
  <div class="dialog" role="dialog" aria-modal="true" aria-label="Record outcome">
    <header>
      <div>
        <span class="kicker">Record Outcome</span>
        <h2>{actionTitle(action)}</h2>
      </div>
      <button class="ghost compact" onclick={onClose}>Close</button>
    </header>

    <div class="body">
      {#if error}
        <p class="error">{error}</p>
      {/if}

      <!-- Phase C.4 — Action header surfacing item state + preconditions. -->
      <div class="action-header">
        <div class="header-row">
          <span class="header-label">Item:</span>
          <span class="header-value">
            {item.rarity}
            · ilvl {item.ilvl}
            · {item.prefixes.length}/3 prefixes
            · {item.suffixes.length}/3 suffixes
            {#if item.prefixes.concat(item.suffixes).some((m) => m.is_fractured)}
              · <span class="frac-marker">fractured</span>
            {/if}
            {#if item.corrupted}
              · <span class="corr-marker">corrupted</span>
            {/if}
          </span>
        </div>
        {#if minRequiredLevel > 0}
          <div class="header-row">
            <span class="header-label">Currency floor:</span>
            <span class="header-value">required level ≥ {minRequiredLevel}</span>
          </div>
        {/if}
        {#if cannotApplyReason}
          <p class="cannot-apply">
            Cannot apply: {cannotApplyReason}
          </p>
        {/if}
      </div>

      {#if mode === 'none'}
        <p class="muted">This action does not require a manual outcome.</p>
      {:else if mode === 'reveal'}
        <!-- Phase C.5 — Bone reveal sub-control: pick which omen the
             user pre-bound to this reveal. The picker updates the
             cost band (not implemented in v1 mock) and the reveal
             pool's affix bias. -->
        <div class="block">
          <span class="block-title">Bone Reveal</span>
          <p class="hint">
            Pick the omen you applied (if any). The reveal pool reflects
            the omen's bias.
          </p>
          <div class="omen-picker">
            <label>
              <input
                type="radio"
                name="omen"
                checked={selectedOmen === null}
                onchange={() => (selectedOmen = null)}
              />
              <span>No omen — unconditioned reveal</span>
            </label>
            {#each omensForReveal() as o (o.id)}
              <label>
                <input
                  type="radio"
                  name="omen"
                  checked={selectedOmen === o.id}
                  onchange={() => (selectedOmen = o.id)}
                />
                <span>{o.label}</span>
              </label>
            {/each}
          </div>
        </div>
        <p class="muted">
          The reveal applies a desecrated mod to the hidden slot. After
          confirming the omen choice, click the visible affix on the
          item preview to record what was revealed.
        </p>
      {:else if mode === 'remove'}
        <p class="hint">Pick which modifier was removed.</p>
        {#if removeOptions.length === 0}
          <p class="muted">Item has no removable mods.</p>
        {:else}
          <ul class="remove-list">
            {#each removeOptions as opt, i (i)}
              <li>
                <label>
                  <input
                    type="radio"
                    name="remove"
                    checked={removeAffix === opt.affix && removeIndex === opt.index}
                    onchange={() => {
                      removeAffix = opt.affix;
                      removeIndex = opt.index;
                    }}
                  />
                  <span class="rm-affix">{opt.affix}</span>
                  <span class="rm-mod">{opt.mod_id}</span>
                </label>
              </li>
            {/each}
          </ul>
        {/if}
      {:else}
        {#if mode === 'replace'}
          <div class="block">
            <span class="block-title">Removed mod</span>
            {#if removeOptions.length === 0}
              <p class="muted">Item has no removable mods.</p>
            {:else}
              <select
                bind:value={removeAffix}
                onchange={() => (removeIndex = 0)}
              >
                <option value="prefix">prefix</option>
                <option value="suffix">suffix</option>
              </select>
              <select bind:value={removeIndex}>
                {#each removeOptions.filter((o) => o.affix === removeAffix) as opt, i (i)}
                  <option value={opt.index}>{opt.mod_id}</option>
                {/each}
              </select>
            {/if}
          </div>
        {/if}

        <!-- Phase C.3 — filter chips: roll-source, tier band, pool view. -->
        <div class="chip-row">
          <span class="chip-label">Source:</span>
          {#each [
            { v: 'all', label: 'All' },
            { v: 'currency', label: 'Currency' },
            { v: 'essence', label: 'Essence' },
            { v: 'desecrated', label: 'Desecrated' },
            { v: 'vaal', label: 'Vaal' },
          ] as opt (opt.v)}
            <button
              type="button"
              class="chip"
              class:active={rollSource === opt.v}
              onclick={() => (rollSource = opt.v as RollSource)}
            >
              {opt.label}
            </button>
          {/each}
        </div>
        <div class="chip-row">
          <span class="chip-label">Tier:</span>
          {#each [
            { v: 'all', label: 'All' },
            { v: 't1', label: 'T1' },
            { v: 't1_t2', label: 'T1–T2' },
            { v: 't1_t3', label: 'T1–T3' },
          ] as opt (opt.v)}
            <button
              type="button"
              class="chip"
              class:active={tierBand === opt.v}
              onclick={() => (tierBand = opt.v as TierBand)}
            >
              {opt.label}
            </button>
          {/each}
        </div>
        {#if targetConcepts.length > 0}
          <div class="chip-row">
            <span class="chip-label">Pool:</span>
            {#each [
              { v: 'all', label: 'All' },
              { v: 'target', label: 'Target only' },
              { v: 'non_target', label: 'Non-target' },
            ] as opt (opt.v)}
              <button
                type="button"
                class="chip"
                class:active={poolView === opt.v}
                onclick={() => (poolView = opt.v as PoolView)}
              >
                {opt.label}
              </button>
            {/each}
          </div>
        {/if}

        <div class="filters">
          <input
            type="search"
            placeholder="Search mods, concepts, text…"
            bind:value={search}
          />
          <select bind:value={conceptFilter}>
            <option value={null}>All concepts</option>
            {#each concepts as c (c)}
              <option value={c}>{c}</option>
            {/each}
          </select>
        </div>

        {#if busy}
          <p class="muted">Loading eligible mods…</p>
        {:else if !response}
          <p class="muted">No data yet.</p>
        {:else if !response.data_available}
          <p class="muted">
            No mod data bundled for {response.item_class}. The advisor can
            still recommend steps, but per-mod outcome recording isn't
            available for this item class yet.
          </p>
        {:else}
          <p class="meta">
            {response.mods.length} mods for {response.item_class} ·
            affix scope: {response.affix} ·
            {minRequiredLevel > 0
              ? `currency floor: required level ≥ ${minRequiredLevel}`
              : 'no level floor'}
          </p>
          <ul class="mod-list">
            {#each filtered as m (m.mod_id)}
              <li
                class:eligible={m.eligible_now}
                class:blocked={!m.eligible_now}
                class:picked={m.mod_id === pickedModId}
              >
                <button
                  type="button"
                  disabled={!m.eligible_now}
                  onclick={() => (pickedModId = m.mod_id)}
                >
                  <header>
                    <span class="tier">T{m.tier_index}/{m.tier_count}</span>
                    <span class="mod-name">
                      {m.name ?? m.mod_id}
                      {#if m.is_hybrid}<em>hybrid</em>{/if}
                      {#if m.is_essence_only}<em class="warn">essence</em>{/if}
                      {#if m.is_desecrated_only}<em class="warn">desecrated</em>{/if}
                    </span>
                    <span class="affix">{m.affix_type}</span>
                    <span class="ilvl">ilvl {m.required_level}</span>
                  </header>
                  {#if m.text_template}
                    <p class="tpl">{m.text_template}</p>
                  {/if}
                  <footer>
                    <span class="concepts">
                      {#each m.concepts as c (c)}<span class="concept">{c}</span>{/each}
                    </span>
                    {#if m.eligible_now}
                      <span class="weight">weight share {(m.weight_share * 100).toFixed(1)}%</span>
                    {:else if m.blocked_by_min_level}
                      <span class="reason">blocked by currency tier (need req &lt; {minRequiredLevel})</span>
                    {:else if m.blocked_by_group}
                      <span class="reason">group already on item</span>
                    {:else}
                      <span class="reason">ilvl too low</span>
                    {/if}
                  </footer>
                </button>
              </li>
            {/each}
            {#if filtered.length === 0}
              <li class="muted empty">No mods match the filters.</li>
            {/if}
          </ul>
          {#if pickedMod}
            <div class="rolls">
              <header>
                <span>Roll values · {pickedMod.name ?? pickedMod.mod_id}</span>
                {#if isFracturing}
                  <button type="button" class="warn-btn" onclick={setAllStatsToMax}>
                    Set all stats to max
                  </button>
                {/if}
              </header>
              {#if isFracturing}
                <p class="frac-hint">
                  Fracturing locks the chosen mod permanently. Aim for the
                  highest tier first; only fracture once your keeper has
                  rolled max range.
                </p>
              {/if}
              {#each pickedMod.stats as s, i (i)}
                <label class="stat-slider">
                  <span class="stat-label">{s.stat_id}</span>
                  <input
                    type="range"
                    min="0"
                    max="1"
                    step="0.01"
                    value={rolls[s.stat_id] ?? 0.5}
                    oninput={(e) => {
                      const v = Number((e.currentTarget as HTMLInputElement).value);
                      rolls = { ...rolls, [s.stat_id]: v };
                    }}
                  />
                  <span class="stat-value">
                    {statRolledValue(s.stat_id, s.min, s.max).toFixed(1)}
                    <em>/ {s.min.toFixed(0)}–{s.max.toFixed(0)}</em>
                  </span>
                </label>
              {/each}
            </div>
          {/if}
        {/if}
      {/if}
    </div>

    <footer class="actions">
      <button class="ghost" onclick={onClose}>Cancel</button>
      <button class="primary" onclick={confirm} disabled={busy}>
        {busy ? 'Saving…' : 'Apply outcome'}
      </button>
    </footer>
  </div>
{/if}

<style>
  .scrim {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.6);
    border: 0;
    cursor: pointer;
    z-index: 50;
  }

  .dialog {
    position: fixed;
    top: 5vh;
    left: 50%;
    transform: translateX(-50%);
    width: min(720px, 92vw);
    max-height: 90vh;
    display: grid;
    grid-template-rows: auto 1fr auto;
    background: linear-gradient(180deg, rgba(15, 19, 22, 0.98), rgba(5, 8, 11, 0.98));
    border: 1px solid var(--border-strong);
    border-radius: 6px;
    z-index: 51;
    box-shadow: 0 30px 80px rgba(0, 0, 0, 0.7);
    overflow: hidden;
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.75rem 0.85rem;
    border-bottom: 1px solid var(--border-strong);
    background: rgba(20, 13, 4, 0.85);
  }

  header h2 {
    margin: 0.1rem 0 0;
    font-family: Georgia, 'Times New Roman', serif;
    color: var(--gold-bright);
    letter-spacing: 0.04em;
    font-size: 1.1rem;
  }

  .kicker {
    color: var(--gold);
    text-transform: uppercase;
    letter-spacing: 0.1em;
    font-family: Georgia, 'Times New Roman', serif;
    font-size: 0.75rem;
  }

  .body {
    overflow-y: auto;
    padding: 0.75rem 0.85rem;
    display: grid;
    gap: 0.55rem;
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 0.45rem;
    padding: 0.7rem 0.85rem;
    border-top: 1px solid var(--border-strong);
    background: rgba(0, 0, 0, 0.4);
  }

  .ghost {
    background: rgba(0, 0, 0, 0.35);
    color: var(--gold);
    border: 1px solid var(--border-gold);
    border-radius: 4px;
    padding: 0.4rem 0.7rem;
    cursor: pointer;
    font-size: 0.82rem;
  }

  .ghost.compact {
    padding: 0.3rem 0.55rem;
    font-size: 0.78rem;
  }

  .primary {
    background: linear-gradient(180deg, rgba(220, 165, 70, 0.95), rgba(150, 105, 30, 0.95));
    color: #1a1100;
    border: 1px solid rgba(255, 211, 122, 0.85);
    border-radius: 4px;
    padding: 0.45rem 0.85rem;
    font-weight: 700;
    cursor: pointer;
    font-size: 0.82rem;
  }

  .primary:disabled {
    opacity: 0.5;
    cursor: progress;
  }

  .filters {
    display: flex;
    gap: 0.45rem;
    flex-wrap: wrap;
  }

  .filters input,
  .filters select,
  .block select {
    background: rgba(0, 0, 0, 0.5);
    color: var(--fg);
    border: 1px solid var(--border-strong);
    border-radius: 4px;
    padding: 0.35rem 0.55rem;
    font-size: 0.85rem;
  }

  .filters input {
    flex: 1 1 220px;
  }

  .block {
    display: grid;
    gap: 0.3rem;
  }

  .block-title {
    color: var(--gold);
    text-transform: uppercase;
    letter-spacing: 0.1em;
    font-family: Georgia, 'Times New Roman', serif;
    font-size: 0.75rem;
  }

  .meta {
    margin: 0;
    color: var(--fg-muted);
    font-size: 0.8rem;
  }

  .mod-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
  }

  .mod-list li button {
    width: 100%;
    text-align: left;
    background: linear-gradient(180deg, rgba(15, 19, 22, 0.94), rgba(5, 8, 11, 0.94));
    border: 1px solid rgba(197, 143, 61, 0.3);
    border-radius: 4px;
    padding: 0.55rem 0.7rem;
    color: var(--fg);
    cursor: pointer;
    display: grid;
    gap: 0.3rem;
  }

  .mod-list li.blocked button {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .mod-list li.picked button {
    border-color: rgba(114, 255, 88, 0.7);
    box-shadow: inset 0 0 0 1px rgba(114, 255, 88, 0.3);
    background: linear-gradient(180deg, rgba(20, 35, 18, 0.94), rgba(5, 12, 8, 0.94));
  }

  .mod-list li button header {
    display: flex;
    align-items: baseline;
    gap: 0.55rem;
    background: transparent;
    border: 0;
    padding: 0;
    flex-wrap: wrap;
  }

  .tier {
    font-family: Georgia, 'Times New Roman', serif;
    color: var(--gold-bright);
    font-size: 0.85rem;
  }

  .mod-name {
    color: var(--fg);
    font-weight: 600;
  }

  .mod-name em {
    margin-left: 0.4rem;
    font-style: normal;
    font-size: 0.7rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: #00c8ff;
  }

  .mod-name em.warn {
    color: #ffb96b;
  }

  .affix {
    color: var(--fg-muted);
    text-transform: uppercase;
    font-size: 0.7rem;
    letter-spacing: 0.08em;
  }

  .ilvl {
    color: var(--fg-muted);
    font-family: ui-monospace, 'Fira Code', monospace;
    font-size: 0.78rem;
  }

  .tpl {
    margin: 0;
    color: #00c8ff;
    font-size: 0.82rem;
    white-space: pre-wrap;
  }

  .mod-list li button footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.4rem;
    flex-wrap: wrap;
    background: transparent;
    border: 0;
    padding: 0;
  }

  .concepts {
    display: flex;
    gap: 0.3rem;
    flex-wrap: wrap;
  }

  .concept {
    color: #a98dff;
    font-size: 0.7rem;
    border: 1px solid rgba(169, 141, 255, 0.45);
    background: rgba(40, 25, 70, 0.25);
    border-radius: 999px;
    padding: 0.05rem 0.4rem;
  }

  .weight {
    color: #72ff58;
    font-size: 0.75rem;
    font-family: ui-monospace, 'Fira Code', monospace;
  }

  .reason {
    color: #ffb96b;
    font-size: 0.75rem;
  }

  .remove-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
  }

  .remove-list label {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    border: 1px solid var(--border-strong);
    border-radius: 4px;
    padding: 0.45rem 0.6rem;
    background: rgba(0, 0, 0, 0.3);
  }

  .rm-affix {
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--fg-muted);
    font-size: 0.7rem;
  }

  .rm-mod {
    color: var(--fg);
  }

  .empty {
    text-align: center;
    padding: 1rem;
    border: 1px dashed var(--border-strong);
    border-radius: 4px;
  }

  .hint {
    color: var(--fg-muted);
    margin: 0;
  }

  .muted {
    color: var(--fg-muted);
    margin: 0;
  }

  .error {
    background: #2a1010;
    border: 1px solid #5a2222;
    color: #ff8c8c;
    border-radius: 4px;
    padding: 0.45rem 0.6rem;
    margin: 0;
  }

  .rolls {
    border: 1px solid var(--border-strong);
    border-radius: 4px;
    background: rgba(0, 0, 0, 0.35);
    padding: 0.6rem 0.7rem;
    display: grid;
    gap: 0.45rem;
  }

  .rolls header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    background: transparent;
    border: 0;
    padding: 0;
  }

  .rolls header span {
    color: var(--gold);
    text-transform: uppercase;
    letter-spacing: 0.08em;
    font-size: 0.78rem;
    font-family: Georgia, 'Times New Roman', serif;
  }

  .warn-btn {
    background: linear-gradient(180deg, rgba(220, 165, 70, 0.95), rgba(150, 105, 30, 0.95));
    color: #1a1100;
    border: 1px solid rgba(255, 211, 122, 0.85);
    border-radius: 999px;
    padding: 0.25rem 0.6rem;
    cursor: pointer;
    font-size: 0.74rem;
    font-weight: 700;
  }

  .frac-hint {
    margin: 0;
    color: #ffb96b;
    font-size: 0.78rem;
  }

  .stat-slider {
    display: grid;
    grid-template-columns: 1fr 2fr 0.8fr;
    align-items: center;
    gap: 0.6rem;
    font-size: 0.78rem;
  }

  .stat-label {
    color: var(--fg-soft);
    font-family: ui-monospace, 'Fira Code', monospace;
  }

  .stat-value {
    color: var(--gold-bright);
    font-family: ui-monospace, 'Fira Code', monospace;
    text-align: right;
  }

  .stat-value em {
    font-style: normal;
    color: var(--fg-muted);
    margin-left: 0.3rem;
  }

  input[type='range'] {
    accent-color: var(--gold);
  }

  /* Phase C.4 — action header */
  .action-header {
    border: 1px solid var(--border-strong);
    background: rgba(20, 13, 4, 0.4);
    border-radius: 4px;
    padding: 0.45rem 0.6rem;
    display: grid;
    gap: 0.25rem;
  }

  .header-row {
    display: flex;
    gap: 0.4rem;
    align-items: baseline;
    font-size: 0.78rem;
    flex-wrap: wrap;
  }

  .header-label {
    color: var(--gold);
    text-transform: uppercase;
    letter-spacing: 0.08em;
    font-family: Georgia, 'Times New Roman', serif;
    font-size: 0.7rem;
  }

  .header-value {
    color: var(--fg-soft);
    font-family: ui-monospace, 'Fira Code', monospace;
  }

  .frac-marker {
    color: #ffb96b;
  }

  .corr-marker {
    color: #ff5c5c;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    font-size: 0.7rem;
  }

  .cannot-apply {
    margin: 0.3rem 0 0;
    border: 1px solid #5a2222;
    background: #2a1010;
    color: #ff8c8c;
    border-radius: 4px;
    padding: 0.4rem 0.55rem;
    font-size: 0.78rem;
  }

  /* Phase C.3 — filter chip rows */
  .chip-row {
    display: flex;
    align-items: center;
    gap: 0.3rem;
    flex-wrap: wrap;
  }

  .chip-label {
    color: var(--gold);
    text-transform: uppercase;
    letter-spacing: 0.08em;
    font-family: Georgia, 'Times New Roman', serif;
    font-size: 0.7rem;
    margin-right: 0.2rem;
  }

  .chip {
    background: rgba(0, 0, 0, 0.4);
    color: var(--fg-soft);
    border: 1px solid var(--border-strong);
    border-radius: 999px;
    padding: 0.2rem 0.65rem;
    font-size: 0.74rem;
    cursor: pointer;
    transition: background 0.1s, color 0.1s, border-color 0.1s;
  }

  .chip:hover {
    border-color: var(--border-gold);
    color: var(--fg);
  }

  .chip.active {
    background: linear-gradient(180deg, rgba(220, 165, 70, 0.85), rgba(150, 105, 30, 0.85));
    color: #1a1100;
    border-color: rgba(255, 211, 122, 0.85);
    font-weight: 600;
  }

  /* Phase C.5 — bone reveal omen picker */
  .omen-picker {
    display: grid;
    gap: 0.25rem;
  }

  .omen-picker label {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    border: 1px solid var(--border-strong);
    border-radius: 4px;
    padding: 0.35rem 0.55rem;
    background: rgba(0, 0, 0, 0.3);
    font-size: 0.78rem;
    cursor: pointer;
  }
</style>
