// Screen-region capture (ADR-0013).
//
// Returns the RAW cropped frame for a screen rectangle; pixel preprocessing
// (crop-to-text, invert, upscale for OCR) is the renderer's job. Two paths,
// chosen by the capability gate's `silentRegionCapture`:
//
//   - silent  (win32 / Linux-X11): Electron `desktopCapturer` grabs a screen
//     source (full display thumbnail, then crop — Electron has no region API).
//   - portal  (Linux-Wayland): the same desktopCapturer call triggers the
//     xdg-desktop-portal ScreenCast picker.
//
// Quality tiers keep the watcher cheap: presence uses a tiny display thumbnail;
// OCR uses a capped mid-res thumb. Never request full native 4K every 500 ms.
//
// Only pure geometry/validation lives at module top-level (unit-tested);
// the Electron grab is lazy so tests never construct a desktopCapturer.

export interface CaptureRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export type CaptureQuality = "ocr" | "presence";

export interface CaptureRegionOptions {
  quality?: CaptureQuality;
}

export type CaptureRegionResult =
  | { ok: true; dataUrl: string; width: number; height: number; quality?: CaptureQuality }
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

/** Caps for desktopCapturer thumbnail long-edge (source pixels). */
export const CAPTURE_THUMB_MAX_EDGE: Record<CaptureQuality, number> = {
  /** Open/close + fingerprint only — samplePanel needs ~10×6 probes. */
  presence: 480,
  /** Sharp enough for OCR after crop; still far below full 4K native. */
  ocr: 1600,
};

export const CAPTURE_JPEG_QUALITY: Record<CaptureQuality, number> = {
  presence: 50,
  ocr: 88,
};

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
 * a full-native-size display thumbnail. Callers that use a downscaled thumb
 * must then {@link scaleCrop}.
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

export function nativeDisplaySize(display: DisplayBounds): { width: number; height: number } {
  const s = display.scaleFactor || 1;
  return {
    width: Math.max(1, Math.round(display.bounds.width * s)),
    height: Math.max(1, Math.round(display.bounds.height * s)),
  };
}

/**
 * Size of the desktopCapturer thumbnail and the uniform scale relative to
 * native display pixels (cropForDisplay space).
 */
export function thumbnailSizeForDisplay(
  display: DisplayBounds,
  quality: CaptureQuality,
): { width: number; height: number; scale: number } {
  const native = nativeDisplaySize(display);
  const maxEdge = CAPTURE_THUMB_MAX_EDGE[quality];
  const long = Math.max(native.width, native.height);
  const scale = long <= maxEdge ? 1 : maxEdge / long;
  return {
    width: Math.max(1, Math.round(native.width * scale)),
    height: Math.max(1, Math.round(native.height * scale)),
    scale,
  };
}

/** Map a native-pixel crop into a downscaled thumbnail's coordinate space. */
export function scaleCrop(crop: CaptureRect, scale: number): CaptureRect {
  if (!Number.isFinite(scale) || scale <= 0) {
    return { x: 0, y: 0, width: 1, height: 1 };
  }
  if (scale === 1) return crop;
  return {
    x: Math.max(0, Math.round(crop.x * scale)),
    y: Math.max(0, Math.round(crop.y * scale)),
    width: Math.max(1, Math.round(crop.width * scale)),
    height: Math.max(1, Math.round(crop.height * scale)),
  };
}

export function coerceCaptureQuality(raw: unknown): CaptureQuality {
  return raw === "presence" ? "presence" : "ocr";
}

/** Serialize capturer work — concurrent getSources spikes DWM/GPU hard on Windows. */
let captureChain: Promise<void> = Promise.resolve();

function enqueueCapture<T>(work: () => Promise<T>): Promise<T> {
  const run = captureChain.then(work, work);
  captureChain = run.then(
    () => undefined,
    () => undefined,
  );
  return run;
}

/** Encode crop as JPEG when possible (far cheaper than PNG for watcher IPC). */
export function encodeCaptureDataUrl(
  image: { toJPEG(quality: number): Buffer; toDataURL(): string; isEmpty(): boolean },
  quality: CaptureQuality,
): string {
  if (image.isEmpty()) return "data:,";
  try {
    const jpeg = image.toJPEG(CAPTURE_JPEG_QUALITY[quality]);
    if (jpeg && jpeg.length > 0) {
      return `data:image/jpeg;base64,${jpeg.toString("base64")}`;
    }
  } catch {
    // fall through to PNG
  }
  return image.toDataURL();
}

/**
 * Capture a screen rectangle. `silent` mirrors the capability gate: false ⇒
 * Wayland portal path (a denied/unavailable grant yields `portal-denied`).
 */
export async function captureRegion(
  raw: unknown,
  silent: boolean,
  options: CaptureRegionOptions = {},
): Promise<CaptureRegionResult> {
  return enqueueCapture(() => captureRegionUnlocked(raw, silent, options));
}

async function captureRegionUnlocked(
  raw: unknown,
  silent: boolean,
  options: CaptureRegionOptions,
): Promise<CaptureRegionResult> {
  const rect = coerceRect(raw);
  if (!rect) return { ok: false, reason: "invalid-rect" };
  const quality = coerceCaptureQuality(options.quality);

  const { screen, desktopCapturer, nativeImage } = await import("electron");

  const displays: DisplayBounds[] = screen.getAllDisplays().map((d) => ({
    id: d.id,
    bounds: d.bounds,
    scaleFactor: d.scaleFactor,
  }));
  const display = pickDisplay(rect, displays);
  if (!display) return { ok: false, reason: "no-display" };

  try {
    const thumb = thumbnailSizeForDisplay(display, quality);
    // On Wayland this is the call that engages xdg-desktop-portal. If the user
    // denies the ScreenCast picker, Electron resolves with no usable sources.
    const sources = await desktopCapturer.getSources({
      types: ["screen"],
      thumbnailSize: { width: thumb.width, height: thumb.height },
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

    const nativeCrop = cropForDisplay(rect, display);
    const crop = scaleCrop(nativeCrop, thumb.scale);
    // Clamp to thumbnail bounds (rounding can overhang by 1px).
    const maxW = Math.max(1, source.thumbnail.getSize().width - crop.x);
    const maxH = Math.max(1, source.thumbnail.getSize().height - crop.y);
    const clamped = {
      x: crop.x,
      y: crop.y,
      width: Math.min(crop.width, maxW),
      height: Math.min(crop.height, maxH),
    };
    const cropped = source.thumbnail.crop(clamped);
    const out = cropped.isEmpty() ? nativeImage.createEmpty() : cropped;
    const size = out.getSize();
    return {
      ok: true,
      dataUrl: encodeCaptureDataUrl(out, quality),
      width: size.width,
      height: size.height,
      quality,
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

/** Test helper: reset the capture mutex chain between suites if needed. */
export function resetCaptureQueueForTests(): void {
  captureChain = Promise.resolve();
}
