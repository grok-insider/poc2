"use client";

/// Base-item icon manifest loader. The icons + `manifest.json` are produced by
/// the `fetch-base-icons` pipeline tool (poe2db class-listing scrape, ALL
/// released gear bases, metadata-id joined) into `apps/web/public/base-icons/`.
/// They're a regenerable, gitignored artifact — when absent, callers fall
/// back to a letter glyph.

import type { BaseIconManifest } from "./types";

let cached: BaseIconManifest | null = null;
let loading: Promise<BaseIconManifest | null> | null = null;

/** Load (and cache) the manifest. Returns `null` if it isn't present. */
export async function loadBaseIconManifest(): Promise<BaseIconManifest | null> {
  if (cached) return cached;
  if (loading) return loading;
  loading = (async () => {
    try {
      // `no-cache` revalidates — the manifest changes whenever the operator
      // re-runs fetch-base-icons, and a stale cached copy hides new icons.
      const r = await fetch("/base-icons/manifest.json", { cache: "no-cache" });
      if (!r.ok) return null;
      cached = (await r.json()) as BaseIconManifest;
      return cached;
    } catch {
      return null;
    } finally {
      loading = null;
    }
  })();
  return loading;
}

/** Resolve the public URL for a base id, or `null` if unknown/unmapped. */
export function baseIconUrl(
  manifest: BaseIconManifest | null,
  baseId: string | null | undefined,
): string | null {
  if (!manifest || !baseId) return null;
  const entry = manifest.entries[baseId];
  return entry ? `/base-icons/${entry.rel}` : null;
}

/** Case-insensitive name → first matching entry (built lazily per manifest object). */
const nameIndexCache = new WeakMap<BaseIconManifest, Map<string, string>>();

function nameIndex(manifest: BaseIconManifest): Map<string, string> {
  let idx = nameIndexCache.get(manifest);
  if (idx) return idx;
  idx = new Map();
  for (const entry of Object.values(manifest.entries)) {
    const key = entry.name.trim().toLowerCase();
    if (key && !idx.has(key)) idx.set(key, entry.rel);
  }
  nameIndexCache.set(manifest, idx);
  return idx;
}

/** Resolve art by base display name (e.g. "Stocky Mitts"), or null. */
export function baseIconUrlByName(
  manifest: BaseIconManifest | null,
  name: string | null | undefined,
): string | null {
  if (!manifest || !name) return null;
  const rel = nameIndex(manifest).get(name.trim().toLowerCase());
  return rel ? `/base-icons/${rel}` : null;
}
