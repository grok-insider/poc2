// Path of Crafting 2 — desktop shell entry (ADR-0010).
//
// A normal windowed app (like Discord) around the apps/web static export:
//   - serves the export over a privileged app:// scheme (root-absolute
//     asset paths keep working; no localhost server)
//   - registers the capture hotkey (global where the platform allows;
//     a second-instance `--capture` flag is the compositor-bind fallback
//     on Wayland: bind = CTRL SHIFT, D, exec, poc2-desktop --capture)
//   - proxies the trade2 API with centralized rate limiting
import { app, BrowserWindow, globalShortcut } from "electron";
import path from "node:path";
import { status as captureStatus } from "./capture";
import { registerIpc, runCapture } from "./ipc";
import { APP_ORIGIN, handleAppScheme, registerAppScheme } from "./serve";
import { loadWindowState, trackWindowState } from "./windowState";

// userData dir name ("poc2-desktop", not the scoped npm package name).
app.setName("poc2-desktop");

const DEV = process.argv.includes("--dev");
const DEV_URL = process.env.POC2_DEV_URL ?? "http://localhost:3000";
const HOTKEY = process.env.POC2_CAPTURE_HOTKEY ?? "CommandOrControl+Shift+D";

let mainWindow: BrowserWindow | null = null;

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
    if (mainWindow.isMinimized()) mainWindow.restore();
    mainWindow.focus();
  });

  app.whenReady().then(() => {
    handleAppScheme();
    registerIpc(() => mainWindow);
    createWindow();

    // Global hotkey: native on Windows/X11; on Wayland this needs the
    // GlobalShortcuts portal (run with --enable-features=GlobalShortcutsPortal
    // --ozone-platform=wayland) — otherwise users bind `--capture` instead.
    try {
      captureStatus.hotkeyRegistered = globalShortcut.register(HOTKEY, () => {
        if (mainWindow) void runCapture(mainWindow, false);
      });
    } catch {
      captureStatus.hotkeyRegistered = false;
    }

    app.on("activate", () => {
      if (BrowserWindow.getAllWindows().length === 0) createWindow();
    });
  });

  app.on("will-quit", () => globalShortcut.unregisterAll());
  app.on("window-all-closed", () => {
    app.quit();
  });
}

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
