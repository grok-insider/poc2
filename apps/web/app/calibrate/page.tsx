"use client";

// Calibration window route (ADR-0013). Loaded as a transparent virtual-desktop
// cover by the Electron shell at /calibrate (never true fullscreen — that
// breaks transparency on Windows). The user drag-selects the in-game price
// region; mouse-up retains a draft; Enter/Space confirms it through
// calibrateRegion. Tokens align with hyproverlay selection chrome.

import { useCallback, useEffect, useRef, useState } from "react";
import { getDesktopBridge, type CaptureRect } from "@/lib/desktop";

/** Match openHyprlandCalibration / hyproverlay selection style. */
const CALIBRATE_TOKENS = {
  dim: "rgba(0, 0, 0, 0.4)", // #00000066
  border: "#50d06d",
  borderWidth: 3,
  hintColor: "#f4f0e6",
  hint: "Press ENTER to confirm, drag to redo · ESC cancels",
  label: "calibrate · drag to select the complete reward rows",
} as const;

interface Drag {
  startX: number;
  startY: number;
  curX: number;
  curY: number;
}

function rectOf(d: Drag): CaptureRect {
  return {
    x: Math.min(d.startX, d.curX),
    y: Math.min(d.startY, d.curY),
    width: Math.abs(d.curX - d.startX),
    height: Math.abs(d.curY - d.startY),
  };
}

export default function CalibratePage() {
  const [drag, setDrag] = useState<Drag | null>(null);
  const [draft, setDraft] = useState<CaptureRect | null>(null);
  const [hasBridge, setHasBridge] = useState(false);
  const dragging = useRef(false);

  useEffect(() => {
    queueMicrotask(() => setHasBridge(getDesktopBridge() !== null));
    // Hydrate prior region as a starting draft (hypr parity).
    void getDesktopBridge()
      ?.getCaptureRegion?.()
      .then((rect) => {
        if (rect && rect.width >= 1 && rect.height >= 1) {
          // Convert global screen coords → window-local for painting.
          setDraft({
            x: rect.x - window.screenX,
            y: rect.y - window.screenY,
            width: rect.width,
            height: rect.height,
          });
        }
      })
      .catch(() => undefined);

    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        dragging.current = false;
        setDrag(null);
        // Main's before-input-event hides the calibrator window.
        return;
      }
      const active = drag ? rectOf(drag) : draft;
      if ((e.key === "Enter" || e.key === " ") && active) {
        if (active.width >= 1 && active.height >= 1) {
          e.preventDefault();
          void getDesktopBridge()?.calibrateRegion({
            x: window.screenX + active.x,
            y: window.screenY + active.y,
            width: active.width,
            height: active.height,
          });
        }
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [drag, draft]);

  const onDown = useCallback((e: React.MouseEvent) => {
    dragging.current = true;
    setDrag({ startX: e.clientX, startY: e.clientY, curX: e.clientX, curY: e.clientY });
  }, []);

  const onMove = useCallback((e: React.MouseEvent) => {
    if (!dragging.current) return;
    setDrag((d) => (d ? { ...d, curX: e.clientX, curY: e.clientY } : d));
  }, []);

  const onUp = useCallback(() => {
    if (!dragging.current) return;
    dragging.current = false;
    setDrag((d) => {
      if (!d) return d;
      const r = rectOf(d);
      if (r.width >= 1 && r.height >= 1) setDraft(r);
      return null;
    });
  }, []);

  const sel = drag ? rectOf(drag) : draft;

  return (
    <main
      onMouseDown={onDown}
      onMouseMove={onMove}
      onMouseUp={onUp}
      onMouseLeave={onUp}
      style={{
        margin: 0,
        height: "100vh",
        width: "100vw",
        cursor: "crosshair",
        background: CALIBRATE_TOKENS.dim,
        color: CALIBRATE_TOKENS.hintColor,
        fontFamily: "system-ui, sans-serif",
        userSelect: "none",
        position: "relative",
        overflow: "hidden",
      }}
    >
      <div style={{ position: "absolute", top: 16, left: 16, fontSize: 14 }}>
        <strong>calibrate</strong>
        {hasBridge
          ? " · drag to select the complete reward rows"
          : " · (no desktop bridge)"}
      </div>
      {sel && (
        <div
          style={{
            position: "absolute",
            left: sel.x,
            top: sel.y,
            width: sel.width,
            height: sel.height,
            border: `${CALIBRATE_TOKENS.borderWidth}px solid ${CALIBRATE_TOKENS.border}`,
            background: "rgba(80, 208, 109, 0.06)",
            pointerEvents: "none",
            boxSizing: "border-box",
          }}
        >
          <span
            style={{
              position: "absolute",
              left: 0,
              top: "calc(100% + 10px)",
              whiteSpace: "nowrap",
              color: CALIBRATE_TOKENS.hintColor,
              fontSize: 14,
              textShadow: "0 1px 3px #000",
            }}
          >
            {CALIBRATE_TOKENS.hint}
          </span>
        </div>
      )}
    </main>
  );
}
