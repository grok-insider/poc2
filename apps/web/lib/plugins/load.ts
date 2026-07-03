"use client";

/// Plugin loading orchestrator (ADR-0014): stored `.wasm` bytes are
/// shipped INTO THE ENGINE WORKER (transferred, not copied), which
/// instantiates them sandboxed, installs emitted strategy/rule TOMLs
/// (phase 1) and wires predicate-capable plugins into the engine's live
/// dispatch (phase 2). Per-plugin failures are isolated; a broken
/// plugin never takes the engine down.

import type { WorkerPluginInfo } from "../types";
import { listStoredPlugins } from "./store";

export type PluginInfoView = WorkerPluginInfo;

export interface ApplyPluginsResult {
  infos: PluginInfoView[];
  strategiesAdded: number;
  rulesAdded: number;
  /** Per-document parse errors reported by the engine. */
  engineErrors: string[];
}

/**
 * (Re)load every stored plugin into the worker. Always runs — with zero
 * plugins that resets the registries to seeds and clears the dispatch,
 * which is exactly what plugin removal needs.
 */
export async function applyPlugins(): Promise<ApplyPluginsResult> {
  const stored = await listStoredPlugins();
  const { engine } = await import("../engine/client");
  const view = await engine.loadPlugins(
    stored.map((p) => ({ name: p.name, bytes: p.bytes })),
  );
  return {
    infos: view.infos,
    strategiesAdded: view.content.strategies_added,
    rulesAdded: view.content.rules_added,
    engineErrors: view.content.errors,
  };
}
