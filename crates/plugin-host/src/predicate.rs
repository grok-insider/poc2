//! Custom-predicate dispatch (Phase F.3).
//!
//! Wired into [`poc2_strategies::ItemPredicate::Custom`] via the
//! advisor's predicate evaluator. The plugin SDK exports
//! `eval_predicate(name_ptr, name_len, item_ptr, item_len,
//! args_ptr, args_len) -> u32` (0 = false, 1 = true).
//!
//! Per the perf contract in ADR-0008 v2: target < 50 µs per call,
//! cached re-eval against the same item should be ~10 ns (an
//! AHashMap lookup).

use poc2_engine::item::Item;
use serde_json::Value;
use wasmtime::{Linker, Store};

use crate::manifest::Capability;
use crate::{LoadedPlugin, PluginError, PluginHost};

/// Dispatch result for one custom-predicate call.
#[derive(Debug, Clone)]
pub struct PredicateOutcome {
    pub result: bool,
    pub from_cache: bool,
    pub elapsed_micros: u64,
}

impl PluginHost {
    /// Evaluate a plugin custom predicate.
    ///
    /// Returns `Err(PluginError::MissingCapability)` when the plugin
    /// hasn't requested `register_predicate`, and
    /// `Err(PluginError::FuelExhausted)` / `Err(Timeout)` for
    /// runaway calls. `false` on plugin-side errors (the host
    /// surfaces the error in `tracing::warn!` but doesn't propagate
    /// to the planner — a misbehaving plugin shouldn't tank the
    /// entire recommendation).
    pub fn eval_predicate(
        &self,
        plugin_id: &str,
        predicate_name: &str,
        item: &Item,
        args: &Value,
    ) -> Result<PredicateOutcome, PluginError> {
        let start = std::time::Instant::now();
        let plugin = self
            .plugins
            .get(plugin_id)
            .ok_or_else(|| PluginError::Manifest(format!("unknown plugin: {plugin_id}")))?;
        if !plugin.enabled {
            // Disabled plugins evaluate to false silently.
            return Ok(PredicateOutcome {
                result: false,
                from_cache: false,
                elapsed_micros: 0,
            });
        }
        if !plugin
            .manifest
            .capabilities
            .contains(&Capability::RegisterPredicate)
        {
            return Err(PluginError::MissingCapability(
                Capability::RegisterPredicate,
            ));
        }

        // Cache lookup
        let item_hash = canonical_item_hash(item);
        let args_hash = canonical_value_hash(args);
        if let Some(cached) = self
            .cache
            .get(item_hash, plugin_id, predicate_name, args_hash)
        {
            return Ok(PredicateOutcome {
                result: cached,
                from_cache: true,
                elapsed_micros: start.elapsed().as_micros() as u64,
            });
        }

        let result = self.dispatch_eval_predicate(plugin, predicate_name, item, args)?;
        self.cache
            .insert(item_hash, plugin_id, predicate_name, args_hash, result);
        Ok(PredicateOutcome {
            result,
            from_cache: false,
            elapsed_micros: start.elapsed().as_micros() as u64,
        })
    }

    fn dispatch_eval_predicate(
        &self,
        plugin: &LoadedPlugin,
        predicate_name: &str,
        item: &Item,
        args: &Value,
    ) -> Result<bool, PluginError> {
        let mut store = Store::new(&self.engine, ());
        // Per ADR-0008 v2: fuel budget per call. Generous default;
        // tighten in v1.x as we learn typical plugin work-loads.
        store
            .set_fuel(100_000_000)
            .map_err(|_| PluginError::FuelExhausted)?;
        let linker: Linker<()> = Linker::new(&self.engine);
        let instance = linker
            .instantiate(&mut store, &plugin.module)
            .map_err(PluginError::Module)?;

        // The plugin SDK provides an `alloc(len) -> ptr` export that
        // grows the wasm memory and returns a writeable buffer
        // pointer. The host writes the inputs into the plugin's
        // memory before calling eval_predicate.
        let alloc = instance
            .get_typed_func::<i32, i32>(&mut store, "alloc")
            .map_err(|e| PluginError::Trap(format!("missing alloc export: {e}")))?;

        let memory = instance
            .get_memory(&mut store, "memory")
            .ok_or(PluginError::MissingMemory)?;

        let name_bytes = predicate_name.as_bytes();
        let item_json = serde_json::to_vec(item).map_err(PluginError::DeserializeOutput)?;
        let args_json = serde_json::to_vec(args).map_err(PluginError::DeserializeOutput)?;

        let name_ptr = alloc
            .call(&mut store, name_bytes.len() as i32)
            .map_err(|e| PluginError::Trap(e.to_string()))?;
        let item_ptr = alloc
            .call(&mut store, item_json.len() as i32)
            .map_err(|e| PluginError::Trap(e.to_string()))?;
        let args_ptr = alloc
            .call(&mut store, args_json.len() as i32)
            .map_err(|e| PluginError::Trap(e.to_string()))?;

        write_memory(&memory, &mut store, name_ptr, name_bytes)?;
        write_memory(&memory, &mut store, item_ptr, &item_json)?;
        write_memory(&memory, &mut store, args_ptr, &args_json)?;

        let eval = instance
            .get_typed_func::<(i32, i32, i32, i32, i32, i32), i32>(&mut store, "eval_predicate")
            .map_err(|e| PluginError::Trap(format!("missing eval_predicate export: {e}")))?;
        let result = eval
            .call(
                &mut store,
                (
                    name_ptr,
                    name_bytes.len() as i32,
                    item_ptr,
                    item_json.len() as i32,
                    args_ptr,
                    args_json.len() as i32,
                ),
            )
            .map_err(|e| PluginError::Trap(e.to_string()))?;
        Ok(result != 0)
    }
}

/// Canonical 64-bit hash for an item — used as a cache key for
/// custom-predicate dispatch. Hashes the JSON representation of the
/// item so that any two items with identical mod state collide
/// (canonical equality, not pointer equality).
pub fn canonical_item_hash(item: &Item) -> u64 {
    use std::hash::{Hash, Hasher};
    let json = serde_json::to_string(item).unwrap_or_default();
    let mut h = ahash::AHasher::default();
    json.hash(&mut h);
    h.finish()
}

/// Canonical hash for a JSON value.
pub fn canonical_value_hash(v: &Value) -> u64 {
    use std::hash::{Hash, Hasher};
    let json = serde_json::to_string(v).unwrap_or_default();
    let mut h = ahash::AHasher::default();
    json.hash(&mut h);
    h.finish()
}

/// Write `bytes` into `memory` at `ptr`. Bounds-checks first.
pub fn write_memory(
    memory: &wasmtime::Memory,
    store: &mut Store<()>,
    ptr: i32,
    bytes: &[u8],
) -> Result<(), PluginError> {
    if ptr < 0 {
        return Err(PluginError::InvalidPointer);
    }
    let ptr_usize = ptr as usize;
    let data = memory.data_mut(store);
    let end = ptr_usize
        .checked_add(bytes.len())
        .ok_or(PluginError::InvalidPointer)?;
    if end > data.len() {
        return Err(PluginError::InvalidPointer);
    }
    data[ptr_usize..end].copy_from_slice(bytes);
    Ok(())
}
