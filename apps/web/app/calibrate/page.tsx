"use client";

// Calibration window route (ADR-0013). Loaded full-screen + transparent by the
// Electron shell at /calibrate. The user drag-selects the in-game price region;
// on mouse-up we post the rectangle back to main via the desktop bridge
// (calibrateRegion), which persists it and repositions the overlay.
//
// Minimal but real: this is the drag-select surface, not a downstream worker.
// Export-safe (origin-relative assets only); in a plain browser it's an inert
// labelled stub.

import { useCallback, useEffect, useRef, useState } from "react";
import { getDesktopBridge, type CaptureRect } from "@/lib/desktop";

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
  const [hasBridge, setHasBridge] = useState(false);
  const dragging = useRef(false);

  useEffect(() => {
    setHasBridge(getDesktopBridge() !== null);
  }, []);

  const onDown = useCallback((e: React.MouseEvent) => {
    dragging.current = true;
    setDrag({ startX: e.clientX, startY: e.clientY, curX: e.clientX, curY: e.clientY });
  }, []);

  const onMove = useCallback((e: React.MouseEvent) => {
    if (!dragging.current) return;
    setDrag((d) => (d ? { ...d, curX: e.clientX, curY: e.clientY } : d));
  }, []);

  const onUp = useCallback(() => {
    dragging.current = false;
    setDrag((d) => {
      if (d) {
        const rect = rectOf(d);
        if (rect.width >= 1 && rect.height >= 1) {
          getDesktopBridge()?.calibrateRegion(rect);
        }
      }
      return d;
    });
  }, []);

  const sel = drag ? rectOf(drag) : null;

  return (
    <main
      onMouseDown={onDown}
      onMouseMove={onMove}
      onMouseUp={onUp}
      style={{
        margin: 0,
        height: "100vh",
        width: "100vw",
        cursor: "crosshair",
        background: "rgba(10, 9, 7, 0.35)",
        color: "#e8d9b5",
        fontFamily: "system-ui, sans-serif",
        userSelect: "none",
        position: "relative",
        overflow: "hidden",
      }}
    >
      <div style={{ position: "absolute", top: 16, left: 16, fontSize: 14 }}>
        <strong>calibrate</strong>
        {hasBridge ? " · drag to select the price region (Esc cancels)" : " · (no desktop bridge)"}
      </div>
      {sel && (
        <div
          style={{
            position: "absolute",
            left: sel.x,
            top: sel.y,
            width: sel.width,
            height: sel.height,
            border: "2px solid #c9a227",
            background: "rgba(201, 162, 39, 0.15)",
            pointerEvents: "none",
          }}
        />
      )}
    </main>
  );
}
