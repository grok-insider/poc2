/** Load unique catalog + lookup by name. */

import type { UniqueCatalog, UniqueCatalogEntry } from "./types";

let cached: UniqueCatalog | null = null;
let loading: Promise<UniqueCatalog | null> | null = null;

export async function loadUniqueCatalog(): Promise<UniqueCatalog | null> {
  if (cached) return cached;
  if (loading) return loading;
  loading = (async () => {
    try {
      const r = await fetch("/unique-icons/catalog.json", { cache: "no-cache" });
      if (!r.ok) return null;
      cached = (await r.json()) as UniqueCatalog;
      return cached;
    } catch {
      return null;
    } finally {
      loading = null;
    }
  })();
  return loading;
}

/** For tests / SSR. */
export function setUniqueCatalogForTests(catalog: UniqueCatalog | null): void {
  cached = catalog;
}

export function lookupUnique(
  catalog: UniqueCatalog | null | undefined,
  name: string | null | undefined,
): UniqueCatalogEntry | null {
  if (!catalog || !name) return null;
  return catalog.entries[name.trim().toLowerCase()] ?? null;
}
