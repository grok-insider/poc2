import { describe, expect, test } from "bun:test";
import {
  hideRewardSurface,
  publishRewardSurface,
} from "../overlay/publishRewardSurface";
import { buildRewardSurface, errorRewardSurface } from "../overlay/rewards";
import type { Poc2DesktopBridge } from "../desktop";
import type { PricedRow } from "../ocr/priceSource";

function row(over: Partial<PricedRow> = {}): PricedRow {
  return {
    key: "x",
    name: "Uhtred's Saga",
    quantity: 1,
    method: "exact",
    score: 1,
    perUnit: 1,
    total: 1,
    totalDivine: 1,
    unit: "div",
    geometry: {
      bbox: { x0: 0, y0: 0.2, x1: 1, y1: 0.3 },
      baseline: { x0: 0, y0: 0.25, x1: 1, y1: 0.25 },
      center: { x: 0.5, y: 0.25 },
    },
    ...over,
  };
}

function mockBridge(calls: string[]): Pick<
  Poc2DesktopBridge,
  | "hyprOverlayRender"
  | "overlayShow"
  | "overlayHide"
  | "overlaySetContentBounds"
> {
  return {
    hyprOverlayRender: async () => {
      calls.push("hypr");
      return true;
    },
    overlayShow: async () => {
      calls.push("show");
      return "full";
    },
    overlayHide: async () => {
      calls.push("hide");
      return true;
    },
    overlaySetContentBounds: async (rect) => {
      calls.push(`bounds:${rect.x},${rect.y},${rect.width},${rect.height}`);
      return true;
    },
  };
}

describe("publishRewardSurface", () => {
  test("hyprland-plugin renders compositor payload only", async () => {
    const calls: string[] = [];
    const bridge = mockBridge(calls) as Poc2DesktopBridge;
    const model = buildRewardSurface(
      { x: 10, y: 20, width: 100, height: 200 },
      [row()],
      1920,
      1080,
      { supportsPositionedRows: true },
    );
    await publishRewardSurface(bridge, "hyprland-plugin", model);
    expect(calls).toEqual(["hypr"]);
  });

  test("full mode sets content bounds then shows", async () => {
    const calls: string[] = [];
    const bridge = mockBridge(calls) as Poc2DesktopBridge;
    const model = buildRewardSurface(
      { x: 10, y: 20, width: 100, height: 200 },
      [row()],
      1920,
      1080,
      { supportsPositionedRows: true },
    );
    expect(model.kind).toBe("positioned");
    await publishRewardSurface(bridge, "full", model);
    expect(calls[0]?.startsWith("bounds:")).toBe(true);
    expect(calls).toContain("show");
    expect(calls).not.toContain("hypr");
  });

  test("error surface still shows on full without bounds", async () => {
    const calls: string[] = [];
    const bridge = mockBridge(calls) as Poc2DesktopBridge;
    await publishRewardSurface(bridge, "full", errorRewardSurface("T", "m"));
    expect(calls).toEqual(["show"]);
  });

  test("hideRewardSurface uses the right transport", async () => {
    const calls: string[] = [];
    const bridge = mockBridge(calls) as Poc2DesktopBridge;
    await hideRewardSurface(bridge, "hyprland-plugin", { x: 1, y: 2, width: 3, height: 4 });
    await hideRewardSurface(bridge, "full", null);
    expect(calls).toEqual(["hypr", "hide"]);
  });
});
