import { describe, expect, test } from "bun:test";
import { parseLimitHeader, RateLimiter } from "../src/trade/rateLimiter";

describe("parseLimitHeader", () => {
  test("parses GGG multi-rule format", () => {
    expect(parseLimitHeader("5:10:60,15:60:300")).toEqual([
      { max: 5, windowMs: 10_000 },
      { max: 15, windowMs: 60_000 },
    ]);
  });

  test("ignores malformed segments", () => {
    expect(parseLimitHeader("nonsense,3:5:10")).toEqual([{ max: 3, windowMs: 5_000 }]);
    expect(parseLimitHeader("")).toEqual([]);
  });
});

describe("RateLimiter", () => {
  test("default allows one request per 5s", () => {
    const rl = new RateLimiter();
    const t0 = 1_000_000;
    expect(rl.delayUntilNext(t0)).toBe(0);
    rl.record(t0);
    expect(rl.delayUntilNext(t0 + 100)).toBe(4_900);
    expect(rl.delayUntilNext(t0 + 5_001)).toBe(0);
  });

  test("sliding window with server rules", () => {
    const rl = new RateLimiter([{ max: 2, windowMs: 1_000 }]);
    const t0 = 50_000;
    rl.record(t0);
    rl.record(t0 + 100);
    expect(rl.delayUntilNext(t0 + 200)).toBe(800); // oldest expires at t0+1000
    expect(rl.delayUntilNext(t0 + 1_001)).toBe(0);
  });

  test("updateFromHeaders adopts reported rules", () => {
    const rl = new RateLimiter();
    rl.updateFromHeaders({
      "x-rate-limit-rules": "Ip",
      "x-rate-limit-ip": "2:1:60",
    });
    const t0 = 9_000_000;
    rl.record(t0);
    rl.record(t0 + 10);
    expect(rl.delayUntilNext(t0 + 20)).toBeGreaterThan(0);
    // two slots within 1s window, third must wait
    expect(rl.delayUntilNext(t0 + 1_011)).toBe(0);
  });
});
