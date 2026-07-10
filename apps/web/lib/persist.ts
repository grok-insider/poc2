"use client";

/// IndexedDB persistence — the browser replacement for the desktop's
/// `~/.config/poc2/state.toml` + `recipes/*.toml`. All values are plain JSON,
/// stored under a single keyval store via `idb-keyval`. Saves are debounced so
/// a burst of edits writes once. Everything degrades to in-memory if IndexedDB
/// is unavailable (private mode, SSR) — the engine still works.

import { get, set, del, keys } from "idb-keyval";
import type { Goal, HistoryEntry, Item, Recipe } from "./types";

const STATE_KEY = "poc2:craft-state";
const RECIPE_PREFIX = "poc2:recipe:";

export interface PersistedCraft {
  item: Item;
  goal: Goal;
  risk: number;
  depth: number;
  history: HistoryEntry[];
  league: string;
  /** One-time migration version for the market-league default. */
  marketLeagueVersion?: number;
  /** Engine League ruleset ("standard" | "challenge"); distinct from the
   * free-text price-API `league` above. */
  engineLeague?: string;
  notes: string;
  /** Raw clipboard text of the last imported item (drives the price check). */
  lastItemText?: string | null;
}

const canPersist = (): boolean =>
  typeof window !== "undefined" && typeof indexedDB !== "undefined";

let saveTimer: ReturnType<typeof setTimeout> | null = null;
let queued: PersistedCraft | null = null;

/** Debounced save (250ms). Safe to call on every keystroke. */
export function saveCraft(state: PersistedCraft): void {
  if (!canPersist()) return;
  queued = state;
  if (saveTimer) clearTimeout(saveTimer);
  saveTimer = setTimeout(() => {
    saveTimer = null;
    if (queued) void set(STATE_KEY, queued).catch(() => {});
  }, 250);
}

export async function loadCraft(): Promise<PersistedCraft | null> {
  if (!canPersist()) return null;
  try {
    return (await get<PersistedCraft>(STATE_KEY)) ?? null;
  } catch {
    return null;
  }
}

// ---- recipes (saved item+goal pairs) ----------------------------------

export async function saveRecipe(r: Recipe): Promise<void> {
  if (!canPersist()) return;
  await set(RECIPE_PREFIX + r.name, r).catch(() => {});
}

export async function deleteRecipe(name: string): Promise<void> {
  if (!canPersist()) return;
  await del(RECIPE_PREFIX + name).catch(() => {});
}

export async function listRecipes(): Promise<Recipe[]> {
  if (!canPersist()) return [];
  try {
    const allKeys = (await keys()) as string[];
    const recipeKeys = allKeys.filter(
      (k) => typeof k === "string" && k.startsWith(RECIPE_PREFIX),
    );
    const recipes = await Promise.all(recipeKeys.map((k) => get<Recipe>(k)));
    return recipes
      .filter((r): r is Recipe => !!r)
      .sort((a, b) => (a.created_at < b.created_at ? 1 : -1));
  } catch {
    return [];
  }
}
