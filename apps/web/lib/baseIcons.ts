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
