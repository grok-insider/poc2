import { execFile } from "node:child_process";
import { coerceRect, type CaptureRegionResult } from "./screen";
import { withWaylandDisplay } from "./wayland";

const GRIM_TIMEOUT_MS = 10_000;
const GRIM_MAX_BUFFER = 32 * 1024 * 1024;

export type GrimRunner = (args: string[]) => Promise<Buffer>;

function defaultRunner(args: string[]): Promise<Buffer> {
  return new Promise((resolve, reject) => {
    execFile(
      "grim",
      args,
      {
        encoding: "buffer",
        timeout: GRIM_TIMEOUT_MS,
        maxBuffer: GRIM_MAX_BUFFER,
        windowsHide: true,
        env: withWaylandDisplay(process.env),
      },
      (error, stdout) => {
        if (error) {
          reject(error);
          return;
        }
        resolve(stdout);
      },
    );
  });
}

export function grimGeometry(raw: unknown): string | null {
  const rect = coerceRect(raw);
  if (!rect) return null;
  return `${Math.round(rect.x)},${Math.round(rect.y)} ${Math.round(rect.width)}x${Math.round(rect.height)}`;
}

/** Silent wlroots/Hyprland region capture through the compositor. */
export async function captureRegionWithGrim(
  raw: unknown,
  runner: GrimRunner = defaultRunner,
): Promise<CaptureRegionResult> {
  const rect = coerceRect(raw);
  const geometry = grimGeometry(raw);
  if (!rect || !geometry) return { ok: false, reason: "invalid-rect" };
  try {
    const png = await runner(["-g", geometry, "-t", "png", "-"]);
    if (png.length < 8 || png.subarray(0, 8).toString("hex") !== "89504e470d0a1a0a") {
      return {
        ok: false,
        reason: "capture-failed",
        message: "grim returned no PNG data",
      };
    }
    return {
      ok: true,
      dataUrl: `data:image/png;base64,${png.toString("base64")}`,
      width: Math.round(rect.width),
      height: Math.round(rect.height),
    };
  } catch (error) {
    return {
      ok: false,
      reason: "capture-failed",
      message: error instanceof Error ? error.message : String(error),
    };
  }
}
