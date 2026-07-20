import { describe, expect, test } from "bun:test";
import {
  REWARD_TOKENS,
  buildRewardSurface,
  formatRewardMarker,
  rewardOverlayPayload,
  toHyprPayload,
} from "../overlay/rewards";
import type { PricedRow } from "../ocr/priceSource";

function row(over: Partial<PricedRow>): PricedRow {
  return {
    key: "x",
    name: "Uhtred's Saga",
    quantity: 1,
    method: "exact",
    score: 1,
    perUnit: 1.2,
    total: 1.2,
    totalDivine: 1.2,
    unit: "div",
    ...over,
  };
}

describe("reward surface model", () => {
  const rows = [
    row({}),
    row({
      key: "greater",
      name: "Greater Chaos Orb",
      quantity: 3,
      perUnit: 0.36,
      total: 1.08,
    }),
  ];

  test("stack docks beside a left-side capture instead of covering it", () => {
    const model = buildRewardSurface(
      { x: 40, y: 120, width: 600, height: 180 },
      rows,
      2560,
      1440,
    );
    expect(model.kind).toBe("stack");
    if (model.kind !== "stack") return;
    expect(model.strip.x).toBe(40 + 600 + REWARD_TOKENS.edge);
    expect(model.strip.width).toBe(REWARD_TOKENS.panelWidth);
    expect(toHyprPayload(model).style?.background).toEndWith("c8");
  });

  test("stack flips to the left when a right-side capture has no room", () => {
    const model = buildRewardSurface(
      { x: 1900, y: 80, width: 600, height: 200 },
      rows,
      2560,
      1440,
    );
    expect(model.kind).toBe("stack");
    if (model.kind !== "stack") return;
    expect(model.strip.x).toBe(1900 - REWARD_TOKENS.panelWidth - REWARD_TOKENS.edge);
    expect(model.rows[1]).toMatchObject({
      label: "3x Greater Chaos Orb",
      value: "1.1 div",
      detail: "0.4 div ea",
    });
  });

  test("positions icon-and-value markers at OCR row centers", () => {
    const model = buildRewardSurface(
      { x: 40, y: 120, width: 600, height: 400 },
      [
        row({
          geometry: {
            bbox: { x0: 0.3, y0: 0.18, x1: 0.9, y1: 0.22 },
            baseline: { x0: 0.3, y0: 0.21, x1: 0.9, y1: 0.21 },
            center: { x: 0.6, y: 0.2 },
          },
        }),
        row({
          key: "greater",
          quantity: 3,
          perUnit: 20,
          total: 60,
          totalDivine: 0.3,
          unit: "ex",
          geometry: {
            bbox: { x0: 0.3, y0: 0.68, x1: 0.9, y1: 0.72 },
            baseline: { x0: 0.3, y0: 0.71, x1: 0.9, y1: 0.71 },
            center: { x: 0.6, y: 0.7 },
          },
        }),
      ],
      2560,
      1440,
      {
        supportsPositionedRows: true,
        iconIds: { div: "poc2.currency.div", ex: "poc2.currency.ex" },
        ttlMs: 0,
      },
    );
    expect(model.kind).toBe("positioned");
    if (model.kind !== "positioned") return;
    expect(model.ttlMs).toBe(0);
    expect(model.strip).toEqual({
      x: 40 + 600 + REWARD_TOKENS.edge,
      y: 120,
      width: REWARD_TOKENS.markerWidth,
      height: 400,
    });
    expect(model.markers[0]).toMatchObject({
      label: "1.2",
      top: 60,
      height: REWARD_TOKENS.markerHeight,
      iconRef: "poc2.currency.div",
      color: REWARD_TOKENS.colorHighest,
    });
    expect(model.markers[1]).toMatchObject({
      label: "60 (20 each)",
      top: 260,
      iconRef: "poc2.currency.ex",
    });

    const payload = toHyprPayload(model);
    expect(payload.style?.background).toBe("#00000000");
    expect(payload.rect).toEqual({
      x: model.strip.x,
      y: model.strip.y,
      w: model.strip.width,
      h: model.strip.height,
    });
    expect(payload.rows?.[0]).toMatchObject({
      label: "1.2",
      top: 60,
      height: 40,
      iconId: "poc2.currency.div",
      color: "#50ff78ff",
    });
  });

  test("rewardOverlayPayload remains a thin hypr wrapper", () => {
    const payload = rewardOverlayPayload(
      { x: 40, y: 120, width: 600, height: 180 },
      rows,
      2560,
      1440,
    );
    expect(payload.rect.x).toBe(652);
    expect(payload.rect.w).toBe(380);
  });

  test("marker label omits unit when an icon is present", () => {
    expect(formatRewardMarker(row({ total: 2.5, unit: "div" }), true)).toBe("2.5");
    expect(formatRewardMarker(row({ total: 2.5, unit: "div" }), false)).toBe("2.5 div");
  });
});
