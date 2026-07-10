// Path of Crafting 2 — desktop shell entry (ADR-0010 + ADR-0013).
//
// A normal windowed app (like Discord) around the apps/web static export:
//   - serves the export over a privileged app:// scheme (root-absolute
//     asset paths keep working; no localhost server)
//   - registers the capture hotkey (global where the platform allows;
//     a second-instance `--capture` flag is the compositor-bind fallback
//     on Wayland: bind = ALT, C, exec, poc2-desktop --capture)
//   - proxies the trade2 API with centralized rate limiting
//   - (ADR-0013) hosts a capability-gated price OVERLAY window + a full-screen
//     CALIBRATION window for the screen-region OCR capture flow. The overlay is
//     a transparent click-through Electron window (NOT a layer-shell surface;
//     ADR-0009 stays deferred). On compositors that can't do click-through
//     overlays (Hyprland/wlroots, probe-fail Wayland) we signal the renderer to
//     show an in-app panel instead — "degraded" mode.
import { app, BrowserWindow, globalShortcut, Menu, nativeImage, Tray } from "electron";
import { existsSync } from "node:fs";
import path from "node:path";
import { captureItemText, status as captureStatus } from "./capture";
import {
  type Capabilities,
  detectCapabilities,
} from "./capture/capabilities";
import { probeOverlaySupport } from "./capture/overlayProbe";
import {
  detectHyprOverlay,
  getHyprOverlayStatus,
  hideHyprOverlay,
  sendHyprOverlaySelection,
  startHyprOverlaySelectionListener,
  virtualDesktopBounds,
} from "./capture/hyprOverlay";
import { selectSlurpRegion } from "./capture/calibration";
import {
  isOverlayRendererReady,
  showElectronBeforeCommand,
  type OverlayCommandAction,
} from "./capture/overlayPolicy";
import type { CaptureRect } from "./capture/screen";
import {
  CHANNELS,
  type OverlayController,
  registerIpc,
  runCapture,
  stopRegexOverlayEventSession,
} from "./ipc";
import {
  createWindowsOcrController,
  type NativeOcrController,
} from "./ocr/windowsNative";
import { startPriceScheduler } from "./prices/scheduler";
import { APP_ORIGIN, handleAppScheme, registerAppScheme } from "./serve";
import {
  loadCaptureRegion,
  loadWindowState,
  saveCaptureRegion,
  trackWindowState,
} from "./windowState";

// userData dir name ("poc2-desktop", not the scoped npm package name).
app.setName("poc2-desktop");

const DEV = process.argv.includes("--dev");
const DEV_URL = process.env.POC2_DEV_URL ?? "http://localhost:3000";
const HOTKEY = process.env.POC2_CAPTURE_HOTKEY ?? "Alt+C";
const PRICE_HOTKEY = process.env.POC2_PRICE_HOTKEY ?? "Alt+E";
const SCAN_HOTKEY = process.env.POC2_SCAN_HOTKEY ?? "Alt+V";
const WATCHER_HOTKEY = process.env.POC2_WATCHER_HOTKEY ?? "Alt+Shift+V";
const RECALIBRATE_HOTKEY =
  process.env.POC2_RECALIBRATE_HOTKEY ?? "Alt+L";
const REGEX_HOTKEY = process.env.POC2_REGEX_HOTKEY ?? "Alt+F";
const REGEX_COPY_HOTKEY = process.env.POC2_REGEX_COPY_HOTKEY ?? "Alt+Shift+F";

const DEV_BASE = () => (DEV ? DEV_URL : APP_ORIGIN);

let mainWindow: BrowserWindow | null = null;
let overlayWindow: BrowserWindow | null = null;
let overlayRendererReady = false;
let calibrationWindow: BrowserWindow | null = null;
let tray: Tray | null = null;
let capabilities: Capabilities | null = null;
let nativeOcr: NativeOcrController | null = null;
let rewardWatcherEnabled = false;
let isQuitting = false;
const pendingInstanceCommands: string[][] = [];

// Single instance: a second `poc2-desktop --capture` invocation forwards to
// the running app — the Wayland/Hyprland hotkey path (ADR-0010).
const gotLock = app.requestSingleInstanceLock();
if (!gotLock) {
  app.quit();
} else {
  registerAppScheme();

  app.on("second-instance", (_e, argv) => {
    if (!app.isReady() || capabilities === null) {
      pendingInstanceCommands.push(argv);
      return;
    }
    if (argv.includes("--capture")) {
      void runCapture(ensureMainWindow(), argv.includes("--advanced"));
      return;
    }
    if (argv.includes("--price-check")) {
      void triggerPriceCheck();
      return;
    }
    if (argv.includes("--scan")) {
      triggerScan();
      return;
    }
    if (argv.includes("--scan-rewards")) {
      triggerScan();
      return;
    }
    if (argv.includes("--watch-rewards")) {
      overlayController.setRewardWatcher(!rewardWatcherEnabled);
      return;
    }
    if (argv.includes("--stop-watching-rewards")) {
      overlayController.setRewardWatcher(false);
      return;
    }
    if (argv.includes("--recalibrate")) {
      overlayController.openCalibration();
      return;
    }
    if (argv.includes("--regex-open")) {
      triggerRegex("regex-open");
      return;
    }
    if (argv.includes("--regex-next")) {
      triggerRegex("regex-next");
      return;
    }
    if (argv.includes("--regex-prev")) {
      triggerRegex("regex-prev");
      return;
    }
    if (argv.includes("--regex-tab-next")) {
      triggerRegex("regex-tab-next");
      return;
    }
    if (argv.includes("--regex-tab-prev")) {
      triggerRegex("regex-tab-prev");
      return;
    }
    if (argv.includes("--regex-toggle")) {
      triggerRegex("regex-toggle");
      return;
    }
    if (argv.includes("--regex-copy")) {
      triggerRegex("regex-copy");
      return;
    }
    if (argv.includes("--regex-apply")) {
      triggerRegex("regex-apply");
      return;
    }
    if (argv.includes("--overlay-hide")) {
      overlayController.hideOverlay();
      return;
    }
    focusMainWindow();
  });

  app.whenReady().then(async () => {
    handleAppScheme();

    // Capability gate runs once at startup. The GNOME/KDE-Wayland branch gets a
    // real runtime overlay probe; every other session decides from env alone.
    capabilities = await detectCapabilities({
      probeOverlay: probeOverlaySupport,
      probeHyprOverlay: detectHyprOverlay,
    });
    if (capabilities.overlayMode === "hyprland-plugin") {
      const status = await getHyprOverlayStatus();
      capabilities.hyprOverlay = status
        ? {
            loaded: status.loaded,
            protocolVersion: status.protocolVersion,
            capabilities: status.capabilities,
            limits: status.limits as Record<string, number>,
            images: status.images,
          }
        : null;
    }

    nativeOcr = createWindowsOcrController({
      platform: process.platform,
      resourcesPath: process.resourcesPath,
      appPath: app.getAppPath(),
      override: process.env.POC2_WINDOWS_OCR_PATH,
    });
    registerIpc(() => mainWindow, overlayController, nativeOcr);
    createWindow();
    createTray();

    // poe2scout price cache: refresh on startup + hourly. League is empty here
    // (auto-detected from poe2scout's IsCurrent); the renderer syncs the user's
    // chosen league via pricesSetLeague once the store hydrates.
    startPriceScheduler(app.getPath("userData"), "");

    // Global hotkeys: native on Windows/X11; on Wayland these need the
    // GlobalShortcuts portal (run with --enable-features=GlobalShortcutsPortal
    // --ozone-platform=wayland) — otherwise users bind `--capture`/`--scan`.
    registerHotkeys();
    dispatchInitialCommand(process.argv);
    for (const argv of pendingInstanceCommands.splice(0)) dispatchInitialCommand(argv);

    app.on("activate", () => {
      if (BrowserWindow.getAllWindows().length === 0) createWindow();
    });
  });

  app.on("before-quit", () => {
    isQuitting = true;
  });
  app.on("will-quit", () => {
    globalShortcut.unregisterAll();
    nativeOcr?.stop();
    nativeOcr = null;
    tray?.destroy();
    tray = null;
  });
  app.on("window-all-closed", () => {
    if (isQuitting || !tray) app.quit();
  });
}

function dispatchInitialCommand(argv: string[]): void {
  if (argv.includes("--capture")) {
    void runCapture(ensureMainWindow(), argv.includes("--advanced"));
  } else if (argv.includes("--price-check")) {
    void triggerPriceCheck();
  } else if (argv.includes("--scan") || argv.includes("--scan-rewards")) {
    triggerScan();
  } else if (argv.includes("--watch-rewards")) {
    overlayController.setRewardWatcher(true);
  } else if (argv.includes("--recalibrate")) {
    overlayController.openCalibration();
  } else if (argv.includes("--regex-open")) {
    triggerRegex("regex-open");
  }
}

function registerHotkeys(): void {
  try {
    captureStatus.hotkeyRegistered = globalShortcut.register(HOTKEY, () => {
      if (mainWindow) void runCapture(mainWindow, false);
    });
    globalShortcut.register(PRICE_HOTKEY, () => void triggerPriceCheck());
    // Scan = single OCR pass over the calibrated region.
    globalShortcut.register(SCAN_HOTKEY, () => triggerScan());
    globalShortcut.register(WATCHER_HOTKEY, () =>
      overlayController.setRewardWatcher(!rewardWatcherEnabled),
    );
    // Recalibrate = open the full-screen calibrator.
    globalShortcut.register(RECALIBRATE_HOTKEY, () => overlayController.openCalibration());
    globalShortcut.register(REGEX_HOTKEY, () => triggerRegex("regex-open"));
    globalShortcut.register(REGEX_COPY_HOTKEY, () => triggerRegex("regex-copy"));
  } catch {
    captureStatus.hotkeyRegistered = false;
  }
}

function resolveTrayIconPath(): string {
  const iconName = process.platform === "win32" ? "icon.ico" : "icon.png";
  const fallback = path.join(__dirname, "..", "build", iconName);
  const candidates = [
    fallback,
    path.join(app.getAppPath(), "build", iconName),
    path.join(process.resourcesPath, "build", iconName),
    path.join(process.resourcesPath, iconName),
  ];
  return candidates.find((candidate) => existsSync(candidate)) ?? fallback;
}

function buildTrayMenu(): Menu {
  return Menu.buildFromTemplate([
    { label: "Show Path of Crafting 2", click: () => focusMainWindow() },
    { type: "separator" },
    { label: "Capture Item", click: () => void runCapture(ensureMainWindow(), false) },
    { label: "Price Check", click: () => void triggerPriceCheck() },
    { label: "Scan Rewards", click: () => triggerScan() },
    {
      label: rewardWatcherEnabled ? "Stop Reward Watcher" : "Start Reward Watcher",
      click: () => overlayController.setRewardWatcher(!rewardWatcherEnabled),
    },
    { label: "Calibrate OCR Region", click: () => overlayController.openCalibration() },
    { label: "Hide Overlay", click: () => overlayController.hideOverlay() },
    { type: "separator" },
    {
      label: "Quit",
      click: () => {
        isQuitting = true;
        app.quit();
      },
    },
  ]);
}

function createTray(): void {
  if (tray) return;

  const image = nativeImage.createFromPath(resolveTrayIconPath());
  tray = new Tray(image);
  tray.setToolTip("Path of Crafting 2");
  tray.setContextMenu(buildTrayMenu());
  tray.on("click", () => focusMainWindow());
}

/**
 * One scan: position + reveal the overlay (full mode) or push the degraded
 * signal (in-app panel), then tell the renderer to run the OCR pass. The actual
 * capture + OCR is renderer-driven via `captureRegion`; here we just surface the
 * UI and forward the trigger.
 */
function triggerScan(): void {
  const rect = loadCaptureRegion().rect;
  if (capabilities?.overlayMode === "full" && rect) {
    overlayController.setOverlayRegion(rect);
  } else if (capabilities?.overlayMode === "hyprland-plugin") {
    if (rect) overlayController.setOverlayRegion(rect);
  }
  sendOverlayCommand("reward-scan");
}

async function openHyprlandCalibration(): Promise<boolean> {
  const status = capabilities?.hyprOverlay;
  if (!status?.capabilities.includes("selection.dragConfirm")) return false;
  const bounds = await virtualDesktopBounds();
  if (!bounds) return false;
  const overlayId = `poc2-calibrate-${Date.now()}`;
  let listener: Awaited<ReturnType<typeof startHyprOverlaySelectionListener>>;
  try {
    listener = await startHyprOverlaySelectionListener(overlayId, { timeoutMs: 120_000 });
  } catch {
    return false;
  }
  const prior = loadCaptureRegion().rect;
  const draft = prior
    ? {
        x: prior.x - bounds.x,
        y: prior.y - bounds.y,
        w: prior.width,
        h: prior.height,
      }
    : undefined;
  unregisterEscToHide();
  const sent = await sendHyprOverlaySelection({
    visible: true,
    rect: bounds,
    ttlMs: 0,
    style: {
      background: "#00000066",
      border: "#00000000",
      text: "#f4f0e6ff",
      accent: "#50d06dff",
      font: "Sans",
      fontSize: 14,
      padding: 12,
    },
    interactive: {
      enabled: true,
      pointer: true,
      keyboard: true,
      passthroughOutside: false,
      overlayId,
    },
    selection: {
      draft,
      border: "#50d06dff",
      borderWidth: 3,
      hint: "Press ENTER to confirm, drag to redo · ESC cancels",
      hintColor: "#f4f0e6ff",
      hintSize: 14,
    },
  });
  if (!sent) {
    listener.close();
    return false;
  }
  const selected = await listener.promise;
  await hideHyprOverlay();
  if (selected) {
    overlayController.applyCalibration({
      x: Math.round(selected.x),
      y: Math.round(selected.y),
      width: Math.round(selected.w),
      height: Math.round(selected.h),
    });
  }
  return true;
}

async function triggerPriceCheck(): Promise<void> {
  const result = await captureItemText(false);
  if (!result.ok) {
    sendOverlayCommand("price-check", {
      itemText: "",
      error: result.reason,
    });
    return;
  }
  sendOverlayCommand("price-check", { itemText: result.text });
}

function triggerRegex(action: OverlayCommandAction): void {
  sendOverlayCommand(action);
}

// Esc dismisses a *visible* overlay. Registered only while shown so we don't
// hijack Esc globally (the calibrator handles its own Esc window-locally).
function registerEscToHide(): void {
  try {
    if (!globalShortcut.isRegistered("Escape")) {
      globalShortcut.register("Escape", () => overlayController.hideOverlay());
    }
  } catch {
    // best-effort
  }
}

function unregisterEscToHide(): void {
  try {
    globalShortcut.unregister("Escape");
  } catch {
    // best-effort
  }
}

function pushOverlayState(
  visible: boolean,
  degraded: boolean,
  extra: Partial<{ action: string; itemText: string; error: string }> = {},
): void {
  const payload = {
    visible,
    degraded,
    mode: capabilities?.overlayMode ?? (degraded ? "degraded" : "full"),
    ...extra,
  };
  mainWindow?.webContents.send(CHANNELS.overlayState, payload);
  overlayWindow?.webContents.send(CHANNELS.overlayState, payload);
}

function sendOverlayCommand(
  action: OverlayCommandAction,
  extra: Partial<{ itemText: string; error: string }> = {},
): void {
  ensureOverlayWindow();
  // Reward OCR must capture before any Electron surface is placed over the
  // target. The hidden renderer still receives the command and reveals the
  // appropriate transport only after OCR has completed.
  if (showElectronBeforeCommand(capabilities?.overlayMode, action)) {
    overlayWindow?.showInactive();
  } else {
    overlayWindow?.hide();
  }
  registerEscToHide();
  const payload = {
    visible: true,
    degraded: capabilities?.overlayMode === "degraded",
    mode: capabilities?.overlayMode ?? "degraded",
    action,
    ...extra,
  };
  const send = () => {
    mainWindow?.webContents.send(CHANNELS.overlayState, payload);
    overlayWindow?.webContents.send(CHANNELS.overlayState, payload);
  };
  const contents = overlayWindow?.webContents;
  if (
    overlayRendererReady &&
    contents &&
    isOverlayRendererReady(contents.getURL(), contents.isLoadingMainFrame())
  ) {
    send();
  } else {
    contents?.once("did-finish-load", send);
  }
}

/** The window/overlay surface ipc.ts delegates to. main.ts owns all windows. */
const overlayController: OverlayController = {
  capabilities(): Capabilities {
    // Defaulted defensively; whenReady always sets it before registerIpc.
    return (
      capabilities ?? {
        sessionKind: "linux-wayland-other",
        overlayMode: "degraded",
        silentRegionCapture: false,
        regionPicker: "electron",
        captureBackend: "portal",
      }
    );
  },
  scanRewards(): void {
    triggerScan();
  },
  setRewardWatcher(enabled: boolean): void {
    rewardWatcherEnabled = enabled;
    tray?.setContextMenu(buildTrayMenu());
    if (capabilities?.overlayMode === "hyprland-plugin") {
      void hideHyprOverlay();
    }
    sendOverlayCommand(enabled ? "reward-watch-start" : "reward-watch-stop");
  },
  rewardWatcherEnabled(): boolean {
    return rewardWatcherEnabled;
  },
  showOverlay(): void {
    if (this.capabilities().overlayMode === "hyprland-plugin") {
      ensureOverlayWindow();
      overlayWindow?.hide();
      registerEscToHide();
      pushOverlayState(true, false);
      return;
    }
    if (this.capabilities().overlayMode !== "full") {
      // Degraded: show a small normal Electron fallback instead of silently
      // updating a hidden renderer when compositor-native rendering is absent.
      ensureOverlayWindow();
      overlayWindow?.showInactive();
      pushOverlayState(true, true);
      return;
    }
    ensureOverlayWindow();
    overlayWindow?.showInactive();
    registerEscToHide();
    pushOverlayState(true, false);
  },
  hideOverlay(): void {
    stopRegexOverlayEventSession();
    if (this.capabilities().overlayMode === "hyprland-plugin") {
      void hideHyprOverlay();
    }
    overlayWindow?.hide();
    unregisterEscToHide();
    pushOverlayState(false, this.capabilities().overlayMode === "degraded");
  },
  isOverlayVisible(): boolean {
    return overlayWindow?.isVisible() ?? false;
  },
  setOverlayVisible(visible: boolean): void {
    if (!overlayWindow) return;
    if (visible) overlayWindow.showInactive();
    else overlayWindow.hide();
  },
  setOverlayRegion(rect: CaptureRect): void {
    if (
      this.capabilities().overlayMode !== "full" &&
      this.capabilities().overlayMode !== "hyprland-plugin"
    ) {
      return;
    }
    ensureOverlayWindow();
    if (this.capabilities().overlayMode === "full") {
      overlayWindow?.setBounds({
        x: Math.round(rect.x),
        y: Math.round(rect.y),
        width: Math.max(1, Math.round(rect.width)),
        height: Math.max(1, Math.round(rect.height)),
      });
    }
    // Keep the overlay renderer's cached region in sync with its bounds so a
    // scan triggered right after positioning has a rect to capture.
    overlayWindow?.webContents.send(CHANNELS.regionCalibrated, rect);
  },
  openCalibration(): void {
    if (this.capabilities().overlayMode === "hyprland-plugin") {
      void openHyprlandCalibration().then((usedPlugin) => {
        if (usedPlugin || this.capabilities().regionPicker !== "slurp") return;
        void selectSlurpRegion().then((rect) => {
          if (rect) this.applyCalibration(rect);
        });
      });
      return;
    }
    if (this.capabilities().regionPicker === "slurp") {
      void selectSlurpRegion().then((rect) => {
        if (rect) this.applyCalibration(rect);
      });
      return;
    }
    ensureCalibrationWindow();
    calibrationWindow?.show();
    calibrationWindow?.focus();
  },
  applyCalibration(rect: CaptureRect): void {
    saveCaptureRegion({ rect });
    calibrationWindow?.hide();
    // Tell the main app AND the overlay that a new region is in effect — the
    // overlay's runScan reads its own regionRef, populated via this push, so it
    // must receive regionCalibrated too (else it reports "no region" on scan).
    mainWindow?.webContents.send(CHANNELS.regionCalibrated, rect);
    overlayWindow?.webContents.send(CHANNELS.regionCalibrated, rect);
    if (
      this.capabilities().overlayMode === "full" ||
      this.capabilities().overlayMode === "hyprland-plugin"
    ) {
      this.setOverlayRegion(rect);
    }
  },
};

function createWindow(): void {
  const state = loadWindowState();
  const win = new BrowserWindow({
    ...state,
    minWidth: 980,
    minHeight: 640,
    backgroundColor: "#0a0907",
    autoHideMenuBar: true,
    webPreferences: {
      preload: path.join(__dirname, "preload.js"),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
      backgroundThrottling: false,
    },
  });
  if (state.maximized) win.maximize();
  trackWindowState(win);

  // External links (trade deep links etc.) open in the system browser.
  win.webContents.setWindowOpenHandler(({ url }) => {
    if (/^https?:\/\//.test(url)) {
      void import("electron").then(({ shell }) => shell.openExternal(url));
    }
    return { action: "deny" };
  });

  void win.loadURL(DEV ? DEV_URL : `${APP_ORIGIN}/index.html`);
  mainWindow = win;
  win.on("close", (event) => {
    if (isQuitting) return;
    event.preventDefault();
    win.hide();
  });
  win.on("closed", () => {
    mainWindow = null;
  });
}

function ensureMainWindow(): BrowserWindow {
  if (!mainWindow || mainWindow.isDestroyed()) {
    createWindow();
  }
  return mainWindow!;
}

function focusMainWindow(): BrowserWindow {
  const win = ensureMainWindow();
  if (win.isMinimized()) win.restore();
  if (!win.isVisible()) win.show();
  win.focus();
  return win;
}

/**
 * Transparent, frameless, always-on-top, click-through overlay (full mode).
 * Loads /overlay. Created lazily on first show so degraded sessions never
 * allocate it. NOT a layer-shell surface (ADR-0009 deferred) — a normal
 * Electron window with override-redirect-style hints.
 */
function ensureOverlayWindow(): void {
  if (overlayWindow && !overlayWindow.isDestroyed()) return;
  overlayRendererReady = false;
  const win = new BrowserWindow({
    width: 320,
    height: 120,
    show: false,
    frame: false,
    transparent: true,
    alwaysOnTop: true,
    focusable: false,
    skipTaskbar: true,
    resizable: false,
    hasShadow: false,
    backgroundColor: "#00000000",
    opacity: 0.62,
    webPreferences: {
      preload: path.join(__dirname, "preload.js"),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
    },
  });
  win.setIgnoreMouseEvents(true, { forward: true });
  win.setAlwaysOnTop(true, "screen-saver");
  win.webContents.on("did-start-navigation", () => {
    overlayRendererReady = false;
  });
  win.webContents.on("did-finish-load", () => {
    overlayRendererReady = true;
  });
  void win.loadURL(`${DEV_BASE()}/overlay/index.html`);
  win.on("closed", () => {
    overlayRendererReady = false;
    overlayWindow = null;
  });
  overlayWindow = win;
}

/**
 * Full-screen transparent calibration window. The user drag-selects the price
 * region; the /calibrate route posts the rect back through the bridge
 * (calibrateRegion) → overlayController.applyCalibration.
 */
function ensureCalibrationWindow(): void {
  if (calibrationWindow && !calibrationWindow.isDestroyed()) return;
  const win = new BrowserWindow({
    show: false,
    frame: false,
    transparent: true,
    alwaysOnTop: true,
    fullscreen: true,
    skipTaskbar: true,
    backgroundColor: "#00000000",
    webPreferences: {
      preload: path.join(__dirname, "preload.js"),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
    },
  });
  win.setAlwaysOnTop(true, "screen-saver");
  void win.loadURL(`${DEV_BASE()}/calibrate/index.html`);
  // Esc closes the calibrator without applying.
  win.webContents.on("before-input-event", (_e, input) => {
    if (input.key === "Escape") win.hide();
  });
  win.on("closed", () => {
    calibrationWindow = null;
  });
  calibrationWindow = win;
}
