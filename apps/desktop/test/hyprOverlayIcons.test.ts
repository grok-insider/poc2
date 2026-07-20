import { beforeEach, describe, expect, test } from "bun:test";
import {
  bgraToRgba,
  isAllowedPriceIconUrl,
  prepareHyprOverlayPriceIcons,
  preparePriceIconDataUrls,
  resetHyprOverlayPriceIconCacheForTests,
  type PriceIconDataUrlDependencies,
  type PriceIconDependencies,
} from "../src/capture/hyprOverlayIcons";

beforeEach(() => resetHyprOverlayPriceIconCacheForTests());

describe("hypr-overlay price icons", () => {
  test("converts nativeImage BGRA bytes to plugin RGBA", () => {
    expect([...bgraToRgba(Uint8Array.of(3, 2, 1, 255, 7, 6, 5, 255))]).toEqual([
      1, 2, 3, 255, 5, 6, 7, 255,
    ]);
    expect(() => bgraToRgba(Uint8Array.of(1))).toThrow();
    expect([...bgraToRgba(Uint8Array.of(25, 50, 100, 128))]).toEqual([
      199, 100, 50, 128,
    ]);
  });

  test("accepts only trusted HTTPS icon hosts", () => {
    expect(isAllowedPriceIconUrl("https://web.poecdn.com/image.png")).toBe(true);
    expect(isAllowedPriceIconUrl("https://poe2scout.com/icon.png")).toBe(true);
    expect(isAllowedPriceIconUrl("http://web.poecdn.com/image.png")).toBe(false);
    expect(isAllowedPriceIconUrl("https://poecdn.com.evil.example/image.png")).toBe(false);
  });

  test("registers each decoded unit once and returns available ids", async () => {
    const registered: Array<{ id: string; rgbaBase64: string }> = [];
    const deps: PriceIconDependencies = {
      fetchBytes: async () => Buffer.from("png"),
      decodeBgra: async () => ({ width: 1, height: 1, data: Buffer.from([3, 2, 1, 255]) }),
      register: async (input) => {
        registered.push(input);
        return true;
      },
    };
    const urls = {
      div: "https://web.poecdn.com/div.png",
      ex: "https://web.poecdn.com/ex.png",
    };
    expect(await prepareHyprOverlayPriceIcons(urls, deps)).toEqual({
      div: "poc2.currency.div",
      ex: "poc2.currency.ex",
    });
    await prepareHyprOverlayPriceIcons(urls, deps);
    expect(registered).toHaveLength(2);
    expect(Buffer.from(registered[0].rgbaBase64, "base64")).toEqual(Buffer.from([1, 2, 3, 255]));
  });

  test("Electron path returns data URLs from the same allowlisted sources", async () => {
    const deps: PriceIconDataUrlDependencies = {
      fetchBytes: async () => Buffer.from("png"),
      decodeBgra: async () => ({ width: 1, height: 1, data: Buffer.from([3, 2, 1, 255]) }),
      toDataUrl: async () => "data:image/png;base64,abc",
    };
    const urls = {
      div: "https://web.poecdn.com/div.png",
      ex: "https://evil.example/ex.png",
    };
    expect(await preparePriceIconDataUrls(urls, deps)).toEqual({
      div: "data:image/png;base64,abc",
    });
    // Cache hit: no second toDataUrl needed for same URL.
    expect(await preparePriceIconDataUrls(urls, deps)).toEqual({
      div: "data:image/png;base64,abc",
    });
  });
});
