import { describe, expect, test } from "bun:test";
import {
  applyScan,
  emptyRowLock,
  type SlotRead,
  type RowLockState,
} from "../ocr/rowLock";

function read(
  slot: string,
  key: string | null,
  method: SlotRead["method"],
  quantity = 1,
  name = key ?? "?",
  score = key ? 0.9 : 0,
): SlotRead {
  return { slot, key, name, quantity, method, score };
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
