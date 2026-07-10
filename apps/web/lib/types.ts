// TypeScript types mirroring the Rust IPC contract.
//
// Hand-maintained for now; M6 polish will switch to ts-rs auto-generation
// from the Rust source so these stay in sync.

export type Rarity = 'normal' | 'magic' | 'rare' | 'unique';

export type AffixType = 'prefix' | 'suffix' | 'implicit' | 'enchantment';

export interface ModRoll {
  mod_id: string;
  affix_type: AffixType;
  kind:
    | 'explicit'
    | 'implicit'
    | 'enchantment'
    | 'desecrated'
    | 'corrupted'
    | 'crafted';
  values: number[];
  is_fractured: boolean;
}

export type QualityKind = 'Untagged' | { Tagged: string };

export interface Item {
  /** Engine-facing class id (PascalCase, e.g. "BodyArmour"). */
  base: string;
  ilvl: number;
  rarity: Rarity;
  corrupted: boolean;
  sanctified: boolean;
  mirrored: boolean;
  quality: number;
  quality_kind: QualityKind;
  implicits: ModRoll[];
  prefixes: ModRoll[];
  suffixes: ModRoll[];
  enchantments: ModRoll[];
  hidden_desecrated: null | unknown;
  sockets: unknown[];
  hinekora_lock: number | null;
  /** Full bundle BaseTypeId (e.g. "Metadata/Items/Armours/BodyArmours/FourBodyInt3").
   * UI-only field used to render base art and the human-readable base name.
   * The engine ignores this when planning. */
  base_type_id?: string | null;
  /** Display name for the base (e.g. "Hexer's Robe"). UI-only. */
  base_display_name?: string | null;
}

export interface ValuePredicate {
  op: 'eq' | 'ne' | 'lt' | 'lte' | 'gt' | 'gte';
  value: number;
}

export type ItemPredicate =
  | { ilvl: ValuePredicate }
  | { rarity: Rarity }
  | { corrupted: boolean }
  | { sanctified: boolean }
  | { mirrored: boolean }
  | 'always'
  | 'never'
  | { all: ItemPredicate[] }
  | { any: ItemPredicate[] }
  | { not: ItemPredicate };

export interface TargetSpec {
  concept?: string | null;
  concept_any?: string[];
  affix?: AffixType | null;
  count?: number;
  min_tier?: number | null;
  allow_hybrid?: boolean;
}

export interface Target {
  prefixes?: TargetSpec[];
  suffixes?: TargetSpec[];
  constraints?: ItemPredicate[];
}

export interface DivEquiv {
  min: number;
  expected: number;
  max: number;
}

export interface Goal {
  target: Target;
  abandon_criteria?: ItemPredicate[];
  budget: DivEquiv;
}

export interface Stash {
  currencies?: Record<string, number>;
  omens?: Record<string, number>;
  unlimited?: boolean;
}

/// Phase B.5 — stop condition for a recurring step.
export interface ConceptCriterion {
  concept: string;
  min_tier: number;
  affix?: AffixType | null;
}

export interface StopPredicate {
  concepts?: ConceptCriterion[];
  max_mods?: number | null;
}

/// Phase B.4 — iteration estimate for a recurring step.
export interface LoopEstimate {
  mean_iterations: number;
  iter_stderr: number;
  total_cost: DivEquiv;
}

export type AdvisorAction =
  | { kind: 'apply_currency'; currency: string; omens: string[] }
  | { kind: 'activate_omen'; omen: string }
  | { kind: 'apply_hinekoras_lock' }
  | {
      kind: 'reveal';
      prefer: string[];
      use_abyssal_echoes: boolean;
      min_acceptable: string | null;
      abandon_if_no_match: boolean;
      /// Phase B.6 — bone the user is applying. `null` for legacy
      /// strategy-DSL reveals that don't bind a bone yet.
      bone?: string | null;
      /// Phase B.6 — omen pre-bound to this reveal action.
      omen?: string | null;
    }
  | { kind: 'recombine'; other_item_id: string; omens: string[] }
  | { kind: 'stop' }
  | { kind: 'abandon'; reason: string }
  | { kind: 'guidance'; note: string }
  /// Phase B.4 — recurring step (loop body + stop predicate). The
  /// associated `Recommendation.loop_estimate` carries iteration
  /// count and total-cost estimates the UI renders alongside.
  | { kind: 'recurring'; inner: AdvisorAction[]; stop: StopPredicate };

export type RecommendationSource =
  | { kind: 'rule'; id: string; confidence: 'verified' | 'community' | 'experimental' }
  | { kind: 'strategy'; id: string; step: string }
  | { kind: 'heuristic'; name: string };

export interface Recommendation {
  action: AdvisorAction;
  source: RecommendationSource;
  expected_cost: DivEquiv;
  /** Honest P(reach goal) = execution-reliability × {@link goal_progress},
   * NOT the raw step-execution probability. Low until the item actually
   * carries the target mods. */
  expected_prob: number;
  /** Structural goal-progress of the user's CURRENT item in [0,1]: the
   * fraction of the goal's target specs (prefixes + suffixes) it already
   * satisfies. Drives the "n/m specs" bar and the headline colour. */
  goal_progress: number;
  /** Standard error of the P(reach goal) estimate (Phase C.1), scaled by
   * goal_progress. 0 when the planner ran with mc_samples=1. */
  prob_stderr: number;
  score: number;
  rationale: string;
  depth: number;
  /** Phase B.4 — `loop_estimate` is set when `action.kind === 'recurring'`.
   * Carries the mean iteration count + stderr and the total cost band
   * the UI shows alongside the recurring-step card. */
  loop_estimate?: LoopEstimate | null;
}

export interface RecommendArgs {
  item: Item;
  goal: Goal;
  stash?: Stash;
  risk?: number;
  top_n?: number;
  depth?: number;
  /** UI request token used to discard stale streaming progress events. */
  request_id?: number | null;
}

export interface RecommendResponse {
  recommendations: Recommendation[];
  patch: string;
  rule_count: number;
  strategy_count: number;
  mod_count: number;
  bundle_path: string | null;
}

/// Phase C.2 streaming progress event.
export interface StreamingProgressEvent {
  request_id?: number | null;
  depth: number;
  recommendations: Recommendation[];
  is_final: boolean;
  patch: string;
}

/// Topic the streaming planner emits to.
export const ADVISOR_PROGRESS_EVENT = 'advisor://progress';

/// M16.4 — trained-model cache status (historical: was surfaced by the
/// retired Tauri `trained_model_status` command; kept for the roadmap
/// item that loads trained Q-tables into the WASM engine and renders a
/// topbar badge when the trained policy drives the pick).
export interface TrainedModelStatus {
  /** Number of `(goal × class)` models loaded into the planner cache. */
  models_loaded: number;
  /** Number of artefact files the loader skipped (parse errors, etc.). */
  files_skipped: number;
  /** Directory the loader scanned. Surfaced for the rebuild dialog. */
  cache_dir: string;
  /** True iff `cache_dir` exists on disk. */
  cache_dir_exists: boolean;
}

/** ADR-0014 phase 1 — result of `Engine.setPluginContent` (plugin-emitted
 * strategy/rule TOMLs installed with set semantics). */
export interface PluginContentView {
  strategies_added: number;
  rules_added: number;
  /** Per-document parse errors (warn-and-skip semantics). */
  errors: string[];
}

/** ADR-0014 — per-plugin status from the worker's `__loadPlugins`. */
export interface WorkerPluginInfo {
  name: string;
  strategies: number;
  rules: number;
  /** True when the plugin exports the phase 2 predicate surface. */
  predicates: boolean;
  error: string | null;
}

/** ADR-0014 — result of the worker's `__loadPlugins` message. */
export interface PluginLoadView {
  infos: WorkerPluginInfo[];
  content: PluginContentView;
}

/// Phase F — Wasm plugin info surfaced by the plugin host.
export interface PluginInfo {
  id: string;
  name: string;
  version: string;
  description: string;
  capabilities: string[];
  enabled: boolean;
  n_strategies: number;
  n_rules: number;
}

/// Phase E — meta builds aggregator.
export interface NicheTarget {
  concept: string;
  demand: number;
  demand_share: number;
  competition: number;
  score: number;
  rationale: string;
}

export interface MetaResponse {
  league: string;
  fetched_at: string;
  n_builds: number;
  niches: NicheTarget[];
}

/// Phase D.3 — trade search URL adapter.
export interface TradeModLine {
  mod_id: string;
  affix_type: 'prefix' | 'suffix' | 'implicit' | 'enchantment';
  fractured: boolean;
  text_template: string | null;
  values: number[];
}

export interface TradeSearchSummary {
  url: string;
  league: string;
  item_class: string;
  ilvl_min: number;
  mod_lines: TradeModLine[];
}

/// Phase D.1 — Client.txt log watcher.
export type ClientLogEvent =
  | { kind: 'area_entered'; area: string; line: string }
  | { kind: 'player_joined'; player: string; line: string }
  | { kind: 'death'; victim: string; killer: string | null; line: string }
  | { kind: 'whisper'; from: string; message: string; line: string }
  | { kind: 'other'; line: string };

export interface ClientLogStatus {
  watching: boolean;
  path: string | null;
}

export const CLIENT_LOG_EVENT = 'client-log://event';

/// Phase C.3 — bulk simulation distribution.
export interface TrialDistribution {
  n_trials: number;
  success_rate: number;
  success_rate_stderr: number;
  mean_change_count: number;
  /// Histogram of change_count values: bucket → count.
  change_count_histogram: Record<number, number>;
  cost_per_trial_div: number;
  total_cost_div_expected: number;
}

/// Advanced Mod Descriptions header affix kind.
export type AnnotationAffix = 'prefix' | 'suffix' | 'implicit';

/// Parsed `{ <Affix> Modifier "<name>" (Tier: N) — tags }` header.
export interface ModAnnotation {
  affix: AnnotationAffix;
  name: string;
  tier: number | null;
  tags: string[];
}

/// A rolled stat value with the tier's `[min, max]` range (`90(80-91)`).
export interface StatRoll {
  value: number;
  min: number;
  max: number;
}

export interface ModLine {
  text: string;
  fractured: boolean;
  crafted: boolean;
  implicit_tag: boolean;
  /** Advanced format: `Desecrated` header qualifier. */
  desecrated?: boolean;
  /** Advanced format: the `{ ... Modifier ... }` annotation. */
  annotation?: ModAnnotation | null;
  /** Advanced format: per-stat rolled values + ranges, left-to-right. */
  rolls?: StatRoll[];
}

export interface ParsedItem {
  item_class: string;
  rarity: Rarity;
  name: string | null;
  base: string;
  ilvl: number;
  quality: number;
  requirements: {
    level: number | null;
    str_req: number | null;
    dex_req: number | null;
    int_req: number | null;
  };
  /** Raw `Sockets:` line value (Advanced format). */
  sockets?: string | null;
  /** True when the input used the Advanced Mod Descriptions format. */
  advanced?: boolean;
  implicits: ModLine[];
  explicits: ModLine[];
  corrupted: boolean;
  mirrored: boolean;
  sanctified: boolean;
}

export interface ParseClipboardResponse {
  parsed: ParsedItem;
  item: Item;
  unresolved: string[];
  /** Resolved base display name (Magic affixes stripped), e.g. "Tasalian Focus". */
  base_display_name?: string | null;
  /** Canonical item class id, e.g. "Focus". */
  item_class_id?: string | null;
  /** True when the base resolved to a real bundle base (correct mod pool). */
  base_resolved?: boolean;
  /** Non-fatal notices (unresolved base, ambiguity, …). */
  warnings?: string[];
}

/// One poe2scout currency entry (PascalCase fields come straight off the
/// poe2scout REST API; mirrors poc2-market's serde shape).
export interface PoeScoutCurrencyEntry {
  CurrencyItemId: number;
  ItemId: number;
  CurrencyCategoryId: number;
  ApiId: string;
  Text: string;
  CategoryApiId: string;
  IconUrl?: string | null;
  /** Price in BaseCurrency (Exalted Orb); null until the first price log. */
  CurrentPrice: number | null;
  CurrentQuantity?: number | null;
}

/// Composite poe2scout snapshot `applyPrices` consumes — the same shape the
/// native `fetch_snapshot` poller produces; the browser assembles it instead.
export interface PoeScoutSnapshot {
  league: string;
  /** Exalts per divine — converts entry prices to divine-equivalent. */
  divine_price_in_exalts: number;
  chaos_per_divine: number;
  /** api_id (slug) → entry. */
  entries: Record<string, PoeScoutCurrencyEntry>;
  /** ISO-8601 timestamp of the fetch. */
  fetched_at: string;
}

/// Summary returned by the engine's `applyPrices`.
export interface ApplyPricesView {
  /** Feed entries that mapped to an engine currency id and carried a price. */
  applied: number;
  /** Feed slugs with no engine mapping (sorted). */
  unmatched: string[];
}

/// One poe.ninja exchange-economy price entry. Both denominations are derived
/// from the raw `primaryValue` via the league's conversion rates.
export interface NinjaPriceEntry {
  /** Price expressed in divine orbs. */
  divine_value: number;
  /** Price expressed in exalted orbs. */
  exalt_value: number;
  /** Whether poe.ninja had a non-null market value for this item. */
  has_market_data: boolean;
}

/// Composite poe.ninja PoE2 exchange snapshot `applyNinjaPrices` consumes — the
/// PARALLEL source to poe2scout's `PoeScoutSnapshot`. Entries are keyed by
/// normalized display name and resolved onto engine currency ids via the fuzzy
/// matcher; the same shape the native `fetch_ninja_exchange` poller produces.
export interface NinjaExchangeSnapshot {
  league: string;
  /** normalize(name) → entry. */
  entries: Record<string, NinjaPriceEntry>;
  /** ISO-8601 timestamp of the fetch. */
  fetched_at: string;
}

/// Arguments for the engine's `resolveName` fuzzy matcher.
export interface ResolveNameArgs {
  /** The noisy / OCR-supplied name to resolve. */
  raw: string;
  /**
   * Optional ad-hoc candidate keys to match against. When omitted, the
   * matcher resolves over the valuator's currency display names.
   */
  candidates?: string[];
  /**
   * Optional locale code (`de` | `fr` | `pt` | `ru` | `sp`). When set, a
   * localized client/OCR name is translated to its canonical English form
   * before fuzzy scoring. Omit for English clients.
   */
  locale?: "de" | "fr" | "pt" | "ru" | "sp";
}

/// Arguments for the engine's batched `resolveNames` fuzzy matcher.
export interface ResolveNamesArgs {
  /** Noisy / OCR-supplied names, resolved in this order. */
  raws: string[];
  /**
   * Optional shared candidate keys. The engine builds one index for the full
   * batch. When omitted, each name uses the valuator's currency fallback.
   */
  candidates?: string[];
  /** Optional bundled client locale applied to every name in the batch. */
  locale?: "de" | "fr" | "pt" | "ru" | "sp";
}

/// Result returned by the engine's `resolveName` / `resolveNames`.
export interface ResolveView {
  /** The matched canonical key, or `null` when nothing resolved. */
  key: string | null;
  /** Confidence score in `[0, 1]` (`0` when unmatched). */
  score: number;
  /** Match stage: `exact` | `prefix` | `fuzzy` | `skeleton` | `currency` | `none`. */
  method: string;
}

export interface PriceRefreshMeta {
  league: string;
  fetched_at: string;
  applied_count: number;
  total_entries: number;
}

export interface RefreshPricesResponse {
  refreshed: boolean;
  meta: PriceRefreshMeta | null;
  error: string | null;
}

export interface ReloadBundleArgs {
  /** Optional explicit bundle path; null re-runs the XDG-aware search. */
  path?: string | null;
}

export interface ReloadBundleResponse {
  bundle_path: string | null;
  patch: string | null;
  mod_count: number;
  strategy_count: number;
}

/// Recovery hint surface (Phase B.2).
export interface RecoveryHintView {
  message: string;
  goto_step_id: string | null;
  added_cost_div: number | null;
  strategy_id: string;
  step_id: string;
}

export interface RecoveryStepView {
  step_id: string;
  /// Summary of the action that the strategy's on_failure step would
  /// take. Helps the user understand the default-failure flow before
  /// considering the alternative recovery hints.
  next_action_summary: string | null;
  hints: RecoveryHintView[];
}

/// State persisted to ~/.config/poc2/state.toml (Phase B.1).
export interface PersistedState {
  /// JSON-encoded Goal — opaque to the client; the backend reads/writes it.
  goal_json?: string | null;
  /// Last risk slider value (0..1).
  risk?: number | null;
  /// Last beam-search depth slider (1..5).
  depth?: number | null;
  /// Last top_n recommendations to fetch (1..10).
  top_n?: number | null;
  /// JSON-encoded Item — opaque to the backend; frontend validates shape.
  item_json?: string | null;
  /// Last selected market league.
  league?: string | null;
  /// Price auto-refresh interval in minutes.
  auto_refresh_minutes?: 0 | 5 | 30 | 60 | null;
  /// Free-form per-project notes (M17 Notes panel).
  notes?: string | null;
}

/// poe2scout league metadata (Phase B.3; fetched directly in Settings now).
export interface LeagueInfo {
  value: string;
  divine_price_in_exalts: number;
  chaos_per_divine: number;
}

/// Recipe — saved (Item, Goal) pair. Lives in
/// `~/.config/poc2/recipes/<name>.toml` (Phase B.4).
export interface Recipe {
  name: string;
  description: string;
  /// JSON-encoded Item.
  item_json: string;
  /// JSON-encoded Goal.
  goal_json: string;
  /// ISO-8601 / unix-epoch timestamp.
  created_at: string;
}

export interface RecipeSummary {
  name: string;
  description: string;
  created_at: string;
}

export interface AssetEntry {
  id: string;
  name: string;
  kind: string;
  detail_url: string | null;
  source_url: string | null;
  local_path: string | null;
  status: 'missing' | 'cached' | 'failed' | string;
  error: string | null;
}

export interface AssetManifest {
  generated_at: string;
  entries: AssetEntry[];
}

export interface AssetStatus {
  total: number;
  cached: number;
  missing: number;
  failed: number;
  root: string | null;
}

export interface EligibleStatView {
  stat_id: string;
  min: number;
  max: number;
}

export interface EligibleModView {
  mod_id: string;
  name: string | null;
  mod_group: string;
  affix_type: 'prefix' | 'suffix' | 'implicit' | 'enchantment';
  kind: string;
  concepts: string[];
  tags: string[];
  tier_index: number;
  tier_count: number;
  required_level: number;
  eligible_now: boolean;
  blocked_by_min_level: boolean;
  blocked_by_group: boolean;
  weight: number;
  weight_share: number;
  text_template: string | null;
  stats: EligibleStatView[];
  is_hybrid: boolean;
  is_essence_only: boolean;
  is_desecrated_only: boolean;
  is_local: boolean;
}

export interface EligibleModsResponse {
  item_class: string;
  data_available: boolean;
  affix: 'prefix' | 'suffix' | 'either';
  patch: string;
  mods: EligibleModView[];
}

export type RecordOutcome =
  | { kind: 'add_mod'; mod_id: string; roll?: number; currency?: string }
  | { kind: 'remove_mod'; affix: 'prefix' | 'suffix'; index: number }
  | {
      kind: 'replace_mod';
      remove_affix: 'prefix' | 'suffix';
      remove_index: number;
      add_mod_id: string;
      roll?: number;
    }
  | {
      /** Divine Orb (and omen variants) — reroll values on existing
       * mods within their current tier ranges. Rolls are absolute
       * numbers, one per stat in the parent mod definition's stats
       * array, in the same order. `sanctify=true` widens the bounds
       * to `[min × 0.8, max × 1.2]` and sets `Item.sanctified`. */
      kind: 'reroll_values';
      rolls: { slot: 'implicit' | 'prefix' | 'suffix'; index: number; values: number[] }[];
      sanctify?: boolean;
    }
  | { kind: 'set_rarity'; rarity: Rarity };

export interface RecordOutcomeResponse {
  item: Item;
  change: 'added' | 'removed' | 'replaced' | 'rarity' | 'rerolled' | 'sanctified';
  explanation: string;
}

/** Phase D.6 — backs the Divine Orb outcome dialog. Returned by the
 *  engine's `rerollableMods` method. One entry per slot eligible for
 *  Divine reroll on the current item, with the `[min, max]` bounds the
 *  player can record per stat. Sanctification widens the bounds; Omen
 *  of the Blessed restricts the result to implicits only. */
export interface RerollableStatView {
  stat_id: string;
  /** Lower bound the player can record. Sanctified band when active. */
  min: number;
  /** Upper bound the player can record. Sanctified band when active. */
  max: number;
  /** Strict (non-sanctified) lower bound. */
  strict_min: number;
  /** Strict (non-sanctified) upper bound. */
  strict_max: number;
  /** Currently rolled value for this stat. */
  current: number;
}

export interface RerollableMod {
  slot: 'implicit' | 'prefix' | 'suffix';
  index: number;
  mod_id: string;
  name: string | null;
  text_template: string | null;
  tier_index: number;
  tier_count: number;
  is_fractured: boolean;
  stats: RerollableStatView[];
}

export interface RerollableModsResponse {
  patch: string;
  /** True when Omen of Sanctification is active (widened bounds). */
  sanctify: boolean;
  /** True when Omen of the Blessed is active (implicits-only). */
  implicits_only: boolean;
  mods: RerollableMod[];
}

/// Phase A.2 — structured `CannotApply` reason from the engine, returned
/// by `check_can_apply` Tauri command. The OutcomeDialog and AdvisorPanel
/// use this in place of client-side rarity heuristics so the badge
/// matches the engine's verdict exactly.
export type CannotApplyView =
  | { kind: 'ok' }
  | { kind: 'wrong_rarity'; item_rarity: Rarity; expected: Rarity[] }
  | { kind: 'no_open_slots'; affix: string }
  | { kind: 'corrupted' }
  | { kind: 'mirrored' }
  | { kind: 'already_locked' }
  | { kind: 'fracture_requires_four_mods'; current: number }
  | { kind: 'recombinator_input_mismatch' }
  | { kind: 'other'; message: string }
  | { kind: 'unknown_currency' };

export interface HistoryEntry {
  id: string;
  timestamp: string;
  change: 'added' | 'removed' | 'replaced' | 'rarity' | string;
  explanation: string;
  action?: AdvisorAction | null;
  action_label?: string | null;
  cost_div?: number | null;
  materials?: MaterialUse[];
  /** Snapshot of the item before the change, used for Undo. */
  before: Item;
}

export interface MaterialUse {
  id: string;
  quantity: number;
}

export interface BaseSummary {
  id: string;
  name: string;
  class_pascal: string;
  class_display: string;
  drop_level: number;
  attribute_pool: string;
  tags: string[];
  release_state: string;
}

export type DatabaseSection = 'bases' | 'materials';

export interface DatabaseStatLine {
  label: string;
  value: string;
  help?: string | null;
}

export interface DatabaseBaseDetail {
  metadata_type: string;
  drop_level: number;
  class_display: string;
  attribute_pool: string;
  inventory_width: number;
  inventory_height: number;
  tags: string[];
  derived_stats: DatabaseStatLine[];
  requirements: string[];
  granted_effects: DatabaseStatLine[];
  class_notes: string[];
}

export interface DatabaseMaterialDetail {
  source_section: string;
  description: string;
  applies_to: string[];
  tags: string[];
  raw_fields: DatabaseStatLine[];
}

export interface DatabaseEntrySummary {
  id: string;
  name: string;
  section: DatabaseSection;
  category: string;
  kind: string;
  icon_url?: string | null;
  detail_url?: string | null;
  tags: string[];
  description?: string | null;
  base?: BaseSummary | null;
}

export interface DatabaseEntryDetail {
  summary: DatabaseEntrySummary;
  base?: DatabaseBaseDetail | null;
  material?: DatabaseMaterialDetail | null;
}

export interface BaseIconManifestEntry {
  name: string;
  class_pascal: string;
  rel: string;
  source_url: string;
  drop_level: number;
  attribute_pool: string;
}

export interface BaseIconManifest {
  version: number;
  fetched_at: string;
  entries: Record<string, BaseIconManifestEntry>;
  missing: { name: string; class_pascal: string; reason: string; detail_url: string }[];
}

// ---------------------------------------------------------------------------
// Genesis Tree (0.5) — mirrors `crates/poc2-wasm/src/commands/genesis.rs`.
// ---------------------------------------------------------------------------

export type GenesisBranch = 'currency' | 'ring' | 'amulet' | 'belt' | 'breachstone';

export interface GenesisWomb {
  branch: GenesisBranch;
  display_name: string;
  wombgift: string;
  gift_art: string;
  points: number;
  icon_normal: string;
  icon_notable: string;
  blurb: string;
}

export interface GenesisNode {
  id: string;
  branch: GenesisBranch;
  name: string;
  notable: boolean;
  icon: string;
  description: string;
  x: number;
  y: number;
  start: boolean;
  womb_slot: boolean;
  connections: string[];
}

export interface GenesisPresetStep {
  node: string;
  why: string;
  priority: number;
  /** Filler step: spend leftover points on any highlighted copy. */
  fill: boolean;
  /** Respec / swap-in choice — outside the guaranteed point budget. */
  optional: boolean;
  /** Resolved node ids to allocate (graph-verified). */
  node_ids: string[];
  /** Connector nodes the shortest path forces along the way. */
  connector_ids: string[];
  /** Cumulative allocatable points after this step (incl. connectors). */
  points_after: number;
}

export interface GenesisPresetAvoid {
  node: string;
  why: string;
  node_ids: string[];
}

export interface GenesisPreset {
  id: string;
  name: string;
  womb: GenesisBranch;
  /** "measured" | "official" | "community" — community estimates throughout. */
  confidence: string;
  summary: string;
  sources: string[];
  steps: GenesisPresetStep[];
  avoid: GenesisPresetAvoid[];
  gift_advice: string;
  /** Points required by the non-optional path (incl. connectors). */
  core_points: number;
  /** The womb's point cap. */
  points_cap: number;
}

export interface GenesisVideo {
  title: string;
  channel: string;
  url: string;
}

export interface GenesisTreeView {
  available: boolean;
  wombs: GenesisWomb[];
  nodes: GenesisNode[];
  presets: GenesisPreset[];
  farming_notes: string[];
  videos: GenesisVideo[];
}

/** Manifest written by `cargo run -p poc2-pipeline --bin fetch-genesis-assets`. */
export interface GenesisIconManifest {
  version: number;
  fetched_at: string;
  entries: Record<string, string>;
  missing: string[];
}
