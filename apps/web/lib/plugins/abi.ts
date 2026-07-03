/// Browser-side plugin ABI reader (ADR-0014 phase 1).
///
/// Mirrors `crates/plugin-sdk`'s raw-wasm ABI: emission exports return a
/// packed `i64` — `(len << 32) | ptr` — pointing at a UTF-8 JSON
/// `Vec<String>` of TOML documents in the module's linear memory.
///
/// Instantiation provides **no imports**: a plugin cannot reach the
/// network, filesystem, or DOM. Emission runs once at load time; live
/// dispatch (predicates/emitters) is ADR-0014 phase 2.

/** The exports surface the host consumes (structurally typed so tests can
 * inject fakes without compiling real wasm). Emission exports are phase 1;
 * `alloc`/`reset_arena`/`eval_predicate` are the phase 2 dispatch surface. */
export interface PluginExports {
  memory?: WebAssembly.Memory;
  list_strategies?: () => bigint;
  list_rules?: () => bigint;
  /** Grow the guest arena; returns a writable pointer (SDK export). */
  alloc?: (len: number) => number;
  /** Clear the guest arena between calls (SDK export). */
  reset_arena?: () => void;
  /** `eval_predicate(namePtr, nameLen, itemPtr, itemLen, argsPtr, argsLen)
   * -> 0 | 1` (SDK `declare_predicate!`). */
  eval_predicate?: (
    namePtr: number,
    nameLen: number,
    itemPtr: number,
    itemLen: number,
    argsPtr: number,
    argsLen: number,
  ) => number;
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

/** True when the plugin exports the phase 2 predicate surface. */
export function hasPredicateSurface(exports: PluginExports): boolean {
  return (
    typeof exports.eval_predicate === "function" && typeof exports.alloc === "function"
  );
}

/** Write `bytes` into the guest arena via `alloc`, returning the pointer.
 * The view is taken AFTER the alloc — growing memory detaches earlier
 * buffers, but previously returned pointers stay valid addresses. */
function writeToGuest(exports: PluginExports, bytes: Uint8Array): number {
  const alloc = exports.alloc;
  const memory = exports.memory;
  if (!alloc || !memory) throw new Error("plugin lacks alloc/memory exports");
  const ptr = alloc(bytes.length);
  if (ptr === 0 && bytes.length > 0) throw new Error("plugin alloc returned null");
  new Uint8Array(memory.buffer, ptr, bytes.length).set(bytes);
  return ptr;
}

/**
 * Synchronously evaluate a plugin custom predicate — mirrors the native
 * host's protocol (`crates/plugin-host/src/predicate.rs`): reset the
 * arena, `alloc` + write name/item/args, call `eval_predicate`, non-zero
 * means true. Throws on ABI violations (the dispatcher downgrades to
 * `false` + a strike).
 */
export function callPredicate(
  exports: PluginExports,
  name: string,
  itemJson: string,
  argsJson: string,
): boolean {
  const evalFn = exports.eval_predicate;
  if (typeof evalFn !== "function") throw new Error("plugin has no eval_predicate export");
  exports.reset_arena?.();
  const enc = new TextEncoder();
  const nameBytes = enc.encode(name);
  const itemBytes = enc.encode(itemJson);
  const argsBytes = enc.encode(argsJson);
  const namePtr = writeToGuest(exports, nameBytes);
  const itemPtr = writeToGuest(exports, itemBytes);
  const argsPtr = writeToGuest(exports, argsBytes);
  const result = evalFn(
    namePtr,
    nameBytes.length,
    itemPtr,
    itemBytes.length,
    argsPtr,
    argsBytes.length,
  );
  return result !== 0;
}

/** Instantiate a plugin module with an empty import object (sandboxed). */
export async function instantiatePlugin(bytes: ArrayBuffer): Promise<PluginExports> {
  const { instance } = await WebAssembly.instantiate(bytes, {});
  return instance.exports as PluginExports;
}
