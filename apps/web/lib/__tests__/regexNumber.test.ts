import { describe, expect, test } from "bun:test";
import { atLeastRegex, exactAlternation, rangeRegex } from "../regex/numberRegex";

describe("atLeastRegex", () => {
  test("exhaustive: n in 1..300 matches exactly the values >= n (0..999)", () => {
    for (let n = 1; n <= 300; n++) {
      const re = new RegExp(atLeastRegex(n));
      for (let v = 0; v <= 999; v++) {
        const got = re.test(String(v));
        const want = v >= n;
        if (got !== want) {
          throw new Error(`atLeast(${n}) on ${v}: got ${got}, want ${want} — /${re.source}/`);
        }
      }
    }
  });

  test("non-positive and invalid inputs yield no constraint", () => {
    expect(atLeastRegex(0)).toBe("");
    expect(atLeastRegex(-5)).toBe("");
    expect(atLeastRegex(Number.NaN)).toBe("");
  });

  test("compact well-known shapes", () => {
    expect(atLeastRegex(30)).toBe("([3-9]\\d|\\d\\d\\d)");
    expect(atLeastRegex(85)).toBe("(8[5-9]|9\\d|\\d\\d\\d)");
    expect(atLeastRegex(100)).toBe("([1-9]\\d\\d|\\d\\d\\d\\d)");
  });
});

describe("rangeRegex", () => {
  test("exhaustive over the 2-digit domain (anchored like the vendor terms)", () => {
    const cases: [number, number][] = [];
    for (let lo = 1; lo <= 99; lo += 7) {
      for (let hi = lo; hi <= 99; hi += 11) cases.push([lo, hi]);
    }
    cases.push([1, 99], [10, 19], [5, 15], [82, 86], [60, 60], [9, 10], [20, 79]);
    for (const [lo, hi] of cases) {
      const src = rangeRegex(lo, hi);
      expect(src).not.toBe("");
      // Mirror the real usage: a line prefix pins the match position and
      // a \b guard closes the number.
      const re = new RegExp(`level: (?:${src})\\b`);
      for (let v = 0; v <= 100; v++) {
        const got = re.test(`level: ${v}`);
        const want = v >= lo && v <= hi;
        if (got !== want) {
          throw new Error(`range(${lo},${hi}) on ${v}: got ${got}, want ${want} — /${re.source}/`);
        }
      }
    }
  });

  test("open-ended and empty cases", () => {
    expect(rangeRegex(0, 0)).toBe("");
    expect(rangeRegex(50, 0)).toBe(atLeastRegex(50)); // no max → at-least
    expect(rangeRegex(20, 10)).toBe(""); // inverted → no filter
    expect(rangeRegex(60, 60)).toBe("60");
  });
});

describe("exactAlternation", () => {
  test("compresses shared ones digits into tens classes", () => {
    expect(exactAlternation([10, 20, 30])).toBe("[1-3]0");
    expect(exactAlternation([15, 25])).toBe("[12]5");
    expect(exactAlternation([30])).toBe("30");
    expect(exactAlternation([])).toBe("");
  });

  test("matches exactly the requested values in the 2-digit domain", () => {
    const values = [10, 15, 20, 25, 30];
    const re = new RegExp(`^(?:${exactAlternation(values)})$`);
    for (let v = 0; v <= 99; v++) {
      expect(re.test(String(v))).toBe(values.includes(v));
    }
  });
});
