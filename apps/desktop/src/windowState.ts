// Persist window bounds across sessions (userData/window-state.json).
import { app, type BrowserWindow, type Rectangle, screen } from "electron";
import { readFileSync, writeFileSync } from "node:fs";
import path from "node:path";

const FILE = () => path.join(app.getPath("userData"), "window-state.json");
const DEFAULTS: Rectangle = { x: 0, y: 0, width: 1380, height: 900 };

export function loadWindowState(): Partial<Rectangle> & { maximized?: boolean } {
  try {
    const saved = JSON.parse(readFileSync(FILE(), "utf8")) as Rectangle & {
      maximized?: boolean;
    };
    // Discard bounds that no longer intersect any display (monitor removed).
    const visible = screen
      .getAllDisplays()
      .some((d) => saved.x < d.bounds.x + d.bounds.width && saved.x + saved.width > d.bounds.x);
    return visible ? saved : { width: saved.width, height: saved.height };
  } catch {
    return { width: DEFAULTS.width, height: DEFAULTS.height };
  }
}

export function trackWindowState(win: BrowserWindow): void {
  const save = () => {
    try {
      const state = { ...win.getNormalBounds(), maximized: win.isMaximized() };
      writeFileSync(FILE(), JSON.stringify(state));
    } catch {
      // best-effort
    }
  };
  win.on("close", save);
  win.on("moved", save);
  win.on("resized", save);
}

// --- ADR-0013: calibrated capture region + xdg portal token persistence ---

const REGION_FILE = () =>
  path.join(app.getPath("userData"), "capture-region.json");

export interface PersistedCaptureRegion {
  /** Calibrated screen rectangle (global logical px) for the price overlay. */
  rect?: Rectangle;
  /**
   * xdg-desktop-portal restore token (Wayland). Reused on subsequent grabs so
   * the user is only prompted once. Opaque; persisted verbatim.
   */
  portalToken?: string;
}

/** Load the persisted region + portal token; {} on first run / parse error. */
export function loadCaptureRegion(): PersistedCaptureRegion {
  try {
    return JSON.parse(readFileSync(REGION_FILE(), "utf8")) as PersistedCaptureRegion;
  } catch {
    return {};
  }
}

/** Persist (merge) the calibrated region / portal token. Best-effort. */
export function saveCaptureRegion(patch: PersistedCaptureRegion): void {
  try {
    const merged = { ...loadCaptureRegion(), ...patch };
    writeFileSync(REGION_FILE(), JSON.stringify(merged));
  } catch {
    // best-effort
  }
}
