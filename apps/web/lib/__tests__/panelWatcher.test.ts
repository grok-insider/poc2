import { describe, expect, test } from "bun:test";
import {
  emptyPanelWatcherState,
  observePanel,
  samplePanel,
} from "../ocr/panelWatcher";

describe("reward panel watcher", () => {
  test("requires two bright frames, scans twice, then skips identical frames", () => {
    const sample = { luminance: 140, fingerprint: "same" };
    const first = observePanel(emptyPanelWatcherState(), sample);
    expect(first.action).toBe("wait");
    const second = observePanel(first.state, sample);
    expect(second.action).toBe("scan");
    const confirmation = observePanel(second.state, sample);
    expect(confirmation.action).toBe("scan");
    expect(observePanel(confirmation.state, sample).action).toBe("skip");
  });

  test("closes only after three dark frames and scans changed content", () => {
    let state = observePanel(
      observePanel(emptyPanelWatcherState(), { luminance: 140, fingerprint: "a" }).state,
      { luminance: 140, fingerprint: "a" },
    ).state;
    const changed = observePanel(state, { luminance: 140, fingerprint: "different" });
    expect(changed.action).toBe("scan");
    state = changed.state;
    state = observePanel(state, { luminance: 60, fingerprint: "dark" }).state;
    state = observePanel(state, { luminance: 60, fingerprint: "dark" }).state;
    const closed = observePanel(state, { luminance: 60, fingerprint: "dark" });
    expect(closed.action).toBe("close");
    expect(closed.state.open).toBe(false);
  });

  test("samples the right-hand text area into a bounded signature", () => {
    const frame = {
      width: 10,
      height: 6,
      data: new Uint8ClampedArray(10 * 6 * 4).fill(160),
    };
    for (let i = 3; i < frame.data.length; i += 4) frame.data[i] = 255;
    const sample = samplePanel(frame);
    expect(sample.luminance).toBeCloseTo(160);
    expect(sample.brightFraction).toBe(1);
    expect(sample.fingerprint.length).toBe(60);
  });

  test("high-contrast dark content counts as closed instead of a reward panel", () => {
    let state = observePanel(
      observePanel(emptyPanelWatcherState(), {
        luminance: 100,
        contrast: 52,
        brightFraction: 0.37,
        fingerprint: "panel",
      }).state,
      {
        luminance: 100,
        contrast: 52,
        brightFraction: 0.37,
        fingerprint: "panel",
      },
    ).state;
    const terminal = {
      luminance: 25,
      contrast: 48,
      brightFraction: 0.07,
      fingerprint: "terminal",
    };
    state = observePanel(state, terminal).state;
    state = observePanel(state, terminal).state;
    expect(observePanel(state, terminal).action).toBe("close");
  });
});
