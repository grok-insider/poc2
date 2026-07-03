"use client";

/// Plugin loading orchestrator (ADR-0014 phase 1): stored `.wasm` →
/// sandboxed instantiation → emission extraction → one engine
/// `setPluginContent` call (set semantics: seeds + all current plugins).
/// Per-plugin failures are isolated; a broken plugin never takes the
/// engine down.

import { extractContent, instantiatePlugin } from "./abi";
import { listStoredPlugins } from "./store";

export interface PluginInfoView {
  name: string;
  /** TOML documents the plugin emitted (0 when the export is absent). */
  strategies: number;
  rules: number;
  /** Instantiation / ABI error, or null when healthy. */
  error: string | null;
}

export interface ApplyPluginsResult {
  infos: PluginInfoView[];
  strategiesAdded: number;
  rulesAdded: number;
  /** Per-document parse errors reported by the engine. */
  engineErrors: string[];
}

/**
 * (Re)load every stored plugin into the engine. Always calls
 * `setPluginContent` — with zero plugins that resets the registries to
 * seeds, which is exactly what plugin removal needs.
 */
export async function applyPlugins(): Promise<ApplyPluginsResult> {
  const stored = await listStoredPlugins();
  const infos: PluginInfoView[] = [];
  const strategies: string[] = [];
  const rules: string[] = [];

  for (const p of stored) {
    try {
      const exports = await instantiatePlugin(p.bytes);
      const content = extractContent(exports);
      strategies.push(...content.strategies);
      rules.push(...content.rules);
      infos.push({
        name: p.name,
        strategies: content.strategies.length,
        rules: content.rules.length,
        error: null,
      });
    } catch (e) {
      infos.push({
        name: p.name,
        strategies: 0,
        rules: 0,
        error: e instanceof Error ? e.message : String(e),
      });
    }
  }

  const { engine } = await import("../engine/client");
  const view = await engine.setPluginContent(strategies, rules);
  return {
    infos,
    strategiesAdded: view.strategies_added,
    rulesAdded: view.rules_added,
    engineErrors: view.errors,
  };
}
