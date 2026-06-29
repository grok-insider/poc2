// Path of Crafting 2 — desktop shell entry (ADR-0010 + ADR-0013).
//
// A normal windowed app (like Discord) around the apps/web static export:
//   - serves the export over a privileged app:// scheme (root-absolute
//     asset paths keep working; no localhost server)
//   - registers the capture hotkey (global where the platform allows;
//     a second-instance `--capture` flag is the compositor-bind fallback
//     on Wayland: bind = CTRL SHIFT, D, exec, poc2-desktop --capture)
//   - proxies the trade2 API with centralized rate limiting
//   - (ADR-0013) hosts a capability-gated price OVERLAY window + a full-screen
//     CALIBRATION window for the screen-region OCR capture flow. The overlay is
//     a transparent click-through Electron window (NOT a layer-shell surface;
//     ADR-0009 stays deferred). On compositors that can't do click-through
//     overlays (Hyprland/wlroots, probe-fail Wayland) we signal the renderer to
//     show an in-app panel instead — "degraded" mode.
import { app, BrowserWindow, globalShortcut } from "electron";
import path from "node:path";
import { status as captureStatus } from "./capture";
import {
  type Capabilities,
  detectCapabilities,
} from "./capture/capabilities";
import { probeOverlaySupport } from "./capture/overlayProbe";
import type { CaptureRect } from "./capture/screen";
import { CHANNELS, type OverlayController, registerIpc, runCapture } from "./ipc";
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
const HOTKEY = process.env.POC2_CAPTURE_HOTKEY ?? "CommandOrControl+Shift+D";
const SCAN_HOTKEY = process.env.POC2_SCAN_HOTKEY ?? "CommandOrControl+Shift+S";
const RECALIBRATE_HOTKEY =
  process.env.POC2_RECALIBRATE_HOTKEY ?? "CommandOrControl+Shift+C";

const DEV_BASE = () => (DEV ? DEV_URL : APP_ORIGIN);

let mainWindow: BrowserWindow | null = null;
let overlayWindow: BrowserWindow | null = null;
let calibrationWindow: BrowserWindow | null = null;
let capabilities: Capabilities | null = null;

// Single instance: a second `poc2-desktop --capture` invocation forwards to
// the running app — the Wayland/Hyprland hotkey path (ADR-0010).
const gotLock = app.requestSingleInstanceLock();
if (!gotLock) {
  app.quit();
} else {
  registerAppScheme();

  app.on("second-instance", (_e, argv) => {
    if (!mainWindow) return;
    if (argv.includes("--capture")) {
      void runCapture(mainWindow, argv.includes("--advanced"));
      return;
    }
    if (argv.includes("--scan")) {
      triggerScan();
      return;
    }
    if (argv.includes("--recalibrate")) {
      overlayController.openCalibration();
      return;
    }
    if (mainWindow.isMinimized()) mainWindow.restore();
    mainWindow.focus();
  });

  app.whenReady().then(async () => {
    handleAppScheme();

    // Capability gate runs once at startup. The GNOME/KDE-Wayland branch gets a
    // real runtime overlay probe; every other session decides from env alone.
    capabilities = await detectCapabilities({ probeOverlay: probeOverlaySupport });

    registerIpc(() => mainWindow, overlayController);
    createWindow();

    // Global hotkeys: native on Windows/X11; on Wayland these need the
    // GlobalShortcuts portal (run with --enable-features=GlobalShortcutsPortal
    // --ozone-platform=wayland) — otherwise users bind `--capture`/`--scan`.
    registerHotkeys();

    app.on("activate", () => {
      if (BrowserWindow.getAllWindows().length === 0) createWindow();
    });
  });

  app.on("will-quit", () => globalShortcut.unregisterAll());
  app.on("window-all-closed", () => {
    app.quit();
  });
}

function registerHotkeys(): void {
  try {
    captureStatus.hotkeyRegistered = globalShortcut.register(HOTKEY, () => {
      if (mainWindow) void runCapture(mainWindow, false);
    });
    // Scan = single OCR pass over the calibrated region.
    globalShortcut.register(SCAN_HOTKEY, () => triggerScan());
    // Recalibrate = open the full-screen calibrator.
    globalShortcut.register(RECALIBRATE_HOTKEY, () => overlayController.openCalibration());
  } catch {
    captureStatus.hotkeyRegistered = false;
  }
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
    overlayController.showOverlay();
  } else {
    pushOverlayState(true, capabilities?.overlayMode !== "full");
  }
  // Nudge the main renderer to kick off a scan of the calibrated region.
  mainWindow?.webContents.send(CHANNELS.overlayState, {
    visible: true,
    degraded: capabilities?.overlayMode !== "full",
  });
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

function pushOverlayState(visible: boolean, degraded: boolean): void {
  const payload = { visible, degraded };
  mainWindow?.webContents.send(CHANNELS.overlayState, payload);
  overlayWindow?.webContents.send(CHANNELS.overlayState, payload);
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
      }
    );
  },
  showOverlay(): void {
    if (this.capabilities().overlayMode !== "full") {
      // Degraded: no click-through window — tell the renderer to show its panel.
      pushOverlayState(true, true);
      return;
    }
    ensureOverlayWindow();
    overlayWindow?.showInactive();
    registerEscToHide();
    pushOverlayState(true, false);
  },
  hideOverlay(): void {
    overlayWindow?.hide();
    unregisterEscToHide();
    pushOverlayState(false, this.capabilities().overlayMode !== "full");
  },
  setOverlayRegion(rect: CaptureRect): void {
    if (this.capabilities().overlayMode !== "full") return;
    ensureOverlayWindow();
    overlayWindow?.setBounds({
      x: Math.round(rect.x),
      y: Math.round(rect.y),
      width: Math.max(1, Math.round(rect.width)),
      height: Math.max(1, Math.round(rect.height)),
    });
  },
  openCalibration(): void {
    ensureCalibrationWindow();
    calibrationWindow?.show();
    calibrationWindow?.focus();
  },
  applyCalibration(rect: CaptureRect): void {
    saveCaptureRegion({ rect });
    calibrationWindow?.hide();
    // Tell the main app (and overlay) that a new region is in effect.
    mainWindow?.webContents.send(CHANNELS.regionCalibrated, rect);
    if (this.capabilities().overlayMode === "full") {
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
  win.on("closed", () => {
    mainWindow = null;
  });
}

/**
 * Transparent, frameless, always-on-top, click-through overlay (full mode).
 * Loads /overlay. Created lazily on first show so degraded sessions never
 * allocate it. NOT a layer-shell surface (ADR-0009 deferred) — a normal
 * Electron window with override-redirect-style hints.
 */
function ensureOverlayWindow(): void {
  if (overlayWindow && !overlayWindow.isDestroyed()) return;
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
    webPreferences: {
      preload: path.join(__dirname, "preload.js"),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
    },
  });
  win.setIgnoreMouseEvents(true, { forward: true });
  win.setAlwaysOnTop(true, "screen-saver");
  void win.loadURL(`${DEV_BASE()}/overlay/index.html`);
  win.on("closed", () => {
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
