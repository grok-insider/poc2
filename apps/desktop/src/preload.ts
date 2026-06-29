// Preload: the ONLY surface the renderer sees from the desktop shell.
// Mirrors the contract in apps/web/lib/desktop.ts (window.poc2Desktop).
import { contextBridge, ipcRenderer } from "electron";

const CHANNELS = {
  itemText: "poc2:item-text",
  captureNow: "poc2:capture-now",
  captureStatus: "poc2:capture-status",
  openExternal: "poc2:open-external",
  tradeSearch: "poc2:trade-search",
  tradeFetch: "poc2:trade-fetch",
  fetchJson: "poc2:fetch-json",
  versions: "poc2:versions",
  // --- ADR-0013: region capture + price overlay / calibration ---
  capabilities: "poc2:capabilities",
  captureRegion: "poc2:capture-region",
  overlayShow: "poc2:overlay-show",
  overlayHide: "poc2:overlay-hide",
  overlaySetRegion: "poc2:overlay-set-region",
  calibrateRegion: "poc2:calibrate-region",
  regionCalibrated: "poc2:region-calibrated",
  overlayState: "poc2:overlay-state",
  // --- poe2scout price cache ---
  pricesSnapshot: "poc2:prices-snapshot",
  pricesStatus: "poc2:prices-status",
  pricesRefresh: "poc2:prices-refresh",
  pricesSetLeague: "poc2:prices-set-league",
} as const;

interface CaptureRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

contextBridge.exposeInMainWorld("poc2Desktop", {
  onItemText(cb: (text: string) => void): () => void {
    const listener = (_e: unknown, text: string) => cb(text);
    ipcRenderer.on(CHANNELS.itemText, listener);
    return () => ipcRenderer.removeListener(CHANNELS.itemText, listener);
  },
  captureNow(advanced = false): Promise<boolean> {
    return ipcRenderer.invoke(CHANNELS.captureNow, advanced);
  },
  captureStatus(): Promise<unknown> {
    return ipcRenderer.invoke(CHANNELS.captureStatus);
  },
  openExternal(url: string): void {
    void ipcRenderer.invoke(CHANNELS.openExternal, url);
  },
  tradeSearch(league: string, query: unknown): Promise<unknown> {
    return ipcRenderer.invoke(CHANNELS.tradeSearch, league, query);
  },
  tradeFetch(ids: string[], searchId: string): Promise<unknown> {
    return ipcRenderer.invoke(CHANNELS.tradeFetch, ids, searchId);
  },
  fetchJson(url: string): Promise<unknown> {
    return ipcRenderer.invoke(CHANNELS.fetchJson, url);
  },
  versions(): Promise<Record<string, string>> {
    return ipcRenderer.invoke(CHANNELS.versions);
  },

  // --- ADR-0013 ---
  capabilities(): Promise<unknown> {
    return ipcRenderer.invoke(CHANNELS.capabilities);
  },
  captureRegion(rect: CaptureRect): Promise<unknown> {
    return ipcRenderer.invoke(CHANNELS.captureRegion, rect);
  },
  overlayShow(): Promise<unknown> {
    return ipcRenderer.invoke(CHANNELS.overlayShow);
  },
  overlayHide(): Promise<boolean> {
    return ipcRenderer.invoke(CHANNELS.overlayHide);
  },
  overlaySetRegion(rect: CaptureRect): Promise<boolean> {
    return ipcRenderer.invoke(CHANNELS.overlaySetRegion, rect);
  },
  /** Open the calibrator (no arg) or report a calibrated rect back to main. */
  calibrateRegion(rect?: CaptureRect): Promise<boolean> {
    return ipcRenderer.invoke(CHANNELS.calibrateRegion, rect ?? null);
  },
  /** Subscribe to "a region was calibrated" pushes. Returns an unsubscribe. */
  onRegionCalibrated(cb: (rect: CaptureRect) => void): () => void {
    const listener = (_e: unknown, rect: CaptureRect) => cb(rect);
    ipcRenderer.on(CHANNELS.regionCalibrated, listener);
    return () => ipcRenderer.removeListener(CHANNELS.regionCalibrated, listener);
  },
  /** Subscribe to overlay state pushes (show/hide + degraded signal). */
  onOverlayState(cb: (state: { visible: boolean; degraded: boolean }) => void): () => void {
    const listener = (_e: unknown, state: { visible: boolean; degraded: boolean }) =>
      cb(state);
    ipcRenderer.on(CHANNELS.overlayState, listener);
    return () => ipcRenderer.removeListener(CHANNELS.overlayState, listener);
  },

  // --- poe2scout price cache ---
  /** Flattened price snapshot for the active league (names + normalized→price). */
  pricesSnapshot(): Promise<unknown> {
    return ipcRenderer.invoke(CHANNELS.pricesSnapshot);
  },
  /** Price-cache status (count, fetchedAt, backend, lastError). */
  pricesStatus(): Promise<unknown> {
    return ipcRenderer.invoke(CHANNELS.pricesStatus);
  },
  /** Force an immediate poe2scout refresh. Resolves true if rows were stored. */
  pricesRefresh(): Promise<boolean> {
    return ipcRenderer.invoke(CHANNELS.pricesRefresh);
  },
  /** Point the cache at a league (refreshes now; keeps the hourly cadence). */
  pricesSetLeague(league: string): Promise<boolean> {
    return ipcRenderer.invoke(CHANNELS.pricesSetLeague, league);
  },
});
