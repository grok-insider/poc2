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
- **In-game market overlay** (ADR-0013 + Hyprland plugin path): calibrate a
  reward rectangle once (drag, then Enter to confirm), then scan Verisium/reward
  rows through geometry-aware OCR + the desktop price cache; hovered items can also be smart
  price-checked through live trade2 with the same 90%/110% stat ranges as the
  Price panel. Hyprland uses `hyproverlay` when loaded; other sessions keep the
  existing Electron full/degraded fallback.
- **Search Regex overlay**: an in-game, hotkey-driven picker for item/map/tablet
  search strings. PoC2 hydrates bundle-backed mod pools and generates regexes;
  `hyproverlay` only renders the generic menu.
- **System tray**: resident tray icon with show, capture, price-check, scan,
  calibration, hide-overlay, and quit actions. Closing the main window hides to
  tray so hotkeys and compositor-bound second-instance commands keep working.
- External links open in the system browser.

## Architecture

| Piece | File | Notes |
|---|---|---|
| Entry / windows / tray | `src/main.ts` | single-instance; `--capture` / `--price-check` / `--scan` / `--watch-rewards` / regex flags / `--recalibrate` forward to the running app; owns main + overlay + calibration windows and the tray menu |
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
  **Requires** `apps/web/public/poc2.bundle.json.gz` (gitignored data
  bundle) and a prior root `bun run wasm && bun run build` so
  `apps/web/out` includes the engine assets. From the repo root:
  `bun run bundle:web && bun run wasm && bun run build`, then package.
  Missing the bundle → Windows/Linux installs show
  `ENGINE FAILED TO LOAD / bundle fetch failed: 404`.

Electron binary resolution (`scripts/run-electron.mjs`): `$POC2_ELECTRON` →
npm-downloaded binary → `electron` on PATH (the Nix devshell provides it).

## Auto-update (packaged installs only)

Packaged builds use [`electron-updater`](https://www.electron.build/auto-update)
against public **GitHub Releases** (`grok-insider/poc2`). Feed metadata is
baked from `electron-builder.yml` `publish` into `app-update.yml` at package
time; CI still uses `--publish never` and uploads installers + `latest*.yml`
onto the release-plz-created Release. If electron-builder omits the feed
files, `scripts/write-update-yml.mjs` synthesizes them before upload.

| Install method | Auto-update |
|---|---|
| Linux **AppImage** | Yes |
| Linux **`.deb`** | No (re-download from Releases) |
| Windows **NSIS** | Yes |
| `bun run desktop:dev` / unpackaged | No (updater no-op) |

Behaviour: check a few seconds after launch (and via Settings → Desktop
updates), download in the background, then **Settings / tray “Install &
restart…”** after user confirm — never silent install. Windows SmartScreen
warnings are expected for unsigned builds.

## Hotkeys

| Action | Default | Env override | Second-instance flag |
|---|---|---|---|
| Item capture | `Alt+C` | `POC2_CAPTURE_HOTKEY` | `poc2-desktop --capture` (`--advanced` for Ctrl+Alt+C) |
| Item price check | `Alt+E` | `POC2_PRICE_HOTKEY` | `poc2-desktop --price-check` |
| Reward OCR scan | `Alt+V` | `POC2_SCAN_HOTKEY` | `poc2-desktop --scan` / `--scan-rewards` |
| Toggle reward watcher | `Alt+Shift+V` | `POC2_WATCHER_HOTKEY` | `poc2-desktop --watch-rewards` |
| Recalibrate region | `Alt+L` | `POC2_RECALIBRATE_HOTKEY` | `poc2-desktop --recalibrate` |
| Regex picker | `Alt+F` | `POC2_REGEX_HOTKEY` | `poc2-desktop --regex-open` |
| Regex copy | `Alt+Shift+F` | `POC2_REGEX_COPY_HOTKEY` | `poc2-desktop --regex-copy` |
| Hide overlay | `Esc` (only while visible) | — | — |

**Search Regex navigation on Hyprland (hypr-overlay):** when the plugin reports
`menu.interactive`, the open menu is **pointer/keyboard interactive** (click
toggles, ↑↓/←→/Enter after focus, on-menu Copy actions). Compositor binds for
`--regex-next` / `--regex-tab-*` / `--regex-toggle` are optional legacy fallbacks
for display-only sessions. Open + copy still need a hotkey (or tray).

- Windows / X11: registered natively via `globalShortcut`.
- Wayland: needs the GlobalShortcuts portal
  (`--enable-features=GlobalShortcutsPortal --ozone-platform=wayland`) —
  or skip portals entirely with compositor binds to the second-instance
  flags, which work everywhere:

```conf
# hyprland.conf
bind = ALT, C, exec, poc2-desktop --capture
bind = CTRL SHIFT, A, exec, poc2-desktop --capture --advanced
bind = ALT, E, exec, poc2-desktop --price-check
bind = ALT, V, exec, poc2-desktop --scan
bind = ALT SHIFT, V, exec, poc2-desktop --watch-rewards
bind = ALT, L, exec, poc2-desktop --recalibrate
bind = ALT, F, exec, poc2-desktop --regex-open
bind = ALT SHIFT, F, exec, poc2-desktop --regex-copy
# Optional (legacy): regex nav when menu.interactive is unavailable
# bind = CTRL SHIFT, left, exec, poc2-desktop --regex-tab-prev
# bind = CTRL SHIFT, right, exec, poc2-desktop --regex-tab-next
# bind = CTRL SHIFT, up, exec, poc2-desktop --regex-prev
# bind = CTRL SHIFT, down, exec, poc2-desktop --regex-next
# bind = CTRL SHIFT, RETURN, exec, poc2-desktop --regex-toggle
```

Linux injection prefers `hyprctl dispatch sendshortcut`, then `ydotool`
(needs `ydotoold`; on NixOS `programs.ydotool.enable = true`), then `wtype`.

## Capture flow (APT semantics)

snapshot clipboard → clear → inject Ctrl+C (Ctrl+Alt+C for advanced mods)
→ poll every 48 ms (≤500 ms) for text starting with a known
`Item Class:` line (any client language) → push to renderer → restore the
user's clipboard after 120 ms.

## Market and Regex overlay flow

Capability gate at startup decides `overlayMode`:

| Session | Mode |
|---|---|
| win32, non-Hyprland Linux X11/XWayland | `full` (silent Electron capture, click-through window) |
| Wayland GNOME/KDE (probe passes) | `full` (portal capture) |
| Hyprland/wlroots with `hyproverlay` v4 loaded | `hyprland-plugin` (compositor drag-confirm calibration, `grim` capture, positioned icon/value rows and generic menus) |
| Wayland wlroots without plugin | `degraded` (`slurp` + `grim`, in-app result panel) |
| Other Wayland probe-fail | `degraded` (portal capture, in-app result panel) |

Reward scan = one `Alt+V` pass, or an opt-in `Alt+Shift+V` watcher with open/close
hysteresis and independent 500 ms presence checks. Match the PoE2 client language
in **Settings → Client language** so OCR maps localized names to English price
keys (`sp` = Spanish). OCR starts no more than once every two seconds and keeps
only the newest pending frame. Windows uses a packaged, persistent
`Windows.Media.Ocr` helper first; any unavailable, failed, or incomplete native
read falls back to the portable worker. The hidden
`/overlay` worker otherwise captures first → native-canvas 1.25x text-column
crop with the fast Tesseract model and PSM 11 (2x/alternate-crop fallback when
fewer than four catalogue rows resolve) → one batched fuzzy resolution against
the price-cache catalogue → spatial row locking. On `hyproverlay` v4, each line center becomes a
transparent, row-aligned currency icon + stack value immediately outside the
capture region; old plugins retain compact cards. Runtime-fetched Divine/Exalted
icons are converted to bounded RGBA and registered in compositor memory, never
committed. Calibration is compositor-native on Hyprland: drag, release, then
Enter/Space to confirm or drag again. Settings → OCR diagnostics exposes
**Calibrate**, **Scan now**, **Start watcher**, the selected OCR backend, row Y
positions, and protocol data.

Item price check = hovered item capture → `/overlay` builds smart trade2 stat
filters (default 90% lower-bound / 110% upper-bound) → desktop proxy search/fetch
→ compact cheapest/median/count rows → desktop market history.

Regex picker = second-instance regex flags → `/overlay` updates a clean-room
menu state, hydrates current item / waystone / tablet pools from the WASM engine,
and sends a generic `mode:"menu"` payload to `hyproverlay`. Copy/apply writes the
generated string through Electron main; if the plugin is absent, the
Electron/degraded overlay shows a compact fallback card.
