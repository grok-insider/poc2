//! poc2-desktop — Tauri 2 entry point.
//!
//! Boots the runtime, builds shared advisor state (mod registry, rule
//! catalogue, strategy registry, currency resolver, valuator), and
//! exposes the `recommend` IPC command for the frontend.
//!
//! Application logic lives in the workspace crates (`poc2-engine`,
//! `poc2-advisor`, etc.). The Tauri layer only adapts those crates to
//! IPC commands and lifecycle events.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};

use poc2_advisor::{
    plan, plan_streaming, BeamConfig, Goal, PlanInput, Recommendation, Stash, StreamingProgress,
};
use poc2_data::Bundle;
use poc2_engine::currency::DefaultCurrencyResolver;
use poc2_engine::item::Item;
use poc2_engine::patch::PatchVersion;
use poc2_engine::registry::ModRegistry;
use poc2_market::{
    apply_feed_to_valuator, default_id_mapping, fetch_snapshot as fetch_price_snapshot, Valuator,
};
use poc2_parser::{lower_to_item, parse_clipboard_text, ParsedItem};
use poc2_plugin_host::PluginHost;
use poc2_rules::RuleSet;
use poc2_strategies::StrategyRegistry;
use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tracing_subscriber::EnvFilter;

mod client_log;
mod trade_search;
use client_log::{start_client_log_watcher, ClientLogEvent, ClientLogWatcher, CLIENT_LOG_EVENT};
use trade_search::build_trade_search_url;

/// Inlined seed strategies. Bundled into the binary so the app is
/// self-contained out of the box; user-provided strategies are loaded
/// from `$XDG_CONFIG_HOME/poc2/strategies/` in addition (M6 polish).
const SEED_STRATEGIES: &[(&str, &str)] = &[
    (
        "3xt1-es-body-armour",
        include_str!("../../../../crates/strategies/strategies/3xt1-es-body-armour.toml"),
    ),
    (
        "apprentice-blueprint",
        include_str!("../../../../crates/strategies/strategies/apprentice-blueprint.toml"),
    ),
    (
        "whittling-cleanup",
        include_str!("../../../../crates/strategies/strategies/whittling-cleanup.toml"),
    ),
    (
        "fracture-then-chaos-spam",
        include_str!("../../../../crates/strategies/strategies/fracture-then-chaos-spam.toml"),
    ),
    (
        "annul-augment-spam",
        include_str!("../../../../crates/strategies/strategies/annul-augment-spam.toml"),
    ),
    (
        "greater-essence-regal-lockin",
        include_str!("../../../../crates/strategies/strategies/greater-essence-regal-lockin.toml"),
    ),
    (
        "sinistral-erasure-cleanup",
        include_str!("../../../../crates/strategies/strategies/sinistral-erasure-cleanup.toml"),
    ),
    (
        "catalysing-exaltation",
        include_str!("../../../../crates/strategies/strategies/catalysing-exaltation.toml"),
    ),
    (
        "perfect-essence-crystallisation",
        include_str!(
            "../../../../crates/strategies/strategies/perfect-essence-crystallisation.toml"
        ),
    ),
    (
        "greater-exaltation-stacking",
        include_str!("../../../../crates/strategies/strategies/greater-exaltation-stacking.toml"),
    ),
    (
        "sanctification-finish",
        include_str!("../../../../crates/strategies/strategies/sanctification-finish.toml"),
    ),
    (
        "omen-of-light-cleanup",
        include_str!("../../../../crates/strategies/strategies/omen-of-light-cleanup.toml"),
    ),
    (
        "hinekoras-lock-save-state",
        include_str!("../../../../crates/strategies/strategies/hinekoras-lock-save-state.toml"),
    ),
    (
        "bones-with-abyssal-echoes",
        include_str!("../../../../crates/strategies/strategies/bones-with-abyssal-echoes.toml"),
    ),
    (
        "abyss-lord-omens",
        include_str!("../../../../crates/strategies/strategies/abyss-lord-omens.toml"),
    ),
    (
        "vaal-corruption-finish",
        include_str!("../../../../crates/strategies/strategies/vaal-corruption-finish.toml"),
    ),
    (
        "double-corruption",
        include_str!("../../../../crates/strategies/strategies/double-corruption.toml"),
    ),
    (
        "recombinator",
        include_str!("../../../../crates/strategies/strategies/recombinator.toml"),
    ),
    (
        "magic-base-exit",
        include_str!("../../../../crates/strategies/strategies/magic-base-exit.toml"),
    ),
    (
        "mark-of-the-abyss-swap",
        include_str!("../../../../crates/strategies/strategies/mark-of-the-abyss-swap.toml"),
    ),
    (
        "beltons-four-t1-rubric",
        include_str!("../../../../crates/strategies/strategies/beltons-four-t1-rubric.toml"),
    ),
    (
        "ilvl-82-tri-resist-convergence",
        include_str!(
            "../../../../crates/strategies/strategies/ilvl-82-tri-resist-convergence.toml"
        ),
    ),
    (
        "wraeclast-workflow-order",
        include_str!("../../../../crates/strategies/strategies/wraeclast-workflow-order.toml"),
    ),
    (
        "exceptional-bases-exploit",
        include_str!("../../../../crates/strategies/strategies/exceptional-bases-exploit.toml"),
    ),
];

/// Bundle-derived application state that can change at runtime via the
/// `reload_bundle` command. Wrapped in [`RwLock`] because reads (every
/// `recommend` invocation) dominate writes (manual reloads).
struct BundleState {
    registry: Arc<ModRegistry>,
    strategies: Arc<StrategyRegistry>,
    resolver: Arc<DefaultCurrencyResolver>,
    bundle_path: Option<PathBuf>,
    bundle_patch: Option<PatchVersion>,
}

/// Shared application state. Built once at startup. Bundle-derived
/// fields live behind an `Arc<RwLock<BundleState>>` so the
/// `reload_bundle` Tauri command can swap them without restarting the
/// app (per A.6 of the v1 execution plan).
struct AdvisorState {
    /// Bundle-derived state, hot-swappable via `reload_bundle`.
    bundle: Arc<RwLock<BundleState>>,
    /// Forward-chain rule catalogue. Static (loaded from embedded
    /// seed_rules/*.toml at startup); plugins extend it in v1.x.
    rules: Arc<RuleSet>,
    /// Mutable so live price refreshes can swap it in.
    valuator: Arc<Mutex<Valuator>>,
    /// Most recent live-price refresh metadata, if any.
    price_refresh: Arc<Mutex<Option<PriceRefreshMeta>>>,
    /// In-flight streaming task. Each new `recommend_streaming` call
    /// aborts the prior task to avoid stale emits clobbering newer
    /// requests (per Phase C.2).
    streaming_task: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
    /// Active Client.txt watcher, if any (Phase D.1). Replaced by
    /// each `start_client_log` invocation (the previous watcher's
    /// inotify subscription is dropped automatically).
    client_log_watcher: Arc<Mutex<Option<ClientLogWatcher>>>,
    /// Wasm plugin host (Phase F). Wrapped in RwLock so the
    /// `reload_plugins` command can swap it without restarting.
    plugin_host: Arc<RwLock<PluginHost>>,
}

#[derive(Debug, Clone, Serialize)]
struct PriceRefreshMeta {
    league: String,
    fetched_at: String,
    /// How many engine-recognized currencies got a fresh price.
    applied_count: usize,
    /// How many entries the snapshot contained (informational).
    total_entries: usize,
}

impl AdvisorState {
    fn build() -> Self {
        let rules = RuleSet::from_rules(poc2_rules::seed_rules());
        let bundle_state = build_bundle_state(None);
        let plugin_host = build_plugin_host();
        Self {
            bundle: Arc::new(RwLock::new(bundle_state)),
            rules: Arc::new(rules),
            valuator: Arc::new(Mutex::new(Valuator::default())),
            price_refresh: Arc::new(Mutex::new(None)),
            streaming_task: Arc::new(Mutex::new(None)),
            client_log_watcher: Arc::new(Mutex::new(None)),
            plugin_host: Arc::new(RwLock::new(plugin_host)),
        }
    }
}

/// Build the Wasm plugin host + scan
/// `$XDG_CONFIG_HOME/poc2/plugins/` for plugins.
fn build_plugin_host() -> PluginHost {
    let mut host = PluginHost::new().unwrap_or_else(|e| {
        tracing::error!(error = %e, "plugin host failed to initialize; running with no-plugin host");
        // Try once more (the failure mode is wasmtime config; should
        // not actually happen in practice). Falling back to a panic
        // here would cripple the app for plugin-less users.
        PluginHost::new().expect("plugin host constructor must succeed")
    });
    let plugin_dir = if let Some(xdg_config) = std::env::var_os("XDG_CONFIG_HOME") {
        Some(Path::new(&xdg_config).join("poc2/plugins"))
    } else {
        std::env::var_os("HOME").map(|home| Path::new(&home).join(".config/poc2/plugins"))
    };
    if let Some(dir) = plugin_dir {
        match host.discover_plugins(&dir) {
            Ok(n) if n > 0 => {
                tracing::info!(plugin_count = n, dir = %dir.display(), "discovered plugins")
            }
            Ok(_) => tracing::debug!(dir = %dir.display(), "no plugins found"),
            Err(e) => tracing::warn!(dir = %dir.display(), error = %e, "plugin discovery failed"),
        }
    }
    host
}

/// Construct a [`BundleState`] from the bundle search machinery (or
/// from an explicit path override). Always succeeds: a missing bundle
/// produces an empty registry with a warning log.
///
/// `path_override`: when `Some(p)` skips the search and loads `p`
/// directly. When `None` runs the standard XDG-aware search per
/// [`load_bundle_from_known_paths`].
fn build_bundle_state(path_override: Option<&Path>) -> BundleState {
    let loaded = match path_override {
        Some(p) => try_load_bundle(p),
        None => load_bundle_from_known_paths(),
    };
    let (registry, bundle_path, bundle_patch, essences, catalysts) = match loaded {
        Some((bundle, path)) => {
            let patch = bundle.game_patch();
            tracing::info!(
                path = %path.display(),
                patch = %patch,
                mods = bundle.mods.len(),
                bases = bundle.base_items.len(),
                omens = bundle.omens.entries.len(),
                essences = bundle.essences.entries.len(),
                catalysts = bundle.catalysts.entries.len(),
                bones = bundle.bones.entries.len(),
                weights = bundle.weights.len(),
                "loaded data bundle"
            );
            let essences = bundle.essence_catalogue();
            let catalysts = bundle.catalyst_catalogue();
            (
                ModRegistry::from_mods(bundle.mods),
                Some(path),
                Some(patch),
                essences,
                catalysts,
            )
        }
        None => {
            tracing::warn!(
                "no data bundle found; running with empty mod registry. \
                 Build a bundle via the pipeline (`cargo run -p poc2-pipeline -- build`) \
                 and place it in `~/.config/poc2/bundles/` or set POC2_BUNDLE."
            );
            (
                ModRegistry::from_mods(Vec::new()),
                None,
                None,
                Vec::new(),
                Vec::new(),
            )
        }
    };

    let mut loaded_strategies = Vec::new();
    for (name, toml) in SEED_STRATEGIES {
        match poc2_strategies::load_strategy_str(toml) {
            Ok(s) => loaded_strategies.push(s),
            Err(e) => tracing::warn!(name, error = %e, "seed strategy failed to load"),
        }
    }
    load_user_strategies(&mut loaded_strategies);
    let strategy_count = loaded_strategies.len();
    let strategies = StrategyRegistry::from_strategies(loaded_strategies);
    tracing::info!(strategy_count, "loaded strategies");

    let resolver = DefaultCurrencyResolver::new()
        .with_essences(essences)
        .with_catalysts(catalysts);

    BundleState {
        registry: Arc::new(registry),
        strategies: Arc::new(strategies),
        resolver: Arc::new(resolver),
        bundle_path,
        bundle_patch,
    }
}

/// Search the conventional locations for a `*.bundle.json[.gz]` and load
/// the first one that parses cleanly.
///
/// Search order (highest priority first):
/// 1. `$POC2_BUNDLE` (if set, must be an absolute file path)
/// 2. `$XDG_CONFIG_HOME/poc2/bundles/*.bundle.json{,.gz}`
///    or `~/.config/poc2/bundles/...`
/// 3. `$XDG_DATA_HOME/poc2/bundles/...` or `~/.local/share/poc2/bundles/...`
///
/// Within each directory, the most recently modified file wins.
fn load_bundle_from_known_paths() -> Option<(Bundle, PathBuf)> {
    if let Ok(env_path) = std::env::var("POC2_BUNDLE") {
        let p = PathBuf::from(env_path);
        if p.is_file() {
            if let Some((b, _)) = try_load_bundle(&p) {
                return Some((b, p));
            }
        }
    }
    let mut search_dirs: Vec<PathBuf> = Vec::new();
    if let Some(xdg_config) = std::env::var_os("XDG_CONFIG_HOME") {
        search_dirs.push(Path::new(&xdg_config).join("poc2/bundles"));
    } else if let Some(home) = std::env::var_os("HOME") {
        search_dirs.push(Path::new(&home).join(".config/poc2/bundles"));
    }
    if let Some(xdg_data) = std::env::var_os("XDG_DATA_HOME") {
        search_dirs.push(Path::new(&xdg_data).join("poc2/bundles"));
    } else if let Some(home) = std::env::var_os("HOME") {
        search_dirs.push(Path::new(&home).join(".local/share/poc2/bundles"));
    }
    for dir in search_dirs {
        if let Some((bundle, path)) = newest_bundle_in_dir(&dir) {
            return Some((bundle, path));
        }
    }
    None
}

/// Find the most recently modified `*.bundle.json{,.gz}` in `dir` and load
/// it. Returns `None` if the directory doesn't exist or no candidate
/// parses cleanly.
fn newest_bundle_in_dir(dir: &Path) -> Option<(Bundle, PathBuf)> {
    let entries = std::fs::read_dir(dir).ok()?;
    let mut candidates: Vec<(PathBuf, std::time::SystemTime)> = Vec::new();
    for entry in entries.flatten() {
        let p = entry.path();
        if !p.is_file() {
            continue;
        }
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if !(name.ends_with(".bundle.json") || name.ends_with(".bundle.json.gz")) {
            continue;
        }
        let mtime = entry
            .metadata()
            .and_then(|m| m.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        candidates.push((p, mtime));
    }
    candidates.sort_by_key(|(_, t)| std::cmp::Reverse(*t));
    for (path, _) in candidates {
        if let Some((b, p)) = try_load_bundle(&path) {
            return Some((b, p));
        }
    }
    None
}

/// Load every `*.toml` strategy in `$XDG_CONFIG_HOME/poc2/strategies/`
/// into the registry. Failures are warned-and-skipped; the rest of the
/// strategies still load.
fn load_user_strategies(out: &mut Vec<poc2_strategies::Strategy>) {
    let dirs: Vec<PathBuf> = if let Some(xdg_config) = std::env::var_os("XDG_CONFIG_HOME") {
        vec![Path::new(&xdg_config).join("poc2/strategies")]
    } else if let Some(home) = std::env::var_os("HOME") {
        vec![Path::new(&home).join(".config/poc2/strategies")]
    } else {
        vec![]
    };
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let is_toml = path.extension().and_then(|e| e.to_str()) == Some("toml");
            if !is_toml {
                continue;
            }
            match poc2_strategies::load_strategy_toml(&path) {
                Ok(s) => {
                    tracing::info!(path = %path.display(), id = %s.id.0, "loaded user strategy");
                    out.push(s);
                }
                Err(e) => {
                    tracing::warn!(path = %path.display(), error = %e, "user strategy failed to load")
                }
            }
        }
    }
}

fn try_load_bundle(path: &Path) -> Option<(Bundle, PathBuf)> {
    match poc2_data::io::read_bundle(path) {
        Ok(b) => match b.validate() {
            Ok(()) => Some((b, path.to_path_buf())),
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "bundle failed validation");
                None
            }
        },
        Err(e) => {
            tracing::warn!(path = %path.display(), error = %e, "bundle read failed");
            None
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
    /// Number of mods in the loaded registry.
    mod_count: usize,
    /// Path of the loaded bundle, when applicable.
    bundle_path: Option<String>,
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
    let bundle_guard = state.bundle.read().expect("bundle rwlock poisoned");
    let (item, unresolved) =
        lower_to_item(&parsed, bundle_guard.registry.as_ref()).map_err(|e| e.to_string())?;
    drop(bundle_guard);
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
    let bundle_guard = state.bundle.read().expect("bundle rwlock poisoned");
    let (item, unresolved) =
        lower_to_item(&parsed, bundle_guard.registry.as_ref()).map_err(|e| e.to_string())?;
    drop(bundle_guard);
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
    let bundle_guard = state.bundle.read().expect("bundle rwlock poisoned");
    // Use the loaded bundle's patch when available; otherwise default to
    // the project's baseline (0.4.0). Falling back to a baseline keeps
    // the rules + strategies in scope when no bundle is loaded.
    let patch = bundle_guard
        .bundle_patch
        .unwrap_or(PatchVersion::PATCH_0_4_0);
    let valuator_guard = state.valuator.lock().expect("valuator mutex poisoned");
    let plugin_guard = state.plugin_host.read().expect("plugin_host poisoned");
    let input = PlanInput {
        item: args.item,
        goal: args.goal,
        rules: state.rules.as_ref(),
        strategies: bundle_guard.strategies.as_ref(),
        registry: bundle_guard.registry.as_ref(),
        resolver: bundle_guard.resolver.as_ref(),
        valuator: &valuator_guard,
        stash: &args.stash,
        patch,
        plugin_dispatch: Some(&*plugin_guard as &dyn poc2_strategies::PluginPredicateDispatch),
        config: BeamConfig {
            width: args.top_n.max(3),
            depth: args.depth.max(1),
            risk: args.risk,
            top_n: args.top_n,
            seed: 0,
            mc_samples: 50,
            weights: poc2_advisor::ScoringWeights::default(),
        },
    };
    let recommendations = plan(&input);
    drop(plugin_guard);
    let response = RecommendResponse {
        recommendations,
        patch: format!("{patch}"),
        rule_count: state.rules.len(),
        strategy_count: bundle_guard.strategies.len(),
        mod_count: bundle_guard.registry.len(),
        bundle_path: bundle_guard
            .bundle_path
            .as_ref()
            .map(|p| p.display().to_string()),
    };
    drop(valuator_guard);
    drop(bundle_guard);
    Ok(response)
}

// ---------------------------------------------------------------------
// Recipe library (Phase B.4) — TOML files in
// $XDG_CONFIG_HOME/poc2/recipes/<name>.toml
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Recipe {
    /// Slug used as the filename stem. Must be a single line of
    /// `[A-Za-z0-9_-]+`.
    name: String,
    /// Optional human-readable description.
    #[serde(default)]
    description: String,
    /// JSON-encoded Item — surfaced as a string so the recipe TOML
    /// stays human-editable.
    item_json: String,
    /// JSON-encoded Goal — same rationale.
    goal_json: String,
    /// ISO-8601 creation timestamp.
    created_at: String,
}

#[derive(Debug, Serialize)]
struct RecipeSummary {
    name: String,
    description: String,
    created_at: String,
}

fn recipes_dir() -> Option<PathBuf> {
    if let Some(xdg_config) = std::env::var_os("XDG_CONFIG_HOME") {
        Some(Path::new(&xdg_config).join("poc2/recipes"))
    } else {
        std::env::var_os("HOME").map(|home| Path::new(&home).join(".config/poc2/recipes"))
    }
}

fn validate_recipe_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("recipe name cannot be empty".into());
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err("recipe name must be [A-Za-z0-9_-]+ (no spaces or path separators)".into());
    }
    Ok(())
}

#[tauri::command]
fn list_recipes() -> Result<Vec<RecipeSummary>, String> {
    let Some(dir) = recipes_dir() else {
        return Ok(Vec::new());
    };
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let entries = std::fs::read_dir(&dir).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("toml") {
            continue;
        }
        let Ok(contents) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(recipe) = toml::from_str::<Recipe>(&contents) else {
            continue;
        };
        out.push(RecipeSummary {
            name: recipe.name,
            description: recipe.description,
            created_at: recipe.created_at,
        });
    }
    out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(out)
}

#[tauri::command]
fn save_recipe(recipe: Recipe) -> Result<(), String> {
    validate_recipe_name(&recipe.name)?;
    let Some(dir) = recipes_dir() else {
        return Err("no $XDG_CONFIG_HOME or $HOME — cannot save recipe".into());
    };
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("{}.toml", recipe.name));
    let serialized = toml::to_string_pretty(&recipe).map_err(|e| e.to_string())?;
    std::fs::write(&path, serialized).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn load_recipe(name: String) -> Result<Recipe, String> {
    validate_recipe_name(&name)?;
    let Some(dir) = recipes_dir() else {
        return Err("no $XDG_CONFIG_HOME or $HOME".into());
    };
    let path = dir.join(format!("{name}.toml"));
    let contents = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    toml::from_str(&contents).map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_recipe(name: String) -> Result<(), String> {
    validate_recipe_name(&name)?;
    let Some(dir) = recipes_dir() else {
        return Err("no $XDG_CONFIG_HOME or $HOME".into());
    };
    let path = dir.join(format!("{name}.toml"));
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn export_recipe_toml(recipe: Recipe) -> Result<String, String> {
    validate_recipe_name(&recipe.name)?;
    toml::to_string_pretty(&recipe).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------
// Recovery hints (Phase B.2)
// ---------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct RecoveryHintView {
    /// Human-readable explanation of the recovery option.
    message: String,
    /// Step id the user would jump to if they accept this hint
    /// (None when the hint is purely advisory).
    goto_step_id: Option<String>,
    /// Estimated additional cost in divines (None when not estimated).
    added_cost_div: Option<u32>,
    /// Strategy + step ids the hint came from, for display.
    strategy_id: String,
    step_id: String,
}

#[derive(Debug, Serialize)]
struct RecoveryStepView {
    step_id: String,
    /// Action description for the goto step (when goto_step_id is set).
    /// Helps the user understand what they'd be applying next.
    next_action_summary: Option<String>,
    /// All hints attached to the step.
    hints: Vec<RecoveryHintView>,
}

#[tauri::command]
fn recovery_hints(
    strategy_id: String,
    step_id: String,
    state: tauri::State<'_, AdvisorState>,
) -> Result<RecoveryStepView, String> {
    use poc2_strategies::{Action, StepId, StrategyId};
    let bundle = state.bundle.read().expect("bundle rwlock poisoned");
    let strategy = bundle
        .strategies
        .get(&StrategyId(strategy_id.clone()))
        .ok_or_else(|| format!("unknown strategy: {strategy_id}"))?;
    let target_step_id = StepId(step_id.clone());
    let step = strategy
        .step(&target_step_id)
        .ok_or_else(|| format!("strategy {strategy_id} has no step {step_id}"))?;
    let mut hints = Vec::with_capacity(step.recovery.len());
    for hint in &step.recovery {
        hints.push(RecoveryHintView {
            message: hint.message.clone(),
            goto_step_id: hint.goto.as_ref().map(|s| s.0.clone()),
            added_cost_div: hint.added_cost_div,
            strategy_id: strategy_id.clone(),
            step_id: step_id.clone(),
        });
    }
    let next_action_summary = step.on_failure.as_ref().and_then(|sid| {
        strategy.step(sid).map(|next| match &next.action {
            Action::ApplyCurrency { currency, omens } => {
                if omens.is_empty() {
                    format!("Apply {currency}")
                } else {
                    format!(
                        "Apply {currency} with omens [{}]",
                        omens
                            .iter()
                            .map(poc2_engine::ids::OmenId::as_str)
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                }
            }
            Action::ActivateOmen { omen } => format!("Activate omen {omen}"),
            Action::HinekorasLock => "Apply Hinekora's Lock".into(),
            Action::Reveal { .. } => "Reveal at Well of Souls".into(),
            Action::Recombine { .. } => "Recombine with second item".into(),
            Action::Done => "Done".into(),
            Action::Abandon { reason } => format!("Abandon: {reason}"),
            Action::Noop => "(no-op)".into(),
            Action::LoopUntil { .. } | Action::Sequence(_) | Action::Branch(_) => {
                "(control-flow)".into()
            }
        })
    });
    drop(bundle);
    Ok(RecoveryStepView {
        step_id,
        next_action_summary,
        hints,
    })
}

// ---------------------------------------------------------------------
// State persistence (Phase B.1) — Goal + risk slider live in
// $XDG_CONFIG_HOME/poc2/state.toml.
// ---------------------------------------------------------------------

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct PersistedState {
    /// Last goal the user had configured. Stored as JSON so the
    /// Goal serde shape is preserved across schema bumps.
    #[serde(default)]
    goal_json: Option<String>,
    /// Last risk slider value, clamped to [0, 1].
    #[serde(default)]
    risk: Option<f64>,
    /// Last beam-search depth slider value (1..=5).
    #[serde(default)]
    depth: Option<u32>,
    /// Last top-N value (1..=10).
    #[serde(default)]
    top_n: Option<u32>,
}

fn state_file_path() -> Option<PathBuf> {
    if let Some(xdg_config) = std::env::var_os("XDG_CONFIG_HOME") {
        Some(Path::new(&xdg_config).join("poc2/state.toml"))
    } else {
        std::env::var_os("HOME").map(|home| Path::new(&home).join(".config/poc2/state.toml"))
    }
}

#[tauri::command]
fn load_state() -> Result<PersistedState, String> {
    let Some(path) = state_file_path() else {
        return Ok(PersistedState::default());
    };
    if !path.exists() {
        return Ok(PersistedState::default());
    }
    let contents = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    toml::from_str(&contents).map_err(|e| e.to_string())
}

#[tauri::command]
fn save_state(state: PersistedState) -> Result<(), String> {
    let Some(path) = state_file_path() else {
        return Err("no $XDG_CONFIG_HOME or $HOME — cannot persist state".into());
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let serialized = toml::to_string_pretty(&state).map_err(|e| e.to_string())?;
    std::fs::write(&path, serialized).map_err(|e| e.to_string())?;
    Ok(())
}

// ---------------------------------------------------------------------
// League listing (Phase B.3) — for the Settings panel dropdown.
// ---------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct LeagueInfo {
    value: String,
    divine_price_in_exalts: f64,
    chaos_per_divine: f64,
}

#[tauri::command]
async fn list_leagues() -> Result<Vec<LeagueInfo>, String> {
    use poc2_market::{POE2SCOUT_BASE_URL, POE2SCOUT_REALM};
    let client = reqwest::Client::builder()
        .user_agent(concat!(
            "poc2-desktop/",
            env!("CARGO_PKG_VERSION"),
            " (+contact: github issues)"
        ))
        .gzip(true)
        .build()
        .map_err(|e| e.to_string())?;
    let url = format!("{POE2SCOUT_BASE_URL}/{POE2SCOUT_REALM}/Leagues");
    let leagues: Vec<poc2_market::PoeScoutLeague> = client
        .get(&url)
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    Ok(leagues
        .into_iter()
        .map(|l| LeagueInfo {
            value: l.value,
            divine_price_in_exalts: l.divine_price,
            chaos_per_divine: l.chaos_divine_price,
        })
        .collect())
}

// ---------------------------------------------------------------------
// Bundle hot-swap (Phase A.6)
// ---------------------------------------------------------------------

// ---------------------------------------------------------------------
// Plugin manager (Phase F.6)
// ---------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct PluginInfo {
    id: String,
    name: String,
    version: String,
    description: String,
    capabilities: Vec<String>,
    enabled: bool,
    n_strategies: usize,
    n_rules: usize,
}

#[tauri::command]
fn list_plugins(state: tauri::State<'_, AdvisorState>) -> Result<Vec<PluginInfo>, String> {
    let host = state
        .plugin_host
        .read()
        .map_err(|_| "plugin_host poisoned".to_string())?;
    Ok(host
        .plugins()
        .map(|p| PluginInfo {
            id: p.manifest.id.clone(),
            name: p.manifest.name.clone(),
            version: p.manifest.version.clone(),
            description: p.manifest.description.clone(),
            capabilities: p
                .manifest
                .capabilities
                .iter()
                .map(|c| format!("{c:?}").to_lowercase())
                .collect(),
            enabled: p.enabled,
            n_strategies: p.strategies.len(),
            n_rules: p.rules.len(),
        })
        .collect())
}

#[tauri::command]
fn reload_plugins(state: tauri::State<'_, AdvisorState>) -> Result<usize, String> {
    let new_host = build_plugin_host();
    let count = new_host.plugin_count();
    let mut guard = state
        .plugin_host
        .write()
        .map_err(|_| "plugin_host poisoned".to_string())?;
    *guard = new_host;
    drop(guard);
    Ok(count)
}

// ---------------------------------------------------------------------
// Meta-build aggregator + off-meta finder (Phase E.1 + E.2)
// ---------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct FetchMetaArgs {
    /// Optional league override; defaults to "Fate of the Vaal".
    #[serde(default)]
    league: Option<String>,
}

#[derive(Debug, Serialize)]
struct MetaResponse {
    league: String,
    fetched_at: String,
    n_builds: usize,
    /// Top-N off-meta niche targets (capped at 12 for UI compactness).
    niches: Vec<poc2_market::NicheTarget>,
}

#[tauri::command]
async fn fetch_meta_builds(args: FetchMetaArgs) -> Result<MetaResponse, String> {
    let client = reqwest::Client::builder()
        .user_agent(concat!(
            "poc2-desktop/",
            env!("CARGO_PKG_VERSION"),
            " (+contact: github issues)"
        ))
        .gzip(true)
        .build()
        .map_err(|e| e.to_string())?;
    let snap = poc2_market::fetch_meta_snapshot(&client, args.league.as_deref())
        .await
        .map_err(|e| e.to_string())?;
    let mut niches = poc2_market::off_meta(&snap, None);
    niches.truncate(12);
    Ok(MetaResponse {
        league: snap.league,
        fetched_at: snap.fetched_at,
        n_builds: snap.builds.len(),
        niches,
    })
}

// ---------------------------------------------------------------------
// Trade-search URL adapter (Phase D.3)
// ---------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct TradeSearchArgs {
    item: Item,
    /// League slug. Defaults to the bundle's patch league when None.
    #[serde(default)]
    league: Option<String>,
    /// When true, opens the URL in the default browser via the shell
    /// plugin. When false, returns the URL only (caller does the open).
    #[serde(default = "default_true_open")]
    open: bool,
}

const fn default_true_open() -> bool {
    true
}

#[tauri::command]
async fn trade_search(
    args: TradeSearchArgs,
    app: tauri::AppHandle,
) -> Result<trade_search::TradeSearchSummary, String> {
    use tauri_plugin_shell::ShellExt;
    let league = args
        .league
        .unwrap_or_else(|| "Fate of the Vaal".to_string());
    let summary = build_trade_search_url(&args.item, &league);
    if args.open {
        // tauri-plugin-shell::open is deprecated upstream in 2.10 in
        // favour of tauri-plugin-opener; the shell plugin's open API
        // still works for v1 and avoids adding another plugin dep.
        // Migrate to tauri-plugin-opener in v1.x.
        #[allow(deprecated)]
        app.shell()
            .open(&summary.url, None)
            .map_err(|e| e.to_string())?;
    }
    Ok(summary)
}

// ---------------------------------------------------------------------
// Client.txt watcher (Phase D.1)
// ---------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct StartClientLogArgs {
    /// Absolute path to PoE2's Client.txt log. The Settings panel
    /// will prompt the user to provide it.
    path: String,
}

#[derive(Debug, Serialize)]
struct ClientLogStatus {
    watching: bool,
    path: Option<String>,
}

#[tauri::command]
fn start_client_log(
    args: StartClientLogArgs,
    app: tauri::AppHandle,
    state: tauri::State<'_, AdvisorState>,
) -> Result<ClientLogStatus, String> {
    let path = PathBuf::from(args.path);
    let app_clone = app.clone();
    let watcher = start_client_log_watcher(&path, move |event: ClientLogEvent| {
        let _ = app_clone.emit(CLIENT_LOG_EVENT, event);
    })
    .map_err(|e| e.to_string())?;
    let mut guard = state
        .client_log_watcher
        .lock()
        .map_err(|_| "client_log mutex poisoned".to_string())?;
    *guard = Some(watcher);
    Ok(ClientLogStatus {
        watching: true,
        path: Some(path.display().to_string()),
    })
}

#[tauri::command]
fn stop_client_log(state: tauri::State<'_, AdvisorState>) -> Result<ClientLogStatus, String> {
    let mut guard = state
        .client_log_watcher
        .lock()
        .map_err(|_| "client_log mutex poisoned".to_string())?;
    *guard = None; // dropping the watcher releases the inotify subscription
    Ok(ClientLogStatus {
        watching: false,
        path: None,
    })
}

#[tauri::command]
fn client_log_status(state: tauri::State<'_, AdvisorState>) -> Result<ClientLogStatus, String> {
    let guard = state
        .client_log_watcher
        .lock()
        .map_err(|_| "client_log mutex poisoned".to_string())?;
    Ok(ClientLogStatus {
        watching: guard.is_some(),
        path: None, // we don't store the path on AdvisorState; UI tracks it
    })
}

// ---------------------------------------------------------------------
// Simulation runner (Phase C.3) — bulk Monte-Carlo of one action.
// ---------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct RunNTrialsArgs {
    item: Item,
    action: poc2_advisor::AdvisorAction,
    /// Number of independent trials. Clamped to [1, 10_000].
    n_trials: u32,
    /// RNG seed base. Default 0.
    #[serde(default)]
    seed: u64,
}

#[derive(Debug, Serialize)]
struct TrialDistribution {
    /// Number of trials actually run.
    n_trials: u32,
    /// Fraction of trials where the action succeeded.
    success_rate: f64,
    /// sqrt(p(1-p)/n) — confidence on the rate estimate.
    success_rate_stderr: f64,
    /// Mean number of mod-affecting changes per trial.
    mean_change_count: f64,
    /// Histogram of `change_count` values: `bucket -> count`.
    change_count_histogram: std::collections::BTreeMap<u32, u32>,
    /// Estimated divine-equivalent cost per trial (constant — we use
    /// the action's cost band's expected value).
    cost_per_trial_div: f64,
    /// Estimated total cost across n_trials at the expected per-trial
    /// cost.
    total_cost_div_expected: f64,
}

#[tauri::command]
fn run_n_trials(
    args: RunNTrialsArgs,
    state: tauri::State<'_, AdvisorState>,
) -> Result<TrialDistribution, String> {
    use poc2_engine::omen::OmenSet;
    let n = args.n_trials.clamp(1, 10_000);
    let bundle = state.bundle.read().expect("bundle rwlock poisoned");
    let valuator = state.valuator.lock().expect("valuator mutex poisoned");
    let patch = bundle.bundle_patch.unwrap_or(PatchVersion::PATCH_0_4_0);
    let omens = OmenSet::new();

    let mut successes = 0_u32;
    let mut total_change_count = 0_u32;
    let mut histogram: std::collections::BTreeMap<u32, u32> = std::collections::BTreeMap::new();
    for i in 0..n {
        let seed = args
            .seed
            .wrapping_add(u64::from(i).wrapping_mul(0x9E37_79B9_7F4A_7C15));
        let outcome = poc2_advisor::simulate(
            &args.item,
            &args.action,
            &omens,
            bundle.registry.as_ref(),
            bundle.resolver.as_ref(),
            patch,
            seed,
        );
        if outcome.success {
            successes += 1;
        }
        total_change_count = total_change_count.saturating_add(outcome.change_count);
        *histogram.entry(outcome.change_count).or_insert(0) += 1;
    }
    let n_f = f64::from(n);
    let p = f64::from(successes) / n_f;
    let stderr = if n <= 1 {
        0.0
    } else {
        (p * (1.0 - p) / n_f).sqrt()
    };
    let cost_per_trial = poc2_advisor::action_cost(&args.action, &valuator).expected;
    drop(valuator);
    drop(bundle);
    Ok(TrialDistribution {
        n_trials: n,
        success_rate: p,
        success_rate_stderr: stderr,
        mean_change_count: f64::from(total_change_count) / n_f,
        change_count_histogram: histogram,
        cost_per_trial_div: cost_per_trial,
        total_cost_div_expected: cost_per_trial * n_f,
    })
}

// ---------------------------------------------------------------------
// Streaming recommendations (Phase C.2)
// ---------------------------------------------------------------------

#[derive(Debug, Serialize, Clone)]
struct StreamingProgressEvent {
    /// Beam-search depth this batch was computed at.
    depth: u32,
    /// Top-N recommendations at this depth.
    recommendations: Vec<Recommendation>,
    /// True iff this is the deepest (final) emission.
    is_final: bool,
    /// Patch the planner ran against.
    patch: String,
}

/// Tauri event topic the streaming planner emits to.
const ADVISOR_PROGRESS_EVENT: &str = "advisor://progress";

#[tauri::command]
async fn recommend_streaming(
    args: RecommendArgs,
    app: tauri::AppHandle,
    state: tauri::State<'_, AdvisorState>,
) -> Result<(), String> {
    // Cancel any in-flight task; the abort is best-effort (the worker
    // task uses spawn_blocking and only checks for cancellation
    // between depth-emits via the channel, which is closed when the
    // app handle is dropped).
    if let Ok(mut guard) = state.streaming_task.lock() {
        if let Some(handle) = guard.take() {
            handle.abort();
        }
    }

    let rules = state.rules.clone();
    let bundle = state.bundle.clone();
    let valuator = state.valuator.clone();
    let plugin_host = state.plugin_host.clone();
    let app_clone = app.clone();
    let item = args.item;
    let goal = args.goal;
    let stash = args.stash;
    let risk = args.risk;
    let top_n = args.top_n;
    let depth = args.depth;

    let task = tokio::task::spawn_blocking(move || {
        let bundle_guard = bundle.read().expect("bundle rwlock poisoned");
        let valuator_guard = valuator.lock().expect("valuator mutex poisoned");
        let plugin_guard = plugin_host.read().expect("plugin_host poisoned");
        let patch = bundle_guard
            .bundle_patch
            .unwrap_or(PatchVersion::PATCH_0_4_0);
        let input = PlanInput {
            item,
            goal,
            rules: rules.as_ref(),
            strategies: bundle_guard.strategies.as_ref(),
            registry: bundle_guard.registry.as_ref(),
            resolver: bundle_guard.resolver.as_ref(),
            valuator: &valuator_guard,
            stash: &stash,
            patch,
            plugin_dispatch: Some(&*plugin_guard as &dyn poc2_strategies::PluginPredicateDispatch),
            config: BeamConfig {
                width: top_n.max(3),
                depth: depth.max(1),
                risk,
                top_n,
                seed: 0,
                mc_samples: 50,
                weights: poc2_advisor::ScoringWeights::default(),
            },
        };
        // Run depth-1 → depth-3 → final-depth, with the final being
        // the user-configured depth (clamped to [1, 8] for sanity).
        let final_depth = depth.clamp(1, 8);
        let mut depths = Vec::with_capacity(3);
        depths.push(1);
        if final_depth >= 3 && !depths.contains(&3) {
            depths.push(3);
        }
        if !depths.contains(&final_depth) {
            depths.push(final_depth);
        }
        plan_streaming(&input, &depths, |progress: StreamingProgress| {
            let event = StreamingProgressEvent {
                depth: progress.depth,
                recommendations: progress.recommendations,
                is_final: progress.is_final,
                patch: format!("{patch}"),
            };
            // Best-effort emit; if the frontend hung up, drop the event.
            let _ = app_clone.emit(ADVISOR_PROGRESS_EVENT, event);
        });
        drop(valuator_guard);
        drop(bundle_guard);
    });

    if let Ok(mut guard) = state.streaming_task.lock() {
        *guard = Some(task);
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
struct ReloadBundleArgs {
    /// Optional explicit path. `None` re-runs the XDG-aware bundle
    /// search; `Some(p)` loads the named file directly.
    #[serde(default)]
    path: Option<String>,
}

#[derive(Debug, Serialize)]
struct ReloadBundleResponse {
    /// Path of the bundle that was loaded (or null when the search
    /// found nothing).
    bundle_path: Option<String>,
    patch: Option<String>,
    mod_count: usize,
    strategy_count: usize,
}

/// Hot-swap the loaded bundle without restarting the app.
///
/// Per A.6 of the v1 execution plan. Acquires a write lock on the
/// shared `BundleState` and replaces the registry, strategies,
/// resolver, bundle_path, and bundle_patch in one atomic update.
/// Subsequent `recommend` calls pick up the new state immediately.
///
/// Re-loads user strategies from `$XDG_CONFIG_HOME/poc2/strategies/`
/// as part of the swap.
#[tauri::command]
fn reload_bundle(
    args: ReloadBundleArgs,
    state: tauri::State<'_, AdvisorState>,
) -> Result<ReloadBundleResponse, String> {
    let path_override = args.path.as_deref().map(Path::new);
    let new_state = build_bundle_state(path_override);
    let response = ReloadBundleResponse {
        bundle_path: new_state
            .bundle_path
            .as_ref()
            .map(|p| p.display().to_string()),
        patch: new_state.bundle_patch.map(|p| format!("{p}")),
        mod_count: new_state.registry.len(),
        strategy_count: new_state.strategies.len(),
    };
    let mut guard = state.bundle.write().expect("bundle rwlock poisoned");
    *guard = new_state;
    drop(guard);
    Ok(response)
}

#[derive(Debug, Deserialize)]
struct RefreshPricesArgs {
    /// Optional league override; defaults to the bundle's patch league.
    #[serde(default)]
    league: Option<String>,
}

#[derive(Debug, Serialize)]
struct RefreshPricesResponse {
    refreshed: bool,
    meta: Option<PriceRefreshMeta>,
    error: Option<String>,
}

#[tauri::command]
async fn refresh_prices(
    args: RefreshPricesArgs,
    state: tauri::State<'_, AdvisorState>,
) -> Result<RefreshPricesResponse, String> {
    let client = reqwest::Client::builder()
        .user_agent(concat!(
            "poc2-desktop/",
            env!("CARGO_PKG_VERSION"),
            " (+contact: github issues)"
        ))
        .gzip(true)
        .build()
        .map_err(|e| e.to_string())?;
    let league = args.league.as_deref();
    match fetch_price_snapshot(&client, league, None).await {
        Ok(snapshot) => {
            let mapping = default_id_mapping();
            let mut guard = state.valuator.lock().expect("valuator mutex poisoned");
            let applied = apply_feed_to_valuator(&mut guard, &snapshot, &mapping);
            let total = snapshot.entries.len();
            let meta = PriceRefreshMeta {
                league: snapshot.league.clone(),
                fetched_at: snapshot.fetched_at.clone(),
                applied_count: applied,
                total_entries: total,
            };
            *state
                .price_refresh
                .lock()
                .expect("price_refresh mutex poisoned") = Some(meta.clone());
            Ok(RefreshPricesResponse {
                refreshed: true,
                meta: Some(meta),
                error: None,
            })
        }
        Err(e) => Ok(RefreshPricesResponse {
            refreshed: false,
            meta: None,
            error: Some(e.to_string()),
        }),
    }
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
            recommend_streaming,
            run_n_trials,
            parse_item_text,
            read_clipboard_item,
            refresh_prices,
            reload_bundle,
            load_state,
            save_state,
            recovery_hints,
            list_leagues,
            list_recipes,
            save_recipe,
            load_recipe,
            delete_recipe,
            export_recipe_toml,
            start_client_log,
            stop_client_log,
            client_log_status,
            trade_search,
            fetch_meta_builds,
            list_plugins,
            reload_plugins,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
