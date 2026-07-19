import { execFile } from "node:child_process";
import type { CaptureRect } from "./screen";
import { withWaylandDisplay } from "./wayland";

const SLURP_TIMEOUT_MS = 5 * 60 * 1000;

export interface CommandResult {
  stdout: string;
  stderr: string;
}

export type CommandRunner = (
  command: string,
  args: string[],
) => Promise<CommandResult>;

function defaultRunner(command: string, args: string[]): Promise<CommandResult> {
  return new Promise((resolve, reject) => {
    execFile(
      command,
      args,
      {
        encoding: "utf8",
        timeout: SLURP_TIMEOUT_MS,
        windowsHide: true,
        env: withWaylandDisplay(process.env),
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

/** Parse `slurp -f '%x,%y %wx%h'` output in global logical pixels. */
export function parseSlurpRect(raw: string): CaptureRect | null {
  const match = /^\s*(-?\d+),(-?\d+)\s+(\d+)x(\d+)\s*$/.exec(raw);
  if (!match) return null;
  const rect = {
    x: Number(match[1]),
    y: Number(match[2]),
    width: Number(match[3]),
    height: Number(match[4]),
  };
  return rect.width > 0 && rect.height > 0 ? rect : null;
}

/** Native Wayland region selection. A cancelled selector resolves to null. */
export async function selectSlurpRegion(
  runner: CommandRunner = defaultRunner,
): Promise<CaptureRect | null> {
  try {
    const result = await runner("slurp", [
      "-f",
      "%x,%y %wx%h",
      "-b",
      "10101866",
      "-c",
      "c9a227ff",
      "-s",
      "c9a22733",
    ]);
    return parseSlurpRect(result.stdout);
  } catch {
    return null;
  }
}
