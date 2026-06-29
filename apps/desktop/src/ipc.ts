// Single registration point for all main↔renderer IPC.
//
// Channel names are the wire contract with apps/web/lib/desktop.ts —
// change them in both places or not at all.
import { BrowserWindow, app, ipcMain, shell } from "electron";
import { captureItemText, status as captureStatus } from "./capture";
import type { Capabilities } from "./capture/capabilities";
import { captureRegion, coerceRect, type CaptureRect } from "./capture/screen";
import { isAllowlistedUrl } from "./fetchAllowlist";
import { getPriceSnapshot, getPriceStatus, refreshNow, setPriceLeague } from "./prices/scheduler";
import { tradeFetch, tradeSearch } from "./trade/proxy";

// Re-export so callers/tests have one import site (impl is electron-free).
export { isAllowlistedUrl } from "./fetchAllowlist";

export const CHANNELS = {
  itemText: "poc2:item-text", // main → renderer (push)
  captureNow: "poc2:capture-now", // renderer → main (invoke)
  captureStatus: "poc2:capture-status", // renderer → main (invoke)
  openExternal: "poc2:open-external", // renderer → main (invoke)
  tradeSearch: "poc2:trade-search", // renderer → main (invoke)
  tradeFetch: "poc2:trade-fetch", // renderer → main (invoke)
  fetchJson: "poc2:fetch-json", // renderer → main (invoke)
  versions: "poc2:versions", // renderer → main (invoke, sync-ish)
  // --- ADR-0013: region capture + price overlay / calibration ---
  capabilities: "poc2:capabilities", // renderer → main (invoke)
  captureRegion: "poc2:capture-region", // renderer → main (invoke)
  overlayShow: "poc2:overlay-show", // renderer → main (invoke)
  overlayHide: "poc2:overlay-hide", // renderer → main (invoke)
  overlaySetRegion: "poc2:overlay-set-region", // renderer → main (invoke)
  calibrateRegion: "poc2:calibrate-region", // renderer → main (invoke / push-back)
  regionCalibrated: "poc2:region-calibrated", // main → renderer (push)
  overlayState: "poc2:overlay-state", // main → renderer (push: show/hide/degraded)
  // --- poe2scout price cache (hourly poe2scout → node:sqlite) ---
  pricesSnapshot: "poc2:prices-snapshot", // renderer → main (invoke)
  pricesStatus: "poc2:prices-status", // renderer → main (invoke)
  pricesRefresh: "poc2:prices-refresh", // renderer → main (invoke)
  pricesSetLeague: "poc2:prices-set-league", // renderer → main (invoke)
} as const;

/**
 * Window/overlay surface that main.ts owns; injected into `registerIpc` so this
 * module never creates BrowserWindows. The renderer's overlay/calibration
 * channels delegate here. All members are optional-safe (degraded sessions may
 * not have an overlay window at all).
 */
export interface OverlayController {
  /** Current capability gate result (computed once at startup). */
  capabilities(): Capabilities;
  /** Show the click-through overlay (full mode only; no-op when degraded). */
  showOverlay(): void;
  /** Hide the overlay window. */
  hideOverlay(): void;
  /** Reposition the overlay over the given region. */
  setOverlayRegion(rect: CaptureRect): void;
  /** Open the full-screen calibration window. */
  openCalibration(): void;
  /** Persist a calibrated region and notify listeners (called from calibrate). */
  applyCalibration(rect: CaptureRect): void;
  /** Whether the overlay window is currently visible. */
  isOverlayVisible(): boolean;
  /**
   * Briefly toggle the overlay window's visibility. Used to take it out of the
   * way during a region capture so the click-through overlay can't occlude /
   * self-capture its own target (full mode positions it AT the region).
   */
  setOverlayVisible(visible: boolean): void;
}

/** Run a capture and push the result to the window. Used by hotkey + IPC. */
export async function runCapture(win: BrowserWindow, advanced: boolean): Promise<boolean> {
  const result = await captureItemText(advanced);
  if (result.ok) {
    win.webContents.send(CHANNELS.itemText, result.text);
    if (!win.isFocused()) win.flashFrame(true);
    return true;
  }
  return false;
}

export function registerIpc(
  getWindow: () => BrowserWindow | null,
  overlay?: OverlayController,
): void {
  ipcMain.handle(CHANNELS.captureNow, async (_e, advanced: unknown) => {
    const win = getWindow();
    if (!win) return false;
    return runCapture(win, advanced === true);
  });

  ipcMain.handle(CHANNELS.captureStatus, () => ({ ...captureStatus }));

  // --- ADR-0013: capabilities + region capture + overlay/calibration ---

  ipcMain.handle(CHANNELS.capabilities, () => overlay?.capabilities() ?? null);

  ipcMain.handle(CHANNELS.captureRegion, async (_e, rect: unknown) => {
    if (!coerceRect(rect)) {
      return { ok: false as const, reason: "invalid-rect" as const };
    }
    const silent = overlay?.capabilities().silentRegionCapture ?? false;
    // In full mode the overlay window sits AT the capture region, so it would
    // occlude / self-capture its own target. Hide it for the duration of the
    // grab, then restore it so it can render the resulting price plates.
    const wasVisible = overlay?.isOverlayVisible() ?? false;
    if (wasVisible) overlay?.setOverlayVisible(false);
    try {
      return await captureRegion(rect, silent);
    } finally {
      if (wasVisible) overlay?.setOverlayVisible(true);
    }
  });

  ipcMain.handle(CHANNELS.overlayShow, () => {
    overlay?.showOverlay();
    return overlay?.capabilities().overlayMode ?? "degraded";
  });

  ipcMain.handle(CHANNELS.overlayHide, () => {
    overlay?.hideOverlay();
    return true;
  });

  ipcMain.handle(CHANNELS.overlaySetRegion, (_e, rect: unknown) => {
    const parsed = coerceRect(rect);
    if (!parsed) throw new Error("overlaySetRegion: invalid rect");
    overlay?.setOverlayRegion(parsed);
    return true;
  });

  // Calibration is bi-directional: the renderer may *open* the calibrator
  // (no arg) or *report* a calibrated rect back (the drag-select result).
  ipcMain.handle(CHANNELS.calibrateRegion, (_e, rect: unknown) => {
    if (rect === undefined || rect === null) {
      overlay?.openCalibration();
      return true;
    }
    const parsed = coerceRect(rect);
    if (!parsed) throw new Error("calibrateRegion: invalid rect");
    overlay?.applyCalibration(parsed);
    return true;
  });

  ipcMain.handle(CHANNELS.openExternal, async (_e, url: unknown) => {
    if (typeof url !== "string" || !/^https?:\/\//.test(url)) return false;
    await shell.openExternal(url);
    return true;
  });

  ipcMain.handle(CHANNELS.tradeSearch, async (_e, league: unknown, query: unknown) => {
    if (typeof league !== "string") throw new Error("league must be a string");
    return tradeSearch(league, query);
  });

  ipcMain.handle(CHANNELS.tradeFetch, async (_e, ids: unknown, searchId: unknown) => {
    if (!Array.isArray(ids) || typeof searchId !== "string") {
      throw new Error("invalid tradeFetch args");
    }
    return tradeFetch(ids.filter((x): x is string => typeof x === "string"), searchId);
  });

  ipcMain.handle(CHANNELS.fetchJson, async (_e, url: unknown) => {
    if (typeof url !== "string" || !isAllowlistedUrl(url)) {
      throw new Error("fetchJson: URL not allowlisted");
    }
    const { net } = await import("electron");
    const res = await net.fetch(url, { headers: { Accept: "application/json" } });
    if (!res.ok) throw new Error(`fetchJson ${res.status}`);
    return res.json();
  });

  ipcMain.handle(CHANNELS.versions, () => ({
    app: app.getVersion(),
    electron: process.versions.electron ?? "unknown",
    chrome: process.versions.chrome ?? "unknown",
    node: process.versions.node ?? "unknown",
  }));

  // --- poe2scout price cache ---
  ipcMain.handle(CHANNELS.pricesSnapshot, () => getPriceSnapshot());
  ipcMain.handle(CHANNELS.pricesStatus, () => getPriceStatus());
  ipcMain.handle(CHANNELS.pricesRefresh, () => refreshNow());
  ipcMain.handle(CHANNELS.pricesSetLeague, (_e, league: unknown) => {
    if (typeof league !== "string" || !league) throw new Error("pricesSetLeague: league required");
    setPriceLeague(league);
    return true;
  });
}
