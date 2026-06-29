# ADR-0011: Browser-side item capture — compositor bind + loopback daemon

- **Status:** accepted (2026-06-11, explicit user decision)
- **Complements:** [ADR-0010](0010-desktop-shell-electron-cross-platform.md)
  (the Electron shell embeds its own capture backend); ADR-0009 (Hyprland
  windowrules for overlay-like placement).
- **Context:** the user wants Awakened-PoE-Trade-style capture — press a
  hotkey while hovering an item in PoE2 and have the advisor import it —
  available **today with the plain browser app**, ahead of (and alongside)
  the ADR-0010 Electron shell.

## Findings (from reading the overlays' source)

Awakened PoE Trade and Exiled Exchange 2 (the PoE2 fork) do **not** OCR.
Their hotkey flow is: global hotkey (uiohook-napi) → inject the game's own
`Ctrl+C` → PoE writes the **hovered item's full text** to the clipboard →
poll clipboard (~48 ms × 10) → parse → overlay. OCR exists in APT only for
Heist reward screens. The clipboard path is lossless; OCR is strictly a
fallback (we keep ours for screenshots).

A browser cannot register global hotkeys, inject keystrokes, or read the
clipboard without focus + user gesture. On the primary platform
(NixOS + Hyprland/Wayland) the missing pieces are all native to the
compositor:

| Need | Hyprland-native answer |
|---|---|
| Global hotkey | `bind = …, exec, <cmd>` |
| Keystroke injection | `hyprctl dispatch sendshortcut` / `ydotool` (uinput) |
| Clipboard read/write unfocused | `wl-paste` / `wl-copy` (wlr-data-control) |
| Cursor-region screenshot | `grim` + `hyprctl cursorpos` |

## Decision

1. **Capture for the browser app is a small Rust daemon,
   `crates/capture` (`poc2-capture`)**:
   - Hyprland bind (`CTRL+SHIFT+D`; `+A` advanced mods; `+S` screenshot-OCR)
     runs `poc2-capture trigger`, which pokes the persistent
     `poc2-capture serve` on `127.0.0.1:17771`.
   - The daemon snapshots the clipboard, injects `Ctrl+C` / `Ctrl+Alt+C`
     (`hyprctl dispatch sendshortcut "CTRL, C, activewindow"`, falling back
     to `ydotool` uinput for raw-input games), polls `wl-paste` (50 ms × 12)
     for text starting with `Item Class:` (localized variants accepted),
     restores the user's clipboard after 120 ms, and broadcasts the item
     over a WebSocket. OCR mode screenshots a 560×360 region around
     `hyprctl cursorpos` via `grim` and broadcasts the PNG.
   - Loopback-only; WebSocket subscribers are origin-checked to
     localhost / `app://` (strict host match, no prefix tricks).
2. **The web app subscribes** (`apps/web/lib/captureBridge.ts`, silent
   auto-reconnect) and routes events through the existing
   `ingestExternalItemText` seam (`item-text` → parse; `item-image` →
   tesseract.js OCR → parse). Capture replaces the craft item immediately
   (undo remains available). Status shows in Settings → Capture and as a
   topbar dot. Browser-only users without the daemon are unaffected.
3. **Relationship to the Electron shell (ADR-0010).** The shell embeds the
   same mechanism in-process (`apps/desktop/src/capture/linux.ts` uses the
   identical hyprctl → ydotool → wtype ladder; Windows uses its own
   backend). The daemon exists so the **plain browser** workflow gets
   overlay-grade capture too — per ADR-0010's principle that the browser
   app remains fully supported. Same semantics, two transports:
   `window.poc2Desktop` (Electron preload) vs `ws://127.0.0.1:17771/ws`
   (daemon). When both are present the desktop bridge wins; the WS bridge
   fails silently in the shell.

## Consequences

- Capture works today with the plain browser app; the mechanism study and
  timing constants are shared with the Electron backend, so behaviour stays
  consistent across both paths.
- Platform-specific code stays out of the engine/web crates, confined to
  `crates/capture` + `examples/hyprland/` (and `apps/desktop` per ADR-0010).
- Non-Hyprland compositors need their own bind + (at minimum) `ydotool`;
  we don't ship presets for them (consistent with ADR-0009).
- The game must be focused with an item hovered — the same constraint as
  every overlay tool.
