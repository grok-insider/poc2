// Main-process proxy for the official PoE2 trade API.
//
// Lives in main (not the renderer) because the API has no CORS headers and
// rate limiting must be centralized. The renderer passes a prebuilt query
// (constructed web-side from bundle data); this layer only transports.
import { net } from "electron";
import { RateLimiter } from "./rateLimiter";

const HOST = "https://www.pathofexile.com";
const UA = "poc2/1.1 (Path of Crafting 2; github.com/0xfell) Electron";

const searchLimiter = new RateLimiter();
const fetchLimiter = new RateLimiter();

export interface TradeSearchResponse {
  id: string;
  result: string[];
  total: number;
}

function headerRecord(res: Response): Record<string, string | undefined> {
  const out: Record<string, string | undefined> = {};
  res.headers.forEach((v, k) => {
    out[k.toLowerCase()] = v;
  });
  return out;
}

async function call(limiter: RateLimiter, url: string, init: RequestInit): Promise<unknown> {
  await limiter.acquire();
  const res = await net.fetch(url, {
    ...init,
    headers: {
      "User-Agent": UA,
      Accept: "application/json",
      ...(init.headers ?? {}),
    },
  });
  limiter.updateFromHeaders(headerRecord(res));
  if (res.status === 429) {
    const retry = Number(res.headers.get("retry-after") ?? "10");
    throw new Error(`trade API rate-limited; retry in ${retry}s`);
  }
  if (!res.ok) {
    const body = await res.text().catch(() => "");
    throw new Error(`trade API ${res.status}: ${body.slice(0, 300)}`);
  }
  return res.json();
}

/**
 * POST /api/trade2/search/{league} with a query JSON.
 * (The `poe2/` realm segment exists only in website deep-link URLs, not in
 * the API path — verified against Exiled-Exchange-2.)
 */
export async function tradeSearch(league: string, query: unknown): Promise<TradeSearchResponse> {
  const url = `${HOST}/api/trade2/search/${encodeURIComponent(league)}`;
  return (await call(searchLimiter, url, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(query),
  })) as TradeSearchResponse;
}

/** GET /api/trade2/fetch/{ids}?query={searchId} — max 10 ids per call. */
export async function tradeFetch(ids: string[], searchId: string): Promise<unknown> {
  const batch = ids.slice(0, 10).map(encodeURIComponent).join(",");
  const url = `${HOST}/api/trade2/fetch/${batch}?query=${encodeURIComponent(searchId)}`;
  return call(fetchLimiter, url, { method: "GET" });
}
