//! # poc2-plugin-host
//!
//! Wasmtime-backed plugin host (Phase F of the v1 execution plan).
//!
//! Loads `*.wasm` modules from `~/.config/poc2/plugins/<plugin-id>/`
//! according to a `poc2-plugin.toml` manifest, sandboxes execution
//! per the security model in [ADR-0008
//! v2](../../../docs/adr/0008-plugin-system-deferred.md), and
//! dispatches three flavours of host call:
//!
//! 1. **Custom predicates**: `eval_predicate(name, item_json, args_json) -> u32`
//!    (returns 0 = false, 1 = true). Wired into
//!    [`poc2_strategies::ItemPredicate::Custom`] (Phase F.3).
//! 2. **Strategy / rule emission**: `list_strategies()` / `list_rules()`
//!    return TOML strings the host parses + adds to the registry at
//!    plugin-load time.
//! 3. **Recommendation emission**: `emit_recommendations(state_json)`
//!    returns a JSON array of plugin candidates the advisor's
//!    candidate generator merges with rules + strategies (Phase F.4).
//!
//! ## ABI shape (v1 raw-Wasm)
//!
//! Plugins export functions returning a `(ptr, len)` pair pointing at
//! UTF-8-encoded JSON buffers in their linear memory; the host
//! reads them through [`wasmtime::Memory`]. The Component Model is
//! a v1.x upgrade — see ADR-0008 v2 future-work.

#![forbid(unsafe_code)]
#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_sign_loss)]

pub mod cache;
pub mod candidate;
pub mod manifest;
pub mod predicate;

use std::path::{Path, PathBuf};

use ahash::AHashMap;
use thiserror::Error;
use tracing::{debug, info, warn};
use wasmtime::{Config, Engine, Linker, Memory, Module, Store};

pub use cache::PredicateCache;
pub use candidate::{PluginCandidate, PluginCandidateAction};
pub use manifest::{Capability, PluginManifest};

/// Implement [`poc2_strategies::PluginPredicateDispatch`] so the
/// `PredicateContext::with_plugin_dispatch(&host)` API works
/// directly with the host.
impl poc2_strategies::PluginPredicateDispatch for PluginHost {
    fn dispatch(
        &self,
        plugin_id: &str,
        name: &str,
        item: &poc2_engine::item::Item,
        args: &serde_json::Value,
    ) -> Result<bool, String> {
        match self.eval_predicate(plugin_id, name, item, args) {
            Ok(outcome) => Ok(outcome.result),
            Err(e) => Err(e.to_string()),
        }
    }
}

/// One loaded plugin, ready for dispatch.
pub struct LoadedPlugin {
    pub manifest: PluginManifest,
    pub manifest_path: PathBuf,
    /// Compiled Wasm module (cheap to instantiate per-call thanks to
    /// wasmtime's compilation cache).
    pub module: Module,
    /// Strategies the plugin emits — TOML source strings that the
    /// host parsed at load time.
    pub strategies: Vec<poc2_strategies::Strategy>,
    /// Rules the plugin emits — same shape as strategies.
    pub rules: Vec<poc2_rules::Rule>,
    /// Whether the plugin is currently enabled. Disabled plugins are
    /// kept loaded so the user can toggle them via the UI without a
    /// reload, but skipped during dispatch.
    pub enabled: bool,
    /// Per-plugin runtime statistics (timeouts, fuel exhausted, etc.).
    pub stats: PluginStats,
}

/// Runtime statistics surfaced to the Plugin Manager UI.
#[derive(Debug, Clone, Default)]
pub struct PluginStats {
    pub total_calls: u64,
    pub timeouts: u64,
    pub fuel_exhausted: u64,
    pub last_call_micros: u64,
}

/// Thread-safe handle to the plugin runtime.
pub struct PluginHost {
    /// Wasmtime engine; shared across all plugin stores.
    engine: Engine,
    /// `plugin_id → LoadedPlugin`.
    plugins: AHashMap<String, LoadedPlugin>,
    /// Custom-predicate eval cache (per (item_canonical, plugin, name, args) key).
    cache: PredicateCache,
}

impl PluginHost {
    /// Build a host with a sandboxed wasmtime engine + an empty plugin
    /// catalogue. Subsequent [`PluginHost::discover_plugins`] calls
    /// add plugins from a directory walk.
    pub fn new() -> Result<Self, PluginError> {
        let mut config = Config::new();
        config.consume_fuel(true);
        config.epoch_interruption(true);
        // Memory cap: 64 MiB per store (the per-plugin memory limit
        // happens at the Store level via ResourceLimiter).
        let engine = Engine::new(&config).map_err(PluginError::Engine)?;
        Ok(Self {
            engine,
            plugins: AHashMap::new(),
            cache: PredicateCache::new(4096),
        })
    }

    /// Discover plugins under `dir`. Each plugin lives in its own
    /// subdirectory containing a `poc2-plugin.toml` manifest + a
    /// `*.wasm` file the manifest references.
    ///
    /// Bad plugins are warned-and-skipped; the host stays usable.
    pub fn discover_plugins(&mut self, dir: &Path) -> Result<usize, PluginError> {
        if !dir.exists() {
            debug!(path = %dir.display(), "plugin dir not found; skipping");
            return Ok(0);
        }
        let entries = std::fs::read_dir(dir).map_err(PluginError::Io)?;
        let mut loaded = 0usize;
        for entry in entries.flatten() {
            let plugin_dir = entry.path();
            if !plugin_dir.is_dir() {
                continue;
            }
            let manifest_path = plugin_dir.join("poc2-plugin.toml");
            if !manifest_path.exists() {
                continue;
            }
            match self.load_plugin(&manifest_path) {
                Ok(plugin) => {
                    info!(
                        id = %plugin.manifest.id,
                        version = %plugin.manifest.version,
                        capabilities = ?plugin.manifest.capabilities,
                        "loaded plugin"
                    );
                    self.plugins.insert(plugin.manifest.id.clone(), plugin);
                    loaded += 1;
                }
                Err(e) => {
                    warn!(path = %manifest_path.display(), error = %e, "plugin failed to load");
                }
            }
        }
        Ok(loaded)
    }

    /// Load a single plugin from its manifest path.
    pub fn load_plugin(&self, manifest_path: &Path) -> Result<LoadedPlugin, PluginError> {
        let manifest = manifest::load_manifest(manifest_path)?;
        let plugin_dir = manifest_path
            .parent()
            .ok_or_else(|| PluginError::Manifest("manifest has no parent dir".into()))?;
        let wasm_path = plugin_dir.join(&manifest.wasm_file);
        if !wasm_path.exists() {
            return Err(PluginError::Manifest(format!(
                "wasm file not found: {}",
                wasm_path.display()
            )));
        }
        let module = Module::from_file(&self.engine, &wasm_path).map_err(PluginError::Module)?;

        // Probe the plugin for emitted strategies + rules at load
        // time — these are static metadata, not per-frame calls.
        let mut strategies = Vec::new();
        let mut rules = Vec::new();
        if manifest.capabilities.contains(&Capability::EmitStrategies) {
            match self.probe_emit_strategies(&module) {
                Ok(toml_strs) => {
                    for s in toml_strs {
                        match poc2_strategies::load_strategy_str(&s) {
                            Ok(strategy) => strategies.push(strategy),
                            Err(e) => {
                                warn!(plugin = %manifest.id, error = %e, "plugin strategy failed to parse");
                            }
                        }
                    }
                }
                Err(e) => {
                    warn!(plugin = %manifest.id, error = %e, "list_strategies probe failed");
                }
            }
        }
        if manifest.capabilities.contains(&Capability::EmitRules) {
            match self.probe_emit_rules(&module) {
                Ok(toml_strs) => {
                    for s in toml_strs {
                        match poc2_rules::load_rules_str(&s) {
                            Ok(rs) => rules.extend(rs),
                            Err(e) => {
                                warn!(plugin = %manifest.id, error = %e, "plugin rules failed to parse");
                            }
                        }
                    }
                }
                Err(e) => {
                    warn!(plugin = %manifest.id, error = %e, "list_rules probe failed");
                }
            }
        }

        Ok(LoadedPlugin {
            manifest,
            manifest_path: manifest_path.to_path_buf(),
            module,
            strategies,
            rules,
            enabled: true,
            stats: PluginStats::default(),
        })
    }

    /// Iterator over loaded plugins.
    pub fn plugins(&self) -> impl Iterator<Item = &LoadedPlugin> {
        self.plugins.values()
    }

    /// Number of loaded plugins.
    pub fn plugin_count(&self) -> usize {
        self.plugins.len()
    }

    /// Probe `list_strategies()` from a wasm module (load-time).
    fn probe_emit_strategies(&self, module: &Module) -> Result<Vec<String>, PluginError> {
        self.read_string_array_export(module, "list_strategies")
    }

    /// Probe `list_rules()` from a wasm module (load-time).
    fn probe_emit_rules(&self, module: &Module) -> Result<Vec<String>, PluginError> {
        self.read_string_array_export(module, "list_rules")
    }

    /// Generic helper: invoke `<export_name>() -> (ptr, len)` returning
    /// a JSON-encoded `Vec<String>`.
    fn read_string_array_export(
        &self,
        module: &Module,
        export_name: &str,
    ) -> Result<Vec<String>, PluginError> {
        let mut store = Store::new(&self.engine, ());
        store
            .set_fuel(10_000_000)
            .map_err(|_| PluginError::FuelExhausted)?;
        let linker: Linker<()> = Linker::new(&self.engine);
        let instance = linker
            .instantiate(&mut store, module)
            .map_err(PluginError::Module)?;
        let Ok(export) = instance.get_typed_func::<(), (i32, i32)>(&mut store, export_name) else {
            return Ok(Vec::new());
        };
        let (ptr, len) = export
            .call(&mut store, ())
            .map_err(|e| PluginError::Trap(e.to_string()))?;
        let memory = instance
            .get_memory(&mut store, "memory")
            .ok_or(PluginError::MissingMemory)?;
        let bytes = read_memory(&memory, &mut store, ptr, len)?;
        let json: Vec<String> =
            serde_json::from_slice(&bytes).map_err(PluginError::DeserializeOutput)?;
        Ok(json)
    }

    /// Get the predicate cache for use by [`crate::predicate`].
    pub fn cache(&self) -> &PredicateCache {
        &self.cache
    }

    /// Mutable cache accessor — used by the dispatcher to insert.
    pub fn cache_mut(&mut self) -> &mut PredicateCache {
        &mut self.cache
    }

    /// Wasmtime engine handle (for dispatchers spawning their own stores).
    pub fn engine(&self) -> &Engine {
        &self.engine
    }
}

/// Read `len` bytes starting at `ptr` from `memory`.
pub fn read_memory(
    memory: &Memory,
    store: &mut Store<()>,
    ptr: i32,
    len: i32,
) -> Result<Vec<u8>, PluginError> {
    if ptr < 0 || len < 0 {
        return Err(PluginError::InvalidPointer);
    }
    let ptr_usize = ptr as usize;
    let len_usize = len as usize;
    let data = memory.data(store);
    let end = ptr_usize
        .checked_add(len_usize)
        .ok_or(PluginError::InvalidPointer)?;
    if end > data.len() {
        return Err(PluginError::InvalidPointer);
    }
    Ok(data[ptr_usize..end].to_vec())
}

#[derive(Debug, Error)]
pub enum PluginError {
    #[error("manifest error: {0}")]
    Manifest(String),
    #[error("toml manifest parse: {0}")]
    ManifestToml(#[from] toml::de::Error),
    #[error("io: {0}")]
    Io(std::io::Error),
    #[error("wasm engine: {0}")]
    Engine(anyhow::Error),
    #[error("wasm module: {0}")]
    Module(anyhow::Error),
    #[error("wasm trap: {0}")]
    Trap(String),
    #[error("plugin missing 'memory' export")]
    MissingMemory,
    #[error("plugin returned invalid memory pointer")]
    InvalidPointer,
    #[error("plugin output failed JSON deserialization: {0}")]
    DeserializeOutput(serde_json::Error),
    #[error("plugin call timed out")]
    Timeout,
    #[error("plugin call exhausted fuel budget")]
    FuelExhausted,
    #[error("missing required capability: {0:?}")]
    MissingCapability(Capability),
}
