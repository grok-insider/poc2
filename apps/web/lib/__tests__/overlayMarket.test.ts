import { afterEach, describe, expect, test } from "bun:test";
import { priceCheckItemOverlay } from "../overlay/market";
import type { Poc2DesktopBridge, TradeSearchResponse } from "../desktop";

const originalFetch = globalThis.fetch;

afterEach(() => {
  globalThis.fetch = originalFetch;
});

describe("overlay market payload", () => {
  test("smart item price check uses trade2 bounds and emits a rich card", async () => {
    globalThis.fetch = (async () =>
      new Response(
        JSON.stringify({
          version: 1,
          source: "fixture",
          generated: "2026-01-01T00:00:00.000Z",
          stats: [
            {
              ref: "# to maximum Life",
              better: 1,
              matchers: [{ string: "+# to maximum Life" }],
              ids: { explicit: ["explicit.stat_life"] },
            },
            {
              ref: "# Attribute Requirements",
              better: -1,
              matchers: [{ string: "#% reduced Attribute Requirements" }],
              ids: { explicit: ["explicit.stat_attr"] },
            },
          ],
        }),
        { status: 200, headers: { "content-type": "application/json" } },
      )) as unknown as typeof fetch;

    let seenQuery: unknown = null;
    const bridge = {
      tradeSearch: async (_league: string, query: unknown): Promise<TradeSearchResponse> => {
        seenQuery = query;
        return { id: "search-id", result: ["a", "b", "c"], total: 42 };
      },
      tradeFetch: async () => ({
        result: [
          {
            listing: {
              account: { name: "seller-a" },
              price: { amount: 10, currency: "divine" },
              indexed: new Date(Date.now() - 6 * 3600 * 1000).toISOString(),
            },
            item: { ilvl: 81 },
          },
          { listing: { account: { name: "seller-b" }, price: { amount: 12, currency: "divine" } }, item: { ilvl: 82 } },
          { listing: { account: { name: "seller-c" }, price: { amount: 20, currency: "divine" } }, item: { ilvl: 83 } },
        ],
      }),
    } as unknown as Poc2DesktopBridge;

    const itemText = [
      "Item Class: Body Armours",
      "Rarity: Rare",
      "Corruption Carapace",
      "Sacrificial Mantle",
      "--------",
      "Item Level: 81",
      "Requires Level 68",
      "--------",
      "+85 to maximum Life",
      "15% reduced Attribute Requirements",
    ].join("\n");

    const result = await priceCheckItemOverlay(bridge, itemText, "Runes of Aldur");

    expect(seenQuery).toMatchObject({
      query: {
        type: "Sacrificial Mantle",
        stats: [
          {
            filters: [
              { id: "explicit.stat_life", value: { min: 76.5 }, disabled: false },
              { id: "explicit.stat_attr", value: { max: 16.5 }, disabled: false },
            ],
          },
        ],
      },
    });
    expect(result.payload.mode).toBe("cards");
    expect(result.payload.style?.font).toBe("Fontin");
    const rows = result.payload.rows ?? [];

    // Rare/unique: double-line headers (name + base) then info rows.
    expect(rows[0]).toMatchObject({ kind: "header", label: "Corruption Carapace" });
    expect(rows[1]).toMatchObject({ kind: "header", label: "Sacrificial Mantle" });

    // Item info block: rarity / item level / required level.
    expect(rows[2]).toMatchObject({ kind: "columns", cells: [{ text: "Item Rarity" }, { text: "Rare" }] });
    expect(rows[3]).toMatchObject({ kind: "columns", cells: [{ text: "Item Level" }, { text: "81" }] });
    expect(rows[4]).toMatchObject({ kind: "columns", cells: [{ text: "Requires Level" }, { text: "68" }] });

    // Mod lines (basic format: no badge, no roll%). "Requires Level" is NOT a mod.
    const modCells = rows
      .filter((r) => r.kind === "columns" && r.cells?.[1]?.color === "#8888ffff")
      .map((r) => r.cells?.[1]?.text);
    expect(modCells).toContain("+85 to maximum Life");
    expect(modCells).toContain("15% reduced Attribute Requirements");
    expect(modCells).not.toContain("Requires Level 68");

    // Estimated Value plaque + trade results line.
    const valueHeader = rows.findIndex((r) => r.kind === "header" && r.label === "Estimated Value");
    expect(valueHeader).toBeGreaterThan(0);
    expect(rows[valueHeader + 1].label).toMatch(/^≈ /);
    expect(rows[valueHeader + 2]).toMatchObject({
      kind: "columns",
      cells: [{ text: "3/42" }, { text: "matched 2/3" }, { text: "pathofexile.com/trade" }],
    });

    // Listings table: gold header row then a listing row.
    const listHead = rows.findIndex(
      (r) => r.kind === "columns" && r.cells?.[0]?.text === "Price",
    );
    expect(listHead).toBeGreaterThan(0);
    expect(rows[listHead + 1]).toMatchObject({
      kind: "columns",
      cells: [
        { text: "10 divine" },
        { text: "81" },
        { text: "-" },
        { text: "seller-a" },
        { text: "6h" },
      ],
    });

    expect(result.history).toMatchObject({
      kind: "item-price",
      title: "Corruption Carapace",
      league: "Runes of Aldur",
    });
    expect(result.history?.summary).toContain("≈ ");
    expect(result.history?.summary).toContain("42 listed");
  });

  test("advanced-format capture renders tier badges + roll scores", async () => {
    globalThis.fetch = (async () =>
      new Response(
        JSON.stringify({
          version: 1,
          source: "fixture",
          generated: "2026-01-01T00:00:00.000Z",
          stats: [
            {
              ref: "# to maximum Life",
              better: 1,
              matchers: [{ string: "+# to maximum Life" }],
              ids: { explicit: ["explicit.stat_life"] },
            },
          ],
        }),
        { status: 200, headers: { "content-type": "application/json" } },
      )) as unknown as typeof fetch;

    const bridge = {
      tradeSearch: async (): Promise<TradeSearchResponse> => ({ id: "s", result: ["a"], total: 5 }),
      tradeFetch: async () => ({
        result: [
          { listing: { account: { name: "seller-a" }, price: { amount: 8, currency: "divine" } }, item: { ilvl: 82 } },
        ],
      }),
    } as unknown as Poc2DesktopBridge;

    const itemText = [
      "Item Class: Body Armours",
      "Rarity: Rare",
      "Vile Robe",
      "Widowsilk Robe",
      "--------",
      "Item Level: 82",
      "--------",
      '{ Prefix Modifier "Rounded" (Tier: 2) }',
      "+90(80-91) to maximum Life",
    ].join("\n");

    const result = await priceCheckItemOverlay(bridge, itemText, "Runes of Aldur");
    const rows = result.payload.rows ?? [];
    const modRow = rows.find(
      (r) => r.kind === "columns" && r.cells?.[1]?.text === "+90 to maximum Life",
    );
    expect(modRow).toBeDefined();
    expect(modRow?.cells?.[0]).toMatchObject({ text: "P2" });
    // roll% present and > 90% for a near-max roll.
    expect(modRow?.cells?.[2]?.text).toMatch(/^\d+%$/);
    expect(Number.parseInt(modRow?.cells?.[2]?.text ?? "0", 10)).toBeGreaterThanOrEqual(90);
  });
});
