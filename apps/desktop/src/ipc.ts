// Single registration point for all main↔renderer IPC.
//
// Channel names are the wire contract with apps/web/lib/desktop.ts —
// change them in both places or not at all.
import { BrowserWindow, app, clipboard, ipcMain, screen, shell } from "electron";
import { captureItemText, status as captureStatus } from "./capture";
import type { Capabilities } from "./capture/capabilities";
import { captureRegion, coerceRect, type CaptureRect } from "./capture/screen";
import { captureRegionWithGrim } from "./capture/grim";
import { isAllowlistedUrl } from "./fetchAllowlist";
import { loadCaptureRegion } from "./windowState";
import { getPriceSnapshot, getPriceStatus, refreshNow, setPriceLeague } from "./prices/scheduler";
import { tradeFetch, tradeSearch } from "./trade/proxy";
import {
  fetchHyprOverlayMenuOutput,
  focusHyprOverlay,
  hideHyprOverlay,
  isInteractiveRegexMenuPayload,
  sendHyprOverlay,
  startHyprOverlayEventSession,
  type HyprOverlayEvent,
  type HyprOverlayEventSession,
  REGEX_OVERLAY_ID,
} from "./capture/hyprOverlay";
import {
  prepareHyprOverlayPriceIcons,
  preparePriceIconDataUrls,
} from "./capture/hyprOverlayIcons";
import { addMarketHistory, listMarketHistory } from "./marketHistory";
import { getScanDiagnostics, setScanDiagnostics } from "./scanDiagnostics";
import type { NativeOcrController } from "./ocr/windowsNative";
import {
  checkForUpdates,
  getUpdateStatus,
  installUpdate,
} from "./updater";

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
  scanRewards: "poc2:scan-rewards", // renderer → main (invoke)
  overlayShow: "poc2:overlay-show", // renderer → main (invoke)
  overlayHide: "poc2:overlay-hide", // renderer → main (invoke)
  overlaySetRegion: "poc2:overlay-set-region", // renderer → main (invoke: capture region notify)
  overlaySetContentBounds: "poc2:overlay-set-content-bounds", // renderer → main (paint window bounds)
  calibrateRegion: "poc2:calibrate-region", // renderer → main (invoke / push-back)
  getCaptureRegion: "poc2:get-capture-region", // renderer → main (invoke: persisted rect)
  regionCalibrated: "poc2:region-calibrated", // main → renderer (push)
  overlayState: "poc2:overlay-state", // main → renderer (push: show/hide/degraded)
  hyprOverlayRender: "poc2:hypr-overlay-render", // renderer → main (invoke)
  hyprOverlayPreparePriceIcons: "poc2:hypr-overlay-prepare-price-icons",
  preparePriceIconDataUrls: "poc2:prepare-price-icon-data-urls",
  hyprOverlayEvent: "poc2:hypr-overlay-event", // main → renderer (push)
  rewardWatcher: "poc2:reward-watcher",
  rewardWatcherStatus: "poc2:reward-watcher-status",
  clipboardWrite: "poc2:clipboard-write", // renderer → main (invoke)
  marketHistoryAdd: "poc2:market-history-add", // renderer → main (invoke)
  marketHistoryList: "poc2:market-history-list", // renderer → main (invoke)
  scanDiagnosticsGet: "poc2:scan-diagnostics-get", // renderer → main (invoke)
  scanDiagnosticsSet: "poc2:scan-diagnostics-set", // renderer → main (invoke)
  nativeOcrRecognize: "poc2:native-ocr-recognize", // renderer → main (invoke)
  nativeOcrStatus: "poc2:native-ocr-status", // renderer → main (invoke)
  // --- poe2scout price cache (hourly poe2scout → node:sqlite) ---
  pricesSnapshot: "poc2:prices-snapshot", // renderer → main (invoke)
  pricesStatus: "poc2:prices-status", // renderer → main (invoke)
  pricesRefresh: "poc2:prices-refresh", // renderer → main (invoke)
  pricesSetLeague: "poc2:prices-set-league", // renderer → main (invoke)
  // --- desktop auto-updater (GitHub Releases / electron-updater) ---
  updatesStatus: "poc2:updates-status", // renderer → main (invoke)
  updatesCheck: "poc2:updates-check", // renderer → main (invoke)
  updatesInstall: "poc2:updates-install", // renderer → main (invoke)
  updatesState: "poc2:updates-state", // main → renderer (push)
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
  /** Trigger one reward OCR scan through the active overlay transport. */
  scanRewards(): void;
  setRewardWatcher(enabled: boolean): void;
  rewardWatcherEnabled(): boolean;
  /** Hide the overlay window. */
  hideOverlay(): void;
  /**
   * Notify the overlay renderer of the OCR capture region.
   * Does not place the paint window on the capture rect (that caused self-occlusion).
   */
  setOverlayRegion(rect: CaptureRect): void;
  /** Position the Electron full-mode paint window (marker strip / stack panel). */
  setOverlayContentBounds(rect: CaptureRect): void;
  /** Open the full-screen calibration window. */
  openCalibration(): void;
  /** Persist a calibrated region and notify listeners (called from calibrate). */
  applyCalibration(rect: CaptureRect): void;
  /** Whether the overlay window is currently visible. */
  isOverlayVisible(): boolean;
  /**
   * Briefly toggle the overlay window's visibility. Used to take it out of the
   * way during a region capture so the click-through paint window can't occlude
   * the calibrated OCR target (content bounds sit beside the region).
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

let regexEventSession: HyprOverlayEventSession | null = null;
let regexEventSessionStarting: Promise<void> | null = null;

/** Stop the interactive Search Regex hyproverlay event subscription. */
export function stopRegexOverlayEventSession(): void {
  regexEventSession?.close();
  regexEventSession = null;
  regexEventSessionStarting = null;
}

function broadcastHyprOverlayEvent(event: HyprOverlayEvent): void {
  for (const win of BrowserWindow.getAllWindows()) {
    try {
      win.webContents.send(CHANNELS.hyprOverlayEvent, event);
    } catch {
      // window may be destroying
    }
  }
}

async function ensureRegexOverlayEventSession(overlay?: OverlayController): Promise<void> {
  const caps = overlay?.capabilities().hyprOverlay;
  if (!caps?.capabilities.includes("menu.interactive")) return;
  if (regexEventSession) return;
  if (regexEventSessionStarting) {
    await regexEventSessionStarting;
    return;
  }

  regexEventSessionStarting = (async () => {
    try {
      const session = await startHyprOverlayEventSession(REGEX_OVERLAY_ID, {
        onEvent: (event) => {
          void (async () => {
            let enriched: HyprOverlayEvent = event;
            // Hyprland truncates event JSON at 1024 bytes; recover full selection.
            if (event.selectedIdsTruncated) {
              const out = await fetchHyprOverlayMenuOutput();
              if (out?.selected) {
                enriched = {
                  ...event,
                  selectedIds: out.selected.map((row) => row.id),
                  selectedIdsTruncated: false,
                  ...(out.activeTab ? { activeTab: out.activeTab } : {}),
                  ...(typeof out.focusIndex === "number"
                    ? { focusIndex: out.focusIndex }
                    : {}),
                };
              }
            }
            broadcastHyprOverlayEvent(enriched);
          })();
        },
      });
      regexEventSession = session;
    } catch {
      regexEventSession = null;
    } finally {
      regexEventSessionStarting = null;
    }
  })();

  await regexEventSessionStarting;
}

export function registerIpc(
  getWindow: () => BrowserWindow | null,
  overlay?: OverlayController,
  nativeOcr?: NativeOcrController,
): void {
  ipcMain.handle(CHANNELS.captureNow, async (_e, advanced: unknown) => {
    const win = getWindow();
    if (!win) return false;
    return runCapture(win, advanced === true);
  });

  ipcMain.handle(CHANNELS.captureStatus, () => ({ ...captureStatus }));

  // --- ADR-0013: capabilities + region capture + overlay/calibration ---

  ipcMain.handle(CHANNELS.capabilities, () => overlay?.capabilities() ?? null);
  ipcMain.handle(CHANNELS.scanRewards, () => {
    overlay?.scanRewards();
    return true;
  });

  ipcMain.handle(CHANNELS.captureRegion, async (_e, rect: unknown, preserveOverlay: unknown) => {
    const parsedRect = coerceRect(rect);
    if (!parsedRect) {
      return { ok: false as const, reason: "invalid-rect" as const };
    }
    const caps = overlay?.capabilities();
    const silent = caps?.silentRegionCapture ?? false;
    // Full mode paints beside the capture region; still hide the window so a
    // large strip or mis-docked panel cannot contaminate the OCR grab.
    const wasVisible = overlay?.isOverlayVisible() ?? false;
    if (wasVisible) overlay?.setOverlayVisible(false);
    if (caps?.overlayMode === "hyprland-plugin" && preserveOverlay !== true) {
      await hideHyprOverlay();
    }
    try {
      const result = caps?.captureBackend === "grim"
        ? await captureRegionWithGrim(parsedRect)
        : await captureRegion(parsedRect, silent);
      if (!result.ok) return result;
      const bounds = screen.getDisplayMatching(parsedRect).bounds;
      return {
        ...result,
        displayBounds: {
          x: bounds.x,
          y: bounds.y,
          width: bounds.width,
          height: bounds.height,
        },
      };
    } finally {
      if (wasVisible) overlay?.setOverlayVisible(true);
    }
  });

  ipcMain.handle(CHANNELS.overlayShow, () => {
    overlay?.showOverlay();
    return overlay?.capabilities().overlayMode ?? "degraded";
  });

  ipcMain.handle(CHANNELS.overlayHide, () => {
    stopRegexOverlayEventSession();
    overlay?.hideOverlay();
    return true;
  });

  ipcMain.handle(CHANNELS.hyprOverlayRender, async (_e, payload: unknown) => {
    const interactiveRegex = isInteractiveRegexMenuPayload(payload);
    if (interactiveRegex) {
      await ensureRegexOverlayEventSession(overlay);
    } else {
      // Cards / non-interactive menus end the regex interaction session.
      stopRegexOverlayEventSession();
    }
    const ok = await sendHyprOverlay(payload as Parameters<typeof sendHyprOverlay>[0]);
    if (ok && interactiveRegex) {
      // Keyboard capture requires plugin menu focus (click also sets this).
      void focusHyprOverlay();
    }
    return ok;
  });
  ipcMain.handle(CHANNELS.hyprOverlayPreparePriceIcons, async () => {
    const caps = overlay?.capabilities().hyprOverlay;
    if (!caps?.capabilities.includes("images.rgba")) return {};
    return prepareHyprOverlayPriceIcons(getPriceSnapshot().unitIcons);
  });
  ipcMain.handle(CHANNELS.preparePriceIconDataUrls, async () => {
    // Electron full-mode marker paint; decorative only.
    return preparePriceIconDataUrls(getPriceSnapshot().unitIcons);
  });
  ipcMain.handle(CHANNELS.rewardWatcher, (_e, enabled: unknown) => {
    overlay?.setRewardWatcher(enabled === true);
    return overlay?.rewardWatcherEnabled() ?? false;
  });
  ipcMain.handle(CHANNELS.rewardWatcherStatus, () =>
    overlay?.rewardWatcherEnabled() ?? false,
  );

  ipcMain.handle(CHANNELS.clipboardWrite, (_e, text: unknown) => {
    if (typeof text !== "string") return false;
    clipboard.writeText(text);
    return true;
  });

  ipcMain.handle(CHANNELS.marketHistoryAdd, (_e, entry: unknown) => {
    if (!entry || typeof entry !== "object") throw new Error("marketHistoryAdd: entry object required");
    return addMarketHistory(entry as Parameters<typeof addMarketHistory>[0]);
  });

  ipcMain.handle(CHANNELS.marketHistoryList, () => listMarketHistory());
  ipcMain.handle(CHANNELS.scanDiagnosticsGet, () => getScanDiagnostics());
  ipcMain.handle(CHANNELS.scanDiagnosticsSet, (_e, diagnostics: unknown) =>
    setScanDiagnostics(diagnostics),
  );
  ipcMain.handle(CHANNELS.nativeOcrRecognize, async (_e, dataUrl: unknown, language: unknown) => {
    if (typeof dataUrl !== "string") throw new Error("nativeOcrRecognize: data URL required");
    try {
      return await nativeOcr?.recognize(
        dataUrl,
        typeof language === "string" ? language : undefined,
      ) ?? null;
    } catch {
      return null;
    }
  });
  ipcMain.handle(CHANNELS.nativeOcrStatus, () => nativeOcr?.status() ?? {
    available: false,
    backend: "windows-media-ocr",
    helperPath: null,
    lastError: null,
  });

  ipcMain.handle(CHANNELS.overlaySetRegion, (_e, rect: unknown) => {
    const parsed = coerceRect(rect);
    if (!parsed) throw new Error("overlaySetRegion: invalid rect");
    overlay?.setOverlayRegion(parsed);
    return true;
  });

  ipcMain.handle(CHANNELS.overlaySetContentBounds, (_e, rect: unknown) => {
    const parsed = coerceRect(rect);
    if (!parsed) throw new Error("overlaySetContentBounds: invalid rect");
    overlay?.setOverlayContentBounds(parsed);
    return true;
  });

  // Persisted-region hydration: the overlay route pulls the calibrated
  // rect on mount so the FIRST hotkey scan works even when the overlay
  // window was created after calibration (the push-only path raced it).
  ipcMain.handle(CHANNELS.getCaptureRegion, () => loadCaptureRegion().rect ?? null);

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
    return setPriceLeague(league);
  });

  // --- desktop auto-updater ---
  ipcMain.handle(CHANNELS.updatesStatus, () => getUpdateStatus());
  ipcMain.handle(CHANNELS.updatesCheck, () => checkForUpdates());
  ipcMain.handle(CHANNELS.updatesInstall, () => installUpdate());
}
