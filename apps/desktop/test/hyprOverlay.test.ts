import { describe, expect, test } from "bun:test";
import {
  detectHyprOverlay,
  hideHyprOverlay,
  sendHyprOverlay,
  type HyprctlRunner,
} from "../src/capture/hyprOverlay";

function runnerFor(map: Record<string, string>): HyprctlRunner {
  return async (args) => {
    const key = args.join("\0");
    const stdout = map[key];
    if (stdout === undefined) throw new Error(`unexpected hyprctl ${args.join(" ")}`);
    return { stdout, stderr: "" };
  };
}

describe("hypr-overlay hyprctl transport", () => {
  test("detects loaded plugin with healthy JSON status", async () => {
    const runner = runnerFor({
      ["plugin\0list"]: "Plugin hyproverlay by grok-insider\n",
      ["-j\0hyproverlay\0status"]: '{"loaded":true,"visible":false}',
    });
    expect(await detectHyprOverlay(runner)).toBe(true);
  });

  test("detect returns false when plugin list lacks hyproverlay", async () => {
    const runner = runnerFor({
      ["plugin\0list"]: "no plugins loaded\n",
    });
    expect(await detectHyprOverlay(runner)).toBe(false);
  });

  test("send serializes bounded payload as one hyprctl argument", async () => {
    const seen: string[][] = [];
    const runner: HyprctlRunner = async (args) => {
      seen.push(args);
      return { stdout: "ok\n", stderr: "" };
    };
    const ok = await sendHyprOverlay(
      {
        rect: { x: 1, y: 2, w: 3, h: 4 },
        rows: [{ label: "Divine Orb", value: "142 ex", emphasis: true }],
      },
      runner,
    );
    expect(ok).toBe(true);
    expect(seen[0]?.slice(0, 2)).toEqual(["hyproverlay", "set-json"]);
    expect(JSON.parse(seen[0]?.[2] ?? "{}").rows[0].label).toBe("Divine Orb");
  });

  test("hide maps to hyproverlay hide", async () => {
    const runner = runnerFor({
      ["hyproverlay\0hide"]: "ok\n",
    });
    expect(await hideHyprOverlay(runner)).toBe(true);
  });
});
