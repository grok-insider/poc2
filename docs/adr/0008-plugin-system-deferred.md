# ADR-0008 — Wasm plugin SDK ships in v1 (re-opened)

- Status: **Accepted (revised 2026-04, Phase F)** — supersedes the
  original "deferred to v1.1" decision.
- Original date: 2026-04-26
- Revision date: 2026-04 (Phase F implementation)
- **Current wiring note (2026-07):** the SDK (`crates/plugin-sdk`) and
  wasmtime host (`crates/plugin-host`) shipped and are tested, using a
  **raw-Wasm `(ptr, len)` ABI** (the Component Model remains future
  work, despite the v1 title below). The app-side re-wire is governed by
  [ADR-0014](0014-plugin-rewire-browser-host.md): a **browser-side JS
  host** — phase 1 (strategy/rule emission via `Engine.setPluginContent`
  + Settings → Plugins) is live; phase 2 (custom predicates +
  recommendation emitters during planning) is pending, so
  `plugin_dispatch` is still `None` and `ItemPredicate::Custom`
  evaluates to false.

## Decision

Ship a **Wasm plugin SDK** in v1.0 covering:

1. **Custom predicates** — plugins extend `ItemPredicate` with the
   new `Custom { plugin_id, name, args }` variant. Strategies + rules
   reference plugin predicates by id.
2. **Strategy / rule emission** — plugins ship strategies (TOML)
   and rules (TOML) bundled inside the WASM artifact; the host
   discovers + loads them at startup.
3. **Recommendation emission** — plugins implement an
   `emit_recommendations(state) -> Vec<PluginCandidate>` hook
   the candidate generator calls per beam-search expansion.

The decision change vs the original ADR-0008: the v1 user feedback
on the strategy/rule DSL surface (Phase A.5) made it clear that
power users want code-level extensibility for niche custom-pool
detection (e.g., "predicate: matches builds in the top-3 most-played
ascendancies"). The TOML DSL can't express compile-time rules
without effectively becoming a programming language; pushing those
to Wasm is the right separation of concerns.

## Capability set

Each plugin declares its required capabilities in its manifest TOML
(`poc2-plugin.toml`). The host refuses to load plugins that
declare capabilities the user hasn't approved.

| Capability | Allows the plugin to… |
|---|---|
| `read_engine` | Read the current `Item` state passed by the host |
| `read_market` | Read the current `Valuator` price band |
| `read_advisor_state` | Read goal + cost-spent + recommendations-so-far |
| `register_predicate` | Export `eval_predicate(name, item, args) -> bool` |
| `emit_strategies` | Export `list_strategies() -> Vec<TomlString>` |
| `emit_rules` | Export `list_rules() -> Vec<TomlString>` |
| `emit_recommendations` | Export `emit_recommendations(state) -> Vec<PluginCandidate>` |

No I/O capabilities (network, filesystem) ship in v1; the host
mediates all data access.

## Perf contract

- **Custom predicate eval**: < 50 µs per call (target).
- **Recommendation emitter**: < 1 ms per state per plugin (hard
  cap; plugins exceeding the cap are auto-disabled with a UI
  toast).
- **10 plugins × depth-3 beam search**: < 50 ms total (informed
  by Phase F.8's perf bench).

## Security model

- **Wasmtime `Engine` with sandboxing on**: no `--` envvars, no
  filesystem mount, no socket access.
- **Memory cap**: 64 MiB per plugin store (configurable upward in
  Settings for trusted plugins).
- **Fuel budget**: 10 000 000 instructions per call; exceeding the
  fuel budget aborts the call with a `PluginError::FuelExhausted`.
- **Hard timeout**: every export call wrapped in
  `tokio::time::timeout(Duration::from_millis(N))` for N derived
  from the perf contract.
- **Per-plugin auto-disable**: 3 timeouts in 1 minute → plugin is
  disabled until the user re-enables it from the Plugin Manager.

## Plugin manifest example

```toml
# poc2-plugin.toml
id = "predicate-meta-build-match"
name = "Meta-build matcher"
version = "0.1.0"
poc2_api_version = "1.0.0"
authors = ["alice@example.com"]
description = "Matches items against the top-3 most-played ascendancies."

capabilities = ["read_engine", "read_market", "register_predicate"]

[wasm]
file = "predicate-meta-build-match.wasm"
```

## Plugin SDK layout

- `crates/plugin-host` — wasmtime + capability gating + dispatch
  cache + integration tests
- `crates/plugin-sdk` — guest-side helpers + macros
  (`declare_predicate!`, `declare_strategy!`, `declare_rule!`,
  `declare_recommendation_emitter!`)

## Backward compatibility

- The `Custom` predicate variant is added to `ItemPredicate` with
  `#[serde(rename = "custom")]`; existing strategies/rules without
  it are unaffected.
- A bundle without any plugins works exactly as before (the host
  only loads from `~/.config/poc2/plugins/` when the directory
  exists).

## Future work (post-v1)

- **UI panel plugins** — render custom side panels.
- **Currency plugins** — add new `CurrencyId` types from plugins.
- **Marketplace + signature verification** — the v1 model trusts
  user-installed plugins; a marketplace requires per-plugin signing
  + a verification step at install.
