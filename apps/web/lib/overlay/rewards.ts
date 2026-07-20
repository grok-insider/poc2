import type { CaptureRect, HyprOverlayPayload } from "@/lib/desktop";
import { highestValueIndex, type PricedRow } from "@/lib/ocr/priceSource";

/** Shared visual tokens — layout, hypr payload, and Electron CSS all project these. */
export const REWARD_TOKENS = {
  edge: 12,
  panelWidth: 380,
  panelMinHeight: 88,
  panelMaxHeight: 260,
  markerWidth: 190,
  markerHeight: 40,
  iconSize: 34,
  iconGap: 7,
  fontSize: 20,
  markerRadius: 9,
  markerBg: "#081018a6",
  colorHighest: "#50ff78ff",
  colorDiv: "#f0b847ff",
  colorDefault: "#ffffffff",
  colorMuted: "#a89a85ff",
  stackBg: "#100c08c8",
  stackBorder: "#8f6a32dd",
  stackText: "#e8e0d2ff",
  stackAccent: "#d29933ff",
  defaultTtlMs: 20_000,
} as const;

export interface RewardOverlayOptions {
  /** Prefer row-aligned markers when OCR geometry exists (hypr v4 + Electron full). */
  supportsPositionedRows?: boolean;
  /** Hypr compositor image ids, or Electron data URLs, keyed by unit. */
  iconIds?: Partial<Record<"div" | "ex", string>>;
  displayBounds?: CaptureRect;
  ttlMs?: number;
}

export type RewardUnit = "div" | "ex";

export interface PositionedMarker {
  top: number;
  height: number;
  label: string;
  color: string;
  bg: string;
  unit: RewardUnit | null;
  /** Compositor image id or data URL when available. */
  iconRef?: string;
  emphasis: boolean;
}

export interface StackRow {
  label: string;
  value: string;
  detail?: string;
  emphasis: boolean;
}

export type RewardSurfaceModel =
  | {
      kind: "positioned";
      strip: CaptureRect;
      markers: PositionedMarker[];
      ttlMs: number;
    }
  | {
      kind: "stack";
      strip: CaptureRect;
      rows: StackRow[];
      ttlMs: number;
    }
  | {
      kind: "empty";
      message?: string;
      ttlMs: number;
    }
  | {
      kind: "error";
      title: string;
      message: string;
      ttlMs: number;
    };

export function formatRewardTotal(row: PricedRow): string | null {
  if (row.total === null) return null;
  const value = row.total >= 100
    ? Math.round(row.total).toString()
    : row.total.toFixed(1).replace(/\.0$/, "");
  return row.unit ? `${value} ${row.unit}` : value;
}

export function formatRewardEach(row: PricedRow): string | null {
  if (row.perUnit === null || row.quantity <= 1) return null;
  const value = row.perUnit >= 100
    ? Math.round(row.perUnit).toString()
    : row.perUnit.toFixed(1).replace(/\.0$/, "");
  return `${value}${row.unit ? ` ${row.unit}` : ""} ea`;
}

function valueNumber(value: number): string {
  return value >= 100
    ? Math.round(value).toString()
    : value.toFixed(1).replace(/\.0$/, "");
}

export function formatRewardMarker(row: PricedRow, hasIcon: boolean): string {
  if (row.total === null) return "no info";
  const total = valueNumber(row.total);
  const each = row.perUnit !== null && row.quantity > 1
    ? ` (${valueNumber(row.perUnit)} each)`
    : "";
  const unit = !hasIcon && row.unit ? ` ${row.unit}` : "";
  return `${total}${unit}${each}`;
}

function unitOf(row: PricedRow): RewardUnit | null {
  return row.unit === "div" || row.unit === "ex" ? row.unit : null;
}

function iconRefFor(
  unit: RewardUnit | null,
  iconIds?: Partial<Record<"div" | "ex", string>>,
): string | undefined {
  if (!unit || !iconIds) return undefined;
  return iconIds[unit];
}

function markerColor(row: PricedRow, emphasis: boolean): string {
  if (emphasis) return REWARD_TOKENS.colorHighest;
  if (row.unit === "div") return REWARD_TOKENS.colorDiv;
  return REWARD_TOKENS.colorDefault;
}

function dockX(
  capture: CaptureRect,
  stripWidth: number,
  screenWidth: number,
  displayBounds?: CaptureRect,
): number {
  const displayLeft = displayBounds?.x ?? 0;
  const displayRight = displayBounds
    ? displayBounds.x + displayBounds.width
    : screenWidth;
  const right = capture.x + capture.width + REWARD_TOKENS.edge;
  if (right + stripWidth <= displayRight - REWARD_TOKENS.edge) return right;
  return Math.max(displayLeft + REWARD_TOKENS.edge, capture.x - stripWidth - REWARD_TOKENS.edge);
}

function buildPositioned(
  capture: CaptureRect,
  rows: PricedRow[],
  screenWidth: number,
  options: RewardOverlayOptions,
): RewardSurfaceModel {
  const x = dockX(capture, REWARD_TOKENS.markerWidth, screenWidth, options.displayBounds);
  const highest = highestValueIndex(rows);
  const markers: PositionedMarker[] = [];
  for (let index = 0; index < rows.length; index++) {
    const row = rows[index]!;
    const center = row.geometry?.center.y;
    if (center === undefined) continue;
    const unit = unitOf(row);
    const iconRef = iconRefFor(unit, options.iconIds);
    const emphasis = index === highest;
    markers.push({
      top: Math.max(
        0,
        Math.min(
          capture.height - REWARD_TOKENS.markerHeight,
          center * capture.height - REWARD_TOKENS.markerHeight / 2,
        ),
      ),
      height: REWARD_TOKENS.markerHeight,
      label: formatRewardMarker(row, Boolean(iconRef)),
      color: markerColor(row, emphasis),
      bg: REWARD_TOKENS.markerBg,
      unit,
      iconRef,
      emphasis,
    });
  }
  return {
    kind: "positioned",
    strip: {
      x,
      y: capture.y,
      width: REWARD_TOKENS.markerWidth,
      height: capture.height,
    },
    markers,
    ttlMs: options.ttlMs ?? REWARD_TOKENS.defaultTtlMs,
  };
}

function buildStack(
  capture: CaptureRect,
  rows: PricedRow[],
  screenWidth: number,
  screenHeight: number,
  options: RewardOverlayOptions,
): RewardSurfaceModel {
  const height = Math.max(
    REWARD_TOKENS.panelMinHeight,
    Math.min(REWARD_TOKENS.panelMaxHeight, 28 + rows.length * 42),
  );
  const x = dockX(capture, REWARD_TOKENS.panelWidth, screenWidth, options.displayBounds);
  const y = Math.max(
    REWARD_TOKENS.edge,
    Math.min(capture.y, screenHeight - height - REWARD_TOKENS.edge),
  );
  const highest = highestValueIndex(rows);
  return {
    kind: "stack",
    strip: { x, y, width: REWARD_TOKENS.panelWidth, height },
    rows: rows.map((row, index) => ({
      label: `${row.quantity > 1 ? `${row.quantity}x ` : ""}${row.name}`,
      value: formatRewardTotal(row) ?? "no price",
      detail: formatRewardEach(row) ?? undefined,
      emphasis: index === highest,
    })),
    ttlMs: options.ttlMs ?? REWARD_TOKENS.defaultTtlMs,
  };
}

/** Pure layout: one model for hypr payload and Electron full-mode paint. */
export function buildRewardSurface(
  capture: CaptureRect,
  rows: PricedRow[],
  screenWidth: number,
  screenHeight: number,
  options: RewardOverlayOptions = {},
): RewardSurfaceModel {
  if (rows.length === 0) {
    return {
      kind: "empty",
      message: "No item rows recognized",
      ttlMs: options.ttlMs ?? REWARD_TOKENS.defaultTtlMs,
    };
  }
  if (options.supportsPositionedRows && rows.some((row) => row.geometry)) {
    const positioned = buildPositioned(capture, rows, screenWidth, options);
    if (positioned.kind === "positioned" && positioned.markers.length > 0) {
      return positioned;
    }
  }
  return buildStack(capture, rows, screenWidth, screenHeight, options);
}

export function errorRewardSurface(
  title: string,
  message: string,
  ttlMs = REWARD_TOKENS.defaultTtlMs,
): RewardSurfaceModel {
  return { kind: "error", title, message, ttlMs };
}

/** Project the shared model into hyproverlay JSON. */
export function toHyprPayload(model: RewardSurfaceModel): HyprOverlayPayload {
  if (model.kind === "positioned") {
    return {
      mode: "cards",
      visible: model.markers.length > 0,
      rect: {
        x: model.strip.x,
        y: model.strip.y,
        w: model.strip.width,
        h: model.strip.height,
      },
      ttlMs: model.ttlMs,
      style: {
        font: "monospace",
        fontSize: REWARD_TOKENS.fontSize,
        background: "#00000000",
        border: "#00000000",
        text: REWARD_TOKENS.colorDefault,
        muted: REWARD_TOKENS.colorMuted,
        accent: REWARD_TOKENS.colorDiv,
        radius: REWARD_TOKENS.markerRadius,
        padding: 0,
        gap: 0,
      },
      rows: model.markers.map((m) => ({
        label: m.label,
        top: m.top,
        height: m.height,
        iconId: m.iconRef,
        iconSize: REWARD_TOKENS.iconSize,
        iconGap: REWARD_TOKENS.iconGap,
        size: REWARD_TOKENS.fontSize,
        color: m.color,
        bg: m.bg,
      })),
    };
  }

  if (model.kind === "stack") {
    return {
      mode: "cards",
      visible: model.rows.length > 0,
      rect: {
        x: model.strip.x,
        y: model.strip.y,
        w: model.strip.width,
        h: model.strip.height,
      },
      ttlMs: model.ttlMs,
      style: {
        background: REWARD_TOKENS.stackBg,
        border: REWARD_TOKENS.stackBorder,
        text: REWARD_TOKENS.stackText,
        muted: REWARD_TOKENS.colorMuted,
        accent: REWARD_TOKENS.stackAccent,
        radius: 3,
        padding: 10,
        gap: 5,
      },
      rows: model.rows.map((row) => ({
        label: row.label,
        value: row.value,
        detail: row.detail,
        emphasis: row.emphasis,
      })),
    };
  }

  if (model.kind === "error") {
    return {
      mode: "cards",
      visible: true,
      rect: { x: 80, y: 80, w: 360, h: 96 },
      ttlMs: model.ttlMs,
      style: {
        background: REWARD_TOKENS.stackBg,
        border: REWARD_TOKENS.stackBorder,
        text: REWARD_TOKENS.stackText,
        muted: REWARD_TOKENS.colorMuted,
        accent: REWARD_TOKENS.stackAccent,
        radius: 3,
        padding: 12,
        gap: 6,
      },
      rows: [
        { label: model.title, emphasis: true },
        { label: model.message, color: REWARD_TOKENS.colorMuted },
      ],
    };
  }

  // empty
  return {
    mode: "cards",
    visible: Boolean(model.message),
    rect: { x: 80, y: 80, w: 360, h: 72 },
    ttlMs: model.ttlMs,
    style: {
      background: REWARD_TOKENS.stackBg,
      border: REWARD_TOKENS.stackBorder,
      text: REWARD_TOKENS.stackText,
      muted: REWARD_TOKENS.colorMuted,
      accent: REWARD_TOKENS.stackAccent,
      radius: 3,
      padding: 12,
      gap: 6,
    },
    rows: model.message
      ? [{ label: "Reward Scan", emphasis: true }, { label: model.message, color: REWARD_TOKENS.colorMuted }]
      : [],
  };
}

/**
 * Compatibility wrapper: build surface + hypr projection.
 * Prefer `buildRewardSurface` + `toHyprPayload` at new call sites.
 */
export function rewardOverlayPayload(
  capture: CaptureRect,
  rows: PricedRow[],
  screenWidth: number,
  screenHeight: number,
  options: RewardOverlayOptions = {},
): HyprOverlayPayload {
  return toHyprPayload(
    buildRewardSurface(capture, rows, screenWidth, screenHeight, options),
  );
}
