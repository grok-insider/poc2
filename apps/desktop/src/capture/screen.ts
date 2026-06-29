// Screen-region capture (ADR-0013).
//
// Returns the RAW cropped frame for a screen rectangle; pixel preprocessing
// (crop-to-text, invert, upscale for OCR) is the renderer's job. Two paths,
// chosen by the capability gate's `silentRegionCapture`:
//
//   - silent  (win32 / Linux-X11): Electron `desktopCapturer` grabs every
//     screen with no prompt; we pick the display containing the rect and crop.
//   - portal  (Linux-Wayland): the same desktopCapturer call triggers the
//     xdg-desktop-portal ScreenCast picker. The first grant is remembered by
//     the portal (token persisted in window state); on deny/unavailable we
//     return a typed failure so the renderer can fall back to Ctrl+C import.
//
// Only the pure geometry/validation lives at module top-level (unit-tested);
// the Electron grab is lazy so tests never construct a desktopCapturer.

export interface CaptureRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export type CaptureRegionResult =
  | { ok: true; dataUrl: string; width: number; height: number }
  | {
      ok: false;
      reason: "invalid-rect" | "no-display" | "portal-denied" | "capture-failed";
      message?: string;
    };

/** A display's logical bounds + the scale factor needed to map to source px. */
export interface DisplayBounds {
  id: number;
  bounds: CaptureRect;
  scaleFactor: number;
}

/** Reject empty/negative/non-finite rects before touching the capturer. */
export function isValidRect(rect: CaptureRect): boolean {
  return (
    Number.isFinite(rect.x) &&
    Number.isFinite(rect.y) &&
    Number.isFinite(rect.width) &&
    Number.isFinite(rect.height) &&
    rect.width >= 1 &&
    rect.height >= 1
  );
}

/** Parse + validate an untrusted rect from IPC. Returns null when malformed. */
export function coerceRect(raw: unknown): CaptureRect | null {
  if (typeof raw !== "object" || raw === null) return null;
  const r = raw as Record<string, unknown>;
  const rect: CaptureRect = {
    x: Number(r.x),
    y: Number(r.y),
    width: Number(r.width),
    height: Number(r.height),
  };
  return isValidRect(rect) ? rect : null;
}

/** Area of a rect's intersection with a display (0 when disjoint). */
export function intersectionArea(rect: CaptureRect, display: CaptureRect): number {
  const left = Math.max(rect.x, display.x);
  const top = Math.max(rect.y, display.y);
  const right = Math.min(rect.x + rect.width, display.x + display.width);
  const bottom = Math.min(rect.y + rect.height, display.y + display.height);
  const w = right - left;
  const h = bottom - top;
  return w > 0 && h > 0 ? w * h : 0;
}

/** Pick the display the rect mostly sits on; null when it touches none. */
export function pickDisplay<T extends DisplayBounds>(
  rect: CaptureRect,
  displays: T[],
): T | null {
  let best: T | null = null;
  let bestArea = 0;
  for (const d of displays) {
    const area = intersectionArea(rect, d.bounds);
    if (area > bestArea) {
      bestArea = area;
      best = d;
    }
  }
  return best;
}

/**
 * Convert a rect in global logical coords to source-pixel crop coords inside
 * the chosen display's thumbnail. The thumbnail is captured at
 * display.size * scaleFactor; the rect is clamped to the display bounds first.
 */
export function cropForDisplay(
  rect: CaptureRect,
  display: DisplayBounds,
): CaptureRect {
  const s = display.scaleFactor || 1;
  const localX = Math.max(0, rect.x - display.bounds.x);
  const localY = Math.max(0, rect.y - display.bounds.y);
  const maxW = display.bounds.width - localX;
  const maxH = display.bounds.height - localY;
  const w = Math.min(rect.width, maxW);
  const h = Math.min(rect.height, maxH);
  return {
    x: Math.round(localX * s),
    y: Math.round(localY * s),
    width: Math.max(1, Math.round(w * s)),
    height: Math.max(1, Math.round(h * s)),
  };
}

/**
 * Capture a screen rectangle. `silent` mirrors the capability gate: false ⇒
 * Wayland portal path (a denied/unavailable grant yields `portal-denied`).
 */
export async function captureRegion(
  raw: unknown,
  silent: boolean,
): Promise<CaptureRegionResult> {
  const rect = coerceRect(raw);
  if (!rect) return { ok: false, reason: "invalid-rect" };

  const { screen, desktopCapturer, nativeImage } = await import("electron");

  const displays: DisplayBounds[] = screen.getAllDisplays().map((d) => ({
    id: d.id,
    bounds: d.bounds,
    scaleFactor: d.scaleFactor,
  }));
  const display = pickDisplay(rect, displays);
  if (!display) return { ok: false, reason: "no-display" };

  try {
    const thumbW = Math.round(display.bounds.width * display.scaleFactor);
    const thumbH = Math.round(display.bounds.height * display.scaleFactor);
    // On Wayland this is the call that engages xdg-desktop-portal. If the user
    // denies the ScreenCast picker, Electron resolves with no usable sources.
    const sources = await desktopCapturer.getSources({
      types: ["screen"],
      thumbnailSize: { width: thumbW, height: thumbH },
    });

    // Match the source to the chosen display. Electron exposes display_id as a
    // string on most platforms; fall back to index-order when it's absent.
    const wanted = String(display.id);
    const source =
      sources.find((s) => s.display_id === wanted) ??
      sources[displays.findIndex((d) => d.id === display.id)] ??
      sources[0];

    if (!source || source.thumbnail.isEmpty()) {
      return {
        ok: false,
        reason: silent ? "capture-failed" : "portal-denied",
        message: silent
          ? "no screen source available"
          : "screen-cast permission denied or unavailable",
      };
    }

    const crop = cropForDisplay(rect, display);
    const cropped = source.thumbnail.crop(crop);
    const out = cropped.isEmpty() ? nativeImage.createEmpty() : cropped;
    const size = out.getSize();
    return {
      ok: true,
      dataUrl: out.toDataURL(),
      width: size.width,
      height: size.height,
    };
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    // A throw on Wayland almost always means the portal was dismissed/denied.
    return {
      ok: false,
      reason: silent ? "capture-failed" : "portal-denied",
      message,
    };
  }
}
