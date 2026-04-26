# ADR-0008 — Plugin system deferred to v1.1

- Status: Accepted
- Date: 2026-04-26

## Decision

Strategies and rules in v1.0 are **TOML data files** baked into the data bundle and updated via the auto-update pipeline.

A **Wasm-sandboxed plugin system** for user-authored strategies, rules, and custom currency / engine extensions ships in **v1.1**.

## Rationale

- v1.0 already has 8 milestones (M1-M8). Adding a plugin SDK now bloats scope and risks ship date.
- TOML strategies/rules cover the 23-strategy seed catalog and ~120 rule library cleanly. No need for executable plugins yet.
- Wasm Component Model (the right plugin tech) is still stabilizing; v1.1 lets us pick up better tooling.
- Once v1.0 ships, plugin demand will shape the API better than guessing now.

## v1.0 extension points (without plugins)

Even without plugins, v1.0 supports:

- **User-authored strategies**: drop a TOML file in `~/.config/poc2/strategies/`; advisor auto-loads on launch.
- **User-authored rules**: same in `~/.config/poc2/rules/`.
- **Synergy overrides**: `~/.config/poc2/synergy-overrides.toml`.
- **Bundle override**: load a custom bundle from a file path instead of GitHub Release.

These cover the 90% case for advanced users.

## v1.1 plugin scope

| Plugin type | Purpose |
|---|---|
| Strategy plugin | Programmatic strategies — generate steps based on item state, not just declarative TOML |
| Rule plugin | Custom predicates / actions beyond what the TOML DSL expresses |
| Currency plugin | Add new currency types (e.g., for hypothetical patches before official data lands) |
| UI panel plugin | Side panels rendering custom data (e.g., a builds-overlay) |

## Sandbox design (preview)

- **Wasm Component Model** — strict capability model, no ambient I/O.
- **Plugin manifest** — declares required capabilities (read item state, propose actions, etc.).
- **Capability negotiation** — user reviews capabilities at install; advisor refuses on overreach.
- **Versioned plugin API** — semver; plugins declare `poc2_api_version = "1.1.x"`.
- **No network in plugins** — plugins request the host to fetch on their behalf, with rate limits.
- **Hot reload** — `poc2-cli plugin reload <id>` during dev; production needs app restart.

## Consequences

- v1.0 ships faster.
- v1.0 extension via TOML is good enough for power users.
- v1.1 plugin system is greenfield, not a retrofit, which means we get the API right.
- Trust model: plugin marketplace (if we have one) requires signature verification.
