export const DEFAULT_MARKET_LEAGUE = "HC Runes of Aldur";
export const MARKET_LEAGUE_VERSION = 1;

export const MARKET_LEAGUE_PRESETS = [
  { label: "Runes of Aldur HC", value: "HC Runes of Aldur" },
  { label: "Runes of Aldur", value: "Runes of Aldur" },
  { label: "Hardcore", value: "Hardcore" },
  { label: "Standard", value: "Standard" },
] as const;

/** Move the previous softcore default to HC once without overriding later choices. */
export function migrateMarketLeague(
  league: string | undefined,
  version: number | undefined,
): string {
  if (version === MARKET_LEAGUE_VERSION) return league || DEFAULT_MARKET_LEAGUE;
  if (!league || league === "Runes of Aldur") return DEFAULT_MARKET_LEAGUE;
  return league;
}
