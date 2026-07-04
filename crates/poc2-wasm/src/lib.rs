//! WebAssembly bindings for the Path of Crafting 2 advisor.
//!
//! The whole Rust engine (registries, rules, strategies, beam-search planner,
//! Monte-Carlo simulator) runs **client-side in the browser**. JS builds an
//! [`Engine`] once from the data-bundle bytes, then calls `recommend` (and the
//! other methods) per interaction. Everything crosses the JS boundary as JSON
//! strings, matching the TypeScript IPC contract the previous Tauri UI used.
//!
//! Nothing here adds engine logic — it is a thin adapter over
//! [`poc2_advisor::plan`] and friends.

#![forbid(unsafe_code)]

use std::collections::HashMap;

use poc2_advisor::featurize::featurize;
use poc2_advisor::training::{
    enumerate_solver_actions, goal_hash, load_artefacts_str, solve_goal, CraftingTask,
    SolveProfile, TrainedModelCache,
};
use poc2_advisor::{plan, AdvisorAction, BeamConfig, Goal, PlanInput, Recommendation, Stash};
use poc2_engine::currency::{DefaultCurrencyResolver, Essence};
use poc2_engine::ids::{BaseTypeId, ItemClassId};
use poc2_engine::item::Item;
use poc2_engine::patch::{League, PatchVersion};
use poc2_engine::{BaseRegistry, ModRegistry};
use poc2_market::Valuator;
use poc2_rules::RuleSet;
use poc2_strategies::{seed_strategies, PluginPredicateDispatch, StrategyRegistry};
use wasm_bindgen::prelude::*;

mod commands;

/// Install a panic hook that forwards Rust panics to `console.error` (dev aid).
#[wasm_bindgen(start)]
pub fn start() {
    #[cfg(feature = "console_error_panic_hook")]
    console_error_panic_hook::set_once();
}

/// In-memory engine state, built once from a data bundle. Owns every registry
/// the planner borrows; each call constructs a [`PlanInput`] referencing it.
struct EngineState {
    registry: ModRegistry,
    base_registry: BaseRegistry,
    strategies: StrategyRegistry,
    rules: RuleSet,
    resolver: DefaultCurrencyResolver,
    valuator: Valuator,
    patch: PatchVersion,
    league: League,
    /// Trained Q-table cache (M16.4 + ADR-0015): warm-started from the
    /// optional `/trained-models.json` static asset via
    /// [`Engine::load_trained_models`], then grown **on demand** — a
    /// `recommend` whose `(goal, item-class)` misses (or whose cached
    /// model doesn't cover the current item's state) solves the goal
    /// right there via `training::solve` and caches the exact policy.
    /// `None` → no models yet (first recommend populates).
    trained_models: Option<TrainedModelCache>,
    /// `(goal × class)` models currently held (diagnostic; mirrors the
    /// cache len across loads, solves, and invalidation clears).
    trained_model_count: usize,
    /// Essence catalogue (also registered in `resolver`); the on-demand
    /// solver enumerates goal-relevant essences from it.
    essences: Vec<Essence>,
    /// ADR-0014 phase 2 — synchronous JS callback
    /// `(pluginId, name, itemJson, argsJson) -> boolean` evaluating a
    /// plugin custom predicate. The worker owns the plugin instances;
    /// the engine only calls back. `None` → `ItemPredicate::Custom`
    /// evaluates to false.
    plugin_dispatch_fn: Option<js_sys::Function>,
    bundle: poc2_data::Bundle,
    /// Clipboard `Item Class:` display string (lowercased) → canonical class id.
    class_by_display: HashMap<String, ItemClassId>,
    /// `(class, lowercased base name)` → real bundle base id (for paste-import).
    base_by_class_name: HashMap<(ItemClassId, String), BaseTypeId>,
}

/// ADR-0014 phase 2 — [`PluginPredicateDispatch`] over a synchronous JS
/// callback living in the same worker thread as the engine. Errors and
/// non-boolean returns surface as `Err`, which the predicate evaluator
/// downgrades to `false` (a misbehaving plugin never tanks planning).
struct JsPluginDispatch<'a> {
    f: &'a js_sys::Function,
}

impl PluginPredicateDispatch for JsPluginDispatch<'_> {
    fn dispatch(
        &self,
        plugin_id: &str,
        name: &str,
        item: &Item,
        args: &serde_json::Value,
    ) -> Result<bool, String> {
        let item_json = serde_json::to_string(item).map_err(|e| format!("item serialize: {e}"))?;
        let args_json = serde_json::to_string(args).map_err(|e| format!("args serialize: {e}"))?;
        let js_args = js_sys::Array::of4(
            &JsValue::from_str(plugin_id),
            &JsValue::from_str(name),
            &JsValue::from_str(&item_json),
            &JsValue::from_str(&args_json),
        );
        let ret = self
            .f
            .apply(&JsValue::NULL, &js_args)
            .map_err(|e| format!("plugin dispatch threw: {e:?}"))?;
        ret.as_bool()
            .ok_or_else(|| "plugin dispatch must return a boolean".to_string())
    }
}

impl EngineState {
    fn from_bundle(bundle: poc2_data::Bundle) -> Self {
        let registry = ModRegistry::from_mods(bundle.mods.clone(), bundle.weights.clone());
        let base_registry = BaseRegistry::from_bases(bundle.base_items.clone());
        let strategies = StrategyRegistry::from_strategies(seed_strategies());
        let rules = RuleSet::from_rules(poc2_rules::seed_rules());
        // Alloys + Emotions share the resolver's alloy slot (emotions are
        // base-targeted alloys mechanically).
        let mut alloy_likes = bundle.alloy_catalogue();
        alloy_likes.extend(bundle.emotion_catalogue());
        let essences = bundle.essence_catalogue();
        let resolver = DefaultCurrencyResolver::new()
            .with_essences(essences.clone())
            .with_catalysts(bundle.catalyst_catalogue())
            .with_alloys(alloy_likes);
        let patch = bundle.header.game_patch;
        let class_by_display = commands::parse::build_class_index(&bundle);
        let base_by_class_name = commands::parse::build_base_index(&bundle);
        Self {
            registry,
            base_registry,
            strategies,
            rules,
            resolver,
            valuator: Valuator::default(),
            patch,
            league: League::current(),
            trained_models: None,
            trained_model_count: 0,
            essences,
            plugin_dispatch_fn: None,
            bundle,
            class_by_display,
            base_by_class_name,
        }
    }

    /// Hard cap on cached `(goal × class)` models. On-demand solving
    /// accumulates one entry per distinct goal; past the cap the cache is
    /// cleared wholesale (crude but sufficient — a solve is sub-second,
    /// and a user cycling through 250+ goals in one session is churn, not
    /// a working set).
    const TRAINED_CACHE_CAP: usize = 256;

    /// ADR-0015 — make sure a trained model exists (and covers the
    /// current item's state) for `(goal, item-class)` before planning.
    /// Cache miss or root-coverage miss → solve the goal on the spot at
    /// the on-demand budget and cache the exact policy pair. Empty-target
    /// goals never solve (no terminal to reach).
    fn ensure_goal_model(&mut self, item: &Item, goal: &Goal) {
        if goal.target.prefixes.is_empty() && goal.target.suffixes.is_empty() {
            return;
        }
        let item_class = self.base_registry.resolve_item_class(item);
        let goal_h = goal_hash(goal);
        let root_fv = featurize(item, goal, &self.registry);
        if let Some(cache) = self.trained_models.as_ref() {
            if let Some(model) = cache.lookup(goal_h, &item_class) {
                if model.covers_state(root_fv) {
                    return;
                }
                // Cached, but solved from a different starting point —
                // re-solve from the current item so lookups hit again.
            }
        }

        let task = CraftingTask {
            initial_item: item.clone(),
            goal: goal.clone(),
            registry: &self.registry,
            base_registry: &self.base_registry,
            resolver: &self.resolver,
            patch: self.patch,
            omens: poc2_engine::omen::OmenSet::new(),
        };
        let actions = enumerate_solver_actions(goal, &item_class, &self.essences, &self.registry);
        let solved = solve_goal(&task, &item_class, &actions, SolveProfile::on_demand());

        let mut cache = self.trained_models.take().unwrap_or_default();
        if cache.len() >= Self::TRAINED_CACHE_CAP {
            cache.clear();
        }
        cache.insert_pair(solved.path, Some(solved.cost));
        self.trained_model_count = cache.len();
        self.trained_models = Some(cache);
    }

    /// Drop every trained model (file-loaded and on-demand). Called when
    /// the League ruleset or plugin content/dispatch changes — both alter
    /// candidate/goal semantics, so cached policies may be stale. The
    /// next recommend re-solves its goal on demand.
    fn invalidate_trained_models(&mut self) {
        self.trained_models = None;
        self.trained_model_count = 0;
    }

    fn recommend(
        &mut self,
        item: &Item,
        goal: &Goal,
        risk: f64,
        depth: u32,
        top_n: u32,
    ) -> Vec<Recommendation> {
        self.ensure_goal_model(item, goal);
        let stash = Stash::unlimited();
        let config = BeamConfig {
            depth: depth.max(1),
            risk: risk.clamp(0.0, 1.0),
            top_n: top_n.max(1),
            width: top_n.max(3),
            ..BeamConfig::default()
        };
        let js_dispatch = self
            .plugin_dispatch_fn
            .as_ref()
            .map(|f| JsPluginDispatch { f });
        let input = PlanInput {
            item: item.clone(),
            goal: goal.clone(),
            rules: &self.rules,
            strategies: &self.strategies,
            registry: &self.registry,
            resolver: &self.resolver,
            valuator: &self.valuator,
            stash: &stash,
            patch: self.patch,
            league: self.league,
            config,
            plugin_dispatch: js_dispatch
                .as_ref()
                .map(|d| d as &dyn PluginPredicateDispatch),
            base_registry: Some(&self.base_registry),
            trained_models: self.trained_models.as_ref(),
        };
        plan(&input)
    }
}

/// The JS-facing engine handle. Construct once with the bundle bytes; reuse.
#[wasm_bindgen]
pub struct Engine {
    state: EngineState,
}

#[wasm_bindgen]
impl Engine {
    /// Build the engine from raw data-bundle bytes (gzip `*.bundle.json.gz` or
    /// plain `*.bundle.json` — auto-detected).
    #[wasm_bindgen(constructor)]
    pub fn new(bundle_bytes: &[u8]) -> Result<Engine, JsError> {
        let bundle = poc2_data::io::read_bundle_bytes(bundle_bytes)
            .map_err(|e| JsError::new(&format!("bundle load failed: {e}")))?;
        Ok(Engine {
            state: EngineState::from_bundle(bundle),
        })
    }

    /// The game patch the loaded bundle targets (e.g. "0.5.0").
    #[wasm_bindgen(getter)]
    pub fn patch(&self) -> String {
        self.state.patch.to_string()
    }

    /// The active engine League ruleset: `"standard"` or `"challenge"`.
    /// Drives cross-version gating (Recombinator and the Corruption /
    /// Homogenising omens are Standard-only in 0.5).
    #[wasm_bindgen(getter)]
    pub fn league(&self) -> String {
        match self.state.league {
            League::Standard => "standard".to_string(),
            League::Challenge => "challenge".to_string(),
        }
    }

    /// Switch the engine League ruleset. Accepts `"standard"` or
    /// `"challenge"` (the 0.5 challenge league is Runes of Aldur).
    #[wasm_bindgen(js_name = setLeague)]
    pub fn set_league(&mut self, league: &str) -> Result<(), JsError> {
        let new_league = match league.to_ascii_lowercase().as_str() {
            "standard" => League::Standard,
            "challenge" => League::Challenge,
            other => {
                return Err(JsError::new(&format!(
                    "unknown league `{other}` (expected \"standard\" or \"challenge\")"
                )))
            }
        };
        if new_league != self.state.league {
            self.state.league = new_league;
            // League gates the candidate set (recombinator, omens), so
            // cached trained policies may be stale — clear; on-demand
            // solving rebuilds per goal (ADR-0015).
            self.state.invalidate_trained_models();
        }
        Ok(())
    }

    /// Number of mods indexed (diagnostic).
    #[wasm_bindgen(getter, js_name = modCount)]
    pub fn mod_count(&self) -> usize {
        self.state.bundle.mods.len()
    }

    /// Load trained Q-table artefacts (the JSON `train-advisor` writes —
    /// a `Vec<TrainedModelArtefact>`). Merges into the existing cache;
    /// artefacts trained against a different bundle/engine schema are
    /// version-skipped (heuristic planning stays correct without them).
    /// Returns JSON `{ "loaded": n, "version_skipped": n, "total": n }`.
    #[wasm_bindgen(js_name = loadTrainedModels)]
    pub fn load_trained_models(&mut self, artefacts_json: &str) -> Result<String, JsError> {
        let mut cache = self.state.trained_models.take().unwrap_or_default();
        let (loaded, version_skipped) = load_artefacts_str(artefacts_json, &mut cache)
            .map_err(|e| JsError::new(&format!("trained-models load failed: {e}")))?;
        // Mirror the cache length (same-key merges don't double-count).
        self.state.trained_model_count = cache.len();
        // An all-stale file leaves the cache as-is; only keep a cache
        // when it actually holds models (planner treats None as "off").
        if self.state.trained_model_count > 0 {
            self.state.trained_models = Some(cache);
        }
        Ok(serde_json::json!({
            "loaded": loaded,
            "version_skipped": version_skipped,
            "total": self.state.trained_model_count,
        })
        .to_string())
    }

    /// Number of trained `(goal × class)` models the planner consults
    /// (0 = pure heuristic planning).
    #[wasm_bindgen(getter, js_name = trainedModelCount)]
    pub fn trained_model_count(&self) -> usize {
        self.state.trained_model_count
    }

    /// ADR-0014 phase 1 — install plugin-emitted content. Inputs are
    /// JSON arrays of TOML documents (the browser host extracts them
    /// from plugin wasm via the SDK's `list_strategies` / `list_rules`
    /// exports). **Set semantics**: registries rebuild as seeds + the
    /// given content, so repeated calls never duplicate. Documents that
    /// fail to parse are skipped and reported, mirroring the native
    /// host's warn-and-skip. Returns JSON
    /// `{ "strategies_added": n, "rules_added": n, "errors": [..] }`.
    #[wasm_bindgen(js_name = setPluginContent)]
    pub fn set_plugin_content(
        &mut self,
        strategies_json: &str,
        rules_json: &str,
    ) -> Result<String, JsError> {
        let strategy_tomls: Vec<String> = serde_json::from_str(strategies_json)
            .map_err(|e| JsError::new(&format!("strategies arg must be a JSON string[]: {e}")))?;
        let rule_tomls: Vec<String> = serde_json::from_str(rules_json)
            .map_err(|e| JsError::new(&format!("rules arg must be a JSON string[]: {e}")))?;

        let mut errors: Vec<String> = Vec::new();

        let mut strategies = seed_strategies();
        let mut strategies_added = 0usize;
        for (i, toml) in strategy_tomls.iter().enumerate() {
            match poc2_strategies::load_strategy_str(toml) {
                Ok(s) => {
                    strategies.push(s);
                    strategies_added += 1;
                }
                Err(e) => errors.push(format!("strategy[{i}]: {e}")),
            }
        }

        let mut rules = poc2_rules::seed_rules();
        let mut rules_added = 0usize;
        for (i, toml) in rule_tomls.iter().enumerate() {
            match poc2_rules::load_rules_str(toml) {
                Ok(rs) => {
                    rules_added += rs.len();
                    rules.extend(rs);
                }
                Err(e) => errors.push(format!("rules[{i}]: {e}")),
            }
        }

        self.state.strategies = StrategyRegistry::from_strategies(strategies);
        self.state.rules = RuleSet::from_rules(rules);

        Ok(serde_json::json!({
            "strategies_added": strategies_added,
            "rules_added": rules_added,
            "errors": errors,
        })
        .to_string())
    }

    /// ADR-0014 phase 2 — install the synchronous plugin-predicate
    /// dispatch callback `(pluginId, name, itemJson, argsJson) ->
    /// boolean`. The worker owns the plugin instances and the perf
    /// guard; the engine calls back during planning whenever a rule or
    /// strategy references an `ItemPredicate::Custom`.
    #[wasm_bindgen(js_name = setPluginDispatch)]
    pub fn set_plugin_dispatch(&mut self, f: &js_sys::Function) {
        self.state.plugin_dispatch_fn = Some(f.clone());
        // Custom predicates can appear in goal abandon criteria /
        // constraints — trained policies solved without them are stale.
        self.state.invalidate_trained_models();
    }

    /// Remove the plugin dispatch callback — `Custom` predicates return
    /// to evaluating as false.
    ///
    /// Invalidates trained models only when a dispatch was actually
    /// installed: the worker's plugin loader calls this unconditionally at
    /// boot when zero plugins are configured, and a no-op clear must not
    /// wipe the freshly warm-started cache (ADR-0015).
    #[wasm_bindgen(js_name = clearPluginDispatch)]
    pub fn clear_plugin_dispatch(&mut self) {
        if self.state.plugin_dispatch_fn.take().is_some() {
            self.state.invalidate_trained_models();
        }
    }

    /// Recommend the top-N next actions. `item_json` / `goal_json` are JSON of
    /// the engine `Item` / advisor `Goal`; returns JSON of `Vec<Recommendation>`.
    ///
    /// `&mut self` since ADR-0015: a cache-missing goal is solved on
    /// demand (exact analytic model + value iteration, sub-second at the
    /// on-demand budget) and the resulting policy pair is cached — check
    /// [`Engine::trained_model_count`] to observe growth. The worker is
    /// single-threaded, so the mutation is race-free.
    pub fn recommend(
        &mut self,
        item_json: &str,
        goal_json: &str,
        risk: f64,
        depth: u32,
        top_n: u32,
    ) -> Result<String, JsError> {
        let item: Item = serde_json::from_str(item_json)
            .map_err(|e| JsError::new(&format!("bad item json: {e}")))?;
        let goal: Goal = serde_json::from_str(goal_json)
            .map_err(|e| JsError::new(&format!("bad goal json: {e}")))?;
        let recs = self.state.recommend(&item, &goal, risk, depth, top_n);
        serde_json::to_string(&recs).map_err(|e| JsError::new(&format!("serialize failed: {e}")))
    }

    // ===== parse =====
    /// Parse raw PoE2 clipboard item text into an engine `Item`, resolving the
    /// item class + base against the bundle so the engine applies the correct
    /// attribute-variant modifier pool. Returns JSON of `ParseClipboardResponse`.
    pub fn parse(&self, text: &str) -> Result<String, JsError> {
        let ctx = commands::parse::ParseContext {
            registry: &self.state.registry,
            class_by_display: &self.state.class_by_display,
            base_by_class_name: &self.state.base_by_class_name,
        };
        let resp = commands::parse::parse_item(&ctx, text).map_err(|e| JsError::new(&e))?;
        serde_json::to_string(&resp).map_err(|e| JsError::new(&format!("serialize failed: {e}")))
    }

    // ===== canapply =====
    /// Check whether a currency can apply to an item. `item_json` is JSON of the
    /// engine `Item`; `currency` is the currency id string. Returns JSON of
    /// `CannotApplyView` (a `{ kind, ... }` tagged union mirroring the desktop
    /// `check_can_apply` IPC contract).
    #[wasm_bindgen(js_name = checkCanApply)]
    pub fn check_can_apply(&self, item_json: &str, currency: &str) -> Result<String, JsError> {
        let item: Item = serde_json::from_str(item_json)
            .map_err(|e| JsError::new(&format!("bad item json: {e}")))?;
        let view = commands::canapply::check_can_apply(&self.state.resolver, &item, currency);
        serde_json::to_string(&view).map_err(|e| JsError::new(&format!("serialize failed: {e}")))
    }

    // ===== eligible =====
    /// Enumerate eligible/blocked mods for an (item, affix) slot. `args_json`
    /// is JSON of `{ item, affix?, min_required_level? }`; returns JSON of
    /// `EligibleModsResponse`.
    #[wasm_bindgen(js_name = eligibleMods)]
    pub fn eligible_mods(&self, args_json: &str) -> Result<String, JsError> {
        use commands::eligible::{default_affix_slot, AffixSlotFilter};

        #[derive(serde::Deserialize)]
        struct EligibleModsArgs {
            item: Item,
            #[serde(default = "default_affix_slot")]
            affix: AffixSlotFilter,
            #[serde(default)]
            min_required_level: u32,
        }

        let args: EligibleModsArgs = serde_json::from_str(args_json)
            .map_err(|e| JsError::new(&format!("bad eligible args json: {e}")))?;
        let resp = commands::eligible::eligible_mods(
            &self.state.registry,
            &self.state.base_registry,
            &args.item,
            args.affix,
            args.min_required_level,
            self.state.patch,
        );
        serde_json::to_string(&resp).map_err(|e| JsError::new(&format!("serialize failed: {e}")))
    }

    // ===== reroll =====
    /// Enumerate the mods a Divine-style reroll would touch. `item_json` is JSON
    /// of the engine `Item`; `omen` is the active omen id, if any (e.g.
    /// `"OmenOfSanctification"` / `"OmenOfTheBlessed"`). Returns JSON of
    /// `RerollableModsResponse`.
    #[wasm_bindgen(js_name = rerollableMods)]
    pub fn rerollable_mods(
        &self,
        item_json: &str,
        omen: Option<String>,
    ) -> Result<String, JsError> {
        let item: Item = serde_json::from_str(item_json)
            .map_err(|e| JsError::new(&format!("bad item json: {e}")))?;
        let resp = commands::reroll::rerollable_mods(
            &self.state.registry,
            &item,
            omen.as_deref(),
            self.state.patch,
        );
        serde_json::to_string(&resp).map_err(|e| JsError::new(&format!("serialize failed: {e}")))
    }

    // ===== outcome =====
    /// Apply a recorded crafting outcome (add/remove/replace/reroll/set_rarity)
    /// to an item. `args_json` is a serialized `RecordOutcomeArgs`; the result
    /// is a serialized `RecordOutcomeResponse`.
    #[wasm_bindgen(js_name = recordOutcome)]
    pub fn record_outcome(&self, args_json: &str) -> Result<String, JsError> {
        let args: commands::outcome::RecordOutcomeArgs = serde_json::from_str(args_json)
            .map_err(|e| JsError::new(&format!("deserialize failed: {e}")))?;
        let resp = commands::outcome::record_outcome(&self.state.registry, args)
            .map_err(|e| JsError::new(&e))?;
        serde_json::to_string(&resp).map_err(|e| JsError::new(&format!("serialize failed: {e}")))
    }

    // ===== sim =====
    // Added to `impl Engine { ... }` (the existing `#[wasm_bindgen] impl Engine` block).
    // Requires `pub mod commands { pub mod sim; }` near the top of lib.rs and that
    // `AdvisorAction` is imported (added to the existing `use poc2_advisor::{...}`).

    /// Run `n_trials` Monte-Carlo simulations of a single action against an
    /// item. `item_json` / `action_json` are JSON of the engine `Item` /
    /// advisor `AdvisorAction`; returns JSON of a `TrialDistribution`.
    #[wasm_bindgen(js_name = runNTrials)]
    pub fn run_n_trials(
        &self,
        item_json: &str,
        action_json: &str,
        n_trials: u32,
        seed: u64,
    ) -> Result<String, JsError> {
        let item: Item = serde_json::from_str(item_json)
            .map_err(|e| JsError::new(&format!("bad item json: {e}")))?;
        let action: AdvisorAction = serde_json::from_str(action_json)
            .map_err(|e| JsError::new(&format!("bad action json: {e}")))?;
        let dist = commands::sim::run_n_trials(
            &self.state.registry,
            &self.state.resolver,
            &self.state.valuator,
            &item,
            &action,
            n_trials,
            seed,
            self.state.patch,
        );
        serde_json::to_string(&dist).map_err(|e| JsError::new(&format!("serialize failed: {e}")))
    }

    // ===== recovery =====
    /// Recovery hints for a strategy step. Returns JSON of `RecoveryStepView`.
    #[wasm_bindgen(js_name = recoveryHints)]
    pub fn recovery_hints(&self, strategy_id: &str, step_id: &str) -> Result<String, JsError> {
        let view = commands::recovery::recovery_hints(
            &self.state.strategies,
            strategy_id.to_string(),
            step_id.to_string(),
        )
        .map_err(|e| JsError::new(&e))?;
        serde_json::to_string(&view).map_err(|e| JsError::new(&format!("serialize failed: {e}")))
    }

    // ===== prices =====
    /// Apply a live poe2scout price snapshot to the engine's valuator.
    /// `snapshot_json` is JSON of `PoeScoutSnapshot` (the shape the native
    /// `fetch_snapshot` poller produces; the browser fetches it instead).
    /// Returns JSON of `ApplyPricesView` (`{ applied, unmatched }`).
    #[wasm_bindgen(js_name = applyPrices)]
    pub fn apply_prices(&mut self, snapshot_json: &str) -> Result<String, JsError> {
        let snapshot: poc2_market::PoeScoutSnapshot = serde_json::from_str(snapshot_json)
            .map_err(|e| JsError::new(&format!("bad snapshot json: {e}")))?;
        let view = commands::prices::apply_prices(&mut self.state.valuator, &snapshot);
        serde_json::to_string(&view).map_err(|e| JsError::new(&format!("serialize failed: {e}")))
    }

    /// Apply a live poe.ninja PoE2 exchange snapshot to the engine's valuator —
    /// the PARALLEL source to `applyPrices` (poe2scout). `snapshot_json` is JSON
    /// of `NinjaExchangeSnapshot` (the shape the native `fetch_ninja_exchange`
    /// poller produces; the browser fetches it instead). Entries are keyed by
    /// display name and resolved via the fuzzy matcher. Returns JSON of
    /// `ApplyPricesView` (`{ applied, unmatched }`), where `unmatched` lists
    /// market-data entries whose name didn't resolve.
    #[wasm_bindgen(js_name = applyNinjaPrices)]
    pub fn apply_ninja_prices(&mut self, snapshot_json: &str) -> Result<String, JsError> {
        let snapshot: poc2_market::NinjaExchangeSnapshot = serde_json::from_str(snapshot_json)
            .map_err(|e| JsError::new(&format!("bad snapshot json: {e}")))?;
        let view = commands::ninja_prices::apply_ninja_prices(&mut self.state.valuator, &snapshot);
        serde_json::to_string(&view).map_err(|e| JsError::new(&format!("serialize failed: {e}")))
    }

    // ===== resolve =====
    /// Fuzzy-resolve a noisy item/currency name onto a canonical key.
    /// `args_json` is JSON of `{ raw, candidates?, locale? }`: with
    /// `candidates` the lookup is over that ad-hoc list; without it, over the
    /// engine valuator's currency display names (the matched `CurrencyId`
    /// string). `locale` (one of `de`/`fr`/`pt`/`ru`/`sp`) translates a
    /// localized client name to English before scoring. Returns JSON of
    /// `ResolveView` (`{ key, score, method }`).
    #[wasm_bindgen(js_name = resolveName)]
    pub fn resolve_name(&self, args_json: &str) -> Result<String, JsError> {
        #[derive(serde::Deserialize)]
        struct ResolveNameArgs {
            raw: String,
            #[serde(default)]
            candidates: Option<Vec<String>>,
            #[serde(default)]
            locale: Option<String>,
        }

        let args: ResolveNameArgs = serde_json::from_str(args_json)
            .map_err(|e| JsError::new(&format!("bad resolve args json: {e}")))?;
        let view = commands::resolve::resolve_name(
            &self.state.valuator,
            &args.raw,
            args.candidates,
            args.locale.as_deref(),
        );
        serde_json::to_string(&view).map_err(|e| JsError::new(&format!("serialize failed: {e}")))
    }

    // ===== database =====
    /// List craftable base items. `args_json` is JSON of `BasesArgs`
    /// (`{ class_pascal?, include_legacy? }`); returns JSON of `Vec<BaseSummary>`.
    #[wasm_bindgen(js_name = listBases)]
    pub fn list_bases(&self, args_json: &str) -> Result<String, JsError> {
        let args: commands::database::BasesArgs = serde_json::from_str(args_json)
            .map_err(|e| JsError::new(&format!("bad bases args json: {e}")))?;
        let out = commands::database::list_bases(&self.state.bundle, &args);
        serde_json::to_string(&out).map_err(|e| JsError::new(&format!("serialize failed: {e}")))
    }

    /// List database entries for a section. `args_json` is JSON of
    /// `DatabaseListArgs` (`{ section, search? }`); returns JSON of
    /// `Vec<DatabaseEntrySummary>`.
    #[wasm_bindgen(js_name = listDatabaseEntries)]
    pub fn list_database_entries(&self, args_json: &str) -> Result<String, JsError> {
        let args: commands::database::DatabaseListArgs = serde_json::from_str(args_json)
            .map_err(|e| JsError::new(&format!("bad database list args json: {e}")))?;
        let out = commands::database::list_database_entries(&self.state.bundle, &args);
        serde_json::to_string(&out).map_err(|e| JsError::new(&format!("serialize failed: {e}")))
    }

    /// Resolve a single database entry's detail view. `args_json` is JSON of
    /// `DatabaseDetailArgs` (`{ section, id }`); returns JSON of
    /// `DatabaseEntryDetail`.
    #[wasm_bindgen(js_name = databaseEntryDetail)]
    pub fn database_entry_detail(&self, args_json: &str) -> Result<String, JsError> {
        let args: commands::database::DatabaseDetailArgs = serde_json::from_str(args_json)
            .map_err(|e| JsError::new(&format!("bad database detail args json: {e}")))?;
        let detail = commands::database::database_entry_detail(&self.state.bundle, &args)
            .map_err(|e| JsError::new(&e))?;
        serde_json::to_string(&detail).map_err(|e| JsError::new(&format!("serialize failed: {e}")))
    }

    // ===== genesis tree =====
    /// The Genesis Tree view (0.5): wombs, positioned nodes, goal presets,
    /// farming notes and vetted videos. Returns JSON of `GenesisTreeView`;
    /// `available: false` when the bundle predates 0.5.
    #[wasm_bindgen(js_name = genesisTree)]
    pub fn genesis_tree(&self) -> Result<String, JsError> {
        let view = commands::genesis::genesis_tree(&self.state.bundle);
        serde_json::to_string(&view).map_err(|e| JsError::new(&format!("serialize failed: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use poc2_engine::ids::ConceptId;
    use poc2_engine::item::{AffixType, QualityKind, Rarity};
    use poc2_strategies::{Target, TargetSpec};
    use smallvec::smallvec;

    fn bundle_bytes() -> Option<Vec<u8>> {
        let path = std::env::var("POC2_BUNDLE").ok().or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|h| format!("{h}/.config/poc2/bundles/poc2.bundle.json.gz"))
        })?;
        std::fs::read(&path).ok()
    }

    /// Native parity anchor: the WASM `recommend` wrapper (run natively here)
    /// must reproduce the `live_bundle_smoke` result for the worked example —
    /// an int-armour Body Armour at ilvl 82 with a 3×T1 Energy Shield goal.
    #[test]
    fn wasm_recommend_matches_live_bundle() {
        let Some(bytes) = bundle_bytes() else {
            eprintln!("poc2-wasm parity: no bundle on disk; skipping (set POC2_BUNDLE).");
            return;
        };
        let mut engine = Engine::new(&bytes).expect("engine init");
        assert!(engine.state.bundle.mods.len() > 100);

        let base = engine
            .state
            .bundle
            .base_items
            .iter()
            .find(|b| {
                b.item_class.as_str() == "BodyArmour"
                    && b.tags.iter().any(|t| t.as_str() == "int_armour")
            })
            .map(|b| b.id.clone())
            .expect("int-armour body armour base");

        let item = Item {
            base,
            ilvl: 82,
            rarity: Rarity::Normal,
            corrupted: false,
            sanctified: false,
            mirrored: false,
            quality: 0,
            quality_kind: QualityKind::Untagged,
            implicits: smallvec![],
            prefixes: smallvec![],
            suffixes: smallvec![],
            enchantments: smallvec![],
            hidden_desecrated: None,
            sockets: smallvec![],
            hinekora_lock: None,
        };
        let goal = Goal::new(
            Target {
                prefixes: vec![TargetSpec {
                    concept: Some(ConceptId::from("EnergyShield")),
                    concept_any: vec![],
                    affix: Some(AffixType::Prefix),
                    count: 3,
                    min_tier: None,
                    allow_hybrid: true,
                }],
                suffixes: vec![],
                constraints: vec![],
            },
            poc2_market::DivEquiv::point(100.0),
        );

        let recs = engine.state.recommend(&item, &goal, 0.5, 2, 5);
        assert!(!recs.is_empty(), "expected at least one recommendation");
        match &recs[0].action {
            poc2_advisor::AdvisorAction::ApplyCurrency { currency, .. } => {
                assert_eq!(
                    currency.as_str(),
                    "PerfectOrbOfTransmutation",
                    "top action should match the live_bundle_smoke result"
                );
            }
            other => panic!("expected a concrete ApplyCurrency top action; got {other:?}"),
        }

        // JSON boundary round-trips (what the browser actually calls).
        let item_json = serde_json::to_string(&item).unwrap();
        let goal_json = serde_json::to_string(&goal).unwrap();
        let out = engine
            .recommend(&item_json, &goal_json, 0.5, 2, 5)
            .expect("json recommend");
        let back: Vec<Recommendation> = serde_json::from_str(&out).unwrap();
        assert_eq!(back.len(), recs.len());
    }
}
