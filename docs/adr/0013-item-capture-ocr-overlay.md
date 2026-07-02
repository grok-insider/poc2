# ADR-0013: Screen-region OCR capture + capability-gated price overlay

- **Status:** accepted (2026-06-29, explicit user decision)
- **Complements:**
  [ADR-0010](0010-desktop-shell-electron-cross-platform.md) (the Electron
  windowed shell is the host for native coupling),
  [ADR-0011](0011-browser-capture-daemon.md) (clipboard item capture).
- **Honors / does not reopen:**
  [ADR-0009](0009-defer-wayland-layer-shell-to-v1-1.md) — a real Wayland
  layer-shell overlay surface stays **deferred**; this overlay is a plain
  Electron window. ADR-0010 — the app is a **window, not an overlay**; the
  price overlay is an *opt-in, hotkey-triggered* auxiliary window, not the
  primary UI.

## Context

The planner wants live price snapshots placed next to the in-game price
display. Reading that number means a **screen-region capture** (a small
rectangle the user calibrates once) → downstream OCR (a separate worker) →
price lookup. Two hard problems are cross-platform:

1. **Can we grab a screen rectangle silently?** Windows and X11 (including
   XWayland) allow a no-prompt `desktopCapturer` grab. Wayland requires the
   `xdg-desktop-portal` ScreenCast picker — a one-time grant, then a reusable
   restore token.
2. **Can we float a transparent, click-through, always-on-top window over the
   game?** On win32/X11 yes. On Wayland it depends on the compositor:
   GNOME/KDE often honor Electron's transparent + always-on-top +
   `setIgnoreMouseEvents` hints; **Hyprland/wlroots does not** reliably, and a
   layer-shell surface is deferred (ADR-0009).

A single hard-coded behavior would be wrong on at least one target. So the
overlay is **capability-gated** per session.

## Decision

1. **Capability gate** (`apps/desktop/src/capture/capabilities.ts`, pure +
   unit-tested; the runtime probe is injected from `overlayProbe.ts`):

   | Session | `overlayMode` | `silentRegionCapture` | How decided |
   |---|---|---|---|
   | win32 | `full` | yes | env |
   | Linux X11 / XWayland (`XDG_SESSION_TYPE=x11`) | `full` | yes | env |
   | Linux Wayland + Hyprland/wlroots (`HYPRLAND_INSTANCE_SIGNATURE`, or wlroots/sway/river in `XDG_CURRENT_DESKTOP`) | `degraded` | no | env, **no probe** |
   | Linux Wayland GNOME/KDE (other) | `full` if probe passes else `degraded` | no | **runtime probe** |

   The GNOME/KDE probe creates one offscreen `{transparent, frame:false,
   show:false}` `BrowserWindow`, calls `setIgnoreMouseEvents(true,{forward:true})`
   + `setAlwaysOnTop(true,'screen-saver')`, verifies they took, and destroys it.

2. **Region capture** (`apps/desktop/src/capture/screen.ts`): `captureRegion`
   returns the **raw cropped frame** (`{dataUrl,width,height}`). Pixel
   preprocessing (crop-to-text, invert, upscale) is the renderer's job.
   - Silent path (win32 / X11): `desktopCapturer` → pick the display containing
     the rect → crop in source pixels (HiDPI scale-factor aware).
   - Wayland path: the same call engages the portal. The first grant is
     remembered (token persisted in window state alongside the calibrated
     rect); on deny/unavailable it returns a typed
     `{ ok:false, reason:'portal-denied' }` so the renderer falls back to the
     existing Ctrl+C clipboard import (ADR-0011).

3. **Overlay window** (`apps/desktop/src/main.ts`):
   - `full`: a transparent, frameless, always-on-top, non-focusable
     click-through window (`setIgnoreMouseEvents(true,{forward:true})` +
     `setAlwaysOnTop(true,'screen-saver')`) that loads `/overlay`.
   - `degraded`: **no** click-through window is created. Main pushes an
     `overlay-state {visible, degraded:true}` event and the renderer shows an
     **in-app panel** instead.
   A full-screen transparent **calibration** window loads `/calibrate`; the
   user drag-selects the price region, which posts the rect back through the
   bridge and is persisted (reused on every later scan).

4. **Hotkeys**: scan (single OCR pass over the calibrated region), recalibrate
   (open the calibrator), and Esc to hide a visible overlay. Global where the
   platform allows; on Wayland the same second-instance-flag fallback as
   `--capture` (`poc2-desktop --scan` / `--recalibrate`), bindable from
   Hyprland.

5. **Wire contract**: new IPC channels (`capabilities`, `captureRegion`,
   `overlayShow/Hide/SetRegion`, `calibrateRegion`, plus `regionCalibrated` /
   `overlayState` pushes) are kept in lockstep across `ipc.ts`, `preload.ts`,
   and `apps/web/lib/desktop.ts`. `poe.ninja` is added to the main-process
   fetch allowlist for the price helper.

## Consequences

- One overlay code path serves every target; the *behavior* (real window vs
  in-app panel, silent vs portal capture) is chosen by the gate, not by
  scattered `process.platform` checks. All per-OS logic stays inside the
  `capture/` backend modules.
- Hyprland/wlroots users get a deliberately conservative degraded experience
  (in-app panel + the existing `float` window-rule HUD) rather than a broken
  click-through window — consistent with ADR-0009.
- The plain browser app is unaffected: it has no bridge, so the overlay/scan
  features simply don't appear (graceful degradation, per ADR-0010).
- OCR rendering shipped as a follow-up on this capture path: the `/overlay`
  route runs renderer-side tesseract.js (vendored origin-relative `/ocr/`
  assets) with row-locking de-flicker, resolves names via the engine's fuzzy
  `resolveName`, and prices rows from the desktop poe2scout price cache
  (hourly, node:sqlite, poe.ninja fallback). `/calibrate` is the real
  drag-select calibrator. Known gaps (roadmap): the overlay does not yet
  hydrate the persisted region on first load (a first scan can race the
  calibration push), and the persisted portal restore token is not yet
  passed back to the Wayland portal.
