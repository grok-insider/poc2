import { describe, expect, test } from "bun:test";
import { injectionCandidates } from "../src/capture/linux";

describe("injectionCandidates", () => {
  test("prefers hyprctl, then ydotool, then wtype", () => {
    const c = injectionCandidates(false);
    expect(c.map((x) => x.cmd)).toEqual(["hyprctl", "ydotool", "wtype"]);
  });

  test("plain copy sends CTRL+C", () => {
    const [hypr, ydo, wtype] = injectionCandidates(false);
    expect(hypr!.args).toEqual(["dispatch", "sendshortcut", "CTRL, C, activewindow"]);
    expect(ydo!.args).toEqual(["key", "29:1", "46:1", "46:0", "29:0"]);
    expect(wtype!.args).toEqual(["-M", "ctrl", "c", "-m", "ctrl"]);
  });

  test("advanced copy adds ALT and releases in reverse order", () => {
    const [hypr, ydo] = injectionCandidates(true);
    expect(hypr!.args[2]).toBe("CTRL ALT, C, activewindow");
    expect(ydo!.args).toEqual(["key", "29:1", "56:1", "46:1", "46:0", "56:0", "29:0"]);
  });
});
