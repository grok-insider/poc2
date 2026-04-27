# ADR-0009 — Defer Wayland layer-shell to v1.1

**Status:** Accepted (Phase D.2 / 2026-04).
**Supersedes:** Part of [ADR-0002 platform-nixos-only](0002-platform-nixos-only.md)
(specifically the layer-shell line item).

## Context

The original v1 plan (M7 Live integration) called for a Wayland
layer-shell-based always-on-top overlay so the advisor could float
above the in-game window without OS-level window-manager fiddling.

Current state:

- `tauri 2.10.x` does not expose layer-shell APIs natively.
- `gtk4-layer-shell` requires a custom Tauri runtime fork.
- `smithay-client-toolkit` would mean re-implementing the window
  surface from scratch (mid-six-figure LOC delta).
- The v1 platform target is **NixOS + Hyprland only** (per ADR-0002).
  Hyprland's `windowrulev2` system already supports the always-on-top
  + float behaviour we need without any Tauri-side work.

## Decision

For v1, ship the always-on-top requirement as a documented Hyprland
configuration recipe (NixOS module snippet) instead of via Wayland
layer-shell. Layer-shell-based overlay revisits in v1.1.

## Consequences

### Positive

- Zero Tauri-side overlay work for v1; the entire window-management
  contract is data (a `windowrulev2` line) the user adds to their
  Hyprland config.
- Clipboard reads (the canonical Live-craft input path per M7) work
  out of the box on Wayland via `wl-clipboard` / `tauri-plugin-clipboard-manager`.
- Trade-search URL opens via `tauri-plugin-shell::open` on Wayland
  unchanged.

### Negative

- Users on other Wayland compositors (Sway, KDE, GNOME) need to
  hand-roll the equivalent rule. Documented but not packaged.
- True overlay-over-fullscreen requires the user to run PoE2 in
  borderless windowed mode, not exclusive fullscreen.

## Implementation

### Hyprland windowrulev2 (recommended starting point)

Add to `~/.config/hypr/hyprland.conf` (or your NixOS Hyprland module):

```hyprlang
# Path of Crafting 2 — pin always-on-top, no border, slim.
windowrulev2 = float, class:^(ai\.anomaly\.poc2)$
windowrulev2 = pin, class:^(ai\.anomaly\.poc2)$
windowrulev2 = noborder, class:^(ai\.anomaly\.poc2)$
windowrulev2 = size 480 720, class:^(ai\.anomaly\.poc2)$
windowrulev2 = move 100% 0, class:^(ai\.anomaly\.poc2)$
windowrulev2 = opacity 0.95, class:^(ai\.anomaly\.poc2)$
```

`pin` keeps the window on top of every workspace; `float` keeps it
out of the tiling stack; `move 100% 0` docks it to the right edge.

### NixOS module snippet

For users wiring this through a Hyprland NixOS module:

```nix
{ config, lib, pkgs, ... }: {
  wayland.windowManager.hyprland.settings.windowrulev2 = [
    "float, class:^(ai\\.anomaly\\.poc2)$"
    "pin, class:^(ai\\.anomaly\\.poc2)$"
    "noborder, class:^(ai\\.anomaly\\.poc2)$"
    "size 480 720, class:^(ai\\.anomaly\\.poc2)$"
    "move 100% 0, class:^(ai\\.anomaly\\.poc2)$"
    "opacity 0.95, class:^(ai\\.anomaly\\.poc2)$"
  ];
}
```

### Tauri-side configuration

The Tauri identifier `ai.anomaly.poc2` (set in `tauri.conf.json`) is
what Hyprland's `class:` rule keys off. No changes needed; the rules
above just match the existing identifier.

## Future work (v1.1)

- Investigate `gtk4-layer-shell` integration via a thin Tauri shim.
- If layer-shell lands, the Hyprland rules become a fallback path
  rather than the canonical one.
- Sway / KDE Plasma / GNOME compositor recipes would join the docs.
