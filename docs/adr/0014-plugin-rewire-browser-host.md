# ADR-0014 — Plugin re-wire: browser-side JS host, phased

- Status: **Accepted** (2026-07-03)
- Relates to: [ADR-0008](0008-plugin-system-deferred.md) (the SDK/host
  crates and their capability/perf contracts — unchanged),
  [ADR-0010](0010-desktop-shell-electron-cross-platform.md) (process
  model the host must fit).

## Context

The Wasm plugin SDK (`crates/plugin-sdk`) and the wasmtime host
(`crates/plugin-host`) shipped in v1 and are tested, but the in-process
host was tied to the retired Tauri shell. The current app plans inside a
**Web Worker running the engine as WASM** (browser and Electron alike),
and has been passing `plugin_dispatch: None` ever since.

Where can plugins actually execute now?

1. **wasmtime inside the engine** — impossible: wasmtime does not build
   for `wasm32-unknown-unknown`.
2. **Native host in the Electron main process** — rejected. The planner
   is synchronous inside the worker; a predicate evaluation cannot block
   on an async IPC round-trip to main. It would also make plugins a
   desktop-only feature for no technical reason.
3. **Browser-side JS host** — the renderer (or worker) can instantiate
   plugin `.wasm` modules directly via `WebAssembly.instantiate`. The
   SDK's raw `(ptr, len)` ABI (ADR-0008 v2) needs no host imports for
   its emission exports, and synchronous calls into a JS-instantiated
   module are exactly what the sync planner needs for predicates.

## Decision

Re-wire plugins with a **browser-side JS host**, in two phases:

### Phase 1 — content emission (this change)

- Plugin `.wasm` files are stored by the user from **Settings → Plugins**
  and persisted in IndexedDB (the browser equivalent of
  `~/.config/poc2/plugins/`).
- At boot (and on plugin add/remove) the web app instantiates each
  module on the main thread — **no imports are provided**, so a plugin
  cannot reach network/filesystem/DOM — and reads the SDK's emission
  exports: `list_strategies()` / `list_rules()` (packed `i64` →
  `(ptr, len)` → UTF-8 JSON `Vec<String>` of TOML documents).
- The extracted TOMLs go to the engine over a new boundary method,
  `Engine.setPluginContent(strategies, rules)`, which rebuilds the
  strategy registry / rule set as **seeds + plugin content** (idempotent
  set-semantics — reloading never duplicates). Invalid TOMLs are
  warned-and-skipped per document, mirroring the native host.

### Phase 2 — live dispatch (future work)

Custom predicates (`ItemPredicate::Custom`) and recommendation emitters
need calls *during* planning. Design: the worker holds the plugin
instances and the engine gets a synchronous JS callback
(`Engine.setPluginDispatch(fn)`, a `js_sys::Function` wrapped in a
`PluginPredicateDispatch` impl). Same-thread synchronous JS→wasm calls
satisfy the planner; ADR-0008's capability manifest and perf contract
(auto-disable on timeout) apply. Until phase 2 lands,
`ItemPredicate::Custom` keeps evaluating to `false`.

## Consequences

- Plugins work identically in the plain browser and the Electron shell
  (ADR-0010's "desktop is additive" holds).
- The browser's own WebAssembly sandbox replaces wasmtime's fuel/memory
  caps for phase 1; that is acceptable because emission runs **once at
  load time** on pure exports with no imports. Phase 2 re-introduces
  budget enforcement (call timeouts + auto-disable) in the JS host.
- `crates/plugin-host` remains the native/test host (and the reference
  implementation for ABI semantics); the JS host must stay
  ABI-compatible with `crates/plugin-sdk`'s macros.
- The Settings → Plugins panel replaces the Tauri-era plugin manager UI.
