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
} as const;

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
});
