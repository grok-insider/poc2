# Path of Crafting 2 — Desktop Shell

Electron windowed app (ADR-0010) around the `apps/web` static export. A
normal desktop window like Discord — **not** an in-game overlay.

What it adds over the browser app:

- **Item capture**: hotkey → injects the game's own `Ctrl+C` (the hovered
  item lands in the clipboard) → imports it into the bench. Same mechanism
  as Awakened PoE Trade; no OCR involved.
- **Trade proxy**: official `trade2` API calls run in the main process
  (no CORS, centralized header-driven rate limiting) for price checking.
- External links open in the system browser.

## Architecture

| Piece | File | Notes |
|---|---|---|
| Entry / window | `src/main.ts` | single-instance; `--capture` flag forwards to the running app |
| Static serving | `src/serve.ts` + `src/staticResolve.ts` | privileged `app://` scheme over `apps/web/out` (root-absolute asset URLs keep working) |
| Renderer bridge | `src/preload.ts` | exposes `window.poc2Desktop` — contract mirrored in `apps/web/lib/desktop.ts` |
| Capture | `src/capture/` | orchestrator + `linux.ts` (hyprctl → ydotool → wtype spawns) + `win32.ts` (uiohook-napi, lazy, optionalDependency) |
| Trade proxy | `src/trade/` | `RateLimiter` honors `X-Rate-Limit-*`; search/fetch passthrough |
| IPC | `src/ipc.ts` | the single channel table |

## Commands (from `apps/desktop`)

- `bun install` — deps (Bun skips postinstalls; that's fine on NixOS)
- `bun test` — unit tests (pure modules: rate limiter, capture text
  detection, injection command construction, static resolver)
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

## Capture hotkey

- Default: `Ctrl+Shift+D` (override: `POC2_CAPTURE_HOTKEY`).
- Windows / X11: registered natively via `globalShortcut`.
- Wayland: needs the GlobalShortcuts portal
  (`--enable-features=GlobalShortcutsPortal --ozone-platform=wayland`) —
  or skip portals entirely with a compositor bind to the second-instance
  flag, which works everywhere:

```conf
# hyprland.conf
bind = CTRL SHIFT, D, exec, poc2-desktop --capture
bind = CTRL SHIFT, A, exec, poc2-desktop --capture --advanced
```

Linux injection prefers `hyprctl dispatch sendshortcut`, then `ydotool`
(needs `ydotoold`; on NixOS `programs.ydotool.enable = true`), then `wtype`.

## Capture flow (APT semantics)

snapshot clipboard → clear → inject Ctrl+C (Ctrl+Alt+C for advanced mods)
→ poll every 48 ms (≤500 ms) for text starting with a known
`Item Class:` line (any client language) → push to renderer → restore the
user's clipboard after 120 ms.
