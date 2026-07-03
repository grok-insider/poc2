"use client";

/// Plugin `.wasm` persistence (ADR-0014) — the browser replacement for
/// `~/.config/poc2/plugins/`. Raw module bytes live in IndexedDB under
/// one key; everything soft-fails (private mode / SSR ⇒ no plugins).

import { get, set } from "idb-keyval";

const PLUGINS_KEY = "poc2:plugins:v1";

export interface StoredPlugin {
  /** Display name (file name without `.wasm`). Unique — re-adding replaces. */
  name: string;
  /** Raw wasm module bytes. */
  bytes: ArrayBuffer;
  /** ISO-8601 add time. */
  addedAt: string;
}

const canPersist = (): boolean =>
  typeof window !== "undefined" && typeof indexedDB !== "undefined";

export async function listStoredPlugins(): Promise<StoredPlugin[]> {
  if (!canPersist()) return [];
  try {
    return (await get<StoredPlugin[]>(PLUGINS_KEY)) ?? [];
  } catch {
    return [];
  }
}

export async function addStoredPlugin(name: string, bytes: ArrayBuffer): Promise<void> {
  if (!canPersist()) return;
  const existing = await listStoredPlugins();
  const next = existing.filter((p) => p.name !== name);
  next.push({ name, bytes, addedAt: new Date().toISOString() });
  await set(PLUGINS_KEY, next).catch(() => {});
}

export async function removeStoredPlugin(name: string): Promise<void> {
  if (!canPersist()) return;
  const existing = await listStoredPlugins();
  await set(
    PLUGINS_KEY,
    existing.filter((p) => p.name !== name),
  ).catch(() => {});
}
