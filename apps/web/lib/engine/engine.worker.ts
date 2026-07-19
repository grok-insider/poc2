/// Web Worker hosting the WASM advisor engine.
///
/// The UI thread never blocks on planning. The engine + data bundle load once;
/// after that every call is a synchronous Rust call answered over postMessage.
/// Dispatch is generic: the client names an `Engine` method + args; complex
/// inputs cross as JSON strings and string results are parsed back to objects.

import init, { Engine } from "../wasm/poc2_wasm.js";
import {
  extractContent,
  hasPredicateSurface,
  instantiatePlugin,
  type PluginExports,
} from "../plugins/abi";
import { createPluginDispatcher } from "../plugins/dispatch";

let engine: Engine | null = null;

const ready: Promise<void> = (async () => {
  await init(fetch("/wasm/poc2_wasm_bg.wasm"));
  const res = await fetch("/poc2.bundle.json.gz");
  if (!res.ok) throw new Error(`bundle fetch failed: ${res.status}`);
  const bytes = new Uint8Array(await res.arrayBuffer());
  engine = new Engine(bytes);

  // Optional trained Q-tables (M16.4): operators drop the artefact
  // `train-advisor` writes at public/trained-models.json. Absent (404)
  // or stale-schema artefacts leave the planner on pure heuristics —
  // this must never block engine boot.
  try {
    const tm = await fetch("/trained-models.json");
    if (tm.ok) engine.loadTrainedModels(await tm.text());
  } catch {
    /* optional asset */
  }
})();

type Req = { id: number; method: string; args: unknown[] };

// ---- plugins (ADR-0014) ---------------------------------------------------
// Plugin instances live HERE, in the worker, because the engine's predicate
// dispatch is a synchronous same-thread callback. `__loadPlugins` (a special
// client message carrying transferred wasm bytes) instantiates each module
// sandboxed (no imports), installs the emitted strategy/rule TOMLs (phase 1),
// and wires `eval_predicate`-capable plugins into `setPluginDispatch`
// behind the strike-guarded dispatcher (phase 2).

interface PluginPayload {
  name: string;
  bytes: ArrayBuffer;
}

interface WorkerPluginInfo {
  name: string;
  strategies: number;
  rules: number;
  predicates: boolean;
  error: string | null;
}

async function loadPluginsInWorker(plugins: PluginPayload[]): Promise<unknown> {
  const e = engine;
  if (!e) throw new Error("engine not ready");
  const infos: WorkerPluginInfo[] = [];
  const instances = new Map<string, PluginExports>();
  const strategies: string[] = [];
  const rules: string[] = [];

  for (const p of plugins) {
    try {
      const exports = await instantiatePlugin(p.bytes);
      const content = extractContent(exports);
      strategies.push(...content.strategies);
      rules.push(...content.rules);
      const predicates = hasPredicateSurface(exports);
      if (predicates) instances.set(p.name, exports);
      infos.push({
        name: p.name,
        strategies: content.strategies.length,
        rules: content.rules.length,
        predicates,
        error: null,
      });
    } catch (err) {
      infos.push({
        name: p.name,
        strategies: 0,
        rules: 0,
        predicates: false,
        error: err instanceof Error ? err.message : String(err),
      });
    }
  }

  const content = JSON.parse(
    e.setPluginContent(JSON.stringify(strategies), JSON.stringify(rules)),
  ) as { strategies_added: number; rules_added: number; errors: string[] };

  if (instances.size > 0) {
    e.setPluginDispatch(createPluginDispatcher(instances));
  } else {
    e.clearPluginDispatch();
  }

  return { infos, content };
}

// Structured inputs cross the wasm boundary as JSON strings (every Engine
// method that takes complex data declares an `&str` arg). So objects/arrays are
// serialized here; primitives (numbers, strings, booleans, null) pass through
// untouched — a plain string arg (currency id, raw clipboard text, omen id) is
// already in the shape the method wants. `bigint` (u64 seeds) also passes through.
function encodeArg(arg: unknown): unknown {
  if (arg !== null && typeof arg === "object") return JSON.stringify(arg);
  return arg;
}

function handle(method: string, args: unknown[]): unknown {
  const e = engine as unknown as Record<string, unknown>;
  // Getters (no args).
  if (args.length === 0 && typeof e[method] !== "function") {
    return e[method];
  }
  const fn = e[method];
  if (typeof fn !== "function") {
    throw new Error(`unknown engine method: ${method}`);
  }
  const raw = (fn as (...a: unknown[]) => unknown).apply(engine, args.map(encodeArg));
  // Engine methods return JSON strings for structured data.
  return typeof raw === "string" ? JSON.parse(raw) : raw;
}

self.onmessage = async (ev: MessageEvent<Req>) => {
  const { id, method, args } = ev.data;
  try {
    await ready;
    const result =
      method === "__loadPlugins"
        ? await loadPluginsInWorker(args[0] as PluginPayload[])
        : handle(method, args);
    (self as unknown as Worker).postMessage({ id, ok: true, result });
  } catch (err) {
    (self as unknown as Worker).postMessage({ id, ok: false, error: String(err) });
  }
};
