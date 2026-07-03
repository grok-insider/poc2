"use client";

/// Typed RPC client to the engine Web Worker. Mirrors the old Tauri `invoke`
/// surface so components read naturally. Lazily spawns the worker on first use
/// (browser-only — never during SSR / static export).
///
/// Object/array arguments are forwarded as-is; the worker serializes them to
/// the JSON strings the wasm methods expect. Plain-string args (currency ids,
/// clipboard text, omen ids, step ids) and numbers pass straight through.

import type {
  AdvisorAction,
  ApplyPricesView,
  BaseSummary,
  CannotApplyView,
  DatabaseEntryDetail,
  DatabaseEntrySummary,
  DatabaseSection,
  EligibleModsResponse,
  GenesisTreeView,
  Goal,
  Item,
  NinjaExchangeSnapshot,
  ParseClipboardResponse,
  PluginContentView,
  PluginLoadView,
  PoeScoutSnapshot,
  RecordOutcome,
  RecordOutcomeResponse,
  Recommendation,
  RecoveryStepView,
  RerollableModsResponse,
  ResolveNameArgs,
  ResolveView,
  TrialDistribution,
} from "../types";

type Pending = { resolve: (v: unknown) => void; reject: (e: Error) => void };

let worker: Worker | null = null;
let seq = 0;
const pending = new Map<number, Pending>();

function getWorker(): Worker {
  if (worker) return worker;
  worker = new Worker(new URL("./engine.worker.ts", import.meta.url), {
    type: "module",
  });
  worker.onmessage = (e: MessageEvent) => {
    const { id, ok, result, error } = e.data as {
      id: number;
      ok: boolean;
      result?: unknown;
      error?: string;
    };
    const p = pending.get(id);
    if (!p) return;
    pending.delete(id);
    if (ok) p.resolve(result);
    else p.reject(new Error(error ?? "engine error"));
  };
  return worker;
}

function call<T>(method: string, args: unknown[] = [], transfer: Transferable[] = []): Promise<T> {
  const id = ++seq;
  const w = getWorker();
  return new Promise<T>((resolve, reject) => {
    pending.set(id, { resolve: resolve as (v: unknown) => void, reject });
    w.postMessage({ id, method, args }, transfer);
  });
}

/** Affix-slot filter for the eligible-mods query. */
export type AffixSlot = "prefix" | "suffix" | "either";

/** Engine League ruleset (cross-version gating; 0.5 challenge = Runes of Aldur). */
export type EngineLeague = "standard" | "challenge";

export const engine = {
  // ---- bundle metadata -------------------------------------------------
  patch: () => call<string>("patch"),
  modCount: () => call<number>("modCount"),
  /** Trained `(goal × class)` Q-models loaded (0 = pure heuristics). The
   * worker loads the optional `/trained-models.json` asset at boot. */
  trainedModelCount: () => call<number>("trainedModelCount"),

  // ---- league ruleset ----------------------------------------------------
  /** Current engine League ("standard" | "challenge"). */
  league: () => call<EngineLeague>("league"),
  /** Switch the engine League ruleset (gates Recombinator / 0.5 omens). */
  setLeague: (league: EngineLeague) => call<void>("setLeague", [league]),

  // ---- core planning loop ----------------------------------------------
  recommend: (item: Item, goal: Goal, risk: number, depth: number, topN: number) =>
    call<Recommendation[]>("recommend", [item, goal, risk, depth, topN]),

  // ---- import ----------------------------------------------------------
  /** Parse raw PoE2 clipboard text into a structured + engine `Item`. */
  parseItemText: (text: string) =>
    call<ParseClipboardResponse>("parse", [text]),

  // ---- apply / record --------------------------------------------------
  /** Engine verdict on whether a currency can apply to the current item. */
  checkCanApply: (item: Item, currency: string) =>
    call<CannotApplyView>("checkCanApply", [item, currency]),

  /** Apply a recorded crafting outcome, returning the mutated item. */
  recordOutcome: (item: Item, outcome: RecordOutcome) =>
    call<RecordOutcomeResponse>("recordOutcome", [{ item, outcome }]),

  // ---- inspect ---------------------------------------------------------
  /** Eligible / blocked mods for an (item, affix) slot. */
  eligibleMods: (item: Item, affix: AffixSlot = "either", minRequiredLevel = 0) =>
    call<EligibleModsResponse>("eligibleMods", [
      { item, affix, min_required_level: minRequiredLevel },
    ]),

  /** Mods a Divine-style reroll would touch (for the reroll dialog). */
  rerollableMods: (item: Item, omen: string | null = null) =>
    call<RerollableModsResponse>("rerollableMods", [item, omen]),

  // ---- recover ---------------------------------------------------------
  /** Recovery hints for a strategy step. */
  recoveryHints: (strategyId: string, stepId: string) =>
    call<RecoveryStepView>("recoveryHints", [strategyId, stepId]),

  // ---- simulate --------------------------------------------------------
  /** Monte-Carlo `n` trials of a single action; seed is a u64 (BigInt). */
  runNTrials: (item: Item, action: AdvisorAction, n: number, seed: bigint = 0n) =>
    call<TrialDistribution>("runNTrials", [item, action, n, seed]),

  // ---- database --------------------------------------------------------
  /** List craftable base items, optionally filtered by class / legacy. */
  listBases: (opts: { classPascal?: string | null; includeLegacy?: boolean } = {}) =>
    call<BaseSummary[]>("listBases", [
      { class_pascal: opts.classPascal ?? null, include_legacy: opts.includeLegacy ?? false },
    ]),

  /** List database entries for a section (bases / materials), with search. */
  listDatabaseEntries: (section: DatabaseSection, search?: string) =>
    call<DatabaseEntrySummary[]>("listDatabaseEntries", [
      { section, search: search ?? null },
    ]),

  /** Resolve a single database entry's detail view. */
  databaseEntryDetail: (section: DatabaseSection, id: string) =>
    call<DatabaseEntryDetail>("databaseEntryDetail", [{ section, id }]),

  // ---- prices ------------------------------------------------------------
  /** Apply a browser-fetched poe2scout snapshot to the engine's valuator. */
  applyPrices: (snapshot: PoeScoutSnapshot) =>
    call<ApplyPricesView>("applyPrices", [snapshot]),

  /** Apply a browser-fetched poe.ninja exchange snapshot (parallel source). */
  applyNinjaPrices: (snapshot: NinjaExchangeSnapshot) =>
    call<ApplyPricesView>("applyNinjaPrices", [snapshot]),

  // ---- resolve -----------------------------------------------------------
  /** Fuzzy-resolve a noisy item/currency name onto a canonical key. */
  resolveName: (args: ResolveNameArgs) => call<ResolveView>("resolveName", [args]),

  // ---- plugins (ADR-0014) -------------------------------------------------
  /** Install plugin-emitted strategy/rule TOMLs (set semantics: registries
   * rebuild as seeds + this content; call with `[], []` to reset). */
  setPluginContent: (strategies: string[], rules: string[]) =>
    call<PluginContentView>("setPluginContent", [strategies, rules]),

  /** Load plugin wasm INTO THE WORKER (bytes are transferred): installs
   * emitted content (phase 1) and wires `eval_predicate`-capable plugins
   * into the engine's live predicate dispatch (phase 2). Call with `[]`
   * to unload everything. */
  loadPlugins: (plugins: { name: string; bytes: ArrayBuffer }[]) =>
    call<PluginLoadView>(
      "__loadPlugins",
      [plugins],
      plugins.map((p) => p.bytes),
    ),

  // ---- genesis tree ------------------------------------------------------
  /** The Genesis Tree view (0.5): wombs, nodes, goal presets, farming notes. */
  genesisTree: () => call<GenesisTreeView>("genesisTree"),
};

export type EngineClient = typeof engine;
