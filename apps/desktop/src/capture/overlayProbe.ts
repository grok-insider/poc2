// Runtime overlay probe for the GNOME/KDE Wayland case (ADR-0013).
//
// Electron-touching, so it lives apart from the pure `capabilities.ts`
// classifier and is injected into `detectCapabilities({ probeOverlay })`.
// It creates a tiny offscreen transparent, frameless, hidden window, asks for
// click-through + always-on-top, and verifies those actually took. If the
// compositor refuses any step we report `false` and the gate falls back to the
// degraded in-app panel.
import { BrowserWindow } from "electron";

/** Create the offscreen probe window, assert click-through + on-top, tear down. */
export async function probeOverlaySupport(): Promise<boolean> {
  let win: BrowserWindow | null = null;
  try {
    win = new BrowserWindow({
      width: 1,
      height: 1,
      x: -10_000,
      y: -10_000,
      show: false,
      frame: false,
      transparent: true,
      focusable: false,
      skipTaskbar: true,
      webPreferences: { offscreen: true },
    });

    // These throw or silently no-op on compositors that don't support them.
    win.setIgnoreMouseEvents(true, { forward: true });
    win.setAlwaysOnTop(true, "screen-saver");

    const onTop = win.isAlwaysOnTop();
    // Electron exposes no getter for ignore-mouse-events; window creation with
    // transparency + a successful always-on-top set is our best signal that the
    // compositor honored the override-redirect-style hints.
    return onTop === true;
  } catch {
    return false;
  } finally {
    if (win && !win.isDestroyed()) win.destroy();
  }
}
