# ADR-0006 — Patch versioning baked in from line 1

- Status: Accepted
- Date: 2026-04-26

## Context

PoE2 patches roughly every 3-4 months. Patch 0.4 ("Fate of the Vaal") shipped Dec 12 2025; 0.5 ("Return of the Ancients") is announced for May 29 2026. Each patch can:

- Add new currencies / omens / essences (e.g., Architect's Orb in 0.4)
- Disable existing items (e.g., Omen of Homogenising Exaltation in 0.4)
- Buff / nerf mod values (e.g., Perfect Essence of Battle nerfed in 0.4)
- Rename concepts (e.g., Rune Sockets → Augment Sockets in 0.4)
- Rebalance weights

A single hardcoded patch baseline would obsolete the app on every patch day.

## Decision

Every entity in the system carries a patch range:

```rust
struct PatchRange {
    min: Option<PatchVersion>,    // None = "any version up to max"
    max: Option<PatchVersion>,    // None = "from min onwards (current)"
}
```

Applied to:

| Entity | Patch range field | Notes |
|---|---|---|
| `ModDefinition` | `patch_range` | Tier values, weights tagged separately |
| `Currency` | `patch_range` | E.g., Architect's Orb has `patch_min = 0.4.0` |
| `Omen` | `patch_range` | Homogenising Exaltation has `patch_max = 0.3.x` (disabled in 0.4) |
| `Essence` | `patch_range` | Per-tier (Lesser/Normal/Greater/Perfect/Corrupted) ranges |
| `Bone` | `patch_range` | |
| `Catalyst` | `patch_range` | |
| `Strategy` | `patch_min` / `patch_max` | TOML field; advisor filters by current patch |
| `Rule` | `patch_min` / `patch_max` | Same |
| `Bundle` | `game_patch` | The patch this bundle is built against |

## Bundle compatibility

A bundle declares `game_patch: PatchVersion`. The engine's `apply()` only sees entities whose `PatchRange.contains(bundle.game_patch)`.

When 0.5 ships:
1. The pipeline rebuilds the bundle with `game_patch: 0.5.0`.
2. Entities deprecated in 0.5 disappear (their `patch_max` is now exceeded).
3. Entities introduced in 0.5 appear (their `patch_min == 0.5.0`).
4. The desktop app downloads the new bundle; no rebuild required.

If a bundle's `game_patch` is incompatible with the engine's `ENGINE_SCHEMA_VERSION`, the loader refuses with a clear error.

## Strategy / rule patch warnings

When the user is on 0.5 but a strategy in their library is `patch_max: 0.4`, the advisor:

1. Filters it out of suggestions by default.
2. Surfaces it under "legacy strategies" with a warning badge.
3. Offers to disable globally if not wanted.

Special case for Homogenising Exaltation (disabled in 0.4 but legacy stockpiles still work): per planning, we **never suggest it in 0.4 strategies**. It only appears in patch ≤ 0.3 strategy entries.

## Migration strategy for in-flight crafts

If a user is mid-craft when a patch lands:
- The advisor flags "your bundle is for 0.4; current patch is 0.5"
- User can manually choose to stay on 0.4 bundle (e.g., Standard league items)
- Switching bundles mid-craft is allowed but the advisor warns about strategy validity

## Consequences

- Every TOML strategy/rule file requires a `patch_min` field (validation).
- Bundle versioning is a CI-checkable invariant.
- When a patch breaks something subtle, the fix is a data update, not a code release.
- Cost: a small overhead in bundle size and load-time filtering. Acceptable.
