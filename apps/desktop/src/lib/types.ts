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
    | 'corrupted';
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
  expected_prob: number;
  /** Standard error of the success probability estimate (Phase C.1).
   * 0 when the planner ran with mc_samples=1. */
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

/// M16.4 — trained-model cache status surfaced via the
/// `trained_model_status` Tauri command. The desktop UI reads this on
/// startup (and after `reload_bundle`) to render a topbar badge that
/// tells the user whether the planner is consulting trained Q-tables
/// or running pure heuristic ranking.
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

export interface ModLine {
  text: string;
  fractured: boolean;
  crafted: boolean;
  implicit_tag: boolean;
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

/// Returned by the `list_leagues` Tauri command (Phase B.3).
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
 *  `rerollable_mods` Tauri command. One entry per slot eligible for
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
