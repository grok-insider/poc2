// Single registration point for all main↔renderer IPC.
//
// Channel names are the wire contract with apps/web/lib/desktop.ts —
// change them in both places or not at all.
import { BrowserWindow, app, ipcMain, shell } from "electron";
import { captureItemText, status as captureStatus } from "./capture";
import { tradeFetch, tradeSearch } from "./trade/proxy";

export const CHANNELS = {
  itemText: "poc2:item-text", // main → renderer (push)
  captureNow: "poc2:capture-now", // renderer → main (invoke)
  captureStatus: "poc2:capture-status", // renderer → main (invoke)
  openExternal: "poc2:open-external", // renderer → main (invoke)
  tradeSearch: "poc2:trade-search", // renderer → main (invoke)
  tradeFetch: "poc2:trade-fetch", // renderer → main (invoke)
  fetchJson: "poc2:fetch-json", // renderer → main (invoke)
  versions: "poc2:versions", // renderer → main (invoke, sync-ish)
} as const;

/** Hosts the renderer may fetch JSON from via main (CORS bypass). */
const FETCH_ALLOWLIST = ["poe2scout.com", "www.pathofexile.com"];

export function isAllowlistedUrl(raw: string): boolean {
  try {
    const u = new URL(raw);
    return u.protocol === "https:" && FETCH_ALLOWLIST.includes(u.hostname);
  } catch {
    return false;
  }
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

export function registerIpc(getWindow: () => BrowserWindow | null): void {
  ipcMain.handle(CHANNELS.captureNow, async (_e, advanced: unknown) => {
    const win = getWindow();
    if (!win) return false;
    return runCapture(win, advanced === true);
  });

  ipcMain.handle(CHANNELS.captureStatus, () => ({ ...captureStatus }));

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
}
