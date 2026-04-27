# Hyprland integration examples (Phase D.2)

Per [ADR-0009](../../docs/adr/0009-defer-wayland-layer-shell-to-v1-1.md),
v1's always-on-top behaviour is a documented Hyprland window-rule
configuration rather than a custom Wayland layer-shell surface.

## Files

- `poc2-windowrules.conf` — drop-in Hyprland config you can `source`
  from your `hyprland.conf`.
- `nixos-module.nix` — Home-Manager / NixOS module that appends the
  6 window-rule lines to your existing `wayland.windowManager.hyprland`
  config.

## Behaviour

| Rule | Effect |
|---|---|
| `float` | poc2 doesn't enter the tiling stack |
| `pin` | always-on-top across all workspaces |
| `noborder` | clean overlay look |
| `size 480 720` | sensible initial dimensions |
| `move 100% 0` | docked to the right edge of the primary monitor |
| `opacity 0.95` | slight transparency so the game stays visible |
| `nofocus` (during loading) | no focus steal when the splash shows |

## Caveats

- Run PoE2 in **borderless windowed** mode, not exclusive fullscreen
  — exclusive fullscreen always wins regardless of `pin`.
- Other compositors (Sway, KDE, GNOME) need equivalent rules; we don't
  ship presets for them in v1.

## Verify

```bash
hyprctl clients | grep -A4 "class: ai.anomaly.poc2"
```

You should see `floating: 1`, `pinned: 1`, and your configured size.
