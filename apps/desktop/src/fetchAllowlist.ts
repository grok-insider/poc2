// Pure allowlist for renderer→main JSON fetches (CORS bypass). Kept electron-
// free so it's unit-testable under bun; ipc.ts re-exports it.

/** Hosts the renderer may fetch JSON from via main. */
export const FETCH_ALLOWLIST = [
  "poe2scout.com",
  "www.pathofexile.com",
  "poe.ninja",
];

export function isAllowlistedUrl(raw: string): boolean {
  try {
    const u = new URL(raw);
    return u.protocol === "https:" && FETCH_ALLOWLIST.includes(u.hostname);
  } catch {
    return false;
  }
}
