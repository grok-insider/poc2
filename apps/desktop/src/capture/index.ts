// Capture orchestrator: inject Ctrl+C into the focused game window, poll
// the clipboard for PoE2 item text, restore the user's clipboard.
//
// Semantics follow Awakened PoE Trade (verified against its source in
// example-repos/): snapshot → inject → poll every 48ms up to 500ms until
// the text starts with a known "Item Class:" line → restore after 120ms.
import { clipboard } from "electron";
import {
  DEFAULT_TIMINGS,
  type CaptureTimings,
  isPoeItemText,
} from "./itemText";

export type CaptureResult =
  | { ok: true; text: string; tool: string }
  | { ok: false; reason: "inject-failed" | "timeout" | "busy" };

export interface CaptureStatus {
  platform: NodeJS.Platform;
  lastTool: string | null;
  lastError: string | null;
  hotkeyRegistered: boolean;
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

async function injectCopy(advanced: boolean): Promise<string | null> {
  if (process.platform === "win32") {
    return (await import("./win32")).injectCopy(advanced);
  }
  if (process.platform === "linux") {
    return (await import("./linux")).injectCopy(advanced);
  }
  return null;
}

let inFlight = false;
export const status: CaptureStatus = {
  platform: process.platform,
  lastTool: null,
  lastError: null,
  hotkeyRegistered: false,
};

/**
 * Run one capture. The game must be the focused window (the user pressed
 * the hotkey while hovering an item in PoE2).
 */
export async function captureItemText(
  advanced: boolean,
  timings: CaptureTimings = DEFAULT_TIMINGS,
): Promise<CaptureResult> {
  if (inFlight) return { ok: false, reason: "busy" };
  inFlight = true;
  try {
    const previous = clipboard.readText();
    // Clear so stale item text from an earlier copy can't satisfy the poll.
    clipboard.writeText("");

    if (timings.preInjectDelayMs > 0) {
      await sleep(timings.preInjectDelayMs);
    }

    const tool = await injectCopy(advanced);
    if (!tool) {
      clipboard.writeText(previous);
      status.lastError =
        process.platform === "linux"
          ? "no injection tool worked (tried ydotool, hyprctl, wtype)"
          : "uiohook-napi unavailable";
      return { ok: false, reason: "inject-failed" };
    }
    status.lastTool = tool;

    const deadline = Date.now() + timings.timeoutMs;
    while (Date.now() < deadline) {
      await sleep(timings.pollMs);
      const text = clipboard.readText();
      if (text && isPoeItemText(text)) {
        // Don't restore over what the game wrote until the renderer had a
        // chance to exist independently of the clipboard; APT waits 120ms.
        setTimeout(() => clipboard.writeText(previous), timings.restoreAfterMs);
        status.lastError = null;
        return { ok: true, text, tool };
      }
    }
    clipboard.writeText(previous);
    status.lastError = "clipboard never contained item text (is PoE2 focused with an item hovered?)";
    return { ok: false, reason: "timeout" };
  } finally {
    inFlight = false;
  }
}
