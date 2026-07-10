/**
 * Resolve local (or cached) item artwork for tooltips.
 * Bases → base-icons manifest; uniques → unique-icons manifest.
 * Art is regenerable/gitignored (fetch-base-icons / fetch-unique-icons).
 */

import { baseIconUrl, baseIconUrlByName } from "./baseIcons";
import type { ItemPopupKind } from "./itemPopup/model";
import type { BaseIconManifest } from "./types";

export interface UniqueIconManifestEntry {
  /** Display name as on poe2db (e.g. "Facebreaker"). */
  name: string;
  /** Path under /unique-icons/, e.g. "Facebreaker.webp". */
  rel: string;
  source_url: string;
}

export interface UniqueIconManifest {
  version: number;
  fetched_at: string;
  /** Keyed by lowercased display name. */
  entries: Record<string, UniqueIconManifestEntry>;
}

export type ItemArtSource = "base" | "unique-local";

export interface ItemArtRef {
  url: string;
  source: ItemArtSource;
}

let uniqueCached: UniqueIconManifest | null = null;
let uniqueLoading: Promise<UniqueIconManifest | null> | null = null;

/** Load unique-icon manifest (null when not fetched yet). */
export async function loadUniqueIconManifest(): Promise<UniqueIconManifest | null> {
  if (uniqueCached) return uniqueCached;
  if (uniqueLoading) return uniqueLoading;
  uniqueLoading = (async () => {
    try {
      const r = await fetch("/unique-icons/manifest.json", { cache: "no-cache" });
      if (!r.ok) return null;
      uniqueCached = (await r.json()) as UniqueIconManifest;
      return uniqueCached;
    } catch {
      return null;
    } finally {
      uniqueLoading = null;
    }
  })();
  return uniqueLoading;
}

/** Pascal-ish slug used for unique file names (Facebreaker, BrutusLeadSprinkler). */
export function itemArtSlug(name: string): string {
  return name
    .normalize("NFKD")
    .replace(/[''`]/g, "")
    .replace(/[^a-zA-Z0-9]+/g, "")
    .replace(/^[a-z]/, (c) => c.toUpperCase());
}

export function uniqueIconUrl(
  manifest: UniqueIconManifest | null | undefined,
  uniqueName: string | null | undefined,
): string | null {
  if (!manifest || !uniqueName) return null;
  const entry = manifest.entries[uniqueName.toLowerCase()];
  if (entry) return `/unique-icons/${entry.rel}`;
  // Fallback: slug file if present in entries by matching rel
  const slug = itemArtSlug(uniqueName);
  const byRel = Object.values(manifest.entries).find(
    (e) => e.rel.replace(/\.webp$/i, "") === slug,
  );
  return byRel ? `/unique-icons/${byRel.rel}` : null;
}

export interface ResolveItemArtOpts {
  kind: ItemPopupKind;
  name: string;
  typeLine?: string;
  baseId?: string | null;
  baseManifest?: BaseIconManifest | null;
  uniqueManifest?: UniqueIconManifest | null;
}

/**
 * Prefer unique art for uniques; otherwise base art by id or base/type name.
 */
export function resolveItemArt(opts: ResolveItemArtOpts): ItemArtRef | null {
  const { kind, name, typeLine, baseId, baseManifest, uniqueManifest } = opts;

  if (kind === "unique") {
    const u = uniqueIconUrl(uniqueManifest, name);
    if (u) return { url: u, source: "unique-local" };
  }

  const byId = baseIconUrl(baseManifest ?? null, baseId);
  if (byId) return { url: byId, source: "base" };

  // Bases / rares / magic: prefer typeLine (base name) then display name.
  for (const n of [typeLine, name]) {
    if (!n) continue;
    const byName = baseIconUrlByName(baseManifest ?? null, n);
    if (byName) return { url: byName, source: "base" };
  }

  // Currency / gem often only match by name in base-icons if present.
  if (kind === "currency" || kind === "gem") {
    const byName = baseIconUrlByName(baseManifest ?? null, name);
    if (byName) return { url: byName, source: "base" };
  }

  return null;
}
