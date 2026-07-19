import { describe, expect, test } from "bun:test";
import { callPredicate, hasPredicateSurface, type PluginExports } from "../plugins/abi";
import { createPluginDispatcher } from "../plugins/dispatch";

/**
 * Fake predicate plugin implementing the SDK's guest side faithfully:
 * a bump-allocator arena in real wasm memory + an `eval_predicate` that
 * decodes the host-written buffers. Lets us test the full call protocol
 * without compiling a wasm module.
 */
function fakePredicatePlugin(
  impl: (name: string, item: unknown, args: unknown) => boolean,
): PluginExports {
  const memory = new WebAssembly.Memory({ initial: 1 });
  let arena = 8; // guest arenas never hand out ptr 0
  const dec = new TextDecoder();
  const read = (ptr: number, len: number) =>
    dec.decode(new Uint8Array(memory.buffer, ptr, len));
  return {
    memory,
    alloc: (len: number) => {
      const ptr = arena;
      arena += len;
      return ptr;
    },
    reset_arena: () => {
      arena = 8;
    },
    eval_predicate: (np, nl, ip, il, ap, al) => {
      const name = read(np, nl);
      const item = JSON.parse(read(ip, il));
      const args = JSON.parse(read(ap, al));
      return impl(name, item, args) ? 1 : 0;
    },
  };
}

describe("callPredicate", () => {
  test("round-trips name/item/args through guest memory", () => {
    const plugin = fakePredicatePlugin(
      (name, item, args) =>
        name === "ilvl_at_least" &&
        (item as { ilvl: number }).ilvl >= (args as { min: number }).min,
    );
    expect(callPredicate(plugin, "ilvl_at_least", '{"ilvl":82}', '{"min":80}')).toBe(true);
    expect(callPredicate(plugin, "ilvl_at_least", '{"ilvl":70}', '{"min":80}')).toBe(false);
    expect(callPredicate(plugin, "other_name", '{"ilvl":82}', '{"min":80}')).toBe(false);
  });

  test("consecutive calls reset the arena (no unbounded growth)", () => {
    let seenPtr = -1;
    const plugin = fakePredicatePlugin(() => true);
    const alloc = plugin.alloc!;
    plugin.alloc = (len) => {
      const p = alloc(len);
      if (seenPtr === -1) seenPtr = p;
      return p;
    };
    callPredicate(plugin, "x", "{}", "{}");
    const firstPtr = seenPtr;
    seenPtr = -1;
    callPredicate(plugin, "x", "{}", "{}");
    expect(seenPtr).toBe(firstPtr); // arena reset between calls
  });

  test("missing surface throws (dispatcher turns it into a strike)", () => {
    expect(() => callPredicate({}, "x", "{}", "{}")).toThrow(/eval_predicate/);
  });
});

describe("hasPredicateSurface", () => {
  test("requires eval_predicate + alloc", () => {
    expect(hasPredicateSurface(fakePredicatePlugin(() => true))).toBe(true);
    expect(hasPredicateSurface({})).toBe(false);
    expect(hasPredicateSurface({ eval_predicate: () => 1 })).toBe(false);
  });
});

describe("createPluginDispatcher", () => {
  test("routes to the right plugin by id", () => {
    const instances = new Map<string, PluginExports>([
      ["p-yes", fakePredicatePlugin(() => true)],
      ["p-no", fakePredicatePlugin(() => false)],
    ]);
    const dispatch = createPluginDispatcher(instances, { warn: () => {} });
    expect(dispatch("p-yes", "any", "{}", "{}")).toBe(true);
    expect(dispatch("p-no", "any", "{}", "{}")).toBe(false);
  });

  test("unknown plugin ids evaluate to false with one warning", () => {
    const warnings: string[] = [];
    const dispatch = createPluginDispatcher(new Map(), { warn: (m) => warnings.push(m) });
    expect(dispatch("ghost", "x", "{}", "{}")).toBe(false);
    expect(dispatch("ghost", "x", "{}", "{}")).toBe(false);
    expect(warnings.length).toBe(1);
  });

  test("budget violations strike and auto-disable the plugin", () => {
    // Injected clock: every call appears to take 6ms against a 5ms budget.
    let t = 0;
    const now = () => {
      t += 6; // called twice per dispatch → 6ms elapsed
      return t;
    };
    const warnings: string[] = [];
    const instances = new Map([["slow", fakePredicatePlugin(() => true)]]);
    const dispatch = createPluginDispatcher(instances, {
      budgetMs: 5,
      maxStrikes: 3,
      now,
      warn: (m) => warnings.push(m),
    });
    expect(dispatch("slow", "x", "{}", "{}")).toBe(true); // strike 1
    expect(dispatch("slow", "x", "{}", "{}")).toBe(true); // strike 2
    expect(dispatch("slow", "x", "{}", "{}")).toBe(true); // strike 3 → disabled
    expect(dispatch("slow", "x", "{}", "{}")).toBe(false); // silenced
    expect(warnings.some((w) => w.includes("disabled"))).toBe(true);
  });

  test("throwing plugins strike, return false, and never propagate", () => {
    const broken: PluginExports = {
      memory: new WebAssembly.Memory({ initial: 1 }),
      alloc: () => {
        throw new Error("guest trap");
      },
      eval_predicate: () => 1,
    };
    const instances = new Map([["broken", broken]]);
    const dispatch = createPluginDispatcher(instances, { maxStrikes: 2, warn: () => {} });
    expect(dispatch("broken", "x", "{}", "{}")).toBe(false); // strike 1
    expect(dispatch("broken", "x", "{}", "{}")).toBe(false); // strike 2 → disabled
    expect(dispatch("broken", "x", "{}", "{}")).toBe(false);
  });
});
