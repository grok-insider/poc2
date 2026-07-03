/// Browser-side plugin ABI reader (ADR-0014 phase 1).
///
/// Mirrors `crates/plugin-sdk`'s raw-wasm ABI: emission exports return a
/// packed `i64` — `(len << 32) | ptr` — pointing at a UTF-8 JSON
/// `Vec<String>` of TOML documents in the module's linear memory.
///
/// Instantiation provides **no imports**: a plugin cannot reach the
/// network, filesystem, or DOM. Emission runs once at load time; live
/// dispatch (predicates/emitters) is ADR-0014 phase 2.

/** The exports surface phase 1 consumes (structurally typed so tests can
 * inject fakes without compiling real wasm). */
export interface PluginExports {
  memory?: WebAssembly.Memory;
  list_strategies?: () => bigint;
  list_rules?: () => bigint;
}

/** Unpack the SDK's `(len << 32) | ptr` i64 return. */
export function unpackPtrLen(packed: bigint): { ptr: number; len: number } {
  const ptr = Number(packed & 0xffff_ffffn);
  const len = Number((packed >> 32n) & 0xffff_ffffn);
  return { ptr, len };
}

/**
 * Call one emission export and decode its JSON `Vec<String>` payload.
 * Returns `[]` when the export is absent (plugins declare capabilities by
 * exporting or not) and throws on a malformed payload.
 */
export function readEmission(
  exports: PluginExports,
  fn: "list_strategies" | "list_rules",
): string[] {
  const emit = exports[fn];
  if (typeof emit !== "function") return [];
  const memory = exports.memory;
  if (!memory) throw new Error(`plugin exports ${fn} but no memory`);

  const { ptr, len } = unpackPtrLen(emit());
  if (ptr === 0 || len === 0) return [];
  const bytes = new Uint8Array(memory.buffer, ptr, len);
  const json = new TextDecoder().decode(bytes);
  const parsed: unknown = JSON.parse(json);
  if (!Array.isArray(parsed) || !parsed.every((s) => typeof s === "string")) {
    throw new Error(`${fn} payload is not a string[]`);
  }
  return parsed;
}

export interface PluginContent {
  strategies: string[];
  rules: string[];
}

/** Read both emission exports off an instantiated plugin. */
export function extractContent(exports: PluginExports): PluginContent {
  return {
    strategies: readEmission(exports, "list_strategies"),
    rules: readEmission(exports, "list_rules"),
  };
}

/** Instantiate a plugin module with an empty import object (sandboxed). */
export async function instantiatePlugin(bytes: ArrayBuffer): Promise<PluginExports> {
  const { instance } = await WebAssembly.instantiate(bytes, {});
  return instance.exports as PluginExports;
}
