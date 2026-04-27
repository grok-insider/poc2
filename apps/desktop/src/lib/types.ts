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
    }
  | { kind: 'recombine'; other_item_id: string; omens: string[] }
  | { kind: 'stop' }
  | { kind: 'abandon'; reason: string }
  | { kind: 'guidance'; note: string };

export type RecommendationSource =
  | { kind: 'rule'; id: string; confidence: 'verified' | 'community' | 'experimental' }
  | { kind: 'strategy'; id: string; step: string }
  | { kind: 'heuristic'; name: string };

export interface Recommendation {
  action: AdvisorAction;
  source: RecommendationSource;
  expected_cost: DivEquiv;
  expected_prob: number;
  score: number;
  rationale: string;
  depth: number;
}

export interface RecommendArgs {
  item: Item;
  goal: Goal;
  stash?: Stash;
  risk?: number;
  top_n?: number;
  depth?: number;
}

export interface RecommendResponse {
  recommendations: Recommendation[];
  patch: string;
  rule_count: number;
  strategy_count: number;
  mod_count: number;
  bundle_path: string | null;
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
