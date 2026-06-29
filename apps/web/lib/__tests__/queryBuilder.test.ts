import { describe, expect, test } from "bun:test";
import { buildIndex, type TradeStat } from "../trade/statIndex";
import {
  buildTradeQuery,
  chooseStatId,
  extractItemLines,
  tradeSiteUrl,
} from "../trade/queryBuilder";

// Inline fixture index — never the real trade-stats.json.
const STATS: TradeStat[] = [
  {
    ref: "# to maximum Life",
    better: 1,
    matchers: [{ string: "+# to maximum Life" }],
    ids: { explicit: ["explicit.stat_life"], fractured: ["fractured.stat_life"] },
  },
  {
    ref: "#% to Cold Resistance",
    better: 1,
    matchers: [{ string: "+#% to Cold Resistance" }],
    ids: { explicit: ["explicit.stat_cold"] },
  },
  {
    ref: "# Attribute Requirements",
    better: -1,
    matchers: [{ string: "#% reduced Attribute Requirements" }],
    ids: { explicit: ["explicit.stat_attr"] },
  },
  {
    ref: "+# to Spirit (pseudo only)",
    better: 1,
    matchers: [{ string: "+# to Spirit" }],
    ids: { pseudo: ["pseudo.pseudo_spirit"], rune: ["rune.stat_spirit"] },
  },
];

const index = buildIndex(STATS);

const BASE_INPUT = {
  baseName: "Sacrificial Mantle" as string | null,
  rarity: "rare",
  ilvl: 81,
  league: "Runes of Aldur",
  index,
};

describe("chooseStatId", () => {
  test("bucket priority explicit → pseudo → implicit → fractured → rune → enchant", () => {
    expect(chooseStatId({ rune: ["rune.x"], explicit: ["explicit.x"] })).toBe("explicit.x");
    expect(chooseStatId({ rune: ["rune.x"], pseudo: ["pseudo.x"] })).toBe("pseudo.x");
    expect(chooseStatId({ enchant: ["enchant.x"] })).toBe("enchant.x");
    expect(chooseStatId({})).toBeNull();
    expect(chooseStatId({ explicit: [] })).toBeNull();
  });
});

describe("buildTradeQuery", () => {
  test("exact POST body for matched + unmatched lines", () => {
    const { query, rows } = buildTradeQuery({
      ...BASE_INPUT,
      lines: ["+85 to maximum Life", "+30% to Cold Resistance", "Strange flavour text"],
    });
    expect(query).toEqual({
      query: {
        status: { option: "online" },
        type: "Sacrificial Mantle",
        stats: [
          {
            type: "and",
            filters: [
              { id: "explicit.stat_life", value: { min: 76.5 }, disabled: false },
              { id: "explicit.stat_cold", value: { min: 27 }, disabled: false },
            ],
          },
        ],
      },
      sort: { price: "asc" },
    });
    expect(rows).toHaveLength(3);
    expect(rows[0]).toMatchObject({ id: "explicit.stat_life", value: 85, min: 76.5, enabled: true });
    expect(rows[2]).toMatchObject({ id: null, value: null, min: null, max: null, enabled: false });
  });

  test("min rounding floors to one decimal", () => {
    const { rows } = buildTradeQuery({ ...BASE_INPUT, lines: ["+87 to maximum Life"] });
    // 87 * 0.9 = 78.3 → floor(783)/10
    expect(rows[0].min).toBe(78.3);
  });

  test("custom minFactor", () => {
    const { rows } = buildTradeQuery({
      ...BASE_INPUT,
      lines: ["+85 to maximum Life"],
      minFactor: 0.5,
    });
    expect(rows[0].min).toBe(42.5);
  });

  test("better=-1 sets a ceiled max instead of a min", () => {
    const { query, rows } = buildTradeQuery({
      ...BASE_INPUT,
      lines: ["15% reduced Attribute Requirements"],
    });
    // 15 * 1.1 = 16.5 → ceil; the filter carries max, not min.
    expect(rows[0]).toMatchObject({ id: "explicit.stat_attr", min: null, max: 16.5 });
    expect(query.query.stats[0].filters).toEqual([
      { id: "explicit.stat_attr", value: { max: 16.5 }, disabled: false },
    ]);
  });

  test("disabled rows are excluded from the filters", () => {
    const { query, rows } = buildTradeQuery({
      ...BASE_INPUT,
      lines: ["+85 to maximum Life", "+30% to Cold Resistance"],
      enabled: new Set([1]),
    });
    expect(rows[0].enabled).toBe(false);
    expect(query.query.stats[0].filters).toEqual([
      { id: "explicit.stat_cold", value: { min: 27 }, disabled: false },
    ]);
  });

  test("bound overrides replace the derived bound", () => {
    const { query } = buildTradeQuery({
      ...BASE_INPUT,
      lines: ["+85 to maximum Life"],
      bounds: new Map([[0, 80]]),
    });
    expect(query.query.stats[0].filters[0]).toEqual({
      id: "explicit.stat_life",
      value: { min: 80 },
      disabled: false,
    });
  });

  test("pseudo bucket wins when no explicit id exists", () => {
    const { rows } = buildTradeQuery({ ...BASE_INPUT, lines: ["+40 to Spirit"] });
    expect(rows[0].id).toBe("pseudo.pseudo_spirit");
  });

  test("ilvl misc filter only when includeIlvl", () => {
    const off = buildTradeQuery({ ...BASE_INPUT, lines: ["+85 to maximum Life"] });
    expect(off.query.query.filters).toBeUndefined();
    const on = buildTradeQuery({
      ...BASE_INPUT,
      lines: ["+85 to maximum Life"],
      includeIlvl: true,
    });
    expect(on.query.query.filters).toEqual({
      misc_filters: { filters: { ilvl: { min: 81 } } },
    });
  });

  test("unique rarity consumes the leading name line into query.name", () => {
    const { query, rows } = buildTradeQuery({
      ...BASE_INPUT,
      rarity: "unique",
      lines: ["Kaom's Heart", "+85 to maximum Life"],
    });
    expect(query.query.name).toBe("Kaom's Heart");
    expect(rows).toHaveLength(1);
    expect(rows[0].id).toBe("explicit.stat_life");
  });

  test("no base name ⇒ no type key", () => {
    const { query } = buildTradeQuery({
      ...BASE_INPUT,
      baseName: null,
      lines: ["+85 to maximum Life"],
    });
    expect("type" in query.query).toBe(false);
  });
});

describe("tradeSiteUrl", () => {
  test("poe2 realm deep link with encoded league + query", () => {
    const { query } = buildTradeQuery({ ...BASE_INPUT, lines: ["+85 to maximum Life"] });
    const url = tradeSiteUrl("Runes of Aldur", query);
    expect(url.startsWith("https://www.pathofexile.com/trade2/search/poe2/Runes%20of%20Aldur?q=")).toBe(true);
    const q = decodeURIComponent(url.split("?q=")[1]);
    expect(JSON.parse(q)).toEqual(query);
  });
});

describe("extractItemLines", () => {
  const TEXT = [
    "Item Class: Body Armours",
    "Rarity: Rare",
    "Corruption Carapace",
    "Sacrificial Mantle",
    "--------",
    "Quality: +20% (augmented)",
    "Energy Shield: 410 (augmented)",
    "--------",
    "Requirements:",
    "Level: 65",
    "Int: 157",
    "--------",
    "Item Level: 81",
    "--------",
    "+85 to maximum Life",
    "+30% to Cold Resistance (fractured)",
    "--------",
    "Corrupted",
  ].join("\n");

  test("splits header names from candidate stat lines", () => {
    expect(extractItemLines(TEXT)).toEqual({
      name: "Corruption Carapace",
      baseName: "Sacrificial Mantle",
      lines: ["+85 to maximum Life", "+30% to Cold Resistance (fractured)"],
    });
  });

  test("tolerates text without separators", () => {
    expect(extractItemLines("just a line")).toEqual({
      name: "just a line",
      baseName: null,
      lines: [],
    });
  });
});
