/// ADR-0014 phase 2 — the synchronous predicate dispatcher the engine
/// calls back into during planning (`Engine.setPluginDispatch`).
///
/// Enforces ADR-0008's perf contract the only way a synchronous JS host
/// can: wall-clock measurement AFTER each call with strike-based
/// auto-disable (a plugin that blows the budget `maxStrikes` times is
/// silenced for the session — every later call returns false).
/// Exceptions count as strikes too; a misbehaving plugin can slow a few
/// plans but can never wedge or crash planning.

import { callPredicate, type PluginExports } from "./abi";

export interface DispatcherOptions {
  /** Per-call wall-clock budget in ms. The native target is 50 µs;
   * the JS default is deliberately generous (interp overhead, first-call
   * warmup) while still catching runaway loops. */
  budgetMs?: number;
  /** Budget violations (or throws) before the plugin is disabled. */
  maxStrikes?: number;
  /** Clock override for tests. */
  now?: () => number;
  /** Diagnostic sink (defaults to console.warn, once per event kind). */
  warn?: (msg: string) => void;
}

export type PluginDispatchFn = (
  pluginId: string,
  name: string,
  itemJson: string,
  argsJson: string,
) => boolean;

/**
 * Build the dispatch callback over instantiated plugin exports, keyed by
 * plugin id (= the stored plugin name; rules reference it via
 * `custom = { plugin_id = "...", ... }`).
 */
export function createPluginDispatcher(
  instances: Map<string, PluginExports>,
  opts: DispatcherOptions = {},
): PluginDispatchFn {
  const budgetMs = opts.budgetMs ?? 5;
  const maxStrikes = opts.maxStrikes ?? 3;
  const now = opts.now ?? (() => performance.now());
  const warn = opts.warn ?? ((msg: string) => console.warn(`[plugins] ${msg}`));

  const strikes = new Map<string, number>();
  const disabled = new Set<string>();
  const warned = new Set<string>();

  const warnOnce = (key: string, msg: string) => {
    if (warned.has(key)) return;
    warned.add(key);
    warn(msg);
  };

  const strike = (pluginId: string, reason: string) => {
    const n = (strikes.get(pluginId) ?? 0) + 1;
    strikes.set(pluginId, n);
    if (n >= maxStrikes) {
      disabled.add(pluginId);
      warnOnce(
        `disabled:${pluginId}`,
        `plugin "${pluginId}" disabled for this session after ${n} strikes (last: ${reason})`,
      );
    }
  };

  return (pluginId, name, itemJson, argsJson) => {
    if (disabled.has(pluginId)) return false;
    const exports = instances.get(pluginId);
    if (!exports) {
      warnOnce(`unknown:${pluginId}`, `custom predicate references unknown plugin "${pluginId}"`);
      return false;
    }
    const start = now();
    try {
      const result = callPredicate(exports, name, itemJson, argsJson);
      const elapsed = now() - start;
      if (elapsed > budgetMs) {
        strike(pluginId, `call took ${elapsed.toFixed(1)}ms (budget ${budgetMs}ms)`);
      }
      return result;
    } catch (e) {
      strike(pluginId, e instanceof Error ? e.message : String(e));
      return false;
    }
  };
}
