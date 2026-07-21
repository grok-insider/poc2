"use client";

// Price-overlay window route (ADR-0013). Loaded by the Electron shell's
// transparent click-through overlay BrowserWindow at /overlay (full mode), and
// also drives the in-app panel in "degraded" mode. In a plain browser (no
// desktop bridge) it renders inert.
//
// Scan flow (one-shot or opt-in watcher):
//   bridge pushes a reward action
//     → bridge.captureRegion(rect)            (rect cached from calibration)
//     → native canvas crop/polarity/2× fast path
//     → warm tesseract session (3×/alternate-crop fallback when uncertain)
//     → extractRows → engine.resolveName(name) → best-effort price
//     → rowLock de-flicker → price plates
//   watcher presence capture (~750 ms, quality:"presence" tiny thumb); OCR is
//   serialized, only on open/fingerprint change, latest-frame-wins with a 2s
//   minimum start interval (no re-OCR on a stable panel).
//
//   capability "full"     → click-through plates (pointer-events:none)
//   capability "degraded" → same rows as an in-app panel
//   captureRegion {ok:false, reason:'portal-denied'} → clipboard fallback
//
// All asset URLs are origin-relative so this survives output:'export' + app://.

import { useCallback, useEffect, useRef, useState } from "react";
import {
  getDesktopBridge,
  type DesktopCapabilities,
  type CaptureRect,
  type CaptureRegionResult,
  type HyprOverlayPayload,
  type NativeOcrResult,
  type OverlayState,
} from "@/lib/desktop";
import {
  buildRecognizedVariant,
  MIN_REWARD_ROWS,
  recognizeFrameVariant,
  resolutionScore,
  resolveAndPriceBatch,
  variantNeedsAccurateFallback,
} from "@/lib/ocr/scan";
import { createOcrSession, type OcrSession } from "@/lib/ocr/tesseract";
import type { RgbaFrame } from "@/lib/ocr/preprocess";
import { browserCanvasAdapter } from "@/lib/ocr/canvas";
import { extractRows } from "@/lib/ocr/extractRows";
import { applyScan, emptyRowLock, type RowLockState } from "@/lib/ocr/rowLock";
import {
  emptyPanelWatcherState,
  observePanel,
  samplePanel,
  type PanelWatcherState,
} from "@/lib/ocr/panelWatcher";
import {
  highestValueIndex,
  priceRow,
  type PricedRow,
} from "@/lib/ocr/priceSource";
import { loadPriceSource, priceCandidates } from "@/lib/prices/source";
import { resolveLocaleArg } from "@/lib/ocr/localePrep";
import { useCraft } from "@/lib/store";
import {
  cardPayload,
  errorPayload,
  priceCheckItemOverlay,
} from "@/lib/overlay/market";
import {
  REWARD_TOKENS,
  buildRewardSurface,
  errorRewardSurface,
  formatRewardEach,
  formatRewardTotal,
  type RewardSurfaceModel,
} from "@/lib/overlay/rewards";
import {
  hideRewardSurface,
  publishRewardSurface,
} from "@/lib/overlay/publishRewardSurface";
import {
  applyRegexOverlayEvent,
  emptyRegexOverlayState,
  moveRegexFocus,
  moveRegexTab,
  regexClipboardResult,
  regexForState,
  regexMenuPayload,
  toggleRegexFocused,
  type RegexOverlayData,
  type RegexOverlayState,
} from "@/lib/overlay/regexMenu";
import type { ItemPopupModel } from "@/lib/itemPopup";
import { loadUniqueIconManifest, type UniqueIconManifest } from "@/lib/itemArt";
import { loadBaseIconManifest } from "@/lib/baseIcons";
import {
  buildItemView,
  loadUniqueCatalog,
  type UniqueCatalog,
} from "@/lib/itemView";
import { ItemPopup } from "@/components/ItemPopup";
import type { BaseIconManifest, Item } from "@/lib/types";
import styles from "./overlay.module.css";

type Status = "idle" | "scanning" | "ready" | "empty" | "no-region" | "clipboard";
type OverlayRows = NonNullable<HyprOverlayPayload["rows"]>;
type SuccessfulCapture = Extract<CaptureRegionResult, { ok: true }>;
interface CapturedWatchFrame {
  cap: SuccessfulCapture;
  frame: RgbaFrame;
  rect: CaptureRect;
  watcherGeneration: number;
  panelGeneration: number;
}

/// Reward labels occupy the right half of the calibrated panel. Starting at the
/// text column keeps icon art and the combination diagram out of sparse OCR.
const ICON_CROP = 0.5;
const WIDE_ICON_CROP = 0.12;
const REWARD_FAST_SCALE = 1.25;
const REWARD_FALLBACK_SCALE = 2;

function createRewardOcrSession(): OcrSession {
  return createOcrSession({ model: "fast", psm: "11" });
}

function buildNativeVariant(
  result: NativeOcrResult,
  width: number,
  height: number,
) {
  return buildRecognizedVariant(
    0,
    {
      text: result.text,
      lines: result.lines.map((line) => {
        const box = line.boundingBox;
        return {
          text: line.text,
          confidence: line.confidence <= 1 ? line.confidence * 100 : line.confidence,
          bbox: {
            x0: box.x,
            y0: box.y,
            x1: box.x + box.width,
            y1: box.y + box.height,
          },
          baseline: {
            x0: box.x,
            y0: box.y + box.height,
            x1: box.x + box.width,
            y1: box.y + box.height,
          },
        };
      }),
    },
    {
      source: { width, height },
      crop: { x: 0, y: 0, width, height },
      processed: { width, height },
    },
  );
}

function compact(text: string | undefined, max = 90): string | undefined {
  if (!text) return undefined;
  return text.length > max ? `${text.slice(0, max - 1)}...` : text;
}

function fallbackRows(payload: HyprOverlayPayload): OverlayRows {
  if (payload.rows) {
    return payload.rows.flatMap((row) => {
      if (row.kind === "separator") return [];
      if (row.kind === "header") {
        return [{ label: row.label ?? row.value ?? "Overlay", value: row.value, detail: row.detail, emphasis: true }];
      }
      if (row.kind === "columns") {
        const text = row.cells?.map((cell) => cell.text).filter(Boolean).join(" · ");
        return text ? [{ label: compact(text, 110) ?? text, emphasis: row.emphasis }] : [];
      }
      const label = row.label ?? row.cells?.map((cell) => cell.text).filter(Boolean).join(" · ");
      return label ? [{ label, value: row.value, detail: row.detail, emphasis: row.emphasis }] : [];
    });
  }
  const menu = payload.menu;
  if (!menu) return [{ label: "Overlay", value: "ready" }];
  const controls = menu.controls ?? [];
  const focused = typeof menu.focusIndex === "number" ? controls[menu.focusIndex] : null;
  const selected = controls.filter((c) => c.selected).slice(0, 4);
  return [
    {
      label: menu.title ?? "Overlay",
      value: menu.budget,
      detail: menu.activeTab,
      emphasis: true,
    },
    {
      label: "preview",
      value: compact(menu.preview) ?? "select filters",
      detail: compact(menu.footer, 60),
    },
    ...(focused
      ? [{ label: "focus", value: focused.label, detail: compact(focused.detail, 70) }]
      : []),
    ...selected.map((control) => ({
      label: "selected",
      value: control.label,
      detail: compact(control.detail, 70),
    })),
  ];
}

function bareItemOfClass(classId: string, ilvl: number): Item {
  return {
    base: classId,
    ilvl,
    rarity: "normal",
    corrupted: false,
    sanctified: false,
    mirrored: false,
    quality: 0,
    quality_kind: "Untagged",
    implicits: [],
    prefixes: [],
    suffixes: [],
    enchantments: [],
    hidden_desecrated: null,
    sockets: [],
    hinekora_lock: null,
  };
}

export default function OverlayPage() {
  const [hasBridge, setHasBridge] = useState(false);
  const [state, setState] = useState<OverlayState | null>(null);
  const [status, setStatus] = useState<Status>("idle");
  const [rows, setRows] = useState<PricedRow[]>([]);
  const [surface, setSurface] = useState<RewardSurfaceModel | null>(null);
  const [cardRows, setCardRows] = useState<OverlayRows | null>(null);
  const [itemPopup, setItemPopup] = useState<ItemPopupModel | null>(null);
  const [itemArtUrl, setItemArtUrl] = useState<string | null>(null);
  const [note, setNote] = useState<string | null>(null);
  const baseManifestRef = useRef<BaseIconManifest | null>(null);
  const uniqueManifestRef = useRef<UniqueIconManifest | null>(null);
  const uniqueCatalogRef = useRef<UniqueCatalog | null>(null);
  // Adaptive placement: plates flip to the left edge when the region sits on
  // the right half of the screen (avoids running off-screen).
  const [placeLeft, setPlaceLeft] = useState(false);

  // Latest calibrated region + lock state persist across scans (refs so the
  // scan callback doesn't churn on every render).
  const regionRef = useRef<CaptureRect | null>(null);
  const lockRef = useRef<RowLockState>(emptyRowLock());
  const scanningRef = useRef(false);
  const ocrSessionRef = useRef<OcrSession | null>(null);
  const preferredCropRef = useRef(0.5);
  const overlayModeRef = useRef<DesktopCapabilities["overlayMode"] | null>(null);
  const hyprStatusRef = useRef<DesktopCapabilities["hyprOverlay"]>(null);
  const iconIdsRef = useRef<Partial<Record<"div" | "ex", string>>>({});
  const iconsPreparedRef = useRef(false);
  const iconAttemptAtRef = useRef(0);
  const watcherEnabledRef = useRef(false);
  const watcherTimerRef = useRef<number | null>(null);
  const watcherOcrTimerRef = useRef<number | null>(null);
  const watcherStateRef = useRef<PanelWatcherState>(emptyPanelWatcherState());
  const watcherGenerationRef = useRef(0);
  const panelGenerationRef = useRef(0);
  const pendingWatchFrameRef = useRef<CapturedWatchFrame | null>(null);
  /** True when a watcher scan was requested while OCR was busy / rate-limited. */
  const pendingWatchRescanRef = useRef(false);
  const lastOcrStartedAtRef = useRef(0);
  const regexRef = useRef<RegexOverlayState>(emptyRegexOverlayState());
  const regexDataRef = useRef<RegexOverlayData>({});
  const regexDataKeyRef = useRef<string | null>(null);

  // Fallback when the capture portal is denied: use the clipboard item path —
  // read whatever the user has copied and resolve the recognizable name lines.
  const clipboardFallback = useCallback(async () => {
    const bridge = getDesktopBridge();
    if (!bridge) return;
    setStatus("clipboard");
    try {
      const text = await navigator.clipboard.readText();
      const ocrRows = extractRows(text);
      if (ocrRows.length === 0) {
        setRows([]);
        setStatus("empty");
        setNote("Capture blocked; clipboard had no recognizable item lines.");
        const model = errorRewardSurface(
          "Reward Scan",
          "Capture blocked and clipboard had no item rows",
        );
        setSurface(model);
        await publishRewardSurface(bridge, overlayModeRef.current, model);
        return;
      }
      const { engine } = await import("@/lib/engine/client");
      await bridge.pricesSetLeague(useCraft.getState().league);
      await loadPriceSource();
      const candidates = priceCandidates();
      const locale = resolveLocaleArg(useCraft.getState().clientLocale);
      const { reads, priced } = await resolveAndPriceBatch(ocrRows, (raws) =>
        engine.resolveNames(
          candidates.length > 0 ? { raws, candidates, locale } : { raws, locale },
        ),
      );
      const resolved = priced.filter((_row, index) => reads[index]?.key !== null);
      setRows(resolved);
      setStatus(resolved.length > 0 ? "ready" : "empty");
      setNote("Capture blocked — read from clipboard instead.");
      const capture = regionRef.current ?? { x: 80, y: 80, width: 420, height: 160 };
      const model =
        resolved.length > 0
          ? buildRewardSurface(
              capture,
              resolved,
              window.screen.width,
              window.screen.height,
              {
                supportsPositionedRows: true,
                iconIds: iconIdsRef.current,
              },
            )
          : errorRewardSurface("Reward Scan", "Clipboard rows did not resolve");
      setSurface(model);
      await publishRewardSurface(bridge, overlayModeRef.current, model);
    } catch {
      setRows([]);
      setStatus("empty");
      setNote("Capture blocked and clipboard is unavailable.");
      const model = errorRewardSurface(
        "Reward Scan",
        "Capture blocked and clipboard unavailable",
      );
      setSurface(model);
      await publishRewardSurface(bridge, overlayModeRef.current, model);
    }
  }, []);

  const ensurePriceIcons = useCallback(async () => {
    const bridge = getDesktopBridge();
    if (!bridge || iconsPreparedRef.current) return;
    if (Date.now() - iconAttemptAtRef.current < 30_000) return;
    iconAttemptAtRef.current = Date.now();
    try {
      const mode = overlayModeRef.current;
      if (mode === "hyprland-plugin") {
        if (!hyprStatusRef.current?.capabilities.includes("images.rgba")) return;
        iconIdsRef.current = await bridge.hyprOverlayPreparePriceIcons();
      } else if (mode === "full" && bridge.preparePriceIconDataUrls) {
        iconIdsRef.current = await bridge.preparePriceIconDataUrls();
      } else {
        return;
      }
      iconsPreparedRef.current = Boolean(iconIdsRef.current.div && iconIdsRef.current.ex);
    } catch {
      // Decorative only; the positioned marker includes a text unit fallback.
    }
  }, []);

  const runScan = useCallback(async function executeScan(
    watching = false,
    watchGeneration?: number,
    captured?: CapturedWatchFrame,
  ) {
    const bridge = getDesktopBridge();
    if (!bridge) return;
    const schedulePendingWatchScan = (delay: number) => {
      if (watcherOcrTimerRef.current !== null) {
        window.clearTimeout(watcherOcrTimerRef.current);
      }
      watcherOcrTimerRef.current = window.setTimeout(() => {
        watcherOcrTimerRef.current = null;
        if (!watcherEnabledRef.current) return;
        const pending = pendingWatchFrameRef.current;
        pendingWatchFrameRef.current = null;
        const wantRescan = pendingWatchRescanRef.current;
        pendingWatchRescanRef.current = false;
        if (pending) {
          void executeScan(true, pending.watcherGeneration, pending);
        } else if (wantRescan) {
          // Presence-only path: re-grab at OCR quality (do not reuse a blurry thumb).
          void executeScan(true, watcherGenerationRef.current);
        }
      }, delay);
    };
    if (watching) {
      if (
        !watcherEnabledRef.current ||
        watchGeneration !== watcherGenerationRef.current ||
        (captured && captured.panelGeneration !== panelGenerationRef.current)
      ) {
        return;
      }
      // Latest-frame-wins admission: at most one OCR start every 2s while watching.
      const delay = Math.max(0, 2_000 - (performance.now() - lastOcrStartedAtRef.current));
      if (scanningRef.current || delay > 0) {
        if (captured) pendingWatchFrameRef.current = captured;
        else pendingWatchRescanRef.current = true;
        if (!scanningRef.current) schedulePendingWatchScan(delay);
        return;
      }
    } else if (scanningRef.current) {
      return;
    }
    const scanStartedAt = performance.now();
    let captureMs = 0;
    let decodeMs = 0;
    let fastOcrMs = 0;
    let fallbackOcrMs = 0;
    let transientSession: OcrSession | null = null;
    const watcherStale = () =>
      watching &&
      (!watcherEnabledRef.current ||
        watchGeneration !== watcherGenerationRef.current ||
        captured?.panelGeneration !== panelGenerationRef.current);
    let rect = captured?.rect ?? regionRef.current;
    if (!rect) {
      rect = (await bridge.getCaptureRegion?.().catch(() => null)) ?? null;
      if (rect) regionRef.current = rect;
    }
    scanningRef.current = true;
    setStatus("scanning");
    setNote(null);
    setCardRows(null);
    try {
      let cap: SuccessfulCapture;
      let frame: RgbaFrame;
      if (captured) {
        cap = captured.cap;
        frame = captured.frame;
      } else {
        // No calibrated region yet → captureRegion would reject as invalid-rect.
        // Always OCR-tier (capped mid-res thumb) — never a presence sample.
        const captureStartedAt = performance.now();
        const result = await bridge.captureRegion(
          rect ?? { x: 0, y: 0, width: 0, height: 0 },
          watching,
          { quality: "ocr" },
        );
        captureMs = performance.now() - captureStartedAt;
        if (!result.ok) {
          if (result.reason === "portal-denied") {
            await clipboardFallback();
            return;
          }
          if (result.reason === "invalid-rect" && !rect) {
            setStatus("no-region");
            setNote("No price region calibrated yet.");
            const model = errorRewardSurface("Reward Scan", "No OCR region calibrated");
            setSurface(model);
            await publishRewardSurface(bridge, overlayModeRef.current, model);
            await bridge.scanDiagnosticsSet({
              updatedAt: new Date().toISOString(),
              transport: overlayModeRef.current ?? "degraded",
              error: "No OCR region calibrated",
            });
            return;
          }
          setStatus("empty");
          setNote(`Capture failed (${result.reason}).`);
          const model = errorRewardSurface(
            "Reward Scan",
            `Capture failed: ${result.reason}`,
          );
          setSurface(model);
          await publishRewardSurface(bridge, overlayModeRef.current, model);
          await bridge.scanDiagnosticsSet({
            updatedAt: new Date().toISOString(),
            transport: overlayModeRef.current ?? "degraded",
            error: `Capture failed: ${result.reason}`,
          });
          return;
        }
        cap = result;
        const decodeStartedAt = performance.now();
        frame = await browserCanvasAdapter.toFrame(cap.dataUrl);
        decodeMs = performance.now() - decodeStartedAt;
      }
      if (watcherStale()) return;

      // Adaptive side: region on the right half of its display → plates left.
      if (rect && typeof window !== "undefined" && window.screen?.width) {
        setPlaceLeft(rect.x + rect.width / 2 > window.screen.width / 2);
      }

      const { engine } = await import("@/lib/engine/client");
      // Resolve against the FULL poe2scout catalogue (runes/idols/omens/alloys…)
      // when the price cache is loaded — the engine valuator only knows ~26
      // built-in currency names, which can't match reward-panel items.
      await loadPriceSource();
      if (!hyprStatusRef.current && overlayModeRef.current === "hyprland-plugin") {
        const caps = await bridge.capabilities().catch(() => null);
        hyprStatusRef.current = caps?.hyprOverlay ?? null;
      }
      await ensurePriceIcons();
      const candidates = priceCandidates();
      const locale = resolveLocaleArg(useCraft.getState().clientLocale);
      const resolveVariant = async (variant: Awaited<ReturnType<typeof recognizeFrameVariant>>) => ({
        variant,
        result: await resolveAndPriceBatch(variant.rows, (raws) =>
          engine.resolveNames(
            candidates.length > 0 ? { raws, candidates, locale } : { raws, locale },
          ),
        ),
      });
      const preferredCrop = preferredCropRef.current;
      type ResolvedVariant = Awaited<ReturnType<typeof resolveVariant>> & {
        scale: number;
        backend: "windows-media-ocr" | "tesseract-fast" | "tesseract-fallback";
      };
      const resolvedVariants: ResolvedVariant[] = [];
      lastOcrStartedAtRef.current = performance.now();

      if (bridge.nativeOcrRecognize) {
        const nativeStartedAt = performance.now();
        const nativeResult = await bridge.nativeOcrRecognize(cap.dataUrl, "en-US").catch(() => null);
        fastOcrMs = performance.now() - nativeStartedAt;
        if (nativeResult) {
          const nativeVariant = buildNativeVariant(nativeResult, cap.width, cap.height);
          resolvedVariants.push({
            ...(await resolveVariant(nativeVariant)),
            scale: 1,
            backend: "windows-media-ocr",
          });
        }
      }

      const nativePass = resolvedVariants[0];
      if (!nativePass || variantNeedsAccurateFallback(nativePass.variant, nativePass.result.reads)) {
        let session = ocrSessionRef.current;
        if (!session) {
          session = createRewardOcrSession();
          if (watching || watcherEnabledRef.current) {
            ocrSessionRef.current = session;
          } else {
            transientSession = session;
          }
        }
        const fastStartedAt = performance.now();
        const fastVariant = await recognizeFrameVariant(frame, preferredCrop, session, {
          preprocess: { polarity: "auto", scale: REWARD_FAST_SCALE, trimVertical: true },
          recognize: { psm: "11" },
        });
        const fastResolved = await resolveVariant(fastVariant);
        fastOcrMs += performance.now() - fastStartedAt;
        resolvedVariants.push({
          ...fastResolved,
          scale: REWARD_FAST_SCALE,
          backend: "tesseract-fast",
        });

        if (variantNeedsAccurateFallback(fastVariant, fastResolved.result.reads)) {
          const fallbackStartedAt = performance.now();
          const precisePreferred = await recognizeFrameVariant(frame, preferredCrop, session, {
            preprocess: { polarity: "auto", scale: REWARD_FALLBACK_SCALE, trimVertical: true },
            recognize: { psm: "11" },
          });
          const preciseResolved = await resolveVariant(precisePreferred);
          resolvedVariants.push({
            ...preciseResolved,
            scale: REWARD_FALLBACK_SCALE,
            backend: "tesseract-fallback",
          });
          if (variantNeedsAccurateFallback(precisePreferred, preciseResolved.result.reads)) {
            const alternateCrop = preferredCrop === ICON_CROP ? WIDE_ICON_CROP : ICON_CROP;
            const alternate = await recognizeFrameVariant(frame, alternateCrop, session, {
              preprocess: { polarity: "auto", scale: REWARD_FALLBACK_SCALE, trimVertical: true },
              recognize: { psm: "11" },
            });
            resolvedVariants.push({
              ...(await resolveVariant(alternate)),
              scale: REWARD_FALLBACK_SCALE,
              backend: "tesseract-fallback",
            });
          }
          fallbackOcrMs = performance.now() - fallbackStartedAt;
        }
      }
      if (watcherStale()) return;
      const selected = resolvedVariants.sort(
        (a, b) =>
          resolutionScore(b.result.reads) - resolutionScore(a.result.reads) ||
          b.scale - a.scale,
      )[0];
      if (selected) preferredCropRef.current = selected.variant.iconCrop;
      const resolvedPairs = (selected?.result.reads ?? []).flatMap((read, index) =>
        read.key && selected?.result.priced[index]
          ? [{ read, priced: selected.result.priced[index] }]
          : [],
      );
      const pairs = resolvedPairs.length >= MIN_REWARD_ROWS ? resolvedPairs : [];
      const reads = pairs.map((pair) => pair.read);
      const priced = pairs.map((pair) => pair.priced);
      if (watcherStale()) return;
      const { state: nextLock } = applyScan(lockRef.current, reads);
      lockRef.current = nextLock;

      // Keep every catalogue-resolved spatial slot for its missing-frame grace,
      // not only confidence-locked rows. The watcher intentionally stops OCR on
      // an unchanged frame, so dropping a provisional row on pass two would
      // otherwise hide it forever despite the panel remaining unchanged.
      const out = Object.values(nextLock)
        .filter((row) => row.key !== null)
        .map((row) =>
          priceRow({
            key: row.key,
            name: row.name,
            quantity: row.quantity,
            method: row.method,
            score: row.score,
            ocrConfidence: row.ocrConfidence,
            geometry: row.geometry,
          }),
        )
        .sort((a, b) => (a.geometry?.center.y ?? 2) - (b.geometry?.center.y ?? 2));
      setRows(out);
      setStatus(out.length > 0 ? "ready" : "empty");
      const supportsPositioned =
        overlayModeRef.current === "full" ||
        hyprStatusRef.current?.capabilities.includes("cards.positionedRows") === true;
      const model =
        out.length > 0 && rect
          ? buildRewardSurface(rect, out, window.screen.width, window.screen.height, {
              supportsPositionedRows: supportsPositioned,
              iconIds: iconIdsRef.current,
              displayBounds: cap.displayBounds,
              ttlMs: watching ? 0 : REWARD_TOKENS.defaultTtlMs,
            })
          : errorRewardSurface("Reward Scan", "No item rows recognized");
      setSurface(model);
      const { renderOk } = await publishRewardSurface(
        bridge,
        overlayModeRef.current,
        model,
      );
      if (watcherStale()) {
        await hideRewardSurface(bridge, overlayModeRef.current, rect);
        return;
      }
      await bridge.scanDiagnosticsSet({
        updatedAt: new Date().toISOString(),
        transport: overlayModeRef.current ?? "degraded",
        captureWidth: cap.width,
        captureHeight: cap.height,
        selectedCrop: selected?.variant.iconCrop,
        selectedScale: selected?.scale,
        ocrBackend: selected?.backend,
        captureMs: Math.round(captureMs),
        decodeMs: Math.round(decodeMs),
        fastOcrMs: Math.round(fastOcrMs),
        fallbackOcrMs: Math.round(fallbackOcrMs),
        totalMs: Math.round(performance.now() - scanStartedAt),
        rawText: selected?.variant.text,
        rawRows: selected?.variant.rows.map((row) =>
          `${row.quantity > 1 ? `${row.quantity}x ` : ""}${row.name}`,
        ),
        resolvedRows: reads.map((row) =>
          `${row.quantity > 1 ? `${row.quantity}x ` : ""}${row.name}`,
        ),
        lineRows: out.map((row) => {
          const y = row.geometry?.center.y;
          return `${y === undefined ? "?" : `${Math.round(y * 1000) / 10}%`} ${formatRewardTotal(row) ?? "no price"} ${row.name}`;
        }),
        pluginProtocol: hyprStatusRef.current?.protocolVersion ?? undefined,
        pluginCapabilities: hyprStatusRef.current?.capabilities,
        renderOk,
        watcherEnabled: watcherEnabledRef.current,
        error: reads.length === 0 ? "No catalogue-resolved rows" : undefined,
      });
      if (out.length > 0 && !watching) {
        void bridge.marketHistoryAdd({
          kind: "reward-scan",
          title: "Reward scan",
          league: useCraft.getState().league,
          summary: `${out.length} priced rows`,
          rows: out.map((r) => ({
            label: `${r.quantity > 1 ? `${r.quantity}x ` : ""}${r.name}`,
            value: formatRewardTotal(r) ?? "no price",
            detail: formatRewardEach(r) ?? undefined,
          })),
        });
      }
      if (out.length === 0) setNote("No item rows recognized.");
    } catch (e) {
      if (watcherStale()) return;
      setStatus("empty");
      const message = e instanceof Error ? e.message : String(e);
      setNote(message);
      const model = errorRewardSurface("Reward Scan", message);
      setSurface(model);
      await publishRewardSurface(bridge, overlayModeRef.current, model);
      await bridge.scanDiagnosticsSet({
        updatedAt: new Date().toISOString(),
        transport: overlayModeRef.current ?? "degraded",
        error: message,
      });
    } finally {
      scanningRef.current = false;
      await transientSession?.terminate();
      if (watcherEnabledRef.current) {
        const pending = pendingWatchFrameRef.current;
        const wantRescan = pendingWatchRescanRef.current;
        if (pending || wantRescan) {
          const delay = Math.max(0, 2_000 - (performance.now() - lastOcrStartedAtRef.current));
          schedulePendingWatchScan(delay);
        }
      }
    }
  }, [clipboardFallback, ensurePriceIcons]);

  const runWatchLoop = useCallback(async function tick() {
    if (!watcherEnabledRef.current) return;
    const tickStartedAt = performance.now();
    const generation = watcherGenerationRef.current;
    const bridge = getDesktopBridge();
    if (!bridge) return;
    try {
      // Never stack desktopCapturer under an in-flight OCR (Windows DWM thrash).
      if (scanningRef.current) {
        return;
      }

      let rect = regionRef.current;
      if (!rect) {
        rect = (await bridge.getCaptureRegion?.().catch(() => null)) ?? null;
        if (rect) regionRef.current = rect;
      }
      // Presence-only: tiny display thumbnail + JPEG. Open/close hysteresis only.
      const result = await bridge.captureRegion(
        rect ?? { x: 0, y: 0, width: 0, height: 0 },
        true,
        { quality: "presence" },
      );
      if (!watcherEnabledRef.current || generation !== watcherGenerationRef.current) return;
      if (scanningRef.current) return;

      const previous = watcherStateRef.current;
      let observed: ReturnType<typeof observePanel>;
      if (result.ok) {
        const frame = await browserCanvasAdapter.toFrame(result.dataUrl);
        if (!watcherEnabledRef.current || generation !== watcherGenerationRef.current) return;
        observed = observePanel(previous, samplePanel(frame));
      } else {
        observed = observePanel(previous, {
          luminance: 0,
          contrast: 0,
          fingerprint: "capture-miss",
        });
      }
      watcherStateRef.current = observed.state;

      if (observed.action === "close") {
        panelGenerationRef.current += 1;
        pendingWatchFrameRef.current = null;
        pendingWatchRescanRef.current = false;
        if (watcherOcrTimerRef.current !== null) {
          window.clearTimeout(watcherOcrTimerRef.current);
          watcherOcrTimerRef.current = null;
        }
        lockRef.current = emptyRowLock();
        setRows([]);
        setSurface(null);
        setStatus("idle");
        await hideRewardSurface(bridge, overlayModeRef.current, rect);
      } else if (observed.action === "scan" && rect) {
        // Fingerprint/open changed — schedule OCR at OCR quality (separate capture).
        // Do NOT re-OCR on stable `skip` (that was a major Windows load source).
        if (!previous.open || previous.fingerprint !== observed.state.fingerprint) {
          panelGenerationRef.current += 1;
          pendingWatchFrameRef.current = null;
        }
        void runScan(true, generation);
      } else if (observed.action === "wait" || observed.action === "skip") {
        const next = Object.keys(lockRef.current).length > 0 ? "ready" : "idle";
        setStatus((prev) => (prev === next ? prev : next));
      }
    } catch {
      // A transient capture/decode failure is retried on the next presence tick.
    } finally {
      if (watcherEnabledRef.current && generation === watcherGenerationRef.current) {
        // 750 ms is enough for 2-frame open / 3-frame close hysteresis and
        // halves capturer pressure vs the old 500 ms full-display loop.
        const delay = Math.max(0, 750 - (performance.now() - tickStartedAt));
        watcherTimerRef.current = window.setTimeout(tick, delay);
      }
    }
  }, [runScan]);

  const showPayload = useCallback(async (payload: HyprOverlayPayload) => {
    const bridge = getDesktopBridge();
    if (!bridge) return;
    if (overlayModeRef.current === "hyprland-plugin") {
      await bridge.hyprOverlayRender(payload);
      return;
    }
    setCardRows(fallbackRows(payload));
    setRows([]);
    setSurface(null);
    setStatus("ready");
    await bridge.overlayShow();
  }, []);

  const hydrateRegexData = useCallback(async () => {
    const craft = useCraft.getState();
    const key = `${craft.item.base_type_id ?? craft.item.base}:${craft.item.ilvl}`;
    if (regexDataKeyRef.current === key && Object.keys(regexDataRef.current).length > 0) {
      return;
    }
    try {
      const { engine } = await import("@/lib/engine/client");
      const bareCurrent: Item = { ...craft.item, prefixes: [], suffixes: [] };
      const [itemMods, waystoneMods, tabletMods] = await Promise.allSettled([
        craft.eligible?.mods
          ? Promise.resolve({ mods: craft.eligible.mods })
          : engine.eligibleMods(bareCurrent, "either", 0),
        engine.eligibleMods(bareItemOfClass("Map", 80), "either", 0),
        engine.eligibleMods(bareItemOfClass("TowerAugmentation", 80), "either", 0),
      ]);
      regexDataRef.current = {
        itemMods: itemMods.status === "fulfilled" ? itemMods.value.mods : (craft.eligible?.mods ?? null),
        waystoneMods: waystoneMods.status === "fulfilled" ? waystoneMods.value.mods : null,
        tabletMods: tabletMods.status === "fulfilled" ? tabletMods.value.mods : null,
      };
      regexDataKeyRef.current = key;
    } catch {
      regexDataRef.current = { itemMods: craft.eligible?.mods ?? null };
      regexDataKeyRef.current = key;
    }
  }, []);

  const regexInteractive = useCallback((): boolean => {
    if (overlayModeRef.current !== "hyprland-plugin") return false;
    const caps = hyprStatusRef.current?.capabilities ?? [];
    return caps.includes("menu.interactive");
  }, []);

  const sendRegexMenu = useCallback(async () => {
    const bridge = getDesktopBridge();
    if (!bridge) return;
    const width = 520;
    const height = 500;
    const rect = {
      x: typeof window === "undefined" ? 80 : Math.max(12, window.screen.width - width - 36),
      y: 80,
      width,
      height,
    };
    await showPayload(
      regexMenuPayload(regexRef.current, rect, regexDataRef.current, {
        interactive: regexInteractive(),
      }),
    );
  }, [regexInteractive, showPayload]);

  const copyRegex = useCallback(async (apply: boolean) => {
    const bridge = getDesktopBridge();
    if (!bridge) return;
    const result = regexClipboardResult(regexRef.current, regexDataRef.current, apply);
    if (!result.ok) {
      await showPayload(errorPayload("Search Regex", result.reason));
      return;
    }
    await bridge.clipboardWrite(result.text);
    await showPayload(
      cardPayload(
        [
          {
            label: result.label,
            value: `${result.length}/250`,
            detail: result.detail,
            emphasis: true,
          },
        ],
        { title: "Search Regex", ttlMs: 8_000 },
      ),
    );
  }, [showPayload]);

  const runPriceCheck = useCallback(async (itemText: string | undefined, error: string | undefined) => {
    const bridge = getDesktopBridge();
    if (!bridge) return;
    if (error || !itemText) {
      setItemPopup(null);
      setItemArtUrl(null);
      await showPayload(errorPayload("Item Price", error || "no item captured"));
      return;
    }
    if (!baseManifestRef.current) {
      baseManifestRef.current = await loadBaseIconManifest();
    }
    if (!uniqueManifestRef.current) {
      uniqueManifestRef.current = await loadUniqueIconManifest();
    }
    if (!uniqueCatalogRef.current) {
      uniqueCatalogRef.current = await loadUniqueCatalog();
    }
    const view = buildItemView(itemText, {
      baseManifest: baseManifestRef.current,
      uniqueManifest: uniqueManifestRef.current,
      uniqueCatalog: uniqueCatalogRef.current,
    });
    setItemPopup(view.model);
    setItemArtUrl(view.artUrl);
    await showPayload(cardPayload([{ label: "Searching trade2", value: "..." }], { title: "Item Price", ttlMs: 5_000 }));
    try {
      const result = await priceCheckItemOverlay(bridge, itemText, useCraft.getState().league);
      await showPayload(result.payload);
      if (result.history) await bridge.marketHistoryAdd(result.history);
    } catch (e) {
      await showPayload(errorPayload("Item Price", e instanceof Error ? e.message : String(e)));
    }
  }, [showPayload]);

  const handleAction = useCallback(
    async (s: OverlayState) => {
      const bridge = getDesktopBridge();
      if (!bridge) return;
      switch (s.action) {
        case "reward-scan":
          await runScan();
          return;
        case "reward-watch-start":
          if (watcherEnabledRef.current) return;
          watcherGenerationRef.current += 1;
          panelGenerationRef.current += 1;
          pendingWatchFrameRef.current = null;
          pendingWatchRescanRef.current = false;
          watcherEnabledRef.current = true;
          watcherStateRef.current = emptyPanelWatcherState();
          lastOcrStartedAtRef.current = 0;
          const nativeStatus = bridge.nativeOcrStatus
            ? await bridge.nativeOcrStatus().catch(() => null)
            : null;
          if (!nativeStatus?.available) {
            const warmSession = ocrSessionRef.current ?? createRewardOcrSession();
            ocrSessionRef.current = warmSession;
            void warmSession.prewarm().catch(() => {
              if (ocrSessionRef.current === warmSession) ocrSessionRef.current = null;
            });
          }
          void runWatchLoop();
          return;
        case "reward-watch-stop":
          watcherEnabledRef.current = false;
          watcherGenerationRef.current += 1;
          panelGenerationRef.current += 1;
          pendingWatchFrameRef.current = null;
          pendingWatchRescanRef.current = false;
          if (watcherTimerRef.current !== null) {
            window.clearTimeout(watcherTimerRef.current);
            watcherTimerRef.current = null;
          }
          if (watcherOcrTimerRef.current !== null) {
            window.clearTimeout(watcherOcrTimerRef.current);
            watcherOcrTimerRef.current = null;
          }
          watcherStateRef.current = emptyPanelWatcherState();
          lockRef.current = emptyRowLock();
          const session = ocrSessionRef.current;
          ocrSessionRef.current = null;
          void session?.terminate();
          setRows([]);
          await bridge.overlayHide();
          return;
        case "price-check":
          await runPriceCheck(s.itemText, s.error);
          return;
        case "regex-open":
          await showPayload(cardPayload([{ label: "Loading regex data", value: "..." }], { title: "Search Regex", ttlMs: 5_000 }));
          await hydrateRegexData();
          regexRef.current = emptyRegexOverlayState();
          await sendRegexMenu();
          return;
        case "regex-next":
          regexRef.current = moveRegexFocus(regexRef.current, 1, regexDataRef.current);
          await sendRegexMenu();
          return;
        case "regex-prev":
          regexRef.current = moveRegexFocus(regexRef.current, -1, regexDataRef.current);
          await sendRegexMenu();
          return;
        case "regex-tab-next":
          regexRef.current = moveRegexTab(regexRef.current, 1, regexDataRef.current);
          await sendRegexMenu();
          return;
        case "regex-tab-prev":
          regexRef.current = moveRegexTab(regexRef.current, -1, regexDataRef.current);
          await sendRegexMenu();
          return;
        case "regex-toggle":
          regexRef.current = toggleRegexFocused(regexRef.current, regexDataRef.current);
          await sendRegexMenu();
          return;
        case "regex-copy":
          await copyRegex(false);
          return;
        case "regex-apply":
          await copyRegex(true);
          return;
        default:
          return;
      }
    },
    [copyRegex, hydrateRegexData, runPriceCheck, runScan, runWatchLoop, sendRegexMenu, showPayload],
  );

  useEffect(() => {
    const bridge = getDesktopBridge();
    setHasBridge(bridge !== null);
    if (!bridge) return;

    // Hydrate window.poc2PriceSource + the fuzzy candidate names from the
    // desktop poe2scout cache so the very first scan can price + resolve.
    void loadPriceSource();
    void bridge.capabilities().then((caps) => {
      overlayModeRef.current = caps?.overlayMode ?? null;
      hyprStatusRef.current = caps?.hyprOverlay ?? null;
    }).catch(() => {});

    // Interactive hypr-overlay menu events (click / in-overlay keys).
    const unsubHyprEvent = bridge.onHyprOverlayEvent?.((event) => {
      void (async () => {
        const result = applyRegexOverlayEvent(
          regexRef.current,
          event,
          regexDataRef.current,
        );
        if (result.kind === "noop") return;
        if (result.kind === "action") {
          if (result.action === "dismiss") {
            await bridge.overlayHide();
            return;
          }
          await copyRegex(result.action === "apply");
          return;
        }
        regexRef.current = result.state;
        if (result.refresh) await sendRegexMenu();
      })();
    });

    void bridge.rewardWatcherStatus().then((enabled) => {
      if (enabled) {
        void handleAction({
          visible: true,
          degraded: false,
          mode: overlayModeRef.current ?? undefined,
          action: "reward-watch-start",
        });
      }
    }).catch(() => {});

    // Hydrate the persisted calibrated region: the overlay window may be
    // created AFTER calibration happened, so relying on the push alone
    // made the first hotkey scan race (and fail with "no region").
    void bridge
      .getCaptureRegion?.()
      .then((rect) => {
        if (rect && !regionRef.current) regionRef.current = rect;
      })
      .catch(() => {});

    const offRegion = bridge.onRegionCalibrated((rect) => {
      regionRef.current = rect;
      panelGenerationRef.current += 1;
      pendingWatchFrameRef.current = null;
      watcherStateRef.current = emptyPanelWatcherState();
      lockRef.current = emptyRowLock();
    });
    const offState = bridge.onOverlayState((s) => {
      if (s.mode) overlayModeRef.current = s.mode;
      setState(s);
      void handleAction(s);
    });
    return () => {
      watcherEnabledRef.current = false;
      watcherGenerationRef.current += 1;
      panelGenerationRef.current += 1;
      pendingWatchFrameRef.current = null;
      pendingWatchRescanRef.current = false;
      if (watcherTimerRef.current !== null) window.clearTimeout(watcherTimerRef.current);
      if (watcherOcrTimerRef.current !== null) window.clearTimeout(watcherOcrTimerRef.current);
      const session = ocrSessionRef.current;
      ocrSessionRef.current = null;
      void session?.terminate();
      offRegion();
      offState();
      unsubHyprEvent?.();
    };
  }, [copyRegex, handleAction, sendRegexMenu]);

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
  const positioned = surface?.kind === "positioned" ? surface : null;
  const stackSurface = surface?.kind === "stack" ? surface : null;
  // Positioned markers fill the strip window (hypr parity). Stack/plates use the
  // classic column layout for degraded, price-check, and no-geometry fallback.
  const useMarkerStrip =
    !degraded && positioned !== null && !cardRows && !itemPopup;

  return (
    <main
      className={styles.root}
      data-degraded={degraded ? "true" : "false"}
      data-place={placeLeft ? "left" : "right"}
      data-markers={useMarkerStrip ? "true" : "false"}
      style={
        useMarkerStrip
          ? {
              ["--marker-icon-size" as string]: `${REWARD_TOKENS.iconSize}px`,
              ["--marker-icon-gap" as string]: `${REWARD_TOKENS.iconGap}px`,
              ["--marker-font-size" as string]: `${REWARD_TOKENS.fontSize}px`,
              ["--marker-radius" as string]: `${REWARD_TOKENS.markerRadius}px`,
            }
          : undefined
      }
    >
      {useMarkerStrip && positioned ? (
        <div className={styles.markerStrip} aria-label="Reward prices">
          {positioned.markers.map((m, i) => (
            <div
              key={`marker-${i}-${m.label}`}
              className={styles.marker}
              style={{
                top: m.top,
                height: m.height,
                background: m.bg,
                color: m.color,
              }}
            >
              {m.iconRef ? (
                // eslint-disable-next-line @next/next/no-img-element -- data URL / external unit icon
                <img
                  className={styles.markerIcon}
                  src={m.iconRef}
                  alt=""
                  width={REWARD_TOKENS.iconSize}
                  height={REWARD_TOKENS.iconSize}
                  draggable={false}
                />
              ) : null}
              <span className={styles.markerLabel}>{m.label}</span>
            </div>
          ))}
        </div>
      ) : (
        <div className={styles.stack}>
          <button
            type="button"
            className={styles.close}
            aria-label="Close overlay"
            onClick={() => void getDesktopBridge()?.overlayHide()}
          >
            ×
          </button>
          {(status === "scanning" || status === "idle") &&
            rows.length === 0 &&
            !cardRows &&
            !itemPopup && (
              <div className={styles.plate}>
                <span className={styles.muted}>
                  {status === "scanning" ? "scanning…" : "ready"}
                </span>
              </div>
            )}

          {/* Stack surface (shared model) or legacy rows when surface not set */}
          {stackSurface
            ? stackSurface.rows.map((r, i) => (
                <div
                  key={`stack-${r.label}-${i}`}
                  className={`${styles.plate} ${r.emphasis ? styles.best : ""}`}
                >
                  <span className={`${styles.name} r-currency`}>{r.label}</span>
                  <span className={styles.prices}>
                    <span className={styles.total}>{r.value}</span>
                    {r.detail && <span className={styles.each}>{r.detail}</span>}
                  </span>
                </div>
              ))
            : rows.map((r, i) => {
                const total = formatRewardTotal(r);
                const each = formatRewardEach(r);
                return (
                  <div
                    key={`${r.key ?? r.name}-${i}`}
                    className={`${styles.plate} ${i === highest ? styles.best : ""}`}
                  >
                    <span className={`${styles.name} r-currency`}>
                      {r.quantity > 1 && (
                        <span className={styles.qty}>{r.quantity}× </span>
                      )}
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

          {itemPopup && (
            <div className={styles.itemPopupWrap}>
              <ItemPopup model={itemPopup} artUrl={itemArtUrl} />
            </div>
          )}

          {cardRows?.map((r, i) => (
            <div
              key={`card-${r.label ?? "row"}-${i}`}
              className={`${styles.plate} ${r.emphasis ? styles.best : ""}`}
            >
              <span className={`${styles.name} r-currency`}>{r.label ?? ""}</span>
              <span className={styles.prices}>
                {r.value ? <span className={styles.total}>{r.value}</span> : null}
                {r.detail && <span className={styles.each}>{r.detail}</span>}
              </span>
            </div>
          ))}

          {note &&
            rows.length === 0 &&
            !stackSurface &&
            !cardRows &&
            !itemPopup && (
              <div className={styles.plate}>
                <span className={styles.muted}>{note}</span>
              </div>
            )}
        </div>
      )}
    </main>
  );
}
