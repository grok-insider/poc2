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
  overlayMode: "full" | "degraded" | "hyprland-plugin";
  /** Classified session, for diagnostics + fallback copy. */
  sessionKind:
    | "win32"
    | "linux-x11"
    | "linux-wayland-wlroots"
    | "linux-wayland-other";
  /** Region-selection surface selected by the desktop capability gate. */
  regionPicker: "electron" | "slurp";
  captureBackend: "electron" | "portal" | "grim";
  hyprOverlay?: HyprOverlayStatus | null;
}

/** Raw cropped frame from a region capture; preprocessing is renderer-side. */
export type CaptureRegionResult =
  | {
      ok: true;
      dataUrl: string;
      width: number;
      height: number;
      displayBounds?: CaptureRect;
    }
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
  /** Active overlay transport, when supplied by newer desktop shells. */
  mode?: DesktopCapabilities["overlayMode"];
  /** Command for the hidden overlay worker. */
  action?:
    | "reward-scan"
    | "reward-watch-start"
    | "reward-watch-stop"
    | "price-check"
    | "regex-open"
    | "regex-next"
    | "regex-prev"
    | "regex-tab-next"
    | "regex-tab-prev"
    | "regex-toggle"
    | "regex-copy"
    | "regex-apply";
  itemText?: string;
  error?: string;
}

export interface HyprOverlayPayload {
  mode?: "cards" | "menu" | "selection";
  visible?: boolean;
  rect: { x: number; y: number; w: number; h: number };
  ttlMs?: number;
  style?: {
    font?: string;
    fontSize?: number;
    radius?: number;
    padding?: number;
    gap?: number;
    background?: string;
    border?: string;
    text?: string;
    muted?: string;
    accent?: string;
  };
  rows?: Array<{
    kind?: "text" | "header" | "separator" | "columns";
    label?: string;
    value?: string;
    detail?: string;
    emphasis?: boolean;
    color?: string;
    valueColor?: string;
    detailColor?: string;
    bg?: string;
    size?: number;
    align?: "left" | "center" | "right";
    top?: number;
    height?: number;
    iconId?: string;
    iconSize?: number;
    iconGap?: number;
    cells?: Array<{
      text: string;
      color?: string;
      bg?: string;
      size?: number;
      align?: "left" | "center" | "right";
      weight?: number;
    }>;
  }>;
  menu?: {
    title?: string;
    subtitle?: string;
    footer?: string;
    preview?: string;
    budget?: string;
    activeTab?: string;
    focusIndex?: number;
    tabs?: Array<{ id: string; label: string }>;
    controls?: Array<{
      id: string;
      tab?: string;
      label: string;
      value?: string;
      detail?: string;
      kind?: "toggle" | "cycle" | "action";
      selected?: boolean;
      disabled?: boolean;
    }>;
  };
  interactive?: {
    enabled?: boolean;
    pointer?: boolean;
    keyboard?: boolean;
    text?: boolean;
    passthroughOutside?: boolean;
    dismissOnOutside?: boolean;
    overlayId?: string;
  };
  selection?: {
    draft?: { x: number; y: number; w: number; h: number };
    border?: string;
    borderWidth?: number;
    hint?: string;
    hintColor?: string;
    hintSize?: number;
  };
}

export interface HyprOverlayStatus {
  loaded: boolean;
  protocolVersion: number | null;
  capabilities: string[];
  limits: Record<string, number>;
  images?: { count: number; bytes: number };
}

/** A single resolved unit price from the cache. */
export interface PriceInfo {
  perUnit: number;
  unit: string;
  perUnitDivine: number;
  perUnitExalt: number | null;
}

/**
 * Flattened poe2scout price snapshot. `names` feeds the OCR matcher as ad-hoc
 * fuzzy `candidates`; `byName` maps `normalizeName(name)` → price.
 */
export interface PriceSnapshot {
  league: string;
  names: string[];
  byName: Record<string, PriceInfo>;
  unitIcons: { div?: string; ex?: string };
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

export interface OverlayMarketHistoryEntry {
  id: string;
  kind: "item-price" | "reward-scan";
  createdAt: string;
  title: string;
  league?: string;
  summary: string;
  rows: Array<{ label: string; value?: string; detail?: string }>;
}

export interface ScanDiagnostics {
  updatedAt: string;
  transport: DesktopCapabilities["overlayMode"];
  captureWidth?: number;
  captureHeight?: number;
  selectedCrop?: number;
  selectedScale?: number;
  ocrBackend?: "windows-media-ocr" | "tesseract-fast" | "tesseract-fallback";
  captureMs?: number;
  decodeMs?: number;
  fastOcrMs?: number;
  fallbackOcrMs?: number;
  totalMs?: number;
  rawText?: string;
  rawRows?: string[];
  resolvedRows?: string[];
  lineRows?: string[];
  pluginProtocol?: number;
  pluginCapabilities?: string[];
  renderOk?: boolean;
  watcherEnabled?: boolean;
  error?: string;
}

export interface NativeOcrResult {
  text: string;
  lines: Array<{
    text: string;
    confidence: number;
    boundingBox: { x: number; y: number; width: number; height: number };
  }>;
}

export interface NativeOcrStatus {
  available: boolean;
  backend: "windows-media-ocr";
  helperPath: string | null;
  lastError: string | null;
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
  captureRegion(rect: CaptureRect, preserveCompositorOverlay?: boolean): Promise<CaptureRegionResult>;
  /** Trigger one reward OCR scan. */
  scanRewards(): Promise<boolean>;
  /** Show the active overlay path; returns the active overlay mode. */
  overlayShow(): Promise<DesktopCapabilities["overlayMode"]>;
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
  /** Send already-computed rows to the Hyprland compositor overlay plugin. */
  hyprOverlayRender(payload: HyprOverlayPayload): Promise<boolean>;
  /** Fetch/register Divine and Exalted images from the active price snapshot. */
  hyprOverlayPreparePriceIcons(): Promise<Record<string, string>>;
  /** Enable or disable continuous reward-panel monitoring. */
  rewardWatcher(enabled: boolean): Promise<boolean>;
  rewardWatcherStatus(): Promise<boolean>;
  /** Write text through Electron main; used by hidden overlay workers. */
  clipboardWrite(text: string): Promise<boolean>;
  /** Persist a compact overlay market result for desktop history/review. */
  marketHistoryAdd(
    entry: Omit<OverlayMarketHistoryEntry, "id" | "createdAt">,
  ): Promise<OverlayMarketHistoryEntry>;
  /** Read persisted overlay market results. */
  marketHistoryList(): Promise<OverlayMarketHistoryEntry[]>;
  /** Last OCR scan details for local troubleshooting. */
  scanDiagnostics(): Promise<ScanDiagnostics | null>;
  /** Update local troubleshooting details from the hidden OCR worker. */
  scanDiagnosticsSet(diagnostics: ScanDiagnostics): Promise<ScanDiagnostics | null>;
  /** Windows.Media.Ocr fast path. Null means unavailable or failed; use Tesseract. */
  nativeOcrRecognize?(dataUrl: string, language?: string): Promise<NativeOcrResult | null>;
  nativeOcrStatus?(): Promise<NativeOcrStatus>;

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
