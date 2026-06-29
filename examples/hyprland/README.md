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

## Price overlay — DEGRADED mode on wlroots (ADR-0013)

The screen-region price overlay is **capability-gated**
([ADR-0013](../../docs/adr/0013-item-capture-ocr-overlay.md)):

| Session | Overlay |
|---|---|
| win32 / Linux-X11 (incl. XWayland) | **full** — its own transparent click-through Electron window |
| GNOME/KDE Wayland that passes the runtime probe | **full** |
| **Hyprland / wlroots Wayland** | **degraded** — no click-through window |

On Hyprland/wlroots the app runs the overlay in **degraded** mode: Electron's
transparent click-through window is unreliable here and a real layer-shell
surface stays deferred (ADR-0009). The price panel renders **inside the main
window** instead. To get an overlay-like HUD anyway, the shipped
`float`/`pin`/`move 100% 0` rules already make the main window hover over PoE2;
the `windowrulev2 = float, …` line in `poc2-windowrules.conf` is the focused
fallback for this mode.

Scan / recalibrate binds (single OCR pass over the calibrated region) use the
same second-instance flag path as `--capture`:

```conf
bind = CTRL SHIFT, S, exec, poc2-desktop --scan
bind = CTRL SHIFT, C, exec, poc2-desktop --recalibrate
```

Recalibrate opens the full-screen calibrator; drag-select the price region and
the rectangle is persisted (reused on every later scan).

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
