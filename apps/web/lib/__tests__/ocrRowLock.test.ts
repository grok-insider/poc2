import { describe, expect, test } from "bun:test";
import {
  applyScan,
  emptyRowLock,
  type SlotRead,
  type RowLockState,
} from "../ocr/rowLock";
import { spatialSlotKey } from "../ocr/scan";
import type { OcrLineGeometry } from "../ocr/extractRows";

function geometry(centerY: number): OcrLineGeometry {
  return {
    bbox: { x0: 0.2, y0: centerY - 0.02, x1: 0.8, y1: centerY + 0.02 },
    baseline: { x0: 0.2, y0: centerY + 0.01, x1: 0.8, y1: centerY + 0.01 },
    center: { x: 0.5, y: centerY },
  };
}

function read(
  slot: string,
  key: string | null,
  method: SlotRead["method"],
  quantity = 1,
  name = key ?? "?",
  score = key ? 0.9 : 0,
  rowGeometry?: OcrLineGeometry,
): SlotRead {
  return { slot, key, name, quantity, method, score, geometry: rowGeometry };
}

describe("rowLock — locking thresholds", () => {
  test("an exact match locks in a single read", () => {
    const { state, rows } = applyScan(emptyRowLock(), [
      read("0", "chaos", "exact"),
    ]);
    expect(state["0"].locked).toBe(true);
    expect(rows).toHaveLength(1);
    expect(rows[0].key).toBe("chaos");
  });

  test("a 'currency' method also locks in one read", () => {
    const { rows } = applyScan(emptyRowLock(), [read("0", "divine", "currency")]);
    expect(rows).toHaveLength(1);
  });

  test("a fuzzy match needs TWO confirming reads before it locks", () => {
    let s: RowLockState = emptyRowLock();

    const first = applyScan(s, [read("0", "vaal", "fuzzy")]);
    s = first.state;
    expect(s["0"].locked).toBe(false);
    expect(first.rows).toHaveLength(0); // not shown yet

    const second = applyScan(s, [read("0", "vaal", "fuzzy")]);
    s = second.state;
    expect(s["0"].locked).toBe(true);
    expect(second.rows).toHaveLength(1);
    expect(second.rows[0].key).toBe("vaal");
  });

  test("a prefix match needs two confirming reads (same as fuzzy)", () => {
    let s = emptyRowLock();
    s = applyScan(s, [read("0", "exa", "prefix")]).state;
    expect(s["0"].locked).toBe(false);
    const r = applyScan(s, [read("0", "exa", "prefix")]);
    expect(r.rows).toHaveLength(1);
  });

  test("a CHANGED fuzzy key resets the confirmation counter", () => {
    let s = emptyRowLock();
    s = applyScan(s, [read("0", "vaal", "fuzzy")]).state;
    // different key next frame → counter resets, still not locked.
    const r = applyScan(s, [read("0", "chaos", "fuzzy")]);
    expect(r.state["0"].confirms).toBe(1);
    expect(r.state["0"].locked).toBe(false);
    expect(r.rows).toHaveLength(0);
  });
});

describe("rowLock — quantity memory across dropped frames", () => {
  test("a locked slot survives ONE dropped frame, keeping its quantity", () => {
    let s = emptyRowLock();
    s = applyScan(s, [read("0", "chaos", "exact", 4)]).state;
    expect(s["0"].quantity).toBe(4);

    // Frame where slot 0 is missing entirely (dropped read).
    const dropped = applyScan(s, []);
    s = dropped.state;
    expect(s["0"]).toBeDefined();
    expect(s["0"].quantity).toBe(4); // quantity memory held
    expect(s["0"].missing).toBe(1);
    expect(dropped.rows).toHaveLength(1); // still shown
    expect(dropped.rows[0].quantity).toBe(4);
  });

  test("a slot missing for TWO consecutive frames is dropped", () => {
    let s = emptyRowLock();
    s = applyScan(s, [read("0", "chaos", "exact", 4)]).state;
    s = applyScan(s, []).state; // missing 1 (held)
    const second = applyScan(s, []); // missing 2 (expired)
    expect(second.state["0"]).toBeUndefined();
    expect(second.rows).toHaveLength(0);
  });

  test("a reappearing slot resets its missing counter and updates quantity", () => {
    let s = emptyRowLock();
    s = applyScan(s, [read("0", "chaos", "exact", 4)]).state;
    s = applyScan(s, []).state; // missing 1
    const back = applyScan(s, [read("0", "chaos", "exact", 9)]);
    expect(back.state["0"].missing).toBe(0);
    expect(back.state["0"].quantity).toBe(9);
  });
});

describe("rowLock — multiple slots + ordering", () => {
  test("emits locked rows in numeric slot order regardless of read order", () => {
    const { rows } = applyScan(emptyRowLock(), [
      read("2", "gamma", "exact"),
      read("0", "alpha", "exact"),
      read("1", "beta", "exact"),
    ]);
    expect(rows.map((r) => r.key)).toEqual(["alpha", "beta", "gamma"]);
  });

  test("sorts positioned rows top-to-bottom instead of by slot text", () => {
    const { rows } = applyScan(emptyRowLock(), [
      read("0", "bottom", "exact", 1, "bottom", 1, geometry(0.8)),
      read("1", "top", "exact", 1, "top", 1, geometry(0.2)),
    ]);
    expect(rows.map((row) => row.key)).toEqual(["top", "bottom"]);
  });

  test("unlocked (fuzzy, single-read) slots are excluded from rows", () => {
    const { rows } = applyScan(emptyRowLock(), [
      read("0", "alpha", "exact"),
      read("1", "beta", "fuzzy"), // 1 read only → not yet locked
    ]);
    expect(rows.map((r) => r.key)).toEqual(["alpha"]);
  });

  test("a previously-locked slot stays locked when it keeps resolving the same key", () => {
    let s = emptyRowLock();
    s = applyScan(s, [read("0", "vaal", "fuzzy")]).state;
    s = applyScan(s, [read("0", "vaal", "fuzzy")]).state; // locked
    const r = applyScan(s, [read("0", "vaal", "fuzzy", 3)]);
    expect(r.state["0"].locked).toBe(true);
    expect(r.rows[0].quantity).toBe(3);
  });
});

describe("rowLock — spatial slots", () => {
  test("a missing middle line does not shift lower rows and keeps its geometry", () => {
    let s = applyScan(emptyRowLock(), [
      read(spatialSlotKey(0.2), "top", "exact", 1, "top", 1, geometry(0.2)),
      read(spatialSlotKey(0.4), "middle", "exact", 2, "middle", 1, geometry(0.4)),
      read(spatialSlotKey(0.6), "bottom", "exact", 3, "bottom", 1, geometry(0.6)),
    ]).state;

    const next = applyScan(s, [
      read(spatialSlotKey(0.204), "top", "exact", 1, "top", 1, geometry(0.204)),
      read(
        spatialSlotKey(0.604),
        "bottom",
        "exact",
        4,
        "bottom",
        1,
        geometry(0.604),
      ),
    ]);
    s = next.state;

    expect(next.rows.map((row) => row.key)).toEqual(["top", "middle", "bottom"]);
    expect(s[spatialSlotKey(0.4)].missing).toBe(1);
    expect(s[spatialSlotKey(0.4)].geometry?.center.y).toBe(0.4);
    expect(s[spatialSlotKey(0.6)].quantity).toBe(4);
  });

  test("keeps the latest bbox while smoothing center jitter", () => {
    const slot = spatialSlotKey(0.2);
    let s = applyScan(emptyRowLock(), [
      read(slot, "chaos", "exact", 1, "chaos", 1, geometry(0.2)),
    ]).state;
    s = applyScan(s, [
      read(slot, "chaos", "exact", 1, "chaos", 1, geometry(0.204)),
    ]).state;

    expect(s[slot].geometry?.bbox.y0).toBeCloseTo(0.184);
    expect(s[slot].geometry?.center.y).toBeCloseTo(0.202);
    const dropped = applyScan(s, []);
    expect(dropped.rows[0].geometry?.center.y).toBeCloseTo(0.202);
  });

  test("reuses the prior spatial slot across a quantization boundary", () => {
    const geometry = (y: number) => ({
      bbox: { x0: 0.2, y0: y - 0.01, x1: 0.8, y1: y + 0.01 },
      baseline: { x0: 0.2, y0: y, x1: 0.8, y1: y },
      center: { x: 0.5, y },
    });
    const first = applyScan(emptyRowLock(), [
      {
        slot: "y:010",
        key: "same",
        name: "Same Rune",
        quantity: 1,
        method: "exact",
        score: 1,
        geometry: geometry(0.209),
      },
    ]);
    const second = applyScan(first.state, [
      {
        slot: "y:011",
        key: "same",
        name: "Same Rune",
        quantity: 1,
        method: "exact",
        score: 1,
        geometry: geometry(0.211),
      },
    ]);
    expect(Object.keys(second.state)).toEqual(["y:010"]);
    expect(second.rows).toHaveLength(1);
  });
});
