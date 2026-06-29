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
