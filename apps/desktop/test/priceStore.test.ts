import { afterAll, describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import {
  openPriceStore,
  priceBackend,
  priceCount,
  priceSnapshot,
  replaceLeaguePrices,
} from "../src/prices/store";
import type { PriceRow } from "../src/prices/types";

const dir = mkdtempSync(path.join(tmpdir(), "poc2-prices-"));
afterAll(() => rmSync(dir, { recursive: true, force: true }));

function row(over: Partial<PriceRow>): PriceRow {
  return {
    league: "L",
    category: "currency",
    apiId: "divine-orb",
    name: "Divine Orb",
    normalizedName: "divine orb",
    priceExalt: 200,
    priceDivine: 1,
    stackMax: 20,
    iconUrl: null,
    fetchedAt: "2026-06-29T00:00:00Z",
    ...over,
  };
}

describe("price store", () => {
  test("opens with a usable backend (sqlite or json fallback)", () => {
    const { backend } = openPriceStore(dir);
    expect(["sqlite", "json"]).toContain(backend);
    expect(priceBackend()).toBe(backend);
  });

  test("replaceLeague + snapshot round-trips names and prices", () => {
    replaceLeaguePrices("L", [
      row({ apiId: "divine-orb", name: "Divine Orb", normalizedName: "divine orb", priceDivine: 1 }),
      row({ apiId: "mirror", name: "Mirror of Kalandra", normalizedName: "mirror of kalandra", priceDivine: 2500 }),
    ]);
    expect(priceCount("L")).toBe(2);

    const snap = priceSnapshot("L");
    expect(snap.names).toContain("Divine Orb");
    expect(snap.names).toContain("Mirror of Kalandra");
    expect(snap.byName["divine orb"]).toEqual({ perUnit: 1, unit: "div" });
    expect(snap.byName["mirror of kalandra"]).toEqual({ perUnit: 2500, unit: "div" });
    expect(snap.fetchedAt).toBe("2026-06-29T00:00:00Z");
  });

  test("replace is a full swap for the league (stale rows gone)", () => {
    replaceLeaguePrices("L", [row({ apiId: "only", name: "Chaos Orb", normalizedName: "chaos orb", priceDivine: 0.05 })]);
    const snap = priceSnapshot("L");
    expect(priceCount("L")).toBe(1);
    expect(snap.byName["divine orb"]).toBeUndefined();
    expect(snap.byName["chaos orb"]).toEqual({ perUnit: 0.05, unit: "div" });
  });

  test("null-priced rows appear in names but not byName", () => {
    replaceLeaguePrices("L", [
      row({ apiId: "np", name: "No Price Rune", normalizedName: "no price rune", priceDivine: null }),
    ]);
    const snap = priceSnapshot("L");
    expect(snap.names).toContain("No Price Rune");
    expect(snap.byName["no price rune"]).toBeUndefined();
  });

  test("unknown league → empty snapshot", () => {
    const snap = priceSnapshot("NOPE");
    expect(snap.names).toEqual([]);
    expect(snap.fetchedAt).toBeNull();
  });

  test("poe.ninja fallback fills poe2scout gaps without overriding real prices", () => {
    // poe2scout: Divine priced, Some Rune unpriced. Append ninja rows after,
    // mirroring scheduler.refreshNow's [...scout, ...ninja] ordering.
    replaceLeaguePrices("L", [
      row({ apiId: "divine-orb", name: "Divine Orb", normalizedName: "divine orb", priceDivine: 1 }),
      row({ apiId: "rune", name: "Some Rune", normalizedName: "some rune", priceDivine: null }),
      // ninja fallback rows
      row({ category: "ninja", apiId: "ninja:divine orb", name: "Divine Orb", normalizedName: "divine orb", priceDivine: 999 }),
      row({ category: "ninja", apiId: "ninja:some rune", name: "Some Rune", normalizedName: "some rune", priceDivine: 0.25 }),
    ]);
    const snap = priceSnapshot("L");
    // poe2scout's Divine wins over ninja's (first write wins).
    expect(snap.byName["divine orb"]).toEqual({ perUnit: 1, unit: "div" });
    // ninja fills the rune poe2scout left null.
    expect(snap.byName["some rune"]).toEqual({ perUnit: 0.25, unit: "div" });
  });
});
