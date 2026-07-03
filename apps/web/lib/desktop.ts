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

/** A screen rectangle in global logical (CSS) pixels. */
export interface CaptureRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

/** Compositor capability gate result (ADR-0013). */
export interface DesktopCapabilities {
  /** Region capture can be taken without a per-grab permission prompt. */
  silentRegionCapture: boolean;
  /** Whether a real click-through overlay window is usable. */
  overlayMode: "full" | "degraded";
  /** Classified session, for diagnostics + fallback copy. */
  sessionKind:
    | "win32"
    | "linux-x11"
    | "linux-wayland-wlroots"
    | "linux-wayland-other";
}

/** Raw cropped frame from a region capture; preprocessing is renderer-side. */
export type CaptureRegionResult =
  | { ok: true; dataUrl: string; width: number; height: number }
  | {
      ok: false;
      reason: "invalid-rect" | "no-display" | "portal-denied" | "capture-failed";
      message?: string;
    };

/** Overlay window state pushed from main (show/hide + degraded signal). */
export interface OverlayState {
  visible: boolean;
  /** True ⇒ no click-through window exists; render the in-app panel instead. */
  degraded: boolean;
}

/** A single resolved unit price from the cache. */
export interface PriceInfo {
  perUnit: number;
  unit: string;
}

/**
 * Flattened poe2scout price snapshot. `names` feeds the OCR matcher as ad-hoc
 * fuzzy `candidates`; `byName` maps `normalizeName(name)` → price.
 */
export interface PriceSnapshot {
  league: string;
  names: string[];
  byName: Record<string, PriceInfo>;
  fetchedAt: string | null;
}

/** poe2scout price-cache status surface. */
export interface PriceStatus {
  league: string;
  count: number;
  fetchedAt: string | null;
  lastError: string | null;
  refreshing: boolean;
  backend: string;
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
  /** GET JSON from an allowlisted host (poe2scout, poe.ninja) via main, dodging CORS. */
  fetchJson(url: string): Promise<unknown>;
  /** Shell/runtime versions, for diagnostics. */
  versions(): Promise<Record<string, string>>;

  // --- ADR-0013: region capture + price overlay / calibration ---

  /** Compositor capability gate; null only if the shell predates ADR-0013. */
  capabilities(): Promise<DesktopCapabilities | null>;
  /** Capture a screen rectangle; raw cropped frame (preprocess renderer-side). */
  captureRegion(rect: CaptureRect): Promise<CaptureRegionResult>;
  /** Show the click-through overlay (full mode); returns the active overlay mode. */
  overlayShow(): Promise<"full" | "degraded">;
  /** Hide the overlay window. */
  overlayHide(): Promise<boolean>;
  /** Reposition the overlay over a screen region. */
  overlaySetRegion(rect: CaptureRect): Promise<boolean>;
  /** Open the calibrator (no arg) or report a calibrated rect back to main. */
  calibrateRegion(rect?: CaptureRect): Promise<boolean>;
  /** The persisted calibrated region, or null. Optional — absent on
   * pre-hydration shells; the overlay pulls it on mount so the FIRST
   * scan doesn't race the calibration push. */
  getCaptureRegion?(): Promise<CaptureRect | null>;
  /** Subscribe to "a region was calibrated" pushes. Returns an unsubscribe. */
  onRegionCalibrated(cb: (rect: CaptureRect) => void): () => void;
  /** Subscribe to overlay state pushes (show/hide + degraded). */
  onOverlayState(cb: (state: OverlayState) => void): () => void;

  // --- poe2scout price cache (hourly poe2scout → node:sqlite) ---

  /** Flattened price snapshot for the active league. */
  pricesSnapshot(): Promise<PriceSnapshot>;
  /** Price-cache status (count, fetchedAt, backend, lastError). */
  pricesStatus(): Promise<PriceStatus>;
  /** Force an immediate poe2scout refresh; true if rows were stored. */
  pricesRefresh(): Promise<boolean>;
  /** Point the cache at a league (refreshes now; keeps the hourly cadence). */
  pricesSetLeague(league: string): Promise<boolean>;
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
