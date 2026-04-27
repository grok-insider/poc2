import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import { listen as tauriListen, type EventCallback, type UnlistenFn } from '@tauri-apps/api/event';
import { FRESH_BODY_ARMOUR, WORKED_EXAMPLE_GOAL } from './fixtures';
import { previewModsForClass } from './mockMods';
import type {
  AssetManifest,
  BaseSummary,
  CannotApplyView,
  ClientLogStatus,
  DatabaseEntryDetail,
  DatabaseEntrySummary,
  EligibleModsResponse,
  Item,
  LeagueInfo,
  MetaResponse,
  ParsedItem,
  PluginInfo,
  PriceRefreshMeta,
  Recipe,
  RecipeSummary,
  RecordOutcomeResponse,
  RecoveryStepView,
  Recommendation,
  RefreshPricesResponse,
  ReloadBundleResponse,
  RerollableModsResponse,
  TrialDistribution,
} from './types';

const isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

export async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (isTauri) return tauriInvoke<T>(cmd, args);
  return browserInvoke<T>(cmd, args);
}

export async function listen<T>(event: string, handler: EventCallback<T>): Promise<UnlistenFn> {
  if (isTauri) return tauriListen<T>(event, handler);
  void event;
  void handler;
  return () => undefined;
}

async function browserInvoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  void args;
  switch (cmd) {
    case 'ping':
      return 'poc2 browser preview ready (mock IPC)' as T;
    case 'load_state':
      return { goal_json: JSON.stringify(WORKED_EXAMPLE_GOAL) } as T;
    case 'save_state':
      return undefined as T;
    case 'asset_manifest':
      return mockAssetManifest() as T;
    case 'asset_status':
      return { total: mockAssetManifest().entries.length, cached: 0, missing: 0, failed: 0, root: null } as T;
    case 'recommend':
      return {
        recommendations: mockRecommendations(),
        patch: '0.4.0',
        rule_count: 113,
        strategy_count: 24,
        mod_count: 2123,
        bundle_path: 'browser-preview',
      } as T;
    case 'recommend_streaming':
      return undefined as T;
    case 'refresh_prices':
      return {
        refreshed: true,
        meta: mockPriceMeta(),
        error: null,
      } satisfies RefreshPricesResponse as T;
    case 'trade_search':
      return undefined as T;
    case 'parse_item_text':
    case 'read_clipboard_item':
      return {
        parsed: mockParsedItem(),
        item: FRESH_BODY_ARMOUR,
        unresolved: [],
      } as T;
    case 'eligible_mods':
      return mockEligibleMods(args) as T;
    case 'rerollable_mods':
      return mockRerollableMods(args) as T;
    case 'check_can_apply':
      return mockCheckCanApply(args) as T;
    case 'record_outcome':
      return mockRecordOutcome(args) as T;
    case 'list_bases':
      return mockListBases(args) as T;
    case 'list_database_entries':
      return mockListDatabaseEntries(args) as T;
    case 'database_entry_detail':
      return mockDatabaseEntryDetail(args) as T;
    case 'recovery_hints':
      return {
        step_id: 'browser-preview',
        next_action_summary: 'Try a safer deterministic currency step.',
        hints: [
          {
            message: 'If the roll misses, stop and re-plan from the current item state.',
            goto_step_id: null,
            added_cost_div: 0,
            strategy_id: 'browser-preview',
            step_id: 'browser-preview',
          },
        ],
      } satisfies RecoveryStepView as T;
    case 'run_n_trials':
      return {
        n_trials: 1000,
        success_rate: 0.318,
        success_rate_stderr: 0.014,
        mean_change_count: 2.4,
        change_count_histogram: { 1: 120, 2: 430, 3: 310, 4: 140 },
        cost_per_trial_div: 1.25,
        total_cost_div_expected: 392.5,
      } satisfies TrialDistribution as T;
    case 'list_leagues':
      return [
        { value: 'Fate of the Vaal', divine_price_in_exalts: 126, chaos_per_divine: 42 },
      ] satisfies LeagueInfo[] as T;
    case 'reload_bundle':
      return {
        bundle_path: 'browser-preview',
        patch: '0.4.0',
        mod_count: 2123,
        strategy_count: 24,
      } satisfies ReloadBundleResponse as T;
    case 'fetch_meta_builds':
      return {
        league: 'Fate of the Vaal',
        fetched_at: new Date().toISOString(),
        n_builds: 0,
        niches: [],
      } satisfies MetaResponse as T;
    case 'list_plugins':
      return [] satisfies PluginInfo[] as T;
    case 'reload_plugins':
      return 0 as T;
    case 'start_client_log':
    case 'stop_client_log':
    case 'client_log_status':
      return { watching: false, path: null } satisfies ClientLogStatus as T;
    case 'list_recipes':
      return [] satisfies RecipeSummary[] as T;
    case 'save_recipe':
    case 'delete_recipe':
      return undefined as T;
    case 'load_recipe':
      return {
        name: 'browser-preview',
        description: 'Browser preview recipe',
        item_json: JSON.stringify(FRESH_BODY_ARMOUR),
        goal_json: JSON.stringify(WORKED_EXAMPLE_GOAL),
        created_at: String(Math.floor(Date.now() / 1000)),
      } satisfies Recipe as T;
    case 'export_recipe_toml':
      return 'name = "browser-preview"\ndescription = "Browser preview recipe"\n' as T;
    default:
      throw new Error(`browser preview has no mock for invoke(${cmd})`);
  }
}

function mockAssetManifest(): AssetManifest {
  const entries = [
    ['BodyArmour', 'Body Armour', 'class', 'https://cdn.poe2db.tw/image/Art/2DItems/Armours/BodyArmours/Basetypes/BodyInt03.webp'],
    ['Helmet', 'Helmet', 'class', 'https://cdn.poe2db.tw/image/Art/2DItems/Armours/Helmets/Basetypes/HelmetInt03.webp'],
    ['Boots', 'Boots', 'class', 'https://cdn.poe2db.tw/image/Art/2DItems/Armours/Boots/Basetypes/BootsDex01.webp'],
    ['Ring', 'Ring', 'class', 'https://cdn.poe2db.tw/image/Art/2DItems/Rings/Basetypes/IronRing.webp'],
    ['Amulet', 'Amulet', 'class', 'https://cdn.poe2db.tw/image/Art/2DItems/Amulets/Basetypes/GoldAmulet.webp'],
    ['PerfectOrbOfTransmutation', 'Perfect Orb of Transmutation', 'currency', 'https://cdn.poe2db.tw/image/Art/2DItems/Currency/CurrencyUpgradeToMagic.webp'],
    ['ExaltedOrb', 'Exalted Orb', 'currency', 'https://cdn.poe2db.tw/image/Art/2DItems/Currency/CurrencyAddModToRare.webp'],
    ['DivineOrb', 'Divine Orb', 'currency', 'https://cdn.poe2db.tw/image/Art/2DItems/Currency/CurrencyModValues.webp'],
  ];
  return {
    generated_at: String(Math.floor(Date.now() / 1000)),
    entries: entries.map(([id, name, kind, source_url]) => ({
      id,
      name,
      kind,
      detail_url: null,
      source_url,
      local_path: null,
      status: 'remote',
      error: null,
    })),
  };
}

function mockRecommendations(): Recommendation[] {
  return [
    {
      action: { kind: 'apply_currency', currency: 'PerfectOrbOfTransmutation', omens: [] },
      source: { kind: 'strategy', id: 'browser-preview', step: 'start' },
      expected_cost: { min: 0.8, expected: 1.25, max: 1.8 },
      expected_prob: 0.318,
      prob_stderr: 0.014,
      score: 0.71,
      rationale: 'Preview step: create a strong magic base before regal/exalt follow-up.',
      depth: 2,
    },
    {
      action: { kind: 'apply_currency', currency: 'ExaltedOrb', omens: [] },
      source: { kind: 'heuristic', name: 'browser-preview' },
      expected_cost: { min: 1.0, expected: 1.2, max: 1.5 },
      expected_prob: 0.188,
      prob_stderr: 0.01,
      score: 0.44,
      rationale: 'Preview alternative: add a random modifier to an existing rare item.',
      depth: 2,
    },
  ];
}

function mockPriceMeta(): PriceRefreshMeta {
  return {
    league: 'Fate of the Vaal',
    fetched_at: new Date().toISOString(),
    applied_count: 8,
    total_entries: 8,
  };
}

function mockParsedItem(): ParsedItem {
  return {
    item_class: 'BodyArmour',
    rarity: 'normal',
    name: null,
    base: 'BodyArmour',
    ilvl: 82,
    quality: 0,
    requirements: { level: null, str_req: null, dex_req: null, int_req: null },
    implicits: [],
    explicits: [],
    corrupted: false,
    mirrored: false,
    sanctified: false,
  };
}

function mockEligibleMods(args: Record<string, unknown> | undefined): EligibleModsResponse {
  const innerArgs = (args?.args ?? {}) as {
    item?: Item;
    affix?: string;
    min_required_level?: number;
  };
  const item = innerArgs.item;
  const affix = (innerArgs.affix ?? 'either') as 'prefix' | 'suffix' | 'either';
  const floor = innerArgs.min_required_level ?? 0;
  const cls = item?.base ?? 'BodyArmour';
  const ilvl = item?.ilvl ?? 82;
  // Phase C.6 — return the full curated preview pool for this class.
  // The OutcomeDialog applies the affix-scope and currency-floor
  // eligibility flags client-side so it can show greyed-out non-
  // matching rows alongside the eligible ones.
  const all = previewModsForClass(cls).map((m) => {
    const above_ilvl = m.required_level > ilvl;
    const affix_match = affix === 'either' || m.affix_type === affix;
    const meets_floor = m.required_level >= floor;
    return {
      ...m,
      eligible_now: !above_ilvl && affix_match && meets_floor,
      blocked_by_min_level: !meets_floor,
    };
  });
  return {
    item_class: cls,
    data_available: true,
    affix,
    patch: '0.4.0',
    mods: all,
  };
}

/**
 * Browser-preview mock for `check_can_apply`. Mirrors the engine's
 * rarity gating so the dialog's badge text stays consistent with what
 * the real backend reports. Kept short and focused: only the gates
 * actually surfaced by the v2 advisor are emulated.
 */
function mockCheckCanApply(args: Record<string, unknown> | undefined): CannotApplyView {
  const inner = (args?.args ?? {}) as { item?: Item; currency?: string };
  const item = inner.item;
  const currency = inner.currency ?? '';
  if (!item) return { kind: 'ok' };
  if (item.mirrored) return { kind: 'mirrored' };
  if (item.corrupted && currency !== 'VaalOrb') return { kind: 'corrupted' };
  const rarity = item.rarity;
  const isTrans = currency.includes('OrbOfTransmutation');
  const isAug = currency.includes('OrbOfAugmentation');
  const isRegal = currency.includes('RegalOrb');
  const isExalt = currency.includes('ExaltedOrb');
  const isChaos = currency.includes('ChaosOrb');
  const isAnnul = currency === 'OrbOfAnnulment';
  if (isTrans && rarity !== 'normal') {
    return { kind: 'wrong_rarity', item_rarity: rarity, expected: ['normal'] };
  }
  if ((isAug || isRegal) && rarity !== 'magic') {
    return { kind: 'wrong_rarity', item_rarity: rarity, expected: ['magic'] };
  }
  if ((isExalt || isChaos) && rarity !== 'rare') {
    return { kind: 'wrong_rarity', item_rarity: rarity, expected: ['rare'] };
  }
  if (isAnnul && rarity === 'normal') {
    return { kind: 'wrong_rarity', item_rarity: rarity, expected: ['magic', 'rare'] };
  }
  if (currency === 'FracturingOrb') {
    if (rarity !== 'rare') {
      return { kind: 'wrong_rarity', item_rarity: rarity, expected: ['rare'] };
    }
    const visible =
      item.prefixes.length + item.suffixes.length + (item.hidden_desecrated ? 1 : 0);
    if (visible < 4) {
      return { kind: 'fracture_requires_four_mods', current: visible };
    }
  }
  return { kind: 'ok' };
}

function mockRecordOutcome(args: Record<string, unknown> | undefined): RecordOutcomeResponse {
  const inner = (args?.args ?? {}) as { item: ParsedItem; outcome: { kind: string } };
  return {
    item: (inner.item as unknown) as RecordOutcomeResponse['item'],
    change: 'added',
    explanation: `mock outcome (${inner?.outcome?.kind ?? 'unknown'})`,
  };
}

function mockRerollableMods(
  args: Record<string, unknown> | undefined,
): RerollableModsResponse {
  const inner = (args?.args ?? {}) as { item?: Item; omen?: string | null };
  const item = inner.item;
  const omen = inner.omen ?? null;
  const sanctify = omen === 'OmenOfSanctification';
  const implicits_only = omen === 'OmenOfTheBlessed';
  type Slot = 'implicit' | 'prefix' | 'suffix';
  type Roll = Item['prefixes'][number];
  const slots: { slot: Slot; rolls: Roll[] }[] = [];
  if (item) {
    slots.push({ slot: 'implicit', rolls: item.implicits });
    if (!implicits_only) {
      slots.push({ slot: 'prefix', rolls: item.prefixes });
      slots.push({ slot: 'suffix', rolls: item.suffixes });
    }
  }
  const mods = slots.flatMap(({ slot, rolls }) =>
    rolls.map((r, index) => ({
      slot,
      index,
      mod_id: r.mod_id,
      name: r.mod_id,
      text_template: null,
      tier_index: 1,
      tier_count: 1,
      is_fractured: r.is_fractured,
      stats: r.values.map((current, i) => {
        const strict_min = current * 0.9;
        const strict_max = current * 1.1;
        return {
          stat_id: `mock_stat_${i}`,
          min: sanctify ? strict_min * 0.8 : strict_min,
          max: sanctify ? strict_max * 1.2 : strict_max,
          strict_min,
          strict_max,
          current,
        };
      }),
    })),
  );
  return { patch: '0.4.0', sanctify, implicits_only, mods };
}

function mockListBases(_args: Record<string, unknown> | undefined): BaseSummary[] {
  return [
    {
      id: "Metadata/Items/Armours/BodyArmours/FourBodyInt3",
      name: "Hexer's Robe",
      class_pascal: 'BodyArmour',
      class_display: 'Body Armour',
      drop_level: 11,
      attribute_pool: 'int',
      tags: ['int_armour', 'body_armour', 'armour'],
      release_state: 'released',
    },
    {
      id: 'Metadata/Items/Armours/BodyArmours/FourBodyStr1',
      name: 'Rusted Cuirass',
      class_pascal: 'BodyArmour',
      class_display: 'Body Armour',
      drop_level: 1,
      attribute_pool: 'str',
      tags: ['str_armour', 'body_armour', 'armour'],
      release_state: 'released',
    },
    {
      id: 'Metadata/Items/Armours/Boots/BootsStr02',
      name: 'Iron Greaves',
      class_pascal: 'Boots',
      class_display: 'Boots',
      drop_level: 5,
      attribute_pool: 'str',
      tags: ['str_armour', 'boots'],
      release_state: 'released',
    },
    {
      id: 'Metadata/Items/Rings/Ring1',
      name: 'Iron Ring',
      class_pascal: 'Ring',
      class_display: 'Ring',
      drop_level: 1,
      attribute_pool: 'none',
      tags: ['ring'],
      release_state: 'released',
    },
  ];
}

function mockListDatabaseEntries(args: Record<string, unknown> | undefined): DatabaseEntrySummary[] {
  const section = (args?.args as { section?: string } | undefined)?.section ?? 'bases';
  const bases = mockListBases(undefined).map((base) => ({
    id: base.id,
    name: base.name,
    section: 'bases' as const,
    category: base.class_display,
    kind: base.class_pascal,
    icon_url: null,
    detail_url: null,
    tags: base.tags,
    description: `${base.class_display} base item, drop level ${base.drop_level}.`,
    base,
  }));
  const materials: DatabaseEntrySummary[] = [
    {
      id: 'VaalOrb',
      name: 'Vaal Orb',
      section: 'materials',
      category: 'Currency',
      kind: 'currency',
      icon_url: null,
      detail_url: null,
      tags: ['currency', 'corruption'],
      description: 'Corrupts an item, causing an unpredictable crafting outcome.',
      base: null,
    },
    {
      id: 'HinekorasLock',
      name: "Hinekora's Lock",
      section: 'materials',
      category: 'Currency',
      kind: 'currency',
      icon_url: null,
      detail_url: null,
      tags: ['currency', 'preview'],
      description: 'Previews the next crafting outcome before committing it.',
      base: null,
    },
  ];
  const list = section === 'materials' ? materials : bases;
  const search = ((args?.args as { search?: string } | undefined)?.search ?? '').trim().toLowerCase();
  if (!search) return list;
  return list.filter((entry) =>
    [entry.name, entry.category, entry.kind, entry.description ?? '', entry.tags.join(' ')]
      .join(' ')
      .toLowerCase()
      .includes(search),
  );
}

function mockDatabaseEntryDetail(args: Record<string, unknown> | undefined): DatabaseEntryDetail {
  const payload = (args?.args as { section?: 'bases' | 'materials'; id?: string } | undefined) ?? {};
  const section = payload.section ?? 'bases';
  const entry = mockListDatabaseEntries({ args: { section } }).find((candidate) => candidate.id === payload.id)
    ?? mockListDatabaseEntries({ args: { section } })[0];
  if (section === 'materials') {
    return {
      summary: entry,
      base: null,
      material: {
        source_section: entry.kind,
        description: entry.description ?? '',
        applies_to: ['Craftable items'],
        tags: entry.tags,
        raw_fields: [],
      },
    };
  }
  const base = entry.base ?? mockListBases(undefined)[0];
  return {
    summary: entry,
    base: {
      metadata_type: base.id,
      drop_level: base.drop_level,
      class_display: base.class_display,
      attribute_pool: base.attribute_pool,
      inventory_width: 2,
      inventory_height: 3,
      tags: base.tags,
      derived_stats: [{ label: 'Energy Shield', value: 'base defensive stat', help: 'Energy Shield protects Life by taking damage first.' }],
      requirements: [`Level ${base.drop_level}`, 'Intelligence requirement varies by base'],
      granted_effects: [],
      class_notes: [],
    },
    material: null,
  };
}
