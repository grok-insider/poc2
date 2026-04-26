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

use poc2_advisor::{plan, BeamConfig, Goal, PlanInput, Recommendation, Stash};
use poc2_data::Bundle;
use poc2_engine::currency::DefaultCurrencyResolver;
use poc2_engine::item::Item;
use poc2_engine::patch::PatchVersion;
use poc2_engine::registry::ModRegistry;
use poc2_market::{
    apply_feed_to_valuator, default_id_mapping, fetch_snapshot as fetch_price_snapshot, Valuator,
};
use poc2_parser::{lower_to_item, parse_clipboard_text, ParsedItem};
use poc2_rules::RuleSet;
use poc2_strategies::StrategyRegistry;
use serde::{Deserialize, Serialize};
use tauri::Manager;
use tauri_plugin_clipboard_manager::ClipboardExt;
use tracing_subscriber::EnvFilter;

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
        Self {
            bundle: Arc::new(RwLock::new(bundle_state)),
            rules: Arc::new(rules),
            valuator: Arc::new(Mutex::new(Valuator::default())),
            price_refresh: Arc::new(Mutex::new(None)),
        }
    }
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
// Bundle hot-swap (Phase A.6)
// ---------------------------------------------------------------------

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
            parse_item_text,
            read_clipboard_item,
            refresh_prices,
            reload_bundle,
            load_state,
            save_state,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
