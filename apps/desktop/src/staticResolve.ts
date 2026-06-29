// Pure URL→file resolution for the app:// static server (no electron
// imports — unit-tested under bun directly).
import path from "node:path";

/**
 * Map an app:// URL to a file inside `root`, or null when the URL escapes
 * the root or cannot be parsed.
 */
export function resolveAppUrl(root: string, rawUrl: string): string | null {
  let pathname: string;
  try {
    pathname = decodeURIComponent(new URL(rawUrl).pathname);
  } catch {
    return null;
  }
  if (pathname.endsWith("/")) pathname += "index.html";
  if (pathname === "") pathname = "/index.html";
  const joined = path.normalize(path.join(root, pathname));
  // Path-traversal guard: the resolved file must stay inside root.
  if (joined !== root && !joined.startsWith(root + path.sep)) return null;
  return joined;
}

/** SPA fallback applies only to extensionless paths (deep links), not assets. */
export function wantsSpaFallback(file: string): boolean {
  return !path.basename(file).includes(".");
}
