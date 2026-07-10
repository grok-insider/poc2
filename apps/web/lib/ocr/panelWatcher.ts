import type { RgbaFrame } from "./preprocess";

export interface PanelSample {
  luminance: number;
  contrast?: number;
  brightFraction?: number;
  fingerprint: string;
}

export interface PanelWatcherState {
  open: boolean;
  brightFrames: number;
  darkFrames: number;
  fingerprint: string | null;
  scansForFingerprint: number;
}

export type PanelWatcherAction = "wait" | "scan" | "skip" | "close";

export function emptyPanelWatcherState(): PanelWatcherState {
  return {
    open: false,
    brightFrames: 0,
    darkFrames: 0,
    fingerprint: null,
    scansForFingerprint: 0,
  };
}

/** Cheaply sample the text-bearing portion of a captured reward panel. */
export function samplePanel(frame: RgbaFrame): PanelSample {
  const columns = 10;
  const rows = 6;
  let luminance = 0;
  let bright = 0;
  const values: number[] = [];
  const fingerprint: number[] = [];
  for (let row = 0; row < rows; row++) {
    const y = Math.min(frame.height - 1, Math.round(((row + 0.5) / rows) * frame.height));
    for (let column = 0; column < columns; column++) {
      const ratio = 0.4 + ((column + 0.5) / columns) * 0.58;
      const x = Math.min(frame.width - 1, Math.round(ratio * frame.width));
      const offset = (y * frame.width + x) * 4;
      const value =
        frame.data[offset] * 0.2126 +
        frame.data[offset + 1] * 0.7152 +
        frame.data[offset + 2] * 0.0722;
      luminance += value;
      if (value > 110) bright += 1;
      values.push(value);
      fingerprint.push(Math.round(value / 16));
    }
  }
  const mean = luminance / (columns * rows);
  const variance = values.reduce((sum, value) => sum + (value - mean) ** 2, 0) / values.length;
  return {
    luminance: mean,
    contrast: Math.sqrt(variance),
    brightFraction: bright / (columns * rows),
    fingerprint: fingerprint.map((value) => value.toString(16)).join(""),
  };
}

/** Two-frame open and three-frame close hysteresis, with duplicate OCR skips. */
export function observePanel(
  previous: PanelWatcherState,
  sample: PanelSample,
): { state: PanelWatcherState; action: PanelWatcherAction } {
  const contrast = sample.contrast ?? 0;
  const bright = sample.brightFraction === undefined
    ? sample.luminance > 100 || (sample.luminance > 25 && contrast > 35)
    : sample.brightFraction >= 0.2 && sample.luminance > 55;
  const dark = sample.brightFraction === undefined
    ? sample.luminance < 80 && contrast < 20
    : !bright;
  if (!previous.open) {
    const brightFrames = bright ? previous.brightFrames + 1 : 0;
    if (brightFrames < 2) {
      return {
        state: { ...previous, brightFrames, darkFrames: 0 },
        action: "wait",
      };
    }
    return {
      state: {
        open: true,
        brightFrames,
        darkFrames: 0,
        fingerprint: sample.fingerprint,
        scansForFingerprint: 1,
      },
      action: "scan",
    };
  }

  const darkFrames = dark ? previous.darkFrames + 1 : 0;
  if (darkFrames >= 3) {
    return { state: emptyPanelWatcherState(), action: "close" };
  }
  if (dark) {
    return { state: { ...previous, darkFrames }, action: "wait" };
  }

  if (fingerprintDistance(sample.fingerprint, previous.fingerprint) >= 12) {
    return {
      state: {
        ...previous,
        brightFrames: previous.brightFrames + 1,
        darkFrames: 0,
        fingerprint: sample.fingerprint,
        scansForFingerprint: 1,
      },
      action: "scan",
    };
  }
  if (previous.scansForFingerprint < 2) {
    return {
      state: { ...previous, darkFrames: 0, scansForFingerprint: 2 },
      action: "scan",
    };
  }
  return { state: { ...previous, darkFrames: 0 }, action: "skip" };
}

function fingerprintDistance(current: string, previous: string | null): number {
  if (previous === null || current.length !== previous.length) return Infinity;
  let distance = 0;
  for (let i = 0; i < current.length; i++) {
    distance += Math.abs(
      Number.parseInt(current[i] ?? "0", 16) - Number.parseInt(previous[i] ?? "0", 16),
    );
  }
  return distance;
}
