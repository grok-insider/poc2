// Desktop auto-updater: GitHub Releases via electron-updater (unsigned OSS builds).
//
// Packaged installs only (`app.isPackaged`). Dev / unpackaged shells no-op so
// `bun run desktop:dev` never hits the network. Feed metadata comes from the
// electron-builder `publish` block baked into `app-update.yml` at package time
// (owner/repo: grok-insider/poc2). Install only after the user confirms
// (Settings / tray → quitAndInstall).

import { app, BrowserWindow } from "electron";
import {
  applyUpdateEvent,
  initialUpdateStatus,
  type UpdateStatus,
} from "./updaterState";

export type { UpdateStatus } from "./updaterState";
export { updateStatusLabel } from "./updaterState";

type StatusListener = (status: UpdateStatus) => void;

let status: UpdateStatus = initialUpdateStatus("0.0.0", false);
const listeners = new Set<StatusListener>();
let started = false;
let mainWindowGetter: (() => BrowserWindow | null) | null = null;

function emit(next: UpdateStatus): void {
  status = next;
  for (const listener of listeners) {
    try {
      listener(status);
    } catch {
      /* never let a listener break the updater */
    }
  }
  const win = mainWindowGetter?.() ?? null;
  if (win && !win.isDestroyed()) {
    win.webContents.send("poc2:updates-state", status);
  }
}

function pushEvent(
  event: Parameters<typeof applyUpdateEvent>[1],
): void {
  emit(applyUpdateEvent(status, event));
}

/** Current snapshot (sync; IPC handler returns this). */
export function getUpdateStatus(): UpdateStatus {
  return status;
}

/** Subscribe to status changes (tray rebuild, etc.). Returns unsubscribe. */
export function onUpdateStatus(listener: StatusListener): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/**
 * Install a downloaded update. No-op unless phase is `downloaded`.
 * Calls `autoUpdater.quitAndInstall` (restarts the app).
 */
export async function installUpdate(): Promise<boolean> {
  if (!status.enabled || status.phase !== "downloaded") return false;
  try {
    const { autoUpdater } = await import("electron-updater");
    // isSilent=false, isForceRunAfter=true — restart into the new version.
    autoUpdater.quitAndInstall(false, true);
    return true;
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    pushEvent({ type: "error", message });
    return false;
  }
}

/** Manual check (Settings button). Safe when disabled / already checking. */
export async function checkForUpdates(): Promise<UpdateStatus> {
  if (!status.enabled) return status;
  if (status.phase === "checking" || status.phase === "downloading") {
    return status;
  }
  try {
    const { autoUpdater } = await import("electron-updater");
    pushEvent({ type: "checking" });
    await autoUpdater.checkForUpdates();
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    pushEvent({ type: "error", message });
  }
  return status;
}

export interface StartUpdaterOptions {
  /** Main window getter for push IPC. */
  getMainWindow: () => BrowserWindow | null;
  /**
   * Delay before the first automatic check so capture/prices can settle.
   * Default 10s. Set 0 to check immediately after wiring.
   */
  checkDelayMs?: number;
  /** Override packaged detection (tests). */
  isPackaged?: boolean;
  /** Override app version (tests). */
  version?: string;
}

/**
 * Wire electron-updater once after `app.whenReady`. Idempotent.
 * When unpackaged, leaves status at `enabled: false` and returns.
 */
export async function startDesktopUpdater(
  options: StartUpdaterOptions,
): Promise<void> {
  if (started) return;
  started = true;
  mainWindowGetter = options.getMainWindow;

  const packaged = options.isPackaged ?? app.isPackaged;
  const version = options.version ?? app.getVersion();
  emit(initialUpdateStatus(version, packaged));

  if (!packaged) return;

  const { autoUpdater } = await import("electron-updater");

  // Download in the background once found; install only on user confirm.
  autoUpdater.autoDownload = true;
  autoUpdater.autoInstallOnAppQuit = true;
  // Public GitHub repo — no token. logger optional (console is fine).
  autoUpdater.logger = null;

  autoUpdater.on("checking-for-update", () => {
    pushEvent({ type: "checking" });
  });
  autoUpdater.on("update-available", (info) => {
    pushEvent({ type: "available", version: info.version });
  });
  autoUpdater.on("update-not-available", () => {
    pushEvent({ type: "not-available" });
  });
  autoUpdater.on("download-progress", (progress) => {
    pushEvent({ type: "progress", percent: progress.percent });
  });
  autoUpdater.on("update-downloaded", (info) => {
    pushEvent({ type: "downloaded", version: info.version });
  });
  autoUpdater.on("error", (err) => {
    const message = err instanceof Error ? err.message : String(err);
    pushEvent({ type: "error", message });
  });

  const delay = options.checkDelayMs ?? 10_000;
  const runCheck = () => {
    void autoUpdater.checkForUpdates().catch((err: unknown) => {
      const message = err instanceof Error ? err.message : String(err);
      pushEvent({ type: "error", message });
    });
  };
  if (delay <= 0) runCheck();
  else setTimeout(runCheck, delay);
}
