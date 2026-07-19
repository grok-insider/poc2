import { describe, expect, test } from "bun:test";
import { injectionCandidates } from "../src/capture/linux";

describe("injectionCandidates", () => {
  test("prefers ydotool, then hyprctl, then wtype", () => {
    const c = injectionCandidates(false);
    expect(c.map((x) => x.cmd)).toEqual(["ydotool", "hyprctl", "wtype"]);
  });

  test("plain copy sends CTRL+C", () => {
    const [ydo, hypr, wtype] = injectionCandidates(false);
    expect(hypr!.args).toEqual(["dispatch", "sendshortcut", "CTRL, C, activewindow"]);
    expect(ydo!.args).toEqual(["key", "29:1", "46:1", "46:0", "29:0"]);
    expect(wtype!.args).toEqual(["-M", "ctrl", "c", "-m", "ctrl"]);
  });

  test("advanced copy adds ALT and releases in reverse order", () => {
    const [ydo, hypr] = injectionCandidates(true);
    expect(hypr!.args[2]).toBe("CTRL ALT, C, activewindow");
    expect(ydo!.args).toEqual(["key", "29:1", "56:1", "46:1", "46:0", "56:0", "29:0"]);
  });
});
