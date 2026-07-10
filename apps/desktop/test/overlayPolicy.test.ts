import { describe, expect, test } from "bun:test";
import {
  isOverlayRendererReady,
  showElectronBeforeCommand,
} from "../src/capture/overlayPolicy";

describe("overlay command visibility", () => {
  test("reward OCR captures before showing a full Electron overlay", () => {
    expect(showElectronBeforeCommand("full", "reward-scan")).toBe(false);
    expect(showElectronBeforeCommand("full", "reward-watch-start")).toBe(false);
    expect(showElectronBeforeCommand("full", "reward-watch-stop")).toBe(false);
  });

  test("non-capture cards may show immediately in full mode", () => {
    expect(showElectronBeforeCommand("full", "price-check")).toBe(true);
    expect(showElectronBeforeCommand("full", "regex-open")).toBe(true);
  });

  test("plugin and degraded modes never show the Electron overlay", () => {
    expect(showElectronBeforeCommand("hyprland-plugin", "price-check")).toBe(false);
    expect(showElectronBeforeCommand("degraded", "regex-open")).toBe(false);
  });
});

describe("overlay renderer readiness", () => {
  test("about:blank is not ready even before navigation reports loading", () => {
    expect(isOverlayRendererReady("about:blank", false)).toBe(false);
  });

  test("the exported overlay route is ready only after main-frame load", () => {
    const url = "app://poc2/overlay/index.html";
    expect(isOverlayRendererReady(url, true)).toBe(false);
    expect(isOverlayRendererReady(url, false)).toBe(true);
  });
});
