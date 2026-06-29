// Linux (Wayland/Hyprland-first) Ctrl+C injection.
//
// No native Node modules on Linux: we spawn compositor/uinput tools.
// Order of preference (ADR-0010):
//   1. hyprctl dispatch sendshortcut  — zero-setup on Hyprland
//   2. ydotool                        — uinput, compositor-agnostic,
//                                       needs ydotoold + /dev/uinput access
//   3. wtype                          — virtual-keyboard protocol
import { spawn } from "node:child_process";

export interface InjectAttempt {
  cmd: string;
  args: string[];
}

/** Candidate injection commands, most reliable first. Pure — tested. */
export function injectionCandidates(advanced: boolean): InjectAttempt[] {
  const mods = advanced ? "CTRL ALT" : "CTRL";
  // ydotool key codes: 29=LCTRL, 56=LALT, 46=C (press=1 / release=0).
  const ydoKeys = advanced
    ? ["29:1", "56:1", "46:1", "46:0", "56:0", "29:0"]
    : ["29:1", "46:1", "46:0", "29:0"];
  const wtypeArgs = advanced
    ? ["-M", "ctrl", "-M", "alt", "c", "-m", "alt", "-m", "ctrl"]
    : ["-M", "ctrl", "c", "-m", "ctrl"];
  return [
    {
      cmd: "hyprctl",
      args: ["dispatch", "sendshortcut", `${mods}, C, activewindow`],
    },
    { cmd: "ydotool", args: ["key", ...ydoKeys] },
    { cmd: "wtype", args: wtypeArgs },
  ];
}

function run(cmd: string, args: string[]): Promise<boolean> {
  return new Promise((resolve) => {
    const child = spawn(cmd, args, { stdio: "ignore" });
    child.on("error", () => resolve(false)); // ENOENT etc.
    child.on("exit", (code) => resolve(code === 0));
  });
}

/** Try each candidate until one succeeds. Returns the tool used, or null. */
export async function injectCopy(advanced: boolean): Promise<string | null> {
  for (const c of injectionCandidates(advanced)) {
    if (await run(c.cmd, c.args)) return c.cmd;
  }
  return null;
}
