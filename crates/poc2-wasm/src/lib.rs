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

use poc2_advisor::{plan, AdvisorAction, BeamConfig, Goal, PlanInput, Recommendation, Stash};
use poc2_engine::currency::DefaultCurrencyResolver;
use poc2_engine::ids::{BaseTypeId, ItemClassId};
use poc2_engine::item::Item;
use poc2_engine::patch::{League, PatchVersion};
use poc2_engine::{BaseRegistry, ModRegistry};
use poc2_market::Valuator;
use poc2_rules::RuleSet;
use poc2_strategies::{seed_strategies, StrategyRegistry};
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
    bundle: poc2_data::Bundle,
    /// Clipboard `Item Class:` display string (lowercased) → canonical class id.
    class_by_display: HashMap<String, ItemClassId>,
    /// `(class, lowercased base name)` → real bundle base id (for paste-import).
    base_by_class_name: HashMap<(ItemClassId, String), BaseTypeId>,
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
        let resolver = DefaultCurrencyResolver::new()
            .with_essences(bundle.essence_catalogue())
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
            bundle,
            class_by_display,
            base_by_class_name,
        }
    }

    fn recommend(
        &self,
        item: &Item,
        goal: &Goal,
        risk: f64,
        depth: u32,
        top_n: u32,
    ) -> Vec<Recommendation> {
        let stash = Stash::unlimited();
        let config = BeamConfig {
            depth: depth.max(1),
            risk: risk.clamp(0.0, 1.0),
            top_n: top_n.max(1),
            width: top_n.max(3),
            ..BeamConfig::default()
        };
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
            plugin_dispatch: None,
            base_registry: Some(&self.base_registry),
            trained_models: None,
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
        self.state.league = match league.to_ascii_lowercase().as_str() {
            "standard" => League::Standard,
            "challenge" => League::Challenge,
            other => {
                return Err(JsError::new(&format!(
                    "unknown league `{other}` (expected \"standard\" or \"challenge\")"
                )))
            }
        };
        Ok(())
    }

    /// Number of mods indexed (diagnostic).
    #[wasm_bindgen(getter, js_name = modCount)]
    pub fn mod_count(&self) -> usize {
        self.state.bundle.mods.len()
    }

    /// Recommend the top-N next actions. `item_json` / `goal_json` are JSON of
    /// the engine `Item` / advisor `Goal`; returns JSON of `Vec<Recommendation>`.
    pub fn recommend(
        &self,
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
        let engine = Engine::new(&bytes).expect("engine init");
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
