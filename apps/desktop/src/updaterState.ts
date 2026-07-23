// Pure update-status model for the desktop auto-updater.
// electron-free so bun tests can exercise the state machine without Electron.

export type UpdatePhase =
  | "idle"
  | "checking"
  | "available"
  | "not-available"
  | "downloading"
  | "downloaded"
  | "error";

/** Snapshot pushed to the renderer / tray. */
export interface UpdateStatus {
  /** False when the shell is unpackaged (`bun run dev` / `desktop:start` without installer). */
  enabled: boolean;
  phase: UpdatePhase;
  currentVersion: string;
  availableVersion: string | null;
  /** Download progress 0–100 while phase is downloading; otherwise null. */
  percent: number | null;
  error: string | null;
  /** ISO timestamp of the last completed check (available / not-available / error). */
  checkedAt: string | null;
}

export type UpdateEvent =
  | { type: "checking" }
  | { type: "available"; version: string }
  | { type: "not-available" }
  | { type: "progress"; percent: number }
  | { type: "downloaded"; version: string }
  | { type: "error"; message: string };

export function initialUpdateStatus(
  currentVersion: string,
  enabled: boolean,
): UpdateStatus {
  return {
    enabled,
    phase: "idle",
    currentVersion,
    availableVersion: null,
    percent: null,
    error: null,
    checkedAt: null,
  };
}

/** Clamp percent into [0, 100] and round to one decimal. */
export function clampPercent(raw: number): number {
  if (!Number.isFinite(raw)) return 0;
  return Math.min(100, Math.max(0, Math.round(raw * 10) / 10));
}

/**
 * Apply one updater event. Disabled status is a no-op (stays idle forever).
 * `now` is injectable so tests stay deterministic.
 */
export function applyUpdateEvent(
  status: UpdateStatus,
  event: UpdateEvent,
  now: () => string = () => new Date().toISOString(),
): UpdateStatus {
  if (!status.enabled) return status;

  switch (event.type) {
    case "checking":
      return {
        ...status,
        phase: "checking",
        error: null,
        percent: null,
      };
    case "available":
      return {
        ...status,
        phase: "available",
        availableVersion: event.version,
        error: null,
        percent: null,
        checkedAt: now(),
      };
    case "not-available":
      return {
        ...status,
        phase: "not-available",
        availableVersion: null,
        error: null,
        percent: null,
        checkedAt: now(),
      };
    case "progress":
      return {
        ...status,
        phase: "downloading",
        percent: clampPercent(event.percent),
        error: null,
      };
    case "downloaded":
      return {
        ...status,
        phase: "downloaded",
        availableVersion: event.version,
        percent: 100,
        error: null,
        checkedAt: now(),
      };
    case "error":
      return {
        ...status,
        phase: "error",
        error: event.message,
        percent: null,
        checkedAt: now(),
      };
    default: {
      const _exhaustive: never = event;
      return _exhaustive;
    }
  }
}

/** Tray / Settings label for the current phase. */
export function updateStatusLabel(status: UpdateStatus): string {
  if (!status.enabled) return "Updates disabled (dev build)";
  switch (status.phase) {
    case "idle":
      return "No update check yet";
    case "checking":
      return "Checking for updates…";
    case "available":
      return status.availableVersion
        ? `Update ${status.availableVersion} available`
        : "Update available";
    case "not-available":
      return "Up to date";
    case "downloading":
      return status.percent != null
        ? `Downloading update… ${status.percent}%`
        : "Downloading update…";
    case "downloaded":
      return status.availableVersion
        ? `Update ${status.availableVersion} ready to install`
        : "Update ready to install";
    case "error":
      return status.error ? `Update error: ${status.error}` : "Update error";
    default: {
      const _exhaustive: never = status.phase;
      return _exhaustive;
    }
  }
}
