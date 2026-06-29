"use client";

// Price-overlay window route (ADR-0013). Loaded by the Electron shell's
// transparent click-through overlay BrowserWindow at /overlay. In a plain
// browser this is just a labelled stub. The actual OCR-driven price rendering
// is a separate downstream worker — this placeholder only proves the window
// wiring (feature-detect the bridge, subscribe to overlay state) and stays
// export-safe (origin-relative assets only).

import { useEffect, useState } from "react";
import { getDesktopBridge, type OverlayState } from "@/lib/desktop";

export default function OverlayPage() {
  const [state, setState] = useState<OverlayState | null>(null);
  const [hasBridge, setHasBridge] = useState(false);

  useEffect(() => {
    const bridge = getDesktopBridge();
    setHasBridge(bridge !== null);
    if (!bridge) return;
    return bridge.onOverlayState(setState);
  }, []);

  return (
    <main
      style={{
        margin: 0,
        height: "100vh",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        background: "transparent",
        color: "#e8d9b5",
        fontFamily: "system-ui, sans-serif",
        pointerEvents: "none",
        userSelect: "none",
      }}
    >
      <div
        style={{
          padding: "8px 14px",
          borderRadius: 6,
          background: "rgba(10, 9, 7, 0.82)",
          border: "1px solid #8a6d3b",
          fontSize: 13,
        }}
      >
        <strong>overlay</strong>
        {hasBridge ? (
          <span>{state?.visible ? " · scanning" : " · idle"}</span>
        ) : (
          <span> · (no desktop bridge)</span>
        )}
      </div>
    </main>
  );
}
