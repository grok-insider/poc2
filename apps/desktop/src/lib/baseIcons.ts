import type { BaseIconManifest } from './types';

let cached: BaseIconManifest | null = null;
let loading: Promise<BaseIconManifest | null> | null = null;

export async function loadBaseIconManifest(): Promise<BaseIconManifest | null> {
  if (cached) return cached;
  if (loading) return loading;
  loading = (async () => {
    try {
      const r = await fetch('/base-icons/manifest.json', { cache: 'no-cache' });
      if (!r.ok) return null;
      const data = (await r.json()) as BaseIconManifest;
      cached = data;
      return data;
    } catch {
      return null;
    } finally {
      loading = null;
    }
  })();
  return loading;
}

export function baseIconUrl(manifest: BaseIconManifest | null, baseId: string | null | undefined): string | null {
  if (!manifest || !baseId) return null;
  const entry = manifest.entries[baseId];
  if (!entry) return null;
  return `/base-icons/${entry.rel}`;
}
