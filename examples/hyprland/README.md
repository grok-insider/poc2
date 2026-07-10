# Hyprland integration examples (Phase D.2 + ADR-0011)

Per [ADR-0009](../../docs/adr/0009-defer-wayland-layer-shell-to-v1-1.md),
v1's always-on-top behaviour is a documented Hyprland window-rule
configuration rather than a custom Wayland layer-shell surface. Per
[ADR-0011](../../docs/adr/0011-browser-capture-daemon.md), overlay-style
item capture is a compositor bind + the `poc2-capture` loopback daemon.

## Files

- `poc2-windowrules.conf` — drop-in Hyprland config you can `source`
  from your `hyprland.conf`.
- `poc2-capture.conf` — capture binds (`CTRL+SHIFT+D` copy-capture,
  `CTRL+SHIFT+A` advanced mods, `CTRL+SHIFT+S` screenshot-OCR) plus the
  daemon `exec-once`.
- `nixos-module.nix` — Home-Manager / NixOS module that appends the
  window-rule lines to your existing `wayland.windowManager.hyprland`
  config.

## Item capture (ADR-0011)

```bash
cargo install --path crates/capture      # installs `poc2-capture`
# hyprland.conf:
#   source = ~/.config/hypr/poc2-capture.conf
```

Hover an item in PoE2 and press `CTRL+SHIFT+D`: the daemon injects the
game's own Ctrl+C (`hyprctl dispatch sendshortcut`, `ydotool` fallback),
polls the clipboard for the item text, restores your clipboard, and pushes
the item to the web app over `ws://127.0.0.1:17771/ws`. The app imports it
instantly — no alt-tab. The connection indicator lives in Settings →
Capture.

Optional NixOS bits:

```nix
programs.ydotool.enable = true;   # uinput fallback injector
# wl-clipboard + grim come from your Hyprland environment as usual.
```

## In-game overlay — plugin mode on Hyprland (ADR-0013 + hyproverlay)

The screen-region price overlay is **capability-gated**
([ADR-0013](../../docs/adr/0013-item-capture-ocr-overlay.md)):

| Session | Overlay |
|---|---|
| win32 / non-Hyprland Linux-X11 | **full** — its own transparent click-through Electron window |
| GNOME/KDE Wayland that passes the runtime probe | **full** |
| **Hyprland / wlroots with `hyproverlay` v4 loaded** | **hyprland-plugin** — compositor calibration, positioned icon/value rows, cards/menu |
| wlroots without `hyproverlay` | **degraded** — in-app fallback panel |

On Hyprland, load the generic `hyproverlay` plugin before starting PoC2. PoC2
keeps capture/OCR/trade/regex logic in the desktop/web app and sends only small
generic bounded payloads to the compositor. If the plugin is absent or fails,
the app falls back to the existing degraded in-app panel.

This remains true when Electron itself is forced through XWayland: PoC2 detects
the Hyprland host independently, uses `hyproverlay` v4 for dimmed drag-confirm
calibration, `grim` to capture silently, and positioned compositor rows to place
currency icons and values at the matching reward-line centers. `slurp` remains
the calibration fallback for an older plugin. Both `grim` and `slurp` should be
on `PATH`.

One-shot, watcher, and recalibrate binds use the
same second-instance flag path as `--capture`:

```conf
bind = ALT, V, exec, poc2-desktop --scan
bind = ALT SHIFT, V, exec, poc2-desktop --watch-rewards
bind = ALT, L, exec, poc2-desktop --recalibrate
bind = ALT, E, exec, poc2-desktop --price-check
bind = ALT, F, exec, poc2-desktop --regex-open
bind = ALT SHIFT, F, exec, poc2-desktop --regex-copy
# Regex ↑↓/←→/Enter navigation binds are optional when hyproverlay reports
# menu.interactive — the open menu accepts pointer + in-overlay keyboard.
```

Recalibrate dims the desktop through `hyproverlay`: drag the complete reward-row
body, release to retain the green draft, press Enter/Space to persist it, or drag
again to redo. The Electron full-screen calibrator mirrors that confirmation flow
outside Hyprland; `slurp` remains the plugin-compatibility fallback.

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
