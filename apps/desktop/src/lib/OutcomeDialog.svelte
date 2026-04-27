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
    RerollableMod,
    RerollableModsResponse,
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

  type Mode = 'add' | 'remove' | 'replace' | 'reveal' | 'reroll' | 'none';

  /** Phase C.3 filter chips. */
  type RollSource = 'all' | 'currency' | 'essence' | 'desecrated' | 'vaal';
  type PoolView = 'all' | 'target' | 'non_target';
  type TierFilter = 'all' | 't1' | 't2' | 't3' | 't4plus';
  type SortMode = 'best' | 'weight' | 'tier' | 'name';

  /** A mod-group bucket for the accordion-style tier picker.
   * Each group is a single in-game stat-line identity (e.g. "+#% to Fire
   * Resistance"); the player picks the group first, then the specific
   * tier they actually rolled in-game. */
  type ModGroupBucket = {
    mod_group: string;
    /** Stat-line label rendered as the row title. Sourced from the
     * highest-tier (T1) member's `text_template` so the value range
     * placeholder reads as a category, not a specific number. */
    label: string;
    affix_type: 'prefix' | 'suffix' | 'either';
    is_hybrid: boolean;
    /** Tiers visible under the active source/search/pool/concept filters.
     * Ineligible rows stay visible so the user can see why a tier cannot
     * be recorded for this currency/item state. */
    visible_tiers: EligibleModView[];
    /** Total tiers existing in this group (eligible or not). Used to
     * render `T_/{tier_count}` consistently. */
    tier_count: number;
    /** Sum of weight share across the eligible tiers. */
    weight_share: number;
    eligible_count: number;
    concepts: string[];
  };

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
  let poolView = $state<PoolView>('all');
  let tierFilter = $state<TierFilter>('all');
  let sortMode = $state<SortMode>('best');
  /** Currently expanded mod-group in the accordion picker (mod_group id
   * or `null` for fully collapsed). At most one expanded at a time. */
  let expandedGroup = $state<string | null>(null);

  /** Phase D.6 — Divine Orb reroll mode state.
   *
   * `rerollResponse` is the backend's snapshot of which slots can be
   * rerolled and the bounds they must respect. `rerollEdits` carries the
   * user's edited values, keyed by `${slot}:${index}:${statIdx}`; only
   * mods the user actually edited are sent in the outcome. */
  let rerollResponse = $state<RerollableModsResponse | null>(null);
  let rerollEdits = $state<Record<string, number>>({});
  /** Phase B.6 — selected omen for bone reveal (mirrors action.omen). */
  let selectedOmen = $state<string | null>(null);
  /** Phase A.2 — engine's structured cannot-apply verdict for the
   * current `(item, currency)` pair. Refreshed via IPC on each
   * action / item change so the badge text matches the engine. */
  let cannotApplyState = $state<CannotApplyView | null>(null);
  let selectionResetKey = $state('');

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
      poolView,
      tierFilter,
      targetConcepts,
    ),
  );
  /** Mod-group buckets derived from the filtered set. Sorted by
   * descending eligible weight share so the most-likely-to-roll groups
   * surface first. */
  const groupedMods = $derived(buildGroupedMods(filtered));
  const pickedMod = $derived(
    response?.mods.find((m) => m.mod_id === pickedModId) ?? null,
  );
  const pickedGroup = $derived(
    groupedMods.find((g) => g.visible_tiers.some((t) => t.mod_id === pickedModId)) ?? null,
  );
  const activeGroup = $derived(
    pickedGroup ?? groupedMods.find((g) => g.mod_group === expandedGroup) ?? groupedMods[0] ?? null,
  );
  const visibleTierCount = $derived(filtered.length);
  const eligibleTierCount = $derived(filtered.filter((m) => m.eligible_now).length);
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
    const key = `${actionSignature(action)}::${JSON.stringify(item)}`;
    if (key === selectionResetKey) return;
    selectionResetKey = key;
    pickedModId = null;
    rolls = {};
    rerollEdits = {};
    response = null;
    rerollResponse = null;
    search = '';
    conceptFilter = null;
    poolView = 'all';
    tierFilter = 'all';
    sortMode = 'best';
    expandedGroup = null;
    error = null;
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

  // Annul auto-skip — when there is exactly one removable mod (e.g. annul on
  // a magic item with a single suffix), pre-select that option so the user
  // can confirm without the radio-list ceremony. This also fixes the latent
  // bug where the defaults `'prefix'/0` are stale when the only mod is a
  // suffix and the radio list renders. Re-runs whenever the item changes.
  $effect(() => {
    if (mode !== 'remove' && mode !== 'replace') return;
    const opts = removeOptions;
    if (opts.length === 0) return;
    const validForCurrent = opts.some(
      (o) => o.affix === removeAffix && o.index === removeIndex,
    );
    if (!validForCurrent) {
      removeAffix = opts[0].affix;
      removeIndex = opts[0].index;
    }
  });

  // Mod-group accordion — collapse the expanded group when a filter change
  // hides it, and auto-expand the group that contains the currently-picked
  // tier (so the user always sees the selection in context).
  $effect(() => {
    if (groupedMods.length === 0) {
      expandedGroup = null;
      return;
    }
    if (pickedModId) {
      const owner = groupedMods.find((g) =>
        g.visible_tiers.some((t) => t.mod_id === pickedModId),
      );
      if (owner && expandedGroup !== owner.mod_group) {
        expandedGroup = owner.mod_group;
        return;
      }
    }
    if (expandedGroup && !groupedMods.some((g) => g.mod_group === expandedGroup)) {
      expandedGroup = null;
    }
  });

  $effect(() => {
    if (!action) {
      response = null;
      return;
    }
    if (mode === 'remove' || mode === 'none' || mode === 'reroll') {
      response = null;
      return;
    }
    let cancelled = false;
    busy = true;
    error = null;
    const itemSnapshot = item;
    invoke<EligibleModsResponse>('eligible_mods', {
      args: {
        item: itemSnapshot,
        affix: eligibleAffix,
        min_required_level: minRequiredLevel,
      },
    })
      .then((r) => {
        if (!cancelled) response = r;
      })
      .catch((e) => {
        if (!cancelled) error = String(e);
      })
      .finally(() => {
        if (!cancelled) busy = false;
      });
    return () => {
      cancelled = true;
    };
  });

  // Phase D.6 — load the Divine reroll snapshot whenever we enter
  // 'reroll' mode (or the item / omen changes). The backend computes
  // tier numbers and value bounds (widened for sanctification) so the
  // dialog only has to render and validate sliders.
  $effect(() => {
    if (mode !== 'reroll') {
      rerollResponse = null;
      rerollEdits = {};
      return;
    }
    let cancelled = false;
    busy = true;
    error = null;
    const omen = divineOmenOf(action);
    const itemSnapshot = item;
    invoke<RerollableModsResponse>('rerollable_mods', {
      args: { item: itemSnapshot, omen },
    })
      .then((r) => {
        if (cancelled) return;
        rerollResponse = r;
        // Pre-fill edits with current values so untouched stats round-trip
        // unchanged through the absolute-value check.
        const next: Record<string, number> = {};
        for (const m of r.mods) {
          for (let i = 0; i < m.stats.length; i += 1) {
            next[`${m.slot}:${m.index}:${i}`] = m.stats[i].current;
          }
        }
        rerollEdits = next;
      })
      .catch((e) => {
        if (!cancelled) error = String(e);
      })
      .finally(() => {
        if (!cancelled) busy = false;
      });
    return () => {
      cancelled = true;
    };
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
      // Divine Orb (and omen variants) reroll values within current
      // tier ranges — neither adds nor removes mods. Branches into a
      // dedicated dialog mode that lists existing slots with bounded
      // numeric inputs.
      if (id === 'DivineOrb') return 'reroll';
      if (id.includes('Essence')) return 'add';
      return 'none';
    }
    return 'none';
  }

  /** Pick the active Divine omen, if any, from the action's omens list.
   * Recognised: `OmenOfTheBlessed` (implicits-only), `OmenOfSanctification`
   * (widened bounds + locks item). Returns `null` for plain Divine. */
  function divineOmenOf(a: AdvisorAction | null): string | null {
    if (!a || a.kind !== 'apply_currency') return null;
    const blessed = a.omens.find((o) => o === 'OmenOfTheBlessed');
    if (blessed) return blessed;
    const sanct = a.omens.find((o) => o === 'OmenOfSanctification');
    if (sanct) return sanct;
    return null;
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

  function actionSignature(a: AdvisorAction | null): string {
    return JSON.stringify(a ?? null);
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
    pool: PoolView,
    tier: TierFilter,
    targets: string[],
  ) {
    const t = term.trim().toLowerCase();
    return mods.filter((m) => {
      // Concept dropdown filter (legacy).
      if (concept && !m.concepts.includes(concept)) return false;
      if (tier === 't1' && m.tier_index !== 1) return false;
      if (tier === 't2' && m.tier_index !== 2) return false;
      if (tier === 't3' && m.tier_index !== 3) return false;
      if (tier === 't4plus' && m.tier_index < 4) return false;
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
        (m.text_template?.toLowerCase().includes(t) ?? false) ||
        m.concepts.some((c) => c.toLowerCase().includes(t)) ||
        m.tags.some((tag) => tag.toLowerCase().includes(t))
      );
    });
  }

  /** Bucket the visible mods by `mod_group` for the accordion picker.
   *
   * Within each group:
   * - `visible_tiers` keeps every row that matches the filters, including
   *   ineligible rows, because the dialog doubles as an explainable pool
   *   browser. Only `eligible_now` tiers are selectable.
   * - `label` is taken from the highest-tier (T1) member's
   *   `text_template` so the row reads as a category, e.g.
   *   `+#% to Fire Resistance`.
   * - `weight_share` sums the eligible tiers' shares.
   *
   * Groups are sorted by the current sort dropdown. */
  function buildGroupedMods(mods: EligibleModView[]): ModGroupBucket[] {
    const buckets = new Map<string, ModGroupBucket>();
    for (const m of mods) {
      let bucket = buckets.get(m.mod_group);
      if (!bucket) {
        bucket = {
          mod_group: m.mod_group,
          label: m.text_template ?? m.name ?? m.mod_id,
          affix_type: m.affix_type as 'prefix' | 'suffix',
          is_hybrid: m.is_hybrid,
          visible_tiers: [],
          tier_count: m.tier_count,
          weight_share: 0,
          eligible_count: 0,
          concepts: [],
        };
        buckets.set(m.mod_group, bucket);
      }
      // The label tracks the *highest* tier's text_template (lowest
      // `tier_index`) so the placeholder reads as the canonical line.
      // tier_count is taken from any member — registry derives it
      // identically per group.
      if (m.tier_index === 1 && m.text_template) {
        bucket.label = m.text_template;
      }
      bucket.tier_count = Math.max(bucket.tier_count, m.tier_count);
      bucket.visible_tiers.push(m);
      if (m.eligible_now) {
        bucket.eligible_count += 1;
        bucket.weight_share += m.weight_share;
      }
      // Concept set is the union across visible tiers.
      for (const c of m.concepts) {
        if (!bucket.concepts.includes(c)) bucket.concepts.push(c);
      }
    }
    const out = [...buckets.values()].filter((b) => b.visible_tiers.length > 0);
    for (const b of out) {
      b.visible_tiers.sort((a, z) => a.tier_index - z.tier_index);
    }
    out.sort((a, z) => {
      if (sortMode === 'weight' || sortMode === 'best') {
        return (
          z.weight_share - a.weight_share ||
          z.eligible_count - a.eligible_count ||
          a.label.localeCompare(z.label)
        );
      }
      if (sortMode === 'tier') {
        return (
          Math.min(...a.visible_tiers.map((t) => t.tier_index)) -
          Math.min(...z.visible_tiers.map((t) => t.tier_index)) ||
          z.weight_share - a.weight_share
        );
      }
      return a.label.localeCompare(z.label);
    });
    return out;
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
        if (!pickedMod) {
          error = 'Pick a modifier first.';
          busy = false;
          return;
        }
        if (!pickedMod.eligible_now) {
          error = 'Pick an eligible tier before recording the outcome.';
          busy = false;
          return;
        }
        outcome = {
          kind: 'add_mod',
          mod_id: pickedMod.mod_id,
          roll: rollAvg,
          currency: currencyOf(action) ?? undefined,
        };
      } else if (mode === 'remove') {
        outcome = { kind: 'remove_mod', affix: removeAffix, index: removeIndex };
      } else if (mode === 'replace') {
        if (!pickedMod) {
          error = 'Pick a replacement modifier first.';
          busy = false;
          return;
        }
        if (!pickedMod.eligible_now) {
          error = 'Pick an eligible replacement tier before recording the outcome.';
          busy = false;
          return;
        }
        outcome = {
          kind: 'replace_mod',
          remove_affix: removeAffix,
          remove_index: removeIndex,
          add_mod_id: pickedMod.mod_id,
          roll: rollAvg,
        };
      } else if (mode === 'reroll') {
        // Divine — collect only the (slot, index, values) tuples that
        // actually differ from the snapshot so untouched mods stay
        // bit-identical on the backend.
        const rolls = collectRerollEdits();
        if (!rerollResponse) {
          error = 'Reroll snapshot not loaded yet.';
          busy = false;
          return;
        }
        if (rolls.length === 0) {
          error = 'Edit at least one value to record a Divine outcome.';
          busy = false;
          return;
        }
        outcome = {
          kind: 'reroll_values',
          rolls,
          sanctify: rerollResponse.sanctify,
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

  /** Build the `reroll_values` payload from the per-stat edits. Only
   * mods whose values differ from the snapshot's `current` are
   * included; untouched mods are omitted. */
  function collectRerollEdits(): {
    slot: 'implicit' | 'prefix' | 'suffix';
    index: number;
    values: number[];
  }[] {
    if (!rerollResponse) return [];
    const out: {
      slot: 'implicit' | 'prefix' | 'suffix';
      index: number;
      values: number[];
    }[] = [];
    for (const m of rerollResponse.mods) {
      if (m.is_fractured) continue;
      const values: number[] = [];
      let changed = false;
      for (let i = 0; i < m.stats.length; i += 1) {
        const key = `${m.slot}:${m.index}:${i}`;
        const v = rerollEdits[key];
        const current = m.stats[i].current;
        const next = typeof v === 'number' && Number.isFinite(v) ? v : current;
        values.push(next);
        if (Math.abs(next - current) > 1e-9) changed = true;
      }
      if (changed) {
        out.push({ slot: m.slot, index: m.index, values });
      }
    }
    return out;
  }

  /** Update one stat-cell value; clamp into the allowed band so the
   * input + slider stay in sync and the IPC handler never rejects on
   * range. */
  function setRerollValue(m: RerollableMod, statIdx: number, raw: number) {
    if (!Number.isFinite(raw)) return;
    const stat = m.stats[statIdx];
    const v = Math.min(stat.max, Math.max(stat.min, raw));
    rerollEdits = { ...rerollEdits, [`${m.slot}:${m.index}:${statIdx}`]: v };
  }

  function rerollValueOf(m: RerollableMod, statIdx: number): number {
    const key = `${m.slot}:${m.index}:${statIdx}`;
    const v = rerollEdits[key];
    return typeof v === 'number' && Number.isFinite(v) ? v : m.stats[statIdx].current;
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

  function selectTier(t: EligibleModView) {
    if (!t.eligible_now) return;
    pickedModId = t.mod_id;
    expandedGroup = t.mod_group;
  }

  function selectBestTier() {
    const groupFirst = activeGroup?.visible_tiers.find((t) => t.eligible_now);
    const globalFirst = groupedMods
      .flatMap((g) => g.visible_tiers)
      .find((t) => t.eligible_now);
    const next = groupFirst ?? globalFirst;
    if (!next) return;
    selectTier(next);
  }

  function tierBlockReason(t: EligibleModView): string | null {
    if (t.eligible_now) return null;
    if (eligibleAffix !== 'either' && t.affix_type !== eligibleAffix) {
      return `wrong affix for ${eligibleAffix} action`;
    }
    if (t.required_level > item.ilvl) return `requires ilvl ${t.required_level}`;
    if (t.blocked_by_min_level) return `below ${minRequiredLevel}+ floor`;
    if (t.blocked_by_group) return 'modifier family already present';
    if (t.weight <= 0) return 'no spawn weight for this base';
    return 'not eligible now';
  }

  function selectedOutcomeText(): string {
    if (mode === 'remove') {
      const opt = removeOptions.find((o) => o.affix === removeAffix && o.index === removeIndex);
      return opt ? `Remove ${opt.mod_id}` : 'No removable modifier selected';
    }
    if (mode === 'reroll') return 'Record edited values';
    if (mode === 'reveal') return selectedOmen ? `Reveal with ${displayId(selectedOmen)}` : 'Reveal with no omen';
    if (mode === 'none') return 'No outcome required';
    if (!pickedMod) return 'No outcome selected';
    return `${pickedMod.text_template ?? pickedMod.name ?? pickedMod.mod_id} (T${pickedMod.tier_index})`;
  }

  function selectedOutcomeDetail(): string {
    if (mode === 'remove') return 'Records the modifier that was removed in game.';
    if (mode === 'reroll') return 'Only edited values are sent; unchanged rolls stay intact.';
    if (mode === 'reveal') return 'Reveal recording is not yet wired to item mutation.';
    if (mode === 'none') return 'Close this dialog and continue with the advisor.';
    if (!pickedMod) return 'Choose the modifier and tier that actually rolled in game.';
    const chance = pickedMod.weight_share > 0 ? `${(pickedMod.weight_share * 100).toFixed(1)}% weight share` : 'not in current roll pool';
    return `${pickedMod.affix_type} · ilvl ${pickedMod.required_level} · ${chance}`;
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
      {:else if mode === 'reroll'}
        <!-- Phase D.6 — Divine Orb (and omen variants). The dialog
             lists every non-fractured slot of the item with bounded
             numeric inputs + sliders. Tier never changes; only values
             reroll within `[min, max]` (or the widened sanctified
             band). -->
        {#if busy}
          <p class="muted">Loading rerollable mods…</p>
        {:else if !rerollResponse}
          <p class="muted">No data yet.</p>
        {:else}
          {@const r = rerollResponse}
          <p class="hint">
            {#if r.sanctify}
              <strong>Omen of Sanctification</strong> — values reroll, then each is
              independently multiplied by 0.8&times;–1.2&times;. The item becomes
              <strong>Sanctified</strong> and cannot be crafted further.
            {:else if r.implicits_only}
              <strong>Omen of the Blessed</strong> — only implicit modifiers will reroll.
              Explicit values stay unchanged.
            {:else}
              Divine rerolls every non-fractured modifier within its current tier.
              Enter the new in-game values below.
            {/if}
          </p>
          {@const slotOrder = ['implicit', 'prefix', 'suffix'] as const}
          {#each slotOrder as slotName (slotName)}
            {@const slotMods = r.mods.filter((m) => m.slot === slotName)}
            {#if slotMods.length > 0}
              <div class="reroll-section">
                <header class="reroll-section-head">
                  {slotName === 'implicit' ? 'Implicits' : slotName === 'prefix' ? 'Prefixes' : 'Suffixes'}
                </header>
                {#each slotMods as m (m.slot + ':' + m.index)}
                  <div class="reroll-mod" class:fractured={m.is_fractured}>
                    <header class="reroll-mod-head">
                      <span class="tier">T{m.tier_index}/{m.tier_count}</span>
                      <span class="reroll-mod-name">
                        {m.text_template ?? m.name ?? m.mod_id}
                      </span>
                      {#if m.is_fractured}
                        <span class="reroll-frac-badge">fractured · skipped</span>
                      {/if}
                    </header>
                    {#if !m.is_fractured}
                      {#each m.stats as s, i (i)}
                        <label class="reroll-stat">
                          <span class="reroll-stat-label">{s.stat_id}</span>
                          <input
                            type="range"
                            min={s.min}
                            max={s.max}
                            step={(s.max - s.min) / 100 || 0.01}
                            value={rerollValueOf(m, i)}
                            oninput={(e) =>
                              setRerollValue(m, i, Number((e.currentTarget as HTMLInputElement).value))}
                          />
                          <input
                            type="number"
                            class="reroll-num"
                            min={s.min}
                            max={s.max}
                            step="0.1"
                            value={rerollValueOf(m, i)}
                            oninput={(e) =>
                              setRerollValue(m, i, Number((e.currentTarget as HTMLInputElement).value))}
                          />
                          <span class="reroll-range">
                            {s.min.toFixed(1)}–{s.max.toFixed(1)}
                            {#if r.sanctify}
                              <em class="reroll-strict">
                                strict {s.strict_min.toFixed(1)}–{s.strict_max.toFixed(1)}
                              </em>
                            {/if}
                          </span>
                        </label>
                      {/each}
                    {/if}
                  </div>
                {/each}
              </div>
            {/if}
          {/each}
          {#if r.mods.length === 0}
            <p class="muted">
              No rerollable mods on this item{r.implicits_only ? ' (implicits-only filter active)' : ''}.
            </p>
          {/if}
        {/if}
      {:else if mode === 'remove'}
        {#if removeOptions.length === 0}
          <p class="muted">Item has no removable mods.</p>
        {:else if removeOptions.length === 1}
          <!-- Only one mod present — annul has a deterministic outcome.
               Pre-selected automatically; the user just confirms. -->
          <p class="hint">Only one modifier on the item — annul has only one possible outcome.</p>
          <div class="remove-only">
            <span class="rm-affix">{removeOptions[0].affix}</span>
            <span class="rm-mod">{removeOptions[0].mod_id}</span>
            <span class="rm-tag">will be removed</span>
          </div>
        {:else}
          <p class="hint">Pick which modifier was removed.</p>
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

        <div class="outcome-picker">
          <div class="picker-controls">
            <label class="search-box">
              <span>Search</span>
              <input
                type="search"
                placeholder="Search modifiers, stats, tags..."
                bind:value={search}
              />
            </label>

            <div class="filter-grid">
              <div class="chip-row">
                <span class="chip-label">Source</span>
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
                <span class="chip-label">Tier</span>
                {#each [
                  { v: 'all', label: 'Any' },
                  { v: 't1', label: 'T1' },
                  { v: 't2', label: 'T2' },
                  { v: 't3', label: 'T3' },
                  { v: 't4plus', label: 'T4+' },
                ] as opt (opt.v)}
                  <button
                    type="button"
                    class="chip"
                    class:active={tierFilter === opt.v}
                    onclick={() => (tierFilter = opt.v as TierFilter)}
                  >
                    {opt.label}
                  </button>
                {/each}
              </div>
              {#if targetConcepts.length > 0}
                <div class="chip-row">
                  <span class="chip-label">Pool</span>
                  {#each [
                    { v: 'all', label: 'All' },
                    { v: 'target', label: 'Target' },
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
              <label class="compact-select">
                <span>Concept</span>
                <select bind:value={conceptFilter}>
                  <option value={null}>All concepts</option>
                  {#each concepts as c (c)}
                    <option value={c}>{c}</option>
                  {/each}
                </select>
              </label>
              <label class="compact-select">
                <span>Sort</span>
                <select bind:value={sortMode}>
                  <option value="best">Best match</option>
                  <option value="weight">Weight share</option>
                  <option value="tier">Highest tier</option>
                  <option value="name">Name</option>
                </select>
              </label>
            </div>
          </div>

          {#if busy}
            <p class="muted framed">Loading eligible mods...</p>
          {:else if !response}
            <p class="muted framed">No data yet.</p>
          {:else if !response.data_available}
            <p class="muted framed">
              No mod data bundled for {response.item_class}. The advisor can
              still recommend steps, but per-mod outcome recording isn't
              available for this item class yet.
            </p>
          {:else}
            <div class="picker-meta">
              <span>{groupedMods.length} modifier group{groupedMods.length === 1 ? '' : 's'}</span>
              <span>{eligibleTierCount}/{visibleTierCount} selectable tiers</span>
              <span>affix: {response.affix}</span>
              <span>{minRequiredLevel > 0 ? `floor ${minRequiredLevel}+` : 'no level floor'}</span>
            </div>

            <div class="picker-layout">
              <section class="results-pane" aria-label="Modifier results">
                <header class="pane-head">
                  <span>Modifier Results</span>
                  <em>{visibleTierCount} tiers shown</em>
                </header>
                <ul class="result-list">
                  {#each groupedMods as g (g.mod_group)}
                    {@const active = activeGroup?.mod_group === g.mod_group}
                    {@const selected = pickedGroup?.mod_group === g.mod_group}
                    <li class="result-row" class:active class:selected>
                      <button
                        type="button"
                        onclick={() => (expandedGroup = g.mod_group)}
                      >
                        <span class="pick-dot">{selected ? 'selected' : ''}</span>
                        <span class="result-main">
                          <strong>{g.label}</strong>
                          <span class="result-chips">
                            <em>{g.affix_type}</em>
                            {#if g.is_hybrid}<em>hybrid</em>{/if}
                            {#each g.concepts.slice(0, 3) as c (c)}<em>{c}</em>{/each}
                          </span>
                        </span>
                        <span class="result-stats">
                          <strong>{g.eligible_count}/{g.tier_count}</strong>
                          <small>{(g.weight_share * 100).toFixed(1)}% share</small>
                        </span>
                      </button>
                    </li>
                  {/each}
                  {#if groupedMods.length === 0}
                    <li class="muted empty">No modifier types match the filters.</li>
                  {/if}
                </ul>
              </section>

              <aside class="tiers-pane" aria-label="Tier breakdown">
                {#if activeGroup}
                  <header class="pane-head tier-head">
                    <span>{activeGroup.label}</span>
                    <em>{activeGroup.eligible_count}/{activeGroup.tier_count} selectable</em>
                  </header>
                  <div class="tier-concepts">
                    {#each activeGroup.concepts as c (c)}<span class="concept">{c}</span>{/each}
                  </div>
                  <div class="tier-toolbar">
                    <button type="button" class="ghost compact" onclick={selectBestTier}>
                      Select best available tier
                    </button>
                    <span>{(activeGroup.weight_share * 100).toFixed(1)}% group weight share</span>
                  </div>
                  <div class="tier-table" role="table" aria-label="Tier table">
                    <div class="tier-table-head" role="row">
                      <span>Tier</span>
                      <span>Ilvl</span>
                      <span>Stat range</span>
                      <span>Weight</span>
                    </div>
                    {#each activeGroup.visible_tiers as t (t.mod_id)}
                      {@const blocked = tierBlockReason(t)}
                      <button
                        type="button"
                        class="tier-row"
                        class:picked={t.mod_id === pickedModId}
                        class:blocked={blocked !== null}
                        disabled={blocked !== null}
                        onclick={() => selectTier(t)}
                      >
                        <span class="tier">T{t.tier_index}</span>
                        <span class="tier-ilvl">{t.required_level}</span>
                        <span class="tier-tpl">
                          {t.text_template ?? t.name ?? t.mod_id}
                          {#if blocked}<em>{blocked}</em>{/if}
                        </span>
                        <span class="tier-weight">
                          {t.weight_share > 0 ? `${(t.weight_share * 100).toFixed(1)}%` : '-'}
                        </span>
                      </button>
                    {/each}
                  </div>

                  {#if pickedMod}
                    <div class="rolls compact-rolls">
                      <header>
                        <span>Recorded values · {pickedMod.name ?? pickedMod.mod_id}</span>
                        {#if isFracturing}
                          <button type="button" class="warn-btn" onclick={setAllStatsToMax}>
                            Set all stats to max
                          </button>
                        {/if}
                      </header>
                      {#if isFracturing}
                        <p class="frac-hint">
                          Fracturing locks the chosen mod permanently. Record the exact keeper tier and values before continuing.
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
                            <em>/ {s.min.toFixed(0)}-{s.max.toFixed(0)}</em>
                          </span>
                        </label>
                      {/each}
                    </div>
                  {/if}
                {:else}
                  <p class="muted framed">Select a modifier group to inspect its tier ladder.</p>
                {/if}
              </aside>
            </div>
          {/if}
        </div>
      {/if}
    </div>

    <footer class="actions">
      <div class="selected-summary" class:empty={!pickedMod}>
        <span>{pickedMod ? 'Selected outcome' : 'Outcome'}</span>
        <strong>{selectedOutcomeText()}</strong>
        <small>{selectedOutcomeDetail()}</small>
      </div>
      <div class="action-buttons">
        <button class="ghost" onclick={onClose}>Cancel</button>
        <button class="primary" onclick={confirm} disabled={busy || cannotApplyReason !== null}>
          {busy ? 'Saving...' : 'Record outcome'}
        </button>
      </div>
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
    top: 3vh;
    left: 50%;
    transform: translateX(-50%);
    width: min(1180px, 96vw);
    max-height: 94vh;
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
    min-height: 0;
  }

  .actions {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.45rem;
    padding: 0.7rem 0.85rem;
    border-top: 1px solid var(--border-strong);
    background: rgba(0, 0, 0, 0.4);
  }

  .action-buttons {
    display: flex;
    gap: 0.45rem;
    flex-shrink: 0;
  }

  .selected-summary {
    display: grid;
    gap: 0.12rem;
    min-width: 0;
  }

  .selected-summary span {
    color: #72ff58;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    font-size: 0.64rem;
    font-family: Georgia, 'Times New Roman', serif;
  }

  .selected-summary strong {
    color: var(--fg);
    font-size: 0.8rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .selected-summary small {
    color: var(--fg-muted);
    font-family: ui-monospace, 'Fira Code', monospace;
    font-size: 0.68rem;
  }

  .selected-summary.empty span {
    color: var(--gold);
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

  .block select {
    background: rgba(0, 0, 0, 0.5);
    color: var(--fg);
    border: 1px solid var(--border-strong);
    border-radius: 4px;
    padding: 0.35rem 0.55rem;
    font-size: 0.85rem;
  }

  .outcome-picker {
    display: grid;
    grid-template-rows: auto auto minmax(0, 1fr);
    gap: 0.55rem;
    min-height: 0;
  }

  .picker-controls {
    display: grid;
    grid-template-columns: minmax(16rem, 0.85fr) minmax(22rem, 1.6fr);
    gap: 0.7rem;
    align-items: start;
    border: 1px solid rgba(197, 143, 61, 0.25);
    border-radius: 5px;
    padding: 0.55rem;
    background: rgba(0, 0, 0, 0.18);
  }

  .search-box,
  .compact-select {
    display: grid;
    gap: 0.25rem;
  }

  .search-box span,
  .compact-select span {
    color: var(--gold);
    text-transform: uppercase;
    letter-spacing: 0.08em;
    font-family: Georgia, 'Times New Roman', serif;
    font-size: 0.68rem;
  }

  .search-box input,
  .compact-select select {
    width: 100%;
    background: rgba(0, 0, 0, 0.5);
    color: var(--fg);
    border: 1px solid var(--border-strong);
    border-radius: 4px;
    padding: 0.42rem 0.55rem;
    font-size: 0.82rem;
  }

  .filter-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0.4rem 0.65rem;
    align-items: end;
  }

  .picker-meta {
    display: flex;
    flex-wrap: wrap;
    gap: 0.35rem;
    color: var(--fg-muted);
    font-size: 0.72rem;
    font-family: ui-monospace, 'Fira Code', monospace;
  }

  .picker-meta span {
    border: 1px solid rgba(197, 143, 61, 0.22);
    border-radius: 999px;
    padding: 0.18rem 0.45rem;
    background: rgba(0, 0, 0, 0.22);
  }

  .picker-layout {
    display: grid;
    grid-template-columns: minmax(21rem, 1fr) minmax(24rem, 0.95fr);
    gap: 0.65rem;
    min-height: 300px;
    height: min(52vh, 530px);
    min-width: 0;
  }

  .results-pane,
  .tiers-pane {
    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
    min-width: 0;
    min-height: 0;
    border: 1px solid rgba(197, 143, 61, 0.28);
    border-radius: 5px;
    background: linear-gradient(180deg, rgba(12, 17, 20, 0.92), rgba(4, 7, 10, 0.92));
    overflow: hidden;
  }

  .tiers-pane {
    grid-template-rows: auto auto auto minmax(0, 1fr) auto;
  }

  .pane-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
    padding: 0.55rem 0.65rem;
    border: 0;
    border-bottom: 1px solid rgba(197, 143, 61, 0.22);
    background: rgba(20, 13, 4, 0.45);
  }

  .pane-head span {
    color: var(--gold-bright);
    text-transform: uppercase;
    letter-spacing: 0.08em;
    font-family: Georgia, 'Times New Roman', serif;
    font-size: 0.74rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .pane-head em {
    color: var(--fg-muted);
    font-style: normal;
    font-family: ui-monospace, 'Fira Code', monospace;
    font-size: 0.68rem;
    white-space: nowrap;
  }

  .result-list {
    list-style: none;
    margin: 0;
    padding: 0.35rem;
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
    overflow-y: auto;
    min-height: 0;
  }

  .result-row {
    border: 1px solid rgba(197, 143, 61, 0.2);
    border-radius: 4px;
    background: rgba(0, 0, 0, 0.2);
  }

  .result-row.active {
    border-color: rgba(197, 143, 61, 0.55);
    background: rgba(197, 143, 61, 0.08);
  }

  .result-row.selected {
    border-color: rgba(114, 255, 88, 0.62);
    background: linear-gradient(180deg, rgba(20, 42, 18, 0.66), rgba(5, 13, 8, 0.66));
  }

  .result-row button {
    display: grid;
    grid-template-columns: 1.25rem minmax(0, 1fr) auto;
    gap: 0.55rem;
    align-items: center;
    width: 100%;
    border: 0;
    background: transparent;
    color: inherit;
    padding: 0.5rem 0.55rem;
    text-align: left;
    cursor: pointer;
  }

  .pick-dot {
    width: 0.82rem;
    height: 0.82rem;
    border: 1px solid rgba(197, 143, 61, 0.55);
    border-radius: 999px;
    overflow: hidden;
    text-indent: -999px;
  }

  .result-row.selected .pick-dot {
    border-color: rgba(114, 255, 88, 0.9);
    background: radial-gradient(circle, #72ff58 0 34%, transparent 38%);
  }

  .result-main {
    display: grid;
    gap: 0.22rem;
    min-width: 0;
  }

  .result-main strong {
    color: #00c8ff;
    font-size: 0.82rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .result-chips,
  .tier-concepts {
    display: flex;
    flex-wrap: wrap;
    gap: 0.22rem;
  }

  .result-chips em {
    color: #a98dff;
    border: 1px solid rgba(169, 141, 255, 0.32);
    border-radius: 999px;
    padding: 0.02rem 0.35rem;
    font-style: normal;
    font-size: 0.64rem;
  }

  .result-stats {
    display: grid;
    justify-items: end;
    gap: 0.1rem;
    font-family: ui-monospace, 'Fira Code', monospace;
  }

  .result-stats strong {
    color: var(--gold-bright);
    font-size: 0.76rem;
  }

  .result-stats small {
    color: #72ff58;
    font-size: 0.66rem;
  }

  .tier-head span {
    color: #00c8ff;
  }

  .tier-concepts {
    padding: 0.45rem 0.65rem 0;
  }

  .tier-toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    padding: 0.45rem 0.65rem;
    color: var(--fg-muted);
    font-size: 0.72rem;
    font-family: ui-monospace, 'Fira Code', monospace;
  }

  .tier-table {
    overflow-y: auto;
    min-height: 0;
    padding: 0 0.45rem 0.5rem;
  }

  .tier-table-head,
  .tier-row {
    display: grid;
    grid-template-columns: 3.1rem 3.4rem minmax(0, 1fr) 4.8rem;
    gap: 0.45rem;
    align-items: center;
  }

  .tier-table-head {
    position: sticky;
    top: 0;
    z-index: 1;
    padding: 0.35rem 0.45rem;
    color: var(--gold);
    background: rgba(4, 7, 10, 0.96);
    border-bottom: 1px solid rgba(197, 143, 61, 0.22);
    text-transform: uppercase;
    letter-spacing: 0.08em;
    font-size: 0.62rem;
  }

  .tier-row {
    width: 100%;
    border: 1px solid transparent;
    border-radius: 4px;
    background: transparent;
    color: var(--fg-soft);
    padding: 0.42rem 0.45rem;
    text-align: left;
    cursor: pointer;
  }

  .tier-row:not(:disabled):hover {
    border-color: rgba(197, 143, 61, 0.42);
    background: rgba(197, 143, 61, 0.08);
  }

  .tier-row.picked {
    border-color: rgba(114, 255, 88, 0.65);
    background: rgba(20, 42, 18, 0.66);
  }

  .tier-row.blocked {
    opacity: 0.58;
    cursor: not-allowed;
  }

  .tier-row .tier-tpl {
    display: grid;
    gap: 0.1rem;
  }

  .tier-row .tier-tpl em {
    color: #ffb96b;
    font-style: normal;
    font-size: 0.66rem;
  }

  .compact-rolls {
    margin: 0.45rem 0.65rem 0.65rem;
  }

  .framed {
    border: 1px dashed rgba(197, 143, 61, 0.28);
    border-radius: 5px;
    padding: 0.85rem;
    background: rgba(0, 0, 0, 0.22);
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

  .tier-ilvl {
    color: var(--fg-muted);
    font-family: ui-monospace, 'Fira Code', monospace;
    font-size: 0.74rem;
  }

  .tier-tpl {
    color: #00c8ff;
    font-size: 0.8rem;
    flex: 1;
    min-width: 0;
  }

  .tier-weight {
    color: #72ff58;
    font-size: 0.72rem;
    font-family: ui-monospace, 'Fira Code', monospace;
  }

  .tier {
    font-family: Georgia, 'Times New Roman', serif;
    color: var(--gold-bright);
    font-size: 0.85rem;
  }

  .concept {
    color: #a98dff;
    font-size: 0.7rem;
    border: 1px solid rgba(169, 141, 255, 0.45);
    background: rgba(40, 25, 70, 0.25);
    border-radius: 999px;
    padding: 0.05rem 0.4rem;
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

  .remove-only {
    display: flex;
    align-items: center;
    gap: 0.55rem;
    border: 1px solid var(--border-strong);
    border-radius: 4px;
    padding: 0.55rem 0.7rem;
    background: rgba(0, 0, 0, 0.3);
  }

  .remove-only .rm-tag {
    margin-left: auto;
    color: var(--fg-muted);
    font-size: 0.75rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
  }

  /* Phase D.6 — Divine Orb reroll dialog. */
  .reroll-section {
    display: flex;
    flex-direction: column;
    gap: 0.45rem;
    border: 1px solid var(--border-strong);
    border-radius: 4px;
    padding: 0.5rem 0.6rem;
    background: rgba(0, 0, 0, 0.18);
  }

  .reroll-section-head {
    color: var(--gold-bright);
    text-transform: uppercase;
    letter-spacing: 0.08em;
    font-size: 0.74rem;
  }

  .reroll-mod {
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
    border-top: 1px dashed rgba(197, 143, 61, 0.18);
    padding-top: 0.4rem;
  }

  .reroll-mod:first-of-type {
    border-top: 0;
    padding-top: 0;
  }

  .reroll-mod.fractured {
    opacity: 0.6;
  }

  .reroll-mod-head {
    display: flex;
    align-items: baseline;
    gap: 0.55rem;
    flex-wrap: wrap;
  }

  .reroll-mod-name {
    color: #00c8ff;
    font-size: 0.84rem;
    flex: 1;
    min-width: 0;
  }

  .reroll-frac-badge {
    color: #ffb96b;
    font-size: 0.7rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
  }

  .reroll-stat {
    display: grid;
    grid-template-columns: minmax(7rem, 1fr) minmax(8rem, 2fr) 5rem auto;
    align-items: center;
    gap: 0.5rem;
  }

  .reroll-stat-label {
    color: var(--fg-muted);
    font-family: ui-monospace, 'Fira Code', monospace;
    font-size: 0.75rem;
    overflow-wrap: anywhere;
  }

  .reroll-num {
    background: rgba(0, 0, 0, 0.4);
    color: var(--fg);
    border: 1px solid var(--border-strong);
    border-radius: 3px;
    padding: 0.2rem 0.35rem;
    font-family: ui-monospace, 'Fira Code', monospace;
    font-size: 0.78rem;
    width: 100%;
  }

  .reroll-range {
    color: var(--fg-muted);
    font-family: ui-monospace, 'Fira Code', monospace;
    font-size: 0.72rem;
  }

  .reroll-strict {
    margin-left: 0.4rem;
    font-style: normal;
    color: #a98dff;
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

  @media (max-width: 920px) {
    .dialog {
      top: 2vh;
      width: 96vw;
      max-height: 96vh;
    }

    .picker-controls,
    .picker-layout,
    .filter-grid {
      grid-template-columns: 1fr;
    }

    .picker-layout {
      height: auto;
      max-height: none;
    }

    .results-pane,
    .tiers-pane {
      max-height: 42vh;
    }

    .actions {
      align-items: stretch;
      flex-direction: column;
    }

    .action-buttons {
      justify-content: flex-end;
    }
  }
</style>
