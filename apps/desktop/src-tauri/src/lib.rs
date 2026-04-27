//! poc2-desktop — Tauri 2 entry point.
//!
//! Boots the runtime, builds shared advisor state (mod registry, rule
//! catalogue, strategy registry, currency resolver, valuator), and
//! exposes the `recommend` IPC command for the frontend.
//!
//! Application logic lives in the workspace crates (`poc2-engine`,
//! `poc2-advisor`, etc.). The Tauri layer only adapts those crates to
//! IPC commands and lifecycle events.

use std::collections::HashSet;
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
    /// Indexed `bundle.base_items` (M14.2). Currently held but not yet
    /// consumed by Tauri commands; M14.5 (Catalyst class gate) and M14.6
    /// (Bone subtype gate) thread it through `apply_currency_with_bases`
    /// so the engine can resolve real `BaseTypeId → ItemClassId` for
    /// pipeline-built items. Until then, advisor commands run against
    /// fixture-shaped items whose `Item.base` is a class-id placeholder
    /// and falls through `class_for_item`'s back-compat path.
    #[allow(dead_code)]
    base_registry: Arc<poc2_engine::BaseRegistry>,
    strategies: Arc<StrategyRegistry>,
    resolver: Arc<DefaultCurrencyResolver>,
    bundle_path: Option<PathBuf>,
    bundle_patch: Option<PatchVersion>,
    asset_seeds: Arc<Vec<AssetEntry>>,
    /// Structured M14.7 migration warning surfaced when the loader
    /// found a schema-mismatched bundle on disk (typically a v1
    /// bundle from a pre-v3 install). `None` means "no migration
    /// needed". The frontend reads this via `bundle_migration_status`
    /// to render the rebuild dialog.
    migration_warning: Option<BundleMigrationWarning>,
}

/// Structured payload for the v1 → v2 (or future schema bumps)
/// migration UI. Populated by the loader when it finds a bundle that
/// cannot be loaded due to schema mismatch.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
struct BundleMigrationWarning {
    bundle_path: String,
    bundle_version: u32,
    loader_version: u32,
    /// Pre-formatted, human-readable message including the rebuild
    /// command. Surfaced verbatim by the desktop UI's migration dialog.
    message: String,
    /// Whether the loader also detected legacy state (`state.toml` with
    /// the previous schema marker) that will be wiped on next launch.
    /// `false` when state is already on the current schema or absent.
    state_will_be_reset: bool,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
struct AssetEntry {
    id: String,
    name: String,
    kind: String,
    detail_url: Option<String>,
    source_url: Option<String>,
    local_path: Option<String>,
    status: String,
    error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct AssetManifest {
    generated_at: String,
    entries: Vec<AssetEntry>,
}

#[derive(Debug, Serialize)]
struct AssetStatus {
    total: usize,
    cached: usize,
    missing: usize,
    failed: usize,
    root: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CacheAssetsArgs {
    #[serde(default)]
    refresh: bool,
    #[serde(default = "default_asset_limit")]
    limit: usize,
}

const fn default_asset_limit() -> usize {
    96
}

const ASSET_BATCH_LIMIT: usize = 96;

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
    let outcome = match path_override {
        Some(p) => match try_load_bundle(p) {
            BundleLoadOutcome::Loaded(b, path) => BundleSearchOutcome::Loaded(b, path),
            BundleLoadOutcome::SchemaMismatch {
                path,
                bundle_version,
                loader_version,
            } => BundleSearchOutcome::SchemaMismatch {
                path,
                bundle_version,
                loader_version,
            },
            BundleLoadOutcome::Other => BundleSearchOutcome::NotFound,
        },
        None => load_bundle_from_known_paths(),
    };
    let mut migration_warning: Option<BundleMigrationWarning> = None;
    let (registry, base_registry, bundle_path, bundle_patch, essences, catalysts, asset_seeds) =
        match outcome {
            BundleSearchOutcome::Loaded(bundle, path) => {
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
                // M14.7c: when the active bundle is the v3 schema, check
                // legacy state.toml and wipe it. Returns true when state
                // was reset, which feeds into the migration_warning.
                let state_was_reset = legacy_state_hard_reset_if_needed();
                if state_was_reset {
                    migration_warning = Some(BundleMigrationWarning {
                        bundle_path: path.display().to_string(),
                        bundle_version: poc2_data::BUNDLE_SCHEMA_VERSION,
                        loader_version: poc2_data::BUNDLE_SCHEMA_VERSION,
                        message: "Bundle is on the current v3 schema. Legacy user state \
                                  (state.toml + recipes/) was reset to a clean slate per \
                                  the v3 hard-reset migration policy. Cache is preserved."
                            .into(),
                        state_will_be_reset: true,
                    });
                }
                let asset_seeds = build_asset_seeds(&bundle);
                let essences = bundle.essence_catalogue();
                let catalysts = bundle.catalyst_catalogue();
                let base_registry = poc2_engine::BaseRegistry::from_bases(bundle.base_items);
                tracing::info!(
                    bases_indexed = base_registry.len(),
                    "indexed bases into base registry"
                );
                let registry = ModRegistry::from_mods(bundle.mods, bundle.weights);
                tracing::info!(
                    weight_observations = registry.weight_observation_count(),
                    "indexed weight observations into mod registry"
                );
                (
                    registry,
                    base_registry,
                    Some(path),
                    Some(patch),
                    essences,
                    catalysts,
                    asset_seeds,
                )
            }
            BundleSearchOutcome::SchemaMismatch {
                path,
                bundle_version,
                loader_version,
            } => {
                tracing::warn!(
                    path = %path.display(),
                    bundle_version,
                    loader_version,
                    "bundle on disk is the wrong schema version (M14.7 v1→v2)"
                );
                let message = format!(
                    "Found a v{bundle_version} bundle at {} but this build expects v{loader_version}. \
                     Rebuild the bundle via `cargo run -p poc2-pipeline -- build` to upgrade. \
                     The advisor is running with an empty registry until the bundle is rebuilt.",
                    path.display()
                );
                migration_warning = Some(BundleMigrationWarning {
                    bundle_path: path.display().to_string(),
                    bundle_version,
                    loader_version,
                    message,
                    state_will_be_reset: true,
                });
                (
                    ModRegistry::from_mods(Vec::new(), Vec::new()),
                    poc2_engine::BaseRegistry::default(),
                    None,
                    None,
                    Vec::new(),
                    Vec::new(),
                    build_asset_seeds_without_bundle(),
                )
            }
            BundleSearchOutcome::NotFound => {
                tracing::warn!(
                    "no data bundle found; running with empty mod registry. \
                     Build a bundle via the pipeline (`cargo run -p poc2-pipeline -- build`) \
                     and place it in `~/.config/poc2/bundles/` or set POC2_BUNDLE."
                );
                (
                    ModRegistry::from_mods(Vec::new(), Vec::new()),
                    poc2_engine::BaseRegistry::default(),
                    None,
                    None,
                    Vec::new(),
                    Vec::new(),
                    build_asset_seeds_without_bundle(),
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
        base_registry: Arc::new(base_registry),
        strategies: Arc::new(strategies),
        resolver: Arc::new(resolver),
        bundle_path,
        bundle_patch,
        asset_seeds: Arc::new(asset_seeds),
        migration_warning,
    }
}

/// M14.7c — wipe legacy `state.toml` + `recipes/` if present.
///
/// Cache (`~/.config/poc2/cache/`) is preserved per the v3 plan §10:
/// price + meta caches are bundle-version-agnostic.
///
/// The presence-detection heuristic is intentionally simple: any
/// `state.toml` file at the conventional path is considered legacy
/// when this code runs for the first time after a v2 upgrade. We
/// drop a `state.toml.v3-migrated` marker file beside the wiped
/// `state.toml` so subsequent launches don't repeatedly wipe the
/// freshly-written v3 state. Returns `true` iff a wipe happened.
fn legacy_state_hard_reset_if_needed() -> bool {
    let Some(state_dir) = poc2_state_dir() else {
        return false;
    };
    let marker = state_dir.join(".v3-migrated");
    if marker.exists() {
        return false;
    }
    let mut wiped_anything = false;
    let state_toml = state_dir.join("state.toml");
    if state_toml.exists() {
        match std::fs::remove_file(&state_toml) {
            Ok(()) => {
                tracing::info!(path = %state_toml.display(), "M14.7c: wiped legacy state.toml");
                wiped_anything = true;
            }
            Err(e) => {
                tracing::warn!(path = %state_toml.display(), error = %e, "failed to wipe legacy state.toml");
            }
        }
    }
    let recipes_dir = state_dir.join("recipes");
    if recipes_dir.exists() {
        match std::fs::remove_dir_all(&recipes_dir) {
            Ok(()) => {
                tracing::info!(
                    path = %recipes_dir.display(),
                    "M14.7c: wiped legacy recipes/"
                );
                wiped_anything = true;
            }
            Err(e) => {
                tracing::warn!(
                    path = %recipes_dir.display(),
                    error = %e,
                    "failed to wipe legacy recipes/"
                );
            }
        }
    }
    // Drop the marker so we never wipe again.
    if let Err(e) = std::fs::create_dir_all(&state_dir) {
        tracing::warn!(error = %e, "failed to create state dir for v3 marker");
    }
    if let Err(e) = std::fs::write(&marker, b"v3 migration marker; do not delete\n") {
        tracing::warn!(path = %marker.display(), error = %e, "failed to write v3 migration marker");
    }
    wiped_anything
}

/// Resolve `~/.config/poc2/` (or `$XDG_CONFIG_HOME/poc2/`) — the canonical
/// state directory used by the desktop app.
fn poc2_state_dir() -> Option<PathBuf> {
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        return Some(Path::new(&xdg).join("poc2"));
    }
    std::env::var_os("HOME").map(|home| Path::new(&home).join(".config/poc2"))
}

fn build_asset_seeds(bundle: &Bundle) -> Vec<AssetEntry> {
    let mut seeds = build_asset_seeds_without_bundle();
    let mut seen: HashSet<String> = seeds.iter().map(|a| a.id.clone()).collect();

    for entry in &bundle.omens.entries {
        let Some(id) = entry.get("id").and_then(serde_json::Value::as_str) else {
            continue;
        };
        let name = entry
            .get("name")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(id);
        let icon_url = entry
            .get("icon_url")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string);
        if seen.insert(id.to_string()) {
            seeds.push(asset_seed(
                id.to_string(),
                name.to_string(),
                "omen",
                Some(poe2db_detail_url(name)),
                icon_url,
            ));
        }
    }

    for entry in &bundle.essences.entries {
        let Some(name) = entry.get("name").and_then(serde_json::Value::as_str) else {
            continue;
        };
        let id = essence_asset_id(
            name,
            entry
                .get("corrupt")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
        );
        if seen.insert(id.clone()) {
            seeds.push(asset_seed(
                id,
                name.to_string(),
                "essence",
                Some(poe2db_detail_url(name)),
                entry
                    .get("icon_url")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string),
            ));
        }
    }

    for entry in &bundle.bones.entries {
        let Some(id) = entry.get("id").and_then(serde_json::Value::as_str) else {
            continue;
        };
        let name = entry
            .get("name")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(id);
        if seen.insert(id.to_string()) {
            seeds.push(asset_seed(
                id.to_string(),
                name.to_string(),
                "bone",
                Some(poe2db_detail_url(name)),
                None,
            ));
        }
    }

    seeds
}

fn build_asset_seeds_without_bundle() -> Vec<AssetEntry> {
    let mut seeds: Vec<AssetEntry> = known_item_class_assets()
        .into_iter()
        .map(|(id, name)| asset_seed(id.into(), name.into(), "class", None, None))
        .collect();
    seeds.extend(known_currency_assets().into_iter().map(|(id, name)| {
        asset_seed(
            id.into(),
            name.into(),
            "currency",
            Some(poe2db_detail_url(name)),
            None,
        )
    }));
    seeds
}

fn known_item_class_assets() -> Vec<(&'static str, &'static str)> {
    vec![
        ("BodyArmour", "Body Armour"),
        ("Helmet", "Helmet"),
        ("Helmets", "Helmets"),
        ("Gloves", "Gloves"),
        ("Boots", "Boots"),
        ("Bow", "Bow"),
        ("Crossbow", "Crossbow"),
        ("Staff", "Staff"),
        ("Quarterstaff", "Quarterstaff"),
        ("OneHandSword", "One Hand Sword"),
        ("OneHandAxe", "One Hand Axe"),
        ("OneHandMace", "One Hand Mace"),
        ("Spear", "Spear"),
        ("Flail", "Flail"),
        ("Claw", "Claw"),
        ("Dagger", "Dagger"),
        ("Wand", "Wand"),
        ("Sceptre", "Sceptre"),
        ("TwoHandSword", "Two Hand Sword"),
        ("TwoHandAxe", "Two Hand Axe"),
        ("TwoHandMace", "Two Hand Mace"),
        ("OneHandWeapon", "One Hand Weapon"),
        ("TwoHandWeapon", "Two Hand Weapon"),
        ("Ring", "Ring"),
        ("Amulet", "Amulet"),
        ("Belt", "Belt"),
        ("Focus", "Focus"),
        ("Shield", "Shield"),
        ("Quiver", "Quiver"),
    ]
}

fn known_currency_assets() -> Vec<(&'static str, &'static str)> {
    vec![
        ("OrbOfTransmutation", "Orb of Transmutation"),
        ("GreaterOrbOfTransmutation", "Greater Orb of Transmutation"),
        ("PerfectOrbOfTransmutation", "Perfect Orb of Transmutation"),
        ("OrbOfAugmentation", "Orb of Augmentation"),
        ("GreaterOrbOfAugmentation", "Greater Orb of Augmentation"),
        ("PerfectOrbOfAugmentation", "Perfect Orb of Augmentation"),
        ("RegalOrb", "Regal Orb"),
        ("GreaterRegalOrb", "Greater Regal Orb"),
        ("PerfectRegalOrb", "Perfect Regal Orb"),
        ("OrbOfAlchemy", "Orb of Alchemy"),
        ("ExaltedOrb", "Exalted Orb"),
        ("GreaterExaltedOrb", "Greater Exalted Orb"),
        ("PerfectExaltedOrb", "Perfect Exalted Orb"),
        ("OrbOfAnnulment", "Orb of Annulment"),
        ("ChaosOrb", "Chaos Orb"),
        ("GreaterChaosOrb", "Greater Chaos Orb"),
        ("PerfectChaosOrb", "Perfect Chaos Orb"),
        ("DivineOrb", "Divine Orb"),
        ("VaalOrb", "Vaal Orb"),
        ("HinekorasLock", "Hinekora's Lock"),
        ("FracturingOrb", "Fracturing Orb"),
        ("FleshCatalyst", "Flesh Catalyst"),
        ("IntrinsicCatalyst", "Intrinsic Catalyst"),
        ("ReaverCatalyst", "Reaver Catalyst"),
        ("CarapaceCatalyst", "Carapace Catalyst"),
        ("UnstableCatalyst", "Unstable Catalyst"),
        ("AdaptiveCatalyst", "Adaptive Catalyst"),
    ]
}

fn asset_seed(
    id: String,
    name: String,
    kind: impl Into<String>,
    detail_url: Option<String>,
    source_url: Option<String>,
) -> AssetEntry {
    let source_url = source_url.or_else(|| known_remote_asset_url(&id).map(str::to_string));
    AssetEntry {
        id,
        name,
        kind: kind.into(),
        detail_url,
        source_url,
        local_path: None,
        status: "missing".into(),
        error: None,
    }
}

fn known_remote_asset_url(id: &str) -> Option<&'static str> {
    match id {
        "BodyArmour" => Some(
            "https://cdn.poe2db.tw/image/Art/2DItems/Armours/BodyArmours/Basetypes/BodyInt03.webp",
        ),
        "Helmet" | "Helmets" => Some(
            "https://cdn.poe2db.tw/image/Art/2DItems/Armours/Helmets/Basetypes/HelmetInt03.webp",
        ),
        "Boots" => {
            Some("https://cdn.poe2db.tw/image/Art/2DItems/Armours/Boots/Basetypes/BootsDex01.webp")
        }
        "Ring" => Some("https://cdn.poe2db.tw/image/Art/2DItems/Rings/Basetypes/IronRing.webp"),
        "Amulet" => {
            Some("https://cdn.poe2db.tw/image/Art/2DItems/Amulets/Basetypes/GoldAmulet.webp")
        }
        "OrbOfTransmutation" | "GreaterOrbOfTransmutation" | "PerfectOrbOfTransmutation" => {
            Some("https://cdn.poe2db.tw/image/Art/2DItems/Currency/CurrencyUpgradeToMagic.webp")
        }
        "OrbOfAugmentation" | "GreaterOrbOfAugmentation" | "PerfectOrbOfAugmentation" => {
            Some("https://cdn.poe2db.tw/image/Art/2DItems/Currency/CurrencyAddModToMagic.webp")
        }
        "OrbOfAlchemy" => {
            Some("https://cdn.poe2db.tw/image/Art/2DItems/Currency/CurrencyUpgradeToRare.webp")
        }
        "ExaltedOrb" | "GreaterExaltedOrb" | "PerfectExaltedOrb" => {
            Some("https://cdn.poe2db.tw/image/Art/2DItems/Currency/CurrencyAddModToRare.webp")
        }
        "DivineOrb" => {
            Some("https://cdn.poe2db.tw/image/Art/2DItems/Currency/CurrencyModValues.webp")
        }
        "ChaosOrb" | "GreaterChaosOrb" | "PerfectChaosOrb" => {
            Some("https://cdn.poe2db.tw/image/Art/2DItems/Currency/CurrencyRerollRare.webp")
        }
        "RegalOrb" | "GreaterRegalOrb" | "PerfectRegalOrb" => {
            Some("https://cdn.poe2db.tw/image/Art/2DItems/Currency/CurrencyUpgradeMagicToRare.webp")
        }
        "VaalOrb" => Some("https://cdn.poe2db.tw/image/Art/2DItems/Currency/CurrencyCorrupt.webp"),
        "OrbOfAnnulment" => Some("https://cdn.poe2db.tw/image/Art/2DItems/Currency/AnnullOrb.webp"),
        _ => None,
    }
}

fn poe2db_detail_url(name: &str) -> String {
    format!("https://poe2db.tw/us/{}", slug_name(name))
}

fn slug_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c
            } else if c.is_whitespace() || c == '-' || c == '_' {
                '_'
            } else {
                '\0'
            }
        })
        .filter(|c| *c != '\0')
        .collect::<String>()
        .split('_')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("_")
}

fn essence_asset_id(name: &str, corrupt: bool) -> String {
    if corrupt {
        return format!("CorruptedEssenceOf{}", essence_suffix(name));
    }
    let prefix = if name.starts_with("Lesser ") {
        "LesserEssenceOf"
    } else if name.starts_with("Greater ") {
        "GreaterEssenceOf"
    } else if name.starts_with("Perfect ") {
        "PerfectEssenceOf"
    } else {
        "EssenceOf"
    };
    format!("{prefix}{}", essence_suffix(name))
}

fn essence_suffix(name: &str) -> String {
    name.split_whitespace()
        .filter(|w| {
            !matches!(
                *w,
                "Essence" | "of" | "the" | "Lesser" | "Greater" | "Perfect" | "Corrupted"
            )
        })
        .collect()
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
/// Search outcome variant of [`try_load_bundle`] for the bundle-search
/// helpers. Distinguishes a successful load from "I found a bundle but
/// it's the wrong schema (rebuild needed)" vs "no bundle at all".
enum BundleSearchOutcome {
    Loaded(Bundle, PathBuf),
    SchemaMismatch {
        path: PathBuf,
        bundle_version: u32,
        loader_version: u32,
    },
    NotFound,
}

fn load_bundle_from_known_paths() -> BundleSearchOutcome {
    if let Ok(env_path) = std::env::var("POC2_BUNDLE") {
        let p = PathBuf::from(env_path);
        if p.is_file() {
            match try_load_bundle(&p) {
                BundleLoadOutcome::Loaded(b, path) => {
                    return BundleSearchOutcome::Loaded(b, path);
                }
                BundleLoadOutcome::SchemaMismatch {
                    path,
                    bundle_version,
                    loader_version,
                } => {
                    return BundleSearchOutcome::SchemaMismatch {
                        path,
                        bundle_version,
                        loader_version,
                    };
                }
                BundleLoadOutcome::Other => {}
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
    let mut latest_mismatch: Option<(PathBuf, u32, u32)> = None;
    for dir in search_dirs {
        match search_bundle_in_dir(&dir) {
            BundleSearchOutcome::Loaded(b, p) => return BundleSearchOutcome::Loaded(b, p),
            BundleSearchOutcome::SchemaMismatch {
                path,
                bundle_version,
                loader_version,
            } if latest_mismatch.is_none() => {
                latest_mismatch = Some((path, bundle_version, loader_version));
            }
            _ => {}
        }
    }
    if let Some((path, bundle_version, loader_version)) = latest_mismatch {
        BundleSearchOutcome::SchemaMismatch {
            path,
            bundle_version,
            loader_version,
        }
    } else {
        BundleSearchOutcome::NotFound
    }
}

/// Find the most recently modified `*.bundle.json{,.gz}` in `dir` and load
/// it. Surfaces a [`BundleSearchOutcome`] so the caller can distinguish a
/// schema-mismatch (rebuild needed) from a generic miss.
fn search_bundle_in_dir(dir: &Path) -> BundleSearchOutcome {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return BundleSearchOutcome::NotFound;
    };
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
    let mut latest_mismatch: Option<(PathBuf, u32, u32)> = None;
    for (path, _) in candidates {
        match try_load_bundle(&path) {
            BundleLoadOutcome::Loaded(b, p) => return BundleSearchOutcome::Loaded(b, p),
            BundleLoadOutcome::SchemaMismatch {
                path,
                bundle_version,
                loader_version,
            } if latest_mismatch.is_none() => {
                latest_mismatch = Some((path, bundle_version, loader_version));
            }
            _ => {}
        }
    }
    if let Some((path, bundle_version, loader_version)) = latest_mismatch {
        BundleSearchOutcome::SchemaMismatch {
            path,
            bundle_version,
            loader_version,
        }
    } else {
        BundleSearchOutcome::NotFound
    }
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

/// Outcome of a single bundle-load attempt.
///
/// Distinguishes "loaded fine", "loadable but bundle was rejected because
/// it was built against an older schema (rebuild needed)", and "other
/// failure (read/parse/validation)". The desktop UI surfaces the first
/// two via dedicated dialogs; "other" stays as a tracing warning.
enum BundleLoadOutcome {
    Loaded(Bundle, PathBuf),
    /// Bundle parsed but its `schema_version` doesn't match
    /// [`poc2_data::BUNDLE_SCHEMA_VERSION`]. The most common case in
    /// v3 is a leftover v1 bundle from a pre-v3 install.
    SchemaMismatch {
        path: PathBuf,
        bundle_version: u32,
        loader_version: u32,
    },
    /// Read/parse failure or non-schema validation error.
    Other,
}

fn try_load_bundle(path: &Path) -> BundleLoadOutcome {
    match poc2_data::io::read_bundle(path) {
        Ok(b) => match b.validate() {
            Ok(()) => BundleLoadOutcome::Loaded(b, path.to_path_buf()),
            Err(poc2_data::DataError::SchemaVersionMismatch { bundle, expected }) => {
                tracing::warn!(
                    path = %path.display(),
                    bundle_version = bundle,
                    loader_version = expected,
                    "bundle schema mismatch — rebuild needed (M14.7 v1→v2)"
                );
                BundleLoadOutcome::SchemaMismatch {
                    path: path.to_path_buf(),
                    bundle_version: bundle,
                    loader_version: expected,
                }
            }
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "bundle failed validation");
                BundleLoadOutcome::Other
            }
        },
        Err(e) => {
            tracing::warn!(path = %path.display(), error = %e, "bundle read failed");
            BundleLoadOutcome::Other
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

/// M14.7b — surface the bundle migration state to the desktop UI.
///
/// Returns `None` when the bundle loaded cleanly under the current
/// schema. Returns a structured warning when the loader detected:
/// - A bundle on disk built against an older schema (rebuild needed).
/// - Legacy state (`state.toml` / `recipes/`) that was wiped on first
///   v3 launch per the hard-reset migration policy.
///
/// The UI consumes this once on app start to render the migration
/// dialog.
#[tauri::command]
fn bundle_migration_status(
    state: tauri::State<'_, AdvisorState>,
) -> Result<Option<BundleMigrationWarning>, String> {
    let bundle = state.bundle.read().map_err(|e| e.to_string())?;
    Ok(bundle.migration_warning.clone())
}

#[tauri::command]
fn asset_manifest(state: tauri::State<'_, AdvisorState>) -> Result<AssetManifest, String> {
    let entries = merged_asset_entries(&state)?;
    Ok(AssetManifest {
        generated_at: now_iso8601(),
        entries,
    })
}

#[tauri::command]
fn asset_status(state: tauri::State<'_, AdvisorState>) -> Result<AssetStatus, String> {
    let entries = merged_asset_entries(&state)?;
    Ok(asset_status_from_entries(entries, assets_dir()))
}

#[tauri::command]
async fn cache_all_assets(
    args: CacheAssetsArgs,
    state: tauri::State<'_, AdvisorState>,
) -> Result<AssetStatus, String> {
    let Some(root) = assets_dir() else {
        return Err("no $XDG_CONFIG_HOME or $HOME — cannot cache assets".into());
    };
    std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;

    let mut entries = merged_asset_entries(&state)?;
    let client = reqwest::Client::builder()
        .user_agent("poc2-asset-cache/0.1")
        .timeout(std::time::Duration::from_secs(8))
        .build()
        .map_err(|e| e.to_string())?;

    let batch_limit = args.limit.min(ASSET_BATCH_LIMIT);
    let jobs = select_asset_batch(&entries, args.refresh, batch_limit);
    tracing::info!(count = jobs.len(), root = %root.display(), "caching asset batch");

    let mut set = tokio::task::JoinSet::new();
    for mut entry in jobs {
        let client = client.clone();
        let root = root.clone();
        let refresh = args.refresh;
        set.spawn(async move {
            if let Err(e) = cache_one_asset(&client, &root, &mut entry, refresh).await {
                entry.status = "failed".into();
                entry.error = Some(e);
            }
            entry
        });
    }

    while let Some(result) = set.join_next().await {
        let updated = result.map_err(|e| e.to_string())?;
        if let Some(entry) = entries.iter_mut().find(|entry| entry.id == updated.id) {
            *entry = updated;
        }
    }

    write_asset_manifest(&root, &entries)?;
    Ok(asset_status_from_entries(entries, Some(root)))
}

fn select_asset_batch(entries: &[AssetEntry], refresh: bool, limit: usize) -> Vec<AssetEntry> {
    let mut candidates: Vec<AssetEntry> = entries
        .iter()
        .filter(|entry| refresh || entry.status == "missing")
        .cloned()
        .collect();
    candidates.sort_by_key(asset_priority);
    candidates.truncate(limit);
    candidates
}

fn asset_priority(entry: &AssetEntry) -> u8 {
    match entry.kind.as_str() {
        "class" => 0,
        "currency" => 1,
        "omen" => 2,
        "essence" => 3,
        "catalyst" => 4,
        "bone" => 5,
        "base" => 6,
        _ => 9,
    }
}

fn assets_dir() -> Option<PathBuf> {
    if let Some(xdg_config) = std::env::var_os("XDG_CONFIG_HOME") {
        Some(Path::new(&xdg_config).join("poc2/assets"))
    } else {
        std::env::var_os("HOME").map(|home| Path::new(&home).join(".config/poc2/assets"))
    }
}

fn merged_asset_entries(state: &tauri::State<'_, AdvisorState>) -> Result<Vec<AssetEntry>, String> {
    let bundle = state.bundle.read().expect("bundle rwlock poisoned");
    let mut entries = (*bundle.asset_seeds).clone();
    drop(bundle);

    if let Some(root) = assets_dir() {
        let cached = read_asset_manifest(&root);
        for entry in &mut entries {
            if let Some(existing) = cached.iter().find(|candidate| candidate.id == entry.id) {
                entry.source_url = entry
                    .source_url
                    .clone()
                    .filter(|url| url.starts_with("http://") || url.starts_with("https://"))
                    .or_else(|| existing.source_url.clone());
                entry.local_path = existing.local_path.clone();
                entry.status = existing.status.clone();
                entry.error = existing.error.clone();
            }
            if let Some(local_path) = &entry.local_path {
                if Path::new(local_path).is_file() {
                    entry.status = "cached".into();
                    entry.error = None;
                }
            }
        }
    }

    entries.sort_by(|a, b| a.kind.cmp(&b.kind).then_with(|| a.name.cmp(&b.name)));
    Ok(entries)
}

fn read_asset_manifest(root: &Path) -> Vec<AssetEntry> {
    let path = root.join("manifest.json");
    let Ok(contents) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    serde_json::from_str::<AssetManifest>(&contents)
        .map(|m| m.entries)
        .unwrap_or_default()
}

fn write_asset_manifest(root: &Path, entries: &[AssetEntry]) -> Result<(), String> {
    let manifest = AssetManifest {
        generated_at: now_iso8601(),
        entries: entries.to_vec(),
    };
    let path = root.join("manifest.json");
    let tmp = root.join("manifest.json.tmp");
    let serialized = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, serialized).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
    Ok(())
}

async fn cache_one_asset(
    client: &reqwest::Client,
    root: &Path,
    entry: &mut AssetEntry,
    refresh: bool,
) -> Result<(), String> {
    let source_url = match &entry.source_url {
        Some(url) => url.clone(),
        None => discover_asset_url(client, entry).await?,
    };
    let ext = image_extension(&source_url);
    let rel = format!(
        "{}/{}.{}",
        sanitize_path_segment(&entry.kind),
        sanitize_path_segment(&entry.id),
        ext
    );
    let dest = root.join(&rel);
    if dest.is_file() && !refresh {
        entry.source_url = Some(source_url);
        entry.local_path = Some(dest.display().to_string());
        entry.status = "cached".into();
        entry.error = None;
        return Ok(());
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let bytes = if source_url.starts_with("generated:") {
        generated_asset_svg(entry).into_bytes().into()
    } else {
        client
            .get(&source_url)
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?
            .bytes()
            .await
            .map_err(|e| e.to_string())?
    };
    let tmp = dest.with_extension(format!("{ext}.tmp"));
    std::fs::write(&tmp, &bytes).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &dest).map_err(|e| e.to_string())?;
    entry.source_url = Some(source_url);
    entry.local_path = Some(dest.display().to_string());
    entry.status = "cached".into();
    entry.error = None;
    Ok(())
}

async fn discover_asset_url(
    client: &reqwest::Client,
    entry: &AssetEntry,
) -> Result<String, String> {
    let detail_url = entry
        .detail_url
        .as_ref()
        .ok_or_else(|| "no detail page available".to_string())?;
    let html = client
        .get(detail_url)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .text()
        .await
        .map_err(|e| e.to_string())?;
    extract_og_image(&html).ok_or_else(|| format!("no og:image found at {detail_url}"))
}

fn extract_og_image(html: &str) -> Option<String> {
    for marker in [
        "property=\"og:image\"",
        "property='og:image'",
        "name=\"og:image\"",
        "name='og:image'",
    ] {
        let Some(pos) = html.find(marker) else {
            continue;
        };
        let tail = &html[pos..html.len().min(pos + 600)];
        if let Some(content_pos) = tail.find("content=") {
            let after = &tail[content_pos + "content=".len()..];
            let quote = after.chars().next()?;
            if quote != '"' && quote != '\'' {
                continue;
            }
            let rest = &after[quote.len_utf8()..];
            let end = rest.find(quote)?;
            return Some(rest[..end].to_string());
        }
    }
    None
}

fn image_extension(url: &str) -> &'static str {
    if url.starts_with("generated:") {
        return "svg";
    }
    let clean = url.split('?').next().unwrap_or(url).to_ascii_lowercase();
    if clean.ends_with(".png") {
        "png"
    } else if clean.ends_with(".jpg") || clean.ends_with(".jpeg") {
        "jpg"
    } else {
        "webp"
    }
}

fn generated_asset_svg(entry: &AssetEntry) -> String {
    let initials: String = entry
        .name
        .split_whitespace()
        .filter_map(|part| part.chars().next())
        .take(2)
        .map(|c| c.to_ascii_uppercase())
        .collect();
    let text = if initials.is_empty() { "?" } else { &initials };
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256">
<defs>
  <radialGradient id="g" cx="50%" cy="38%" r="66%">
    <stop offset="0" stop-color="#183848"/>
    <stop offset="0.55" stop-color="#0b171d"/>
    <stop offset="1" stop-color="#030506"/>
  </radialGradient>
  <linearGradient id="gold" x1="0" x2="1">
    <stop offset="0" stop-color="#6f4614"/>
    <stop offset="0.5" stop-color="#ffd37a"/>
    <stop offset="1" stop-color="#6f4614"/>
  </linearGradient>
</defs>
<rect width="256" height="256" rx="22" fill="url(#g)"/>
<path d="M26 40h204v176H26z" fill="none" stroke="url(#gold)" stroke-width="5"/>
<path d="M46 62h164v132H46z" fill="none" stroke="#3b2a19" stroke-width="2"/>
<circle cx="128" cy="128" r="54" fill="#081015" stroke="#00c8ff" stroke-width="3" opacity="0.78"/>
<text x="128" y="144" text-anchor="middle" font-family="Georgia,serif" font-size="54" font-weight="700" fill="#ffd37a">{text}</text>
</svg>"##
    )
}

fn sanitize_path_segment(raw: &str) -> String {
    raw.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn asset_status_from_entries(entries: Vec<AssetEntry>, root: Option<PathBuf>) -> AssetStatus {
    let cached = entries.iter().filter(|e| e.status == "cached").count();
    let failed = entries.iter().filter(|e| e.status == "failed").count();
    AssetStatus {
        total: entries.len(),
        cached,
        failed,
        missing: entries.len().saturating_sub(cached + failed),
        root: root.map(|p| p.display().to_string()),
    }
}

fn now_iso8601() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    secs.to_string()
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
// Bases (Phase 9) — list base items the user can pick from.
// ---------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct BaseSummary {
    id: String,
    name: String,
    class_pascal: String,
    class_display: String,
    drop_level: u32,
    attribute_pool: String,
    tags: Vec<String>,
    release_state: String,
}

#[derive(Debug, Deserialize)]
struct BasesArgs {
    /// PascalCase class id like "BodyArmour". When `None`, returns every base.
    #[serde(default)]
    class_pascal: Option<String>,
    /// Include legacy/unreleased bases. Defaults to false.
    #[serde(default)]
    include_legacy: bool,
}

#[tauri::command]
fn list_bases(
    args: BasesArgs,
    state: tauri::State<'_, AdvisorState>,
) -> Result<Vec<BaseSummary>, String> {
    let bundle = state.bundle.read().expect("bundle rwlock poisoned");
    drop(bundle);
    // We don't actually need the bundle's mod registry here — we read the
    // raw bundle from disk for now via the bundle path.
    let path = state
        .bundle
        .read()
        .expect("bundle rwlock poisoned")
        .bundle_path
        .clone();
    let Some(path) = path else {
        return Ok(Vec::new());
    };
    let bundle: Bundle = poc2_data::io::read_bundle(&path).map_err(|e| e.to_string())?;

    let mut out = Vec::with_capacity(bundle.base_items.len());
    for base in &bundle.base_items {
        let display = base.item_class.as_str().to_string();
        let pascal = pascal_class(&display);
        if let Some(filter) = &args.class_pascal {
            if filter != &pascal {
                continue;
            }
        }
        if !args.include_legacy
            && !matches!(
                base.release_state,
                poc2_engine::base::ReleaseState::Released
            )
        {
            continue;
        }
        out.push(BaseSummary {
            id: base.id.as_str().to_string(),
            name: base.name.clone(),
            class_pascal: pascal,
            class_display: display,
            drop_level: base.drop_level,
            attribute_pool: format!("{:?}", base.attribute_pool).to_ascii_lowercase(),
            tags: base.tags.iter().map(|t| t.as_str().to_string()).collect(),
            release_state: format!("{:?}", base.release_state).to_ascii_lowercase(),
        });
    }
    out.sort_by(|a, b| a.drop_level.cmp(&b.drop_level).then(a.name.cmp(&b.name)));
    Ok(out)
}

fn pascal_class(raw: &str) -> String {
    let mut s = String::with_capacity(raw.len());
    let mut up = true;
    for c in raw.chars() {
        if c.is_whitespace() || c == '-' || c == '_' {
            up = true;
        } else if up {
            for u in c.to_uppercase() {
                s.push(u);
            }
            up = false;
        } else {
            s.push(c);
        }
    }
    s
}

// ---------------------------------------------------------------------
// Eligible mods (Phase 1)
//
// Enumerate, for the given (item, affix), every mod the bundle says could
// roll on this base + ilvl, plus any mods that are blocked only by a
// Greater/Perfect "min required level" floor (so the UI can grey them
// out with an explanation).
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum AffixSlotFilter {
    Prefix,
    Suffix,
    Either,
}

impl AffixSlotFilter {
    fn matches(self, ty: poc2_engine::item::AffixType) -> bool {
        use poc2_engine::item::AffixType;
        match self {
            Self::Prefix => matches!(ty, AffixType::Prefix),
            Self::Suffix => matches!(ty, AffixType::Suffix),
            Self::Either => matches!(ty, AffixType::Prefix | AffixType::Suffix),
        }
    }
}

#[derive(Debug, Deserialize)]
struct EligibleModsArgs {
    item: Item,
    #[serde(default = "default_affix_slot")]
    affix: AffixSlotFilter,
    /// `min_required_level` floor (e.g. Perfect Transmute = 70).
    /// Mods below this floor are returned but flagged blocked.
    #[serde(default)]
    min_required_level: u32,
}

const fn default_affix_slot() -> AffixSlotFilter {
    AffixSlotFilter::Either
}

#[derive(Debug, Serialize)]
struct EligibleModView {
    mod_id: String,
    name: Option<String>,
    mod_group: String,
    affix_type: String,
    kind: String,
    /// Concept ids this mod produces, e.g. ["EnergyShield"].
    concepts: Vec<String>,
    /// Tags (e.g. "boots", "movement").
    tags: Vec<String>,
    /// Tier index within the mod-group ladder (1 = highest required level).
    tier_index: u32,
    /// Total tiers for this mod-group on this base.
    tier_count: u32,
    required_level: u32,
    /// Eligible right now (passes class+ilvl+groups+patch+positive weight).
    eligible_now: bool,
    /// Blocked by `min_required_level` even though otherwise eligible.
    blocked_by_min_level: bool,
    /// Already present on the item (mod-group exclusivity).
    blocked_by_group: bool,
    /// Sum of spawn weights for tags relevant on this item.
    weight: u32,
    /// Probability share among the eligible-now set.
    weight_share: f64,
    text_template: Option<String>,
    /// Stat ranges `(stat_id, min, max)`, in mod's own order.
    stats: Vec<EligibleStatView>,
    is_hybrid: bool,
    is_essence_only: bool,
    is_desecrated_only: bool,
    is_local: bool,
}

#[derive(Debug, Serialize)]
struct EligibleStatView {
    stat_id: String,
    min: f64,
    max: f64,
}

#[derive(Debug, Serialize)]
struct EligibleModsResponse {
    /// Item class derived from the input item.
    item_class: String,
    /// Whether the bundle has any mods registered for this item-class+affix.
    /// `false` means the UI should show a "no_data_for_class" notice.
    data_available: bool,
    affix: String,
    /// Patch the registry was loaded for.
    patch: String,
    mods: Vec<EligibleModView>,
}

#[tauri::command]
fn eligible_mods(
    args: EligibleModsArgs,
    state: tauri::State<'_, AdvisorState>,
) -> Result<EligibleModsResponse, String> {
    use poc2_engine::ids::ItemClassId;
    use poc2_engine::item::AffixType;
    use poc2_engine::mods::{ModFlags, ModKind};

    let bundle = state.bundle.read().expect("bundle rwlock poisoned");
    let registry = bundle.registry.clone();
    let patch = bundle.bundle_patch.unwrap_or(PatchVersion::PATCH_0_4_0);
    drop(bundle);

    let item = &args.item;
    let class = ItemClassId::from(item.base.as_str());

    // Collect occupied groups already on the item (from any affix slot).
    let mut occupied_groups: std::collections::HashSet<String> = std::collections::HashSet::new();
    for m in item.prefixes.iter().chain(item.suffixes.iter()) {
        if let Some(g) = registry.group_of(&m.mod_id) {
            occupied_groups.insert(g.as_str().to_string());
        }
    }

    let affix_label = match args.affix {
        AffixSlotFilter::Prefix => "prefix",
        AffixSlotFilter::Suffix => "suffix",
        AffixSlotFilter::Either => "either",
    };

    // Build a candidate index: all mods for the class on the relevant affix.
    let mut indices: Vec<_> = Vec::new();
    if args.affix.matches(AffixType::Prefix) {
        indices.extend(
            registry
                .for_class_affix(&class, AffixType::Prefix)
                .iter()
                .copied(),
        );
    }
    if args.affix.matches(AffixType::Suffix) {
        indices.extend(
            registry
                .for_class_affix(&class, AffixType::Suffix)
                .iter()
                .copied(),
        );
    }

    if indices.is_empty() {
        return Ok(EligibleModsResponse {
            item_class: class.as_str().to_string(),
            data_available: false,
            affix: affix_label.to_string(),
            patch: format!("{patch}"),
            mods: Vec::new(),
        });
    }

    // Group counts for tier_index/tier_count assignment. Tier 1 = highest
    // required_level within group.
    let mut group_levels: std::collections::HashMap<String, Vec<u32>> =
        std::collections::HashMap::new();
    for &idx in &indices {
        let Some(m) = registry.at(idx) else { continue };
        if m.kind != ModKind::Explicit {
            continue;
        }
        if !m.patch_range.contains(patch) {
            continue;
        }
        let total: u32 = m.spawn_weights.iter().map(|sw| sw.weight).sum();
        if total == 0 {
            continue;
        }
        group_levels
            .entry(m.mod_group.0.as_str().to_string())
            .or_default()
            .push(m.required_level);
    }
    for v in group_levels.values_mut() {
        v.sort_unstable_by(|a, b| b.cmp(a)); // descending: highest required_level first = T1
        v.dedup();
    }

    // First pass: build raw list and remember the eligible-now subset's total weight.
    let mut raw: Vec<EligibleModView> = Vec::new();
    let mut eligible_total_weight: u64 = 0;

    for idx in indices {
        let Some(m) = registry.at(idx) else { continue };
        if m.kind != ModKind::Explicit {
            continue;
        }
        if !m.patch_range.contains(patch) {
            continue;
        }
        let group_id = m.mod_group.0.as_str().to_string();
        let weight: u32 = m.spawn_weights.iter().map(|sw| sw.weight).sum();
        if weight == 0 {
            continue;
        }
        let blocked_by_group = occupied_groups.contains(&group_id);
        let blocked_by_min = m.required_level < args.min_required_level;
        let blocked_by_ilvl = m.required_level > item.ilvl;
        let eligible_now = !blocked_by_group && !blocked_by_min && !blocked_by_ilvl;

        if eligible_now {
            eligible_total_weight = eligible_total_weight.saturating_add(u64::from(weight));
        }

        let levels = group_levels.get(&group_id);
        let tier_count = levels.map(|v| v.len() as u32).unwrap_or(1);
        let tier_index = levels
            .and_then(|v| v.iter().position(|l| *l == m.required_level))
            .map(|p| (p + 1) as u32)
            .unwrap_or(1);

        raw.push(EligibleModView {
            mod_id: m.id.as_str().to_string(),
            name: m.name.clone(),
            mod_group: group_id,
            affix_type: match m.affix_type {
                AffixType::Prefix => "prefix".into(),
                AffixType::Suffix => "suffix".into(),
                AffixType::Implicit => "implicit".into(),
                AffixType::Enchantment => "enchantment".into(),
            },
            kind: format!("{:?}", m.kind).to_ascii_lowercase(),
            concepts: m
                .concept_set
                .iter()
                .map(|c| c.as_str().to_string())
                .collect(),
            tags: m.tags.iter().map(|t| t.as_str().to_string()).collect(),
            tier_index,
            tier_count,
            required_level: m.required_level,
            eligible_now,
            blocked_by_min_level: blocked_by_min && !blocked_by_ilvl && !blocked_by_group,
            blocked_by_group,
            weight,
            weight_share: 0.0,
            text_template: m.text_template.clone(),
            stats: m
                .stats
                .iter()
                .map(|s| EligibleStatView {
                    stat_id: s.stat_id.as_str().to_string(),
                    min: s.min,
                    max: s.max,
                })
                .collect(),
            is_hybrid: m.flags.contains(ModFlags::HYBRID),
            is_essence_only: m.flags.contains(ModFlags::ESSENCE_ONLY),
            is_desecrated_only: m.flags.contains(ModFlags::DESECRATED_ONLY),
            is_local: m.flags.contains(ModFlags::LOCAL),
        });
    }

    if eligible_total_weight > 0 {
        for view in &mut raw {
            if view.eligible_now {
                view.weight_share = view.weight as f64 / eligible_total_weight as f64;
            }
        }
    }

    // Sort: eligible first, then by tier_index asc (T1 first), then weight desc.
    raw.sort_by(|a, b| {
        b.eligible_now
            .cmp(&a.eligible_now)
            .then(a.tier_index.cmp(&b.tier_index))
            .then(b.weight.cmp(&a.weight))
            .then(a.mod_id.cmp(&b.mod_id))
    });

    Ok(EligibleModsResponse {
        item_class: class.as_str().to_string(),
        data_available: true,
        affix: affix_label.to_string(),
        patch: format!("{patch}"),
        mods: raw,
    })
}

// ---------------------------------------------------------------------
// check_can_apply (v2 plan, Phase A.2 IPC surface)
//
// Returns the engine's structured CannotApply reason for an
// `(item, currency)` pair. Used by the OutcomeDialog and AdvisorPanel
// to show authoritative "cannot apply" messages without client-side
// rarity/slot reasoning that could drift from the engine's verdict.
//
// Returns `None` (variant kind = "ok") when the action is applicable.
// ---------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct CheckCanApplyArgs {
    item: Item,
    currency: String,
}

/// Mirror of [`poc2_engine::CannotApply`] for serde-stable IPC. Each
/// variant carries the data the UI needs to render a friendly message;
/// the leading `kind` tag matches the discriminator on the TS side.
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum CannotApplyView {
    /// Action is applicable — no obstacle.
    Ok,
    /// Currency rejected because it doesn't accept the item's rarity.
    WrongRarity {
        item_rarity: String,
        expected: Vec<String>,
    },
    /// All affix slots of the relevant kind are full.
    NoOpenSlots { affix: String },
    /// Item is corrupted and the currency can't apply.
    Corrupted,
    /// Item is mirrored and cannot be modified.
    Mirrored,
    /// Hinekora's Lock is already active.
    AlreadyLocked,
    /// Fracture refused — item has fewer than 4 visible mods.
    FractureRequiresFourMods { current: u32 },
    /// Recombinator inputs don't share base / ilvl.
    RecombinatorInputMismatch,
    /// Free-form fallback for variants the v2 IPC hasn't enumerated yet.
    Other { message: String },
    /// Currency id wasn't in the engine's resolver.
    UnknownCurrency,
}

fn rarity_label(r: poc2_engine::Rarity) -> &'static str {
    match r {
        poc2_engine::Rarity::Normal => "normal",
        poc2_engine::Rarity::Magic => "magic",
        poc2_engine::Rarity::Rare => "rare",
        poc2_engine::Rarity::Unique => "unique",
    }
}

fn cannot_apply_to_view(reason: poc2_engine::CannotApply) -> CannotApplyView {
    use poc2_engine::CannotApply;
    match reason {
        CannotApply::WrongRarity {
            item_rarity,
            expected,
        } => CannotApplyView::WrongRarity {
            item_rarity: rarity_label(item_rarity).to_string(),
            expected: expected
                .iter()
                .map(|r| rarity_label(r).to_string())
                .collect(),
        },
        CannotApply::NoOpenSlots { affix } => CannotApplyView::NoOpenSlots {
            affix: format!("{affix:?}").to_lowercase(),
        },
        CannotApply::Corrupted => CannotApplyView::Corrupted,
        CannotApply::Mirrored => CannotApplyView::Mirrored,
        CannotApply::AlreadyLocked => CannotApplyView::AlreadyLocked,
        CannotApply::FractureRequiresFourMods { current } => {
            CannotApplyView::FractureRequiresFourMods {
                #[allow(clippy::cast_possible_truncation)]
                current: current as u32,
            }
        }
        CannotApply::RecombinatorInputMismatch => CannotApplyView::RecombinatorInputMismatch,
        CannotApply::Other(s) => CannotApplyView::Other {
            message: s.to_string(),
        },
    }
}

#[tauri::command]
fn check_can_apply(
    args: CheckCanApplyArgs,
    state: tauri::State<'_, AdvisorState>,
) -> Result<CannotApplyView, String> {
    use poc2_engine::CurrencyResolver as _;
    let bundle = state.bundle.read().expect("bundle rwlock poisoned");
    let resolver = bundle.resolver.clone();
    drop(bundle);
    let id = poc2_engine::ids::CurrencyId::from(args.currency.as_str());
    let Some(currency) = resolver.resolve(&id) else {
        return Ok(CannotApplyView::UnknownCurrency);
    };
    Ok(match currency.can_apply_to(&args.item) {
        Ok(()) => CannotApplyView::Ok,
        Err(reason) => cannot_apply_to_view(reason),
    })
}

// ---------------------------------------------------------------------
// Record outcome (Phase 2)
//
// Apply a user-chosen mod outcome to the in-memory item. This is how the
// UI integrates "I just used Perfect Transmute and rolled X" into the
// session's item state without going through random sampling.
// ---------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct RecordOutcomeArgs {
    item: Item,
    outcome: OutcomeKind,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum OutcomeKind {
    /// Add a mod that the user picked from the eligible-mods list.
    AddMod {
        mod_id: String,
        /// 0..=1 normalized roll along the mod's stat range. None = midpoint.
        #[serde(default)]
        roll: Option<f64>,
        /// Currency that produced this mod (informational, used for rarity
        /// transitions like Normal→Magic on Transmute).
        #[serde(default)]
        currency: Option<String>,
    },
    /// Remove a mod by (affix, index) — used for Annul/Chaos.
    RemoveMod { affix: String, index: usize },
    /// Replace a mod (Chaos): remove `(affix, index)` then add `mod_id`.
    ReplaceMod {
        remove_affix: String,
        remove_index: usize,
        add_mod_id: String,
        #[serde(default)]
        roll: Option<f64>,
    },
    /// Manual rarity bump (no mod change). Used when the engine doesn't
    /// know what to roll for the currency yet.
    SetRarity { rarity: String },
}

#[derive(Debug, Serialize)]
struct RecordOutcomeResponse {
    item: Item,
    change: String,
    explanation: String,
}

#[tauri::command]
fn record_outcome(
    args: RecordOutcomeArgs,
    state: tauri::State<'_, AdvisorState>,
) -> Result<RecordOutcomeResponse, String> {
    use poc2_engine::ids::ModId;
    use poc2_engine::item::{AffixType, ModRoll};

    let bundle = state.bundle.read().expect("bundle rwlock poisoned");
    let registry = bundle.registry.clone();
    drop(bundle);

    let mut item = args.item;

    match args.outcome {
        OutcomeKind::AddMod {
            mod_id,
            roll,
            currency,
        } => {
            let mid = ModId::from(mod_id.clone());
            let def = registry
                .get(&mid)
                .ok_or_else(|| format!("unknown mod id: {mod_id}"))?;
            // Validate ilvl + class.
            if def.required_level > item.ilvl {
                return Err(format!(
                    "mod {mod_id} requires ilvl {} but item has ilvl {}",
                    def.required_level, item.ilvl
                ));
            }
            // Mod-group exclusivity.
            for m in item.prefixes.iter().chain(item.suffixes.iter()) {
                if let Some(g) = registry.group_of(&m.mod_id) {
                    if g.as_str() == def.mod_group.0.as_str() {
                        return Err(format!(
                            "mod-group {} already occupied by {}",
                            def.mod_group.0.as_str(),
                            m.mod_id
                        ));
                    }
                }
            }
            // Slot capacity (assume 3/3).
            match def.affix_type {
                AffixType::Prefix if item.prefixes.len() >= 3 => {
                    return Err("no open prefix slots".into());
                }
                AffixType::Suffix if item.suffixes.len() >= 3 => {
                    return Err("no open suffix slots".into());
                }
                _ => {}
            }
            let t = roll.unwrap_or(0.5).clamp(0.0, 1.0);
            let values = def.stats.iter().map(|s| s.roll(t)).collect();
            let roll = ModRoll {
                mod_id: mid,
                affix_type: def.affix_type,
                kind: def.kind,
                values,
                is_fractured: false,
            };
            match def.affix_type {
                AffixType::Prefix => item.prefixes.push(roll),
                AffixType::Suffix => item.suffixes.push(roll),
                _ => return Err("only prefix/suffix outcomes supported here".into()),
            }
            // Bump rarity for transmute-like flows.
            if let Some(c) = currency.as_deref() {
                let target = match c {
                    "OrbOfTransmutation"
                    | "GreaterOrbOfTransmutation"
                    | "PerfectOrbOfTransmutation" => Some("magic"),
                    "RegalOrb" | "GreaterRegalOrb" | "PerfectRegalOrb" => Some("rare"),
                    "ExaltedOrb" | "GreaterExaltedOrb" | "PerfectExaltedOrb" => Some("rare"),
                    "ChaosOrb" | "GreaterChaosOrb" | "PerfectChaosOrb" => Some("rare"),
                    _ => None,
                };
                if let Some(want) = target {
                    let want_rarity: poc2_engine::item::Rarity =
                        serde_json::from_value(serde_json::json!(want))
                            .map_err(|e| e.to_string())?;
                    use poc2_engine::item::Rarity::*;
                    let cur = item.rarity;
                    let upgrade =
                        matches!((cur, want_rarity), (Normal, Magic | Rare) | (Magic, Rare));
                    if upgrade {
                        item.rarity = want_rarity;
                    }
                }
            }
            Ok(RecordOutcomeResponse {
                item,
                change: "added".into(),
                explanation: format!("added {mod_id}"),
            })
        }
        OutcomeKind::RemoveMod { affix, index } => {
            let removed_id = remove_outcome_slot(&mut item, &affix, index)?;
            Ok(RecordOutcomeResponse {
                item,
                change: "removed".into(),
                explanation: format!("removed {removed_id}"),
            })
        }
        OutcomeKind::ReplaceMod {
            remove_affix,
            remove_index,
            add_mod_id,
            roll,
        } => {
            let removed_id = remove_outcome_slot(&mut item, &remove_affix, remove_index)?;
            let mid = ModId::from(add_mod_id.clone());
            let def = registry
                .get(&mid)
                .ok_or_else(|| format!("unknown mod id: {add_mod_id}"))?;
            let t = roll.unwrap_or(0.5).clamp(0.0, 1.0);
            let values = def.stats.iter().map(|s| s.roll(t)).collect();
            let new_roll = ModRoll {
                mod_id: mid,
                affix_type: def.affix_type,
                kind: def.kind,
                values,
                is_fractured: false,
            };
            match def.affix_type {
                AffixType::Prefix => item.prefixes.push(new_roll),
                AffixType::Suffix => item.suffixes.push(new_roll),
                _ => return Err("only prefix/suffix replacement supported".into()),
            }
            Ok(RecordOutcomeResponse {
                item,
                change: "replaced".into(),
                explanation: format!("replaced {removed_id} with {add_mod_id}"),
            })
        }
        OutcomeKind::SetRarity { rarity } => {
            let r: poc2_engine::item::Rarity = serde_json::from_value(serde_json::json!(rarity))
                .map_err(|e| format!("invalid rarity {rarity}: {e}"))?;
            item.rarity = r;
            Ok(RecordOutcomeResponse {
                item,
                change: "rarity".into(),
                explanation: format!("set rarity to {rarity}"),
            })
        }
    }
}

fn remove_outcome_slot(item: &mut Item, affix: &str, index: usize) -> Result<String, String> {
    use poc2_engine::item::AffixType;
    let af: AffixType = match affix {
        "prefix" => AffixType::Prefix,
        "suffix" => AffixType::Suffix,
        other => return Err(format!("invalid affix: {other}")),
    };
    let removed = match af {
        AffixType::Prefix => {
            if index >= item.prefixes.len() {
                return Err("prefix index out of range".into());
            }
            item.prefixes.remove(index)
        }
        AffixType::Suffix => {
            if index >= item.suffixes.len() {
                return Err("suffix index out of range".into());
            }
            item.suffixes.remove(index)
        }
        _ => return Err("only prefix/suffix removal supported".into()),
    };
    Ok(removed.mod_id.as_str().to_string())
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
            bundle_migration_status,
            asset_manifest,
            asset_status,
            cache_all_assets,
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
            eligible_mods,
            check_can_apply,
            record_outcome,
            list_bases,
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
