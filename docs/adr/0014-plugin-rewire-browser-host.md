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

### Phase 2 — live predicate dispatch ✅ (shipped 2026-07)

Custom predicates (`ItemPredicate::Custom`) need calls *during*
planning. **Plugin instances moved into the engine worker** (the client
transfers the stored wasm bytes via a `__loadPlugins` message); the
engine gets a synchronous JS callback — `Engine.setPluginDispatch(fn)`,
a `js_sys::Function` wrapped in a `PluginPredicateDispatch` impl (the
trait dropped its `Send + Sync` supertrait: dispatch is only ever a
same-thread borrow inside one planning call). The dispatcher enforces
ADR-0008's perf contract the only way a synchronous JS host can:
wall-clock measurement per call with strike-based auto-disable (default
3 strikes over a 5 ms budget; throws count as strikes). The plugin id
is the stored file name; rules reference it via
`custom = { plugin_id = "…" }`.

Shipping phase 2 surfaced and fixed a **latent v1 SDK bug**:
`poc2-plugin-sdk`'s `write_output`/`read_input` indexed the arena `Vec`
with *absolute linear-memory addresses*, trapping (`unreachable`) on
every real emission call — unnoticed because the native host tests used
hand-written WAT fixtures. The SDK now uses raw-pointer access, and an
opt-in web test (`pluginsRealWasm.test.ts`) exercises the real
SDK-built example plugin so the gap can't reopen.

### Phase 3 — recommendation emitters (future work)

`emit_recommendations` needs a hook the planner doesn't have yet
(`PlanInput` only carries predicate dispatch); adding a candidate-source
trait to the advisor is engine work, not host wiring. Until then the
`declare_recommendation_emitter!` surface is dormant. A capability
manifest approval UI also lands here (phase 2 gates on exported
surface: `eval_predicate` + `alloc`).

## Consequences

- Plugins work identically in the plain browser and the Electron shell
  (ADR-0010's "desktop is additive" holds).
- The browser's own WebAssembly sandbox replaces wasmtime's fuel/memory
  caps; emission runs once at load time, and phase 2's per-call budget
  is enforced post-hoc (strike-based auto-disable) since synchronous JS
  cannot preempt a runaway call — a misbehaving plugin can slow a few
  plans but is then silenced for the session.
- `crates/plugin-host` remains the native/test host (and the reference
  implementation for ABI semantics); the JS host must stay
  ABI-compatible with `crates/plugin-sdk`'s macros.
- The Settings → Plugins panel replaces the Tauri-era plugin manager UI.
