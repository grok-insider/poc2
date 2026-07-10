import { describe, expect, test } from "bun:test";
import { rewardOverlayPayload } from "../overlay/rewards";
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

describe("reward compositor payload", () => {
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

  test("docks beside a left-side capture instead of covering it", () => {
    const payload = rewardOverlayPayload(
      { x: 40, y: 120, width: 600, height: 180 },
      rows,
      2560,
      1440,
    );
    expect(payload.rect.x).toBe(652);
    expect(payload.rect.w).toBe(380);
    expect(payload.style?.background).toEndWith("c8");
  });

  test("flips to the left when a right-side capture has no room", () => {
    const payload = rewardOverlayPayload(
      { x: 1900, y: 80, width: 600, height: 200 },
      rows,
      2560,
      1440,
    );
    expect(payload.rect.x).toBe(1508);
    expect(payload.rows?.[1]).toMatchObject({
      label: "3x Greater Chaos Orb",
      value: "1.1 div",
      detail: "0.4 div ea",
    });
  });

  test("positions icon-and-value markers at OCR row centers", () => {
    const payload = rewardOverlayPayload(
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
    expect(payload.ttlMs).toBe(0);
    expect(payload.style?.background).toBe("#00000000");
    expect(payload.rect).toEqual({ x: 652, y: 120, w: 190, h: 400 });
    expect(payload.rows?.[0]).toMatchObject({
      label: "1.2",
      top: 60,
      height: 40,
      iconId: "poc2.currency.div",
      color: "#50ff78ff",
    });
    expect(payload.rows?.[1]).toMatchObject({
      label: "60 (20 each)",
      top: 260,
      iconId: "poc2.currency.ex",
    });
  });
});
