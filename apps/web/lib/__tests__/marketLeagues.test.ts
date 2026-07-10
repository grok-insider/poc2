import { describe, expect, test } from "bun:test";
import {
  DEFAULT_MARKET_LEAGUE,
  MARKET_LEAGUE_VERSION,
  migrateMarketLeague,
} from "../marketLeagues";

describe("market league migration", () => {
  test("moves the previous challenge default to the current HC default once", () => {
    expect(migrateMarketLeague(undefined, undefined)).toBe(DEFAULT_MARKET_LEAGUE);
    expect(migrateMarketLeague("Runes of Aldur", undefined)).toBe(DEFAULT_MARKET_LEAGUE);
  });

  test("preserves explicit choices after the migration version is stored", () => {
    expect(migrateMarketLeague("Runes of Aldur", MARKET_LEAGUE_VERSION)).toBe("Runes of Aldur");
    expect(migrateMarketLeague("Standard", undefined)).toBe("Standard");
  });
});
