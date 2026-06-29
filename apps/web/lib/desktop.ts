/// Renderer-side contract for the optional desktop (Electron) shell. The
/// shell's preload script exposes `window.poc2Desktop`; the web app never
/// imports Electron — it only feature-detects the bridge and consumes this
/// typed surface. Absent bridge ⇒ plain browser (or SSR), everything no-ops.
///
/// Wire contract: apps/desktop/src/preload.ts. Change both or neither.

export interface DesktopCaptureStatus {
  platform: string;
  lastTool: string | null;
  lastError: string | null;
  hotkeyRegistered: boolean;
}

export interface TradeSearchResponse {
  id: string;
  result: string[];
  total: number;
}

export interface Poc2DesktopBridge {
  /** Subscribe to item text captured by the shell. Returns an unsubscribe. */
  onItemText(cb: (text: string) => void): () => void;
  /** Trigger a capture now (game must be focused). True on success. */
  captureNow(advanced?: boolean): Promise<boolean>;
  /** Capture diagnostics for the Settings panel. */
  captureStatus(): Promise<DesktopCaptureStatus>;
  /** Open a URL in the system browser (shell windows stay in-app). */
  openExternal(url: string): void;
  /** trade2 search, proxied through main (rate-limited, no CORS). */
  tradeSearch(league: string, query: unknown): Promise<TradeSearchResponse>;
  /** trade2 fetch for up to 10 result ids. */
  tradeFetch(ids: string[], searchId: string): Promise<unknown>;
  /** GET JSON from an allowlisted host (poe2scout) via main, dodging CORS. */
  fetchJson(url: string): Promise<unknown>;
  /** Shell/runtime versions, for diagnostics. */
  versions(): Promise<Record<string, string>>;
}

declare global {
  interface Window {
    poc2Desktop?: Poc2DesktopBridge;
  }
}

/** The preload bridge, or null in a plain browser (and during SSR). */
export function getDesktopBridge(): Poc2DesktopBridge | null {
  return typeof window === "undefined" ? null : (window.poc2Desktop ?? null);
}

/** True when running inside the desktop shell. */
export function isDesktop(): boolean {
  return getDesktopBridge() !== null;
}
