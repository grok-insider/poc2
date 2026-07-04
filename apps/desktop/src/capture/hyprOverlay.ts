import { execFile } from "node:child_process";

const HYPRCTL_TIMEOUT_MS = 800;
const HYPRCTL_MAX_BUFFER = 64 * 1024;

export interface HyprOverlayRow {
  label: string;
  value?: string;
  detail?: string;
  emphasis?: boolean;
}

export interface HyprOverlayPayload {
  visible?: boolean;
  rect: { x: number; y: number; w?: number; h?: number; width?: number; height?: number };
  ttlMs?: number;
  rows: HyprOverlayRow[];
}

export interface HyprctlResult {
  stdout: string;
  stderr: string;
}

export type HyprctlRunner = (args: string[]) => Promise<HyprctlResult>;

function defaultRunner(args: string[]): Promise<HyprctlResult> {
  return new Promise((resolve, reject) => {
    execFile(
      "hyprctl",
      args,
      {
        encoding: "utf8",
        timeout: HYPRCTL_TIMEOUT_MS,
        maxBuffer: HYPRCTL_MAX_BUFFER,
        windowsHide: true,
      },
      (error, stdout, stderr) => {
        if (error) {
          reject(error);
          return;
        }
        resolve({ stdout, stderr });
      },
    );
  });
}

function hasPluginInList(stdout: string): boolean {
  return stdout.toLowerCase().includes("hyproverlay");
}

function statusLooksHealthy(stdout: string): boolean {
  try {
    const parsed = JSON.parse(stdout) as { loaded?: unknown };
    return parsed.loaded === true;
  } catch {
    return false;
  }
}
export async function detectHyprOverlay(
  runner: HyprctlRunner = defaultRunner,
): Promise<boolean> {
  try {
    const list = await runner(["plugin", "list"]);
    if (!hasPluginInList(list.stdout)) return false;
    const status = await runner(["-j", "hyproverlay", "status"]);
    return statusLooksHealthy(status.stdout);
  } catch {
    return false;
  }
}

export async function sendHyprOverlay(
  payload: HyprOverlayPayload,
  runner: HyprctlRunner = defaultRunner,
): Promise<boolean> {
  try {
    const json = JSON.stringify(payload);
    if (json.length > 16 * 1024) return false;
    const res = await runner(["hyproverlay", "set-json", json]);
    return res.stdout.trim() === "ok";
  } catch {
    return false;
  }
}

export async function hideHyprOverlay(
  runner: HyprctlRunner = defaultRunner,
): Promise<boolean> {
  try {
    const res = await runner(["hyproverlay", "hide"]);
    return res.stdout.trim() === "ok";
  } catch {
    return false;
  }
}
