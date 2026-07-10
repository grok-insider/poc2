"use client";

// Calibration window route (ADR-0013). Loaded full-screen + transparent by the
// Electron shell at /calibrate. The user drag-selects the in-game price region;
// mouse-up retains a draft; Enter/Space confirms it through calibrateRegion.
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
    queueMicrotask(() => setHasBridge(getDesktopBridge() !== null));
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        dragging.current = false;
        setDrag(null);
        return;
      }
      if ((e.key === "Enter" || e.key === " ") && drag) {
        const local = rectOf(drag);
        if (local.width >= 1 && local.height >= 1) {
          e.preventDefault();
          void getDesktopBridge()?.calibrateRegion({
            x: window.screenX + local.x,
            y: window.screenY + local.y,
            width: local.width,
            height: local.height,
          });
        }
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [drag]);

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
        background: "rgba(0, 0, 0, 0.4)",
        color: "#e8d9b5",
        fontFamily: "system-ui, sans-serif",
        userSelect: "none",
        position: "relative",
        overflow: "hidden",
      }}
    >
      <div style={{ position: "absolute", top: 16, left: 16, fontSize: 14 }}>
        <strong>calibrate</strong>
        {hasBridge ? " · drag to select the complete reward rows" : " · (no desktop bridge)"}
      </div>
      {sel && (
        <div
          style={{
            position: "absolute",
            left: sel.x,
            top: sel.y,
            width: sel.width,
            height: sel.height,
            border: "3px solid #50d06d",
            background: "rgba(80, 208, 109, 0.06)",
            pointerEvents: "none",
          }}
        >
          <span
            style={{
              position: "absolute",
              left: 0,
              top: "calc(100% + 10px)",
              whiteSpace: "nowrap",
              color: "#f4f0e6",
              fontSize: 14,
              textShadow: "0 1px 3px #000",
            }}
          >
            Press ENTER to confirm, drag to redo · ESC cancels
          </span>
        </div>
      )}
    </main>
  );
}
