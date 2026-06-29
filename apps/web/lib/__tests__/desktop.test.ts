import { afterEach, describe, expect, test } from "bun:test";
import { getDesktopBridge, isDesktop, type Poc2DesktopBridge } from "../desktop";

// bun test has no DOM: `window` is absent unless we stub it on globalThis.
const g = globalThis as { window?: unknown };

afterEach(() => {
  delete g.window;
});

describe("getDesktopBridge", () => {
  test("null when window is undefined (SSR)", () => {
    expect(getDesktopBridge()).toBeNull();
    expect(isDesktop()).toBe(false);
  });

  test("null in a plain browser window", () => {
    g.window = {};
    expect(getDesktopBridge()).toBeNull();
    expect(isDesktop()).toBe(false);
  });

  test("returns the preload bridge when exposed", () => {
    const seen: string[] = [];
    const bridge: Poc2DesktopBridge = {
      onItemText: (cb) => {
        cb("Item Class: Body Armours");
        return () => seen.push("off");
      },
      captureNow: async () => true,
      captureStatus: async () => ({
        platform: "linux",
        lastTool: null,
        lastError: null,
        hotkeyRegistered: false,
      }),
      openExternal: (url) => seen.push(url),
      tradeSearch: async () => ({ id: "x", result: [], total: 0 }),
      tradeFetch: async () => ({}),
      fetchJson: async () => ({}),
      versions: async () => ({ electron: "41.0.0" }),
      capabilities: async () => ({
        silentRegionCapture: true,
        overlayMode: "full",
        sessionKind: "win32",
      }),
      captureRegion: async () => ({ ok: true, dataUrl: "data:,", width: 1, height: 1 }),
      overlayShow: async () => "full",
      overlayHide: async () => true,
      overlaySetRegion: async () => true,
      calibrateRegion: async () => true,
      onRegionCalibrated: () => () => seen.push("off-region"),
      onOverlayState: () => () => seen.push("off-overlay"),
    };
    g.window = { poc2Desktop: bridge };

    expect(getDesktopBridge()).toBe(bridge);
    expect(isDesktop()).toBe(true);

    // The contract round-trips: subscribe fires, unsubscribe is callable.
    let captured = "";
    const off = getDesktopBridge()!.onItemText((t) => (captured = t));
    off();
    expect(captured).toBe("Item Class: Body Armours");
    expect(seen).toEqual(["off"]);
  });
});
