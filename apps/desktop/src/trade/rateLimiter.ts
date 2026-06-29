// Sliding-window rate limiter for the official trade2 API.
//
// GGG advertises limits via response headers (X-Rate-Limit-Rules,
// X-Rate-Limit-<rule>: "max:windowSec:banSec[,...]"). We start from a
// conservative default and tighten to whatever the server reports —
// the same approach Exiled-Exchange-2 uses.

export interface LimitRule {
  max: number;
  windowMs: number;
}

/** Parse "5:10:60,15:60:300" → rules; ignores malformed segments. Pure. */
export function parseLimitHeader(value: string): LimitRule[] {
  const rules: LimitRule[] = [];
  for (const seg of value.split(",")) {
    const [max, windowSec] = seg.trim().split(":");
    const m = Number(max);
    const w = Number(windowSec);
    if (Number.isFinite(m) && m > 0 && Number.isFinite(w) && w > 0) {
      rules.push({ max: m, windowMs: w * 1000 });
    }
  }
  return rules;
}

export class RateLimiter {
  private rules: LimitRule[];
  private stamps: number[] = [];

  constructor(initial: LimitRule[] = [{ max: 1, windowMs: 5_000 }]) {
    this.rules = initial;
  }

  /** Adopt server-reported rules (keeps the strictest interpretation). */
  updateFromHeaders(headers: Record<string, string | undefined>): void {
    const ruleNames = headers["x-rate-limit-rules"];
    if (!ruleNames) return;
    const parsed: LimitRule[] = [];
    for (const name of ruleNames.split(",")) {
      const v = headers[`x-rate-limit-${name.trim().toLowerCase()}`];
      if (v) parsed.push(...parseLimitHeader(v));
    }
    if (parsed.length > 0) this.rules = parsed;
  }

  /** Milliseconds until a request is allowed (0 = now). Pure given `now`. */
  delayUntilNext(now: number = Date.now()): number {
    let wait = 0;
    for (const rule of this.rules) {
      const windowStart = now - rule.windowMs;
      const inWindow = this.stamps.filter((t) => t > windowStart);
      if (inWindow.length >= rule.max) {
        const oldest = inWindow[inWindow.length - rule.max]!;
        wait = Math.max(wait, oldest + rule.windowMs - now);
      }
    }
    return wait;
  }

  /** Record that a request was just issued. */
  record(now: number = Date.now()): void {
    this.stamps.push(now);
    const horizon = now - Math.max(...this.rules.map((r) => r.windowMs), 0);
    this.stamps = this.stamps.filter((t) => t > horizon);
  }

  /** Wait until a slot is free, then record it. */
  async acquire(): Promise<void> {
    const wait = this.delayUntilNext();
    if (wait > 0) await new Promise((r) => setTimeout(r, wait));
    this.record();
  }
}
