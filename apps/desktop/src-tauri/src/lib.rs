//! poc2-desktop — Tauri 2 entry point.
//!
//! Boots the runtime, builds shared advisor state (mod registry, rule
//! catalogue, strategy registry, currency resolver, valuator), and
//! exposes the `recommend` IPC command for the frontend.
//!
//! Application logic lives in the workspace crates (`poc2-engine`,
//! `poc2-advisor`, etc.). The Tauri layer only adapts those crates to
//! IPC commands and lifecycle events.

use std::sync::Arc;

use poc2_advisor::{plan, BeamConfig, Goal, PlanInput, Recommendation, Stash};
use poc2_engine::currency::DefaultCurrencyResolver;
use poc2_engine::item::Item;
use poc2_engine::patch::PatchVersion;
use poc2_engine::registry::ModRegistry;
use poc2_market::Valuator;
use poc2_parser::{lower_to_item, parse_clipboard_text, ParsedItem};
use poc2_rules::RuleSet;
use poc2_strategies::StrategyRegistry;
use serde::{Deserialize, Serialize};
use tauri::Manager;
use tauri_plugin_clipboard_manager::ClipboardExt;
use tracing_subscriber::EnvFilter;

/// Inlined canonical strategy fixture. Bundled into the binary so the
/// app is self-contained at M6; user-provided strategies will load from
/// `$XDG_CONFIG_HOME/poc2/strategies/` in M6 polish.
const CANONICAL_STRATEGY_TOML: &str =
    include_str!("../../../../crates/strategies/strategies/3xt1-es-body-armour.toml");

/// Shared, read-only application state. Built once at startup; cloned
/// (Arc-wise) into each command invocation.
struct AdvisorState {
    registry: Arc<ModRegistry>,
    rules: Arc<RuleSet>,
    strategies: Arc<StrategyRegistry>,
    resolver: Arc<DefaultCurrencyResolver>,
    valuator: Arc<Valuator>,
}

impl AdvisorState {
    fn build() -> Self {
        // Empty mod registry until the data bundle is loaded (M6 polish).
        // The advisor's rule + strategy paths still work because the
        // seed rules don't query mod content; they only inspect Item
        // state.
        let registry = ModRegistry::from_mods(Vec::new());
        let rules = RuleSet::from_rules(poc2_rules::seed_rules());
        let strategies = match poc2_strategies::load_strategy_str(CANONICAL_STRATEGY_TOML) {
            Ok(s) => StrategyRegistry::from_strategies(vec![s]),
            Err(e) => {
                tracing::warn!(error = %e, "failed to load canonical strategy; using empty registry");
                StrategyRegistry::default()
            }
        };
        let resolver = DefaultCurrencyResolver::new();
        let valuator = Valuator::default();
        Self {
            registry: Arc::new(registry),
            rules: Arc::new(rules),
            strategies: Arc::new(strategies),
            resolver: Arc::new(resolver),
            valuator: Arc::new(valuator),
        }
    }
}

#[derive(Debug, Deserialize)]
struct RecommendArgs {
    item: Item,
    goal: Goal,
    #[serde(default)]
    stash: Stash,
    /// `[0, 1]`; 0 = cautious, 1 = greedy.
    #[serde(default = "default_risk")]
    risk: f64,
    #[serde(default = "default_top_n")]
    top_n: u32,
    #[serde(default = "default_depth")]
    depth: u32,
}

const fn default_risk() -> f64 {
    0.5
}
const fn default_top_n() -> u32 {
    5
}
const fn default_depth() -> u32 {
    2
}

#[derive(Debug, Serialize)]
struct RecommendResponse {
    recommendations: Vec<Recommendation>,
    /// Patch version the advisor evaluated against.
    patch: String,
    /// Number of seed rules in scope.
    rule_count: usize,
    /// Number of strategies in scope.
    strategy_count: usize,
}

#[tauri::command]
fn ping() -> String {
    format!(
        "poc2 v{} ready (engine schema {})",
        env!("CARGO_PKG_VERSION"),
        poc2_engine::ENGINE_SCHEMA_VERSION
    )
}

#[derive(Debug, Serialize)]
struct ParseClipboardResponse {
    /// Phase-1 parse output (text fields).
    parsed: ParsedItem,
    /// Phase-2 lower output (engine `Item`).
    item: Item,
    /// Mod text lines that did not resolve to any registered mod.
    unresolved: Vec<String>,
}

#[tauri::command]
fn parse_item_text(
    text: String,
    state: tauri::State<'_, AdvisorState>,
) -> Result<ParseClipboardResponse, String> {
    let parsed = parse_clipboard_text(&text).map_err(|e| e.to_string())?;
    let (item, unresolved) =
        lower_to_item(&parsed, state.registry.as_ref()).map_err(|e| e.to_string())?;
    Ok(ParseClipboardResponse {
        parsed,
        item,
        unresolved,
    })
}

#[tauri::command]
fn read_clipboard_item(
    app: tauri::AppHandle,
    state: tauri::State<'_, AdvisorState>,
) -> Result<ParseClipboardResponse, String> {
    let text = app
        .clipboard()
        .read_text()
        .map_err(|e| format!("clipboard read failed: {e}"))?;
    let parsed = parse_clipboard_text(&text).map_err(|e| e.to_string())?;
    let (item, unresolved) =
        lower_to_item(&parsed, state.registry.as_ref()).map_err(|e| e.to_string())?;
    Ok(ParseClipboardResponse {
        parsed,
        item,
        unresolved,
    })
}

#[tauri::command]
fn recommend(
    args: RecommendArgs,
    state: tauri::State<'_, AdvisorState>,
) -> Result<RecommendResponse, String> {
    let patch = PatchVersion::PATCH_0_4_0;
    let input = PlanInput {
        item: args.item,
        goal: args.goal,
        rules: state.rules.as_ref(),
        strategies: state.strategies.as_ref(),
        registry: state.registry.as_ref(),
        resolver: state.resolver.as_ref(),
        valuator: state.valuator.as_ref(),
        stash: &args.stash,
        patch,
        config: BeamConfig {
            width: args.top_n.max(3),
            depth: args.depth.max(1),
            risk: args.risk,
            top_n: args.top_n,
            seed: 0,
            weights: poc2_advisor::ScoringWeights::default(),
        },
    };
    let recommendations = plan(&input);
    Ok(RecommendResponse {
        recommendations,
        patch: format!("{patch}"),
        rule_count: state.rules.len(),
        strategy_count: state.strategies.len(),
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,poc2=debug")),
        )
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_http::init())
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            app.manage(AdvisorState::build());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            ping,
            recommend,
            parse_item_text,
            read_clipboard_item
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
