# Path of Crafting 2 — Desktop Shell

Electron windowed app (ADR-0010 + ADR-0013) around the `apps/web` static
export. A normal desktop window like Discord — **not** an in-game overlay
(ADR-0009's layer-shell deferral stands; the price overlay below is a
plain Electron window).

What it adds over the browser app:

- **Item capture**: hotkey → injects the game's own `Ctrl+C` (the hovered
  item lands in the clipboard) → imports it into the bench. Same mechanism
  as Awakened PoE Trade; no OCR involved in *item* capture.
- **Trade proxy**: official `trade2` API calls run in the main process
  (no CORS, centralized header-driven rate limiting) for price checking.
- **Price cache** (ADR-0013 follow-up): hourly poe2scout catalogue
  refresh into `node:sqlite` (JSON/memory fallback), with poe.ninja
  fallback rows for names poe2scout doesn't price. Feeds the OCR overlay
  and the Settings panel status card.
- **Screen-region OCR price overlay** (ADR-0013): calibrate a screen
  rectangle once (`/calibrate` drag-select), then a scan hotkey OCRs it
  and shows price plates — a transparent click-through window on
  win32/X11 (and probe-passing Wayland), an in-app panel in "degraded"
  mode on Hyprland/wlroots.
- External links open in the system browser.

## Architecture

| Piece | File | Notes |
|---|---|---|
| Entry / windows | `src/main.ts` | single-instance; `--capture` / `--scan` / `--recalibrate` flags forward to the running app; owns main + overlay + calibration windows |
| Static serving | `src/serve.ts` + `src/staticResolve.ts` | privileged `app://` scheme over `apps/web/out` (root-absolute asset URLs keep working) |
| Renderer bridge | `src/preload.ts` | exposes `window.poc2Desktop` — contract mirrored in `apps/web/lib/desktop.ts` (change both or neither) |
| Capture | `src/capture/` | orchestrator + `linux.ts` (hyprctl → ydotool → wtype spawns) + `win32.ts` (uiohook-napi, lazy, optionalDependency) + `capabilities.ts`/`overlayProbe.ts` (ADR-0013 gate) + `screen.ts` (region capture) |
| Trade proxy | `src/trade/` | `RateLimiter` honors `X-Rate-Limit-*`; search/fetch passthrough |
| Price cache | `src/prices/` | poe2scout fetcher + poe.ninja fallback + sqlite store + hourly scheduler |
| IPC | `src/ipc.ts` | the single channel table (capture, trade, fetch allowlist, overlay/region, prices) |

## Commands (from `apps/desktop`, or `bun run desktop:*` / `test:desktop` from the repo root)

- `bun install` — deps (Bun skips postinstalls; that's fine on NixOS)
- `bun test` — unit tests (pure modules: rate limiter, capture text
  detection, injection command construction, static resolver, capability
  gate, screen geometry, IPC allowlist, price parsers/store)
- `bun run typecheck` / `bun run build` — tsc
- `bun run dev` — build + launch against the dev server
  (`POC2_DEV_URL`, default `http://localhost:3000`; start `bun run dev`
  at the repo root first)
- `bun run start` — build + launch serving `apps/web/out`
  (run `bun run build` at the repo root first)
- `bun run dist:linux` / `dist:win` — electron-builder packages
  (AppImage/deb, NSIS). CI runs these on ubuntu/windows runners.

Electron binary resolution (`scripts/run-electron.mjs`): `$POC2_ELECTRON` →
npm-downloaded binary → `electron` on PATH (the Nix devshell provides it).

## Hotkeys

| Action | Default | Env override | Second-instance flag |
|---|---|---|---|
| Item capture | `Ctrl+Shift+D` | `POC2_CAPTURE_HOTKEY` | `poc2-desktop --capture` (`--advanced` for Ctrl+Alt+C) |
| OCR scan | `Ctrl+Shift+S` | `POC2_SCAN_HOTKEY` | `poc2-desktop --scan` |
| Recalibrate region | `Ctrl+Shift+C` | `POC2_RECALIBRATE_HOTKEY` | `poc2-desktop --recalibrate` |
| Hide overlay | `Esc` (only while visible) | — | — |

- Windows / X11: registered natively via `globalShortcut`.
- Wayland: needs the GlobalShortcuts portal
  (`--enable-features=GlobalShortcutsPortal --ozone-platform=wayland`) —
  or skip portals entirely with compositor binds to the second-instance
  flags, which work everywhere:

```conf
# hyprland.conf
bind = CTRL SHIFT, D, exec, poc2-desktop --capture
bind = CTRL SHIFT, A, exec, poc2-desktop --capture --advanced
bind = CTRL SHIFT, S, exec, poc2-desktop --scan
bind = CTRL SHIFT, C, exec, poc2-desktop --recalibrate
```

Linux injection prefers `hyprctl dispatch sendshortcut`, then `ydotool`
(needs `ydotoold`; on NixOS `programs.ydotool.enable = true`), then `wtype`.

## Capture flow (APT semantics)

snapshot clipboard → clear → inject Ctrl+C (Ctrl+Alt+C for advanced mods)
→ poll every 48 ms (≤500 ms) for text starting with a known
`Item Class:` line (any client language) → push to renderer → restore the
user's clipboard after 120 ms.

## OCR overlay flow (ADR-0013)

Capability gate at startup decides `overlayMode`:

| Session | Mode |
|---|---|
| win32, Linux X11/XWayland | `full` (silent region capture, click-through window) |
| Wayland GNOME/KDE (probe passes) | `full` (portal capture) |
| Wayland Hyprland/wlroots or probe-fail | `degraded` (in-app panel, portal capture) |

Scan = ONE pass: main reveals the overlay (or signals the panel) → the
`/overlay` route calls `captureRegion` → renderer-side preprocessing +
tesseract.js (vendored `/ocr/` assets) → fuzzy name resolution against
the price-cache catalogue → row-locked price plates. Portal-denied
capture falls back to the clipboard item path.
