"use client";

// Price-overlay window route (ADR-0013). Loaded by the Electron shell's
// transparent click-through overlay BrowserWindow at /overlay (full mode), and
// also drives the in-app panel in "degraded" mode. In a plain browser (no
// desktop bridge) it renders inert.
//
// Scan flow (hotkey-triggered single pass — no continuous loop):
//   bridge pushes overlayState{visible:true}  → run ONE scan
//     → bridge.captureRegion(rect)            (rect cached from calibration)
//     → preprocess (crop icon col, invert, 3× bicubic)
//     → tesseract.recognize (origin-relative /ocr/ runtime)
//     → extractRows → engine.resolveName(name) → best-effort price
//     → rowLock de-flicker → price plates
//
//   capability "full"     → click-through plates (pointer-events:none)
//   capability "degraded" → same rows as an in-app panel
//   captureRegion {ok:false, reason:'portal-denied'} → clipboard fallback
//
// All asset URLs are origin-relative so this survives output:'export' + app://.

import { useCallback, useEffect, useRef, useState } from "react";
import {
  getDesktopBridge,
  type CaptureRect,
  type OverlayState,
} from "@/lib/desktop";
import { recognizeRows, resolveAndPrice } from "@/lib/ocr/scan";
import { extractRows } from "@/lib/ocr/extractRows";
import { applyScan, emptyRowLock, type RowLockState } from "@/lib/ocr/rowLock";
import {
  highestValueIndex,
  priceRow,
  type PricedRow,
} from "@/lib/ocr/priceSource";
import styles from "./overlay.module.css";

type Status = "idle" | "scanning" | "ready" | "empty" | "no-region" | "clipboard";

/// Left-gutter crop applied before OCR. The default preprocess assumes a wide
/// (~30%) icon column, but real in-game currency/reward panels — and a
/// user-calibrated region that already trims most of the icon — have a much
/// narrower gutter; an over-crop eats the start of item names (verified on a
/// Windows 11 VM: "Orb of Annulment" → "Annulment"). A conservative 0.12 keeps
/// the icon out without clipping names.
const ICON_CROP = 0.12;

function fmtTotal(r: PricedRow): string | null {
  if (r.total === null) return null;
  const n = r.total;
  const s = n >= 100 ? Math.round(n).toString() : n.toFixed(1).replace(/\.0$/, "");
  return r.unit ? `${s} ${r.unit}` : s;
}

function fmtEach(r: PricedRow): string | null {
  if (r.perUnit === null || r.quantity <= 1) return null;
  const s =
    r.perUnit >= 100
      ? Math.round(r.perUnit).toString()
      : r.perUnit.toFixed(1).replace(/\.0$/, "");
  return `${s}${r.unit ? ` ${r.unit}` : ""} ea`;
}

export default function OverlayPage() {
  const [hasBridge, setHasBridge] = useState(false);
  const [state, setState] = useState<OverlayState | null>(null);
  const [status, setStatus] = useState<Status>("idle");
  const [rows, setRows] = useState<PricedRow[]>([]);
  const [note, setNote] = useState<string | null>(null);
  // Adaptive placement: plates flip to the left edge when the region sits on
  // the right half of the screen (avoids running off-screen).
  const [placeLeft, setPlaceLeft] = useState(false);

  // Latest calibrated region + lock state persist across scans (refs so the
  // scan callback doesn't churn on every render).
  const regionRef = useRef<CaptureRect | null>(null);
  const lockRef = useRef<RowLockState>(emptyRowLock());
  const scanningRef = useRef(false);

  // Fallback when the capture portal is denied: use the clipboard item path —
  // read whatever the user has copied and resolve the recognizable name lines.
  const clipboardFallback = useCallback(async () => {
    setStatus("clipboard");
    try {
      const text = await navigator.clipboard.readText();
      const ocrRows = extractRows(text);
      if (ocrRows.length === 0) {
        setRows([]);
        setStatus("empty");
        setNote("Capture blocked; clipboard had no recognizable item lines.");
        return;
      }
      const { engine } = await import("@/lib/engine/client");
      const { priced } = await resolveAndPrice(ocrRows, (raw) =>
        engine.resolveName({ raw }),
      );
      setRows(priced);
      setStatus(priced.length > 0 ? "ready" : "empty");
      setNote("Capture blocked — read from clipboard instead.");
    } catch {
      setRows([]);
      setStatus("empty");
      setNote("Capture blocked and clipboard is unavailable.");
    }
  }, []);

  const runScan = useCallback(async () => {
    const bridge = getDesktopBridge();
    if (!bridge || scanningRef.current) return;
    const rect = regionRef.current;
    scanningRef.current = true;
    setStatus("scanning");
    setNote(null);
    try {
      // No calibrated region yet → captureRegion would reject as invalid-rect.
      const cap = await bridge.captureRegion(
        rect ?? { x: 0, y: 0, width: 0, height: 0 },
      );
      if (!cap.ok) {
        if (cap.reason === "portal-denied") {
          await clipboardFallback();
          return;
        }
        if (cap.reason === "invalid-rect" && !rect) {
          setStatus("no-region");
          setNote("No price region calibrated yet.");
          return;
        }
        setStatus("empty");
        setNote(`Capture failed (${cap.reason}).`);
        return;
      }

      // Adaptive side: region on the right half of its display → plates left.
      if (rect && typeof window !== "undefined" && window.screen?.width) {
        setPlaceLeft(rect.x + rect.width / 2 > window.screen.width / 2);
      }

      const ocrRows = await recognizeRows(cap.dataUrl, {
        preprocess: { iconCrop: ICON_CROP },
      });
      const { engine } = await import("@/lib/engine/client");
      const { reads, priced } = await resolveAndPrice(ocrRows, (raw) =>
        engine.resolveName({ raw }),
      );
      const { state: nextLock, rows: locked } = applyScan(lockRef.current, reads);
      lockRef.current = nextLock;

      // Re-price the locked (de-flickered) rows so quantity-memory rows keep a
      // price too. Locked rows carry the stable name/key/quantity.
      const lockedPriced = locked.map((r) =>
        priceRow({
          key: r.key,
          name: r.name,
          quantity: r.quantity,
          method: r.method,
          score: r.score,
        }),
      );
      const out = lockedPriced.length > 0 ? lockedPriced : priced;
      setRows(out);
      setStatus(out.length > 0 ? "ready" : "empty");
      if (out.length === 0) setNote("No item rows recognized.");
    } catch (e) {
      setStatus("empty");
      setNote(e instanceof Error ? e.message : String(e));
    } finally {
      scanningRef.current = false;
    }
  }, [clipboardFallback]);

  useEffect(() => {
    const bridge = getDesktopBridge();
    setHasBridge(bridge !== null);
    if (!bridge) return;

    const offRegion = bridge.onRegionCalibrated((rect) => {
      regionRef.current = rect;
    });
    const offState = bridge.onOverlayState((s) => {
      setState(s);
      // A visible push is the scan trigger (single pass).
      if (s.visible) void runScan();
    });
    return () => {
      offRegion();
      offState();
    };
  }, [runScan]);

  // ---- plain browser: inert stub --------------------------------------
  if (!hasBridge) {
    return (
      <main className={styles.root} data-degraded="true">
        <div className={styles.plate}>
          <strong>overlay</strong>
          <span className={styles.muted}> · no desktop bridge</span>
        </div>
      </main>
    );
  }

  const degraded = state?.degraded ?? false;
  const highest = highestValueIndex(rows);

  return (
    <main
      className={styles.root}
      data-degraded={degraded ? "true" : "false"}
      data-place={placeLeft ? "left" : "right"}
    >
      <div className={styles.stack}>
        {(status === "scanning" || status === "idle") && rows.length === 0 && (
          <div className={styles.plate}>
            <span className={styles.muted}>
              {status === "scanning" ? "scanning…" : "ready"}
            </span>
          </div>
        )}

        {rows.map((r, i) => {
          const total = fmtTotal(r);
          const each = fmtEach(r);
          return (
            <div
              key={`${r.key ?? r.name}-${i}`}
              className={`${styles.plate} ${i === highest ? styles.best : ""}`}
            >
              <span className={`${styles.name} r-currency`}>
                {r.quantity > 1 && <span className={styles.qty}>{r.quantity}× </span>}
                {r.name}
              </span>
              <span className={styles.prices}>
                {total ? (
                  <span className={styles.total}>{total}</span>
                ) : (
                  <span className={styles.muted}>no price</span>
                )}
                {each && <span className={styles.each}>{each}</span>}
              </span>
            </div>
          );
        })}

        {note && rows.length === 0 && (
          <div className={styles.plate}>
            <span className={styles.muted}>{note}</span>
          </div>
        )}
      </div>
    </main>
  );
}
