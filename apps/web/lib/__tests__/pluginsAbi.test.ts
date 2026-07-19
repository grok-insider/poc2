import { describe, expect, test } from "bun:test";
import {
  extractContent,
  readEmission,
  unpackPtrLen,
  type PluginExports,
} from "../plugins/abi";

/** Build fake exports whose emission fn returns `payload` (a string[])
 * written into a real WebAssembly.Memory at `ptr` — exactly the SDK's
 * packed-i64 contract, without compiling a plugin. */
function fakeExports(payloads: { [fn: string]: string[] }, ptr = 64): PluginExports {
  const memory = new WebAssembly.Memory({ initial: 1 });
  const exports: PluginExports = { memory };
  let offset = ptr;
  for (const [fn, payload] of Object.entries(payloads)) {
    const bytes = new TextEncoder().encode(JSON.stringify(payload));
    new Uint8Array(memory.buffer).set(bytes, offset);
    const packed = (BigInt(bytes.length) << 32n) | BigInt(offset);
    (exports as Record<string, unknown>)[fn] = () => packed;
    offset += bytes.length + 8;
  }
  return exports;
}

describe("unpackPtrLen", () => {
  test("splits the SDK's (len << 32) | ptr packing", () => {
    expect(unpackPtrLen((5n << 32n) | 1024n)).toEqual({ ptr: 1024, len: 5 });
    expect(unpackPtrLen(0n)).toEqual({ ptr: 0, len: 0 });
    // ptr occupying the full low 32 bits must not bleed into len.
    expect(unpackPtrLen((1n << 32n) | 0xffff_fff0n)).toEqual({ ptr: 0xffff_fff0, len: 1 });
  });
});

describe("readEmission", () => {
  test("decodes a JSON string[] payload from linear memory", () => {
    const tomls = ['[strategy]\nid = "s1"', '[strategy]\nid = "s2"'];
    const exports = fakeExports({ list_strategies: tomls });
    expect(readEmission(exports, "list_strategies")).toEqual(tomls);
  });

  test("absent export means no capability — empty result", () => {
    const exports = fakeExports({});
    expect(readEmission(exports, "list_strategies")).toEqual([]);
    expect(readEmission(exports, "list_rules")).toEqual([]);
  });

  test("null/empty payload pointer yields empty result", () => {
    const memory = new WebAssembly.Memory({ initial: 1 });
    const exports: PluginExports = { memory, list_rules: () => 0n };
    expect(readEmission(exports, "list_rules")).toEqual([]);
  });

  test("non-array payload is rejected loudly", () => {
    const memory = new WebAssembly.Memory({ initial: 1 });
    const bytes = new TextEncoder().encode('{"not":"an array"}');
    new Uint8Array(memory.buffer).set(bytes, 16);
    const exports: PluginExports = {
      memory,
      list_rules: () => (BigInt(bytes.length) << 32n) | 16n,
    };
    expect(() => readEmission(exports, "list_rules")).toThrow(/string\[\]/);
  });

  test("exports without memory are rejected", () => {
    const exports: PluginExports = { list_rules: () => (1n << 32n) | 8n };
    expect(() => readEmission(exports, "list_rules")).toThrow(/memory/);
  });
});

describe("extractContent", () => {
  test("reads both emission surfaces independently", () => {
    const exports = fakeExports({
      list_strategies: ["s-toml"],
      list_rules: ["r-toml-1", "r-toml-2"],
    });
    const content = extractContent(exports);
    expect(content.strategies).toEqual(["s-toml"]);
    expect(content.rules).toEqual(["r-toml-1", "r-toml-2"]);
  });
});
