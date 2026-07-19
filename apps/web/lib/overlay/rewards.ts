import type { CaptureRect, HyprOverlayPayload } from "@/lib/desktop";
import { highestValueIndex, type PricedRow } from "@/lib/ocr/priceSource";

const PANEL_WIDTH = 380;
const PANEL_MIN_HEIGHT = 88;
const PANEL_MAX_HEIGHT = 260;
const EDGE = 12;
const MARKER_WIDTH = 190;
const MARKER_HEIGHT = 40;

export interface RewardOverlayOptions {
  supportsPositionedRows?: boolean;
  iconIds?: Partial<Record<"div" | "ex", string>>;
  displayBounds?: CaptureRect;
  ttlMs?: number;
}

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

function positionedRewardPayload(
  capture: CaptureRect,
  rows: PricedRow[],
  screenWidth: number,
  options: RewardOverlayOptions,
): HyprOverlayPayload {
  const displayLeft = options.displayBounds?.x ?? 0;
  const displayRight = options.displayBounds
    ? options.displayBounds.x + options.displayBounds.width
    : screenWidth;
  const right = capture.x + capture.width + EDGE;
  const x = right + MARKER_WIDTH <= displayRight - EDGE
    ? right
    : Math.max(displayLeft + EDGE, capture.x - MARKER_WIDTH - EDGE);
  const highest = highestValueIndex(rows);
  return {
    mode: "cards",
    visible: rows.length > 0,
    rect: { x, y: capture.y, w: MARKER_WIDTH, h: capture.height },
    ttlMs: options.ttlMs ?? 20_000,
    style: {
      font: "monospace",
      fontSize: 20,
      background: "#00000000",
      border: "#00000000",
      text: "#ffffffff",
      muted: "#a89a85ff",
      accent: "#f0b847ff",
      radius: 9,
      padding: 0,
      gap: 0,
    },
    rows: rows.flatMap((row, index) => {
      const center = row.geometry?.center.y;
      if (center === undefined) return [];
      const iconId = row.unit === "div"
        ? options.iconIds?.div
        : row.unit === "ex"
          ? options.iconIds?.ex
          : undefined;
      return [{
        label: formatRewardMarker(row, Boolean(iconId)),
        top: Math.max(
          0,
          Math.min(capture.height - MARKER_HEIGHT, center * capture.height - MARKER_HEIGHT / 2),
        ),
        height: MARKER_HEIGHT,
        iconId,
        iconSize: 34,
        iconGap: 7,
        size: 20,
        color: index === highest
          ? "#50ff78ff"
          : row.unit === "div"
            ? "#f0b847ff"
            : "#ffffffff",
        bg: "#081018a6",
      }];
    }),
  };
}

export function rewardOverlayPayload(
  capture: CaptureRect,
  rows: PricedRow[],
  screenWidth: number,
  screenHeight: number,
  options: RewardOverlayOptions = {},
): HyprOverlayPayload {
  if (options.supportsPositionedRows && rows.some((row) => row.geometry)) {
    return positionedRewardPayload(capture, rows, screenWidth, options);
  }
  const height = Math.max(
    PANEL_MIN_HEIGHT,
    Math.min(PANEL_MAX_HEIGHT, 28 + rows.length * 42),
  );
  const right = capture.x + capture.width + EDGE;
  const x = right + PANEL_WIDTH <= screenWidth - EDGE
    ? right
    : Math.max(EDGE, capture.x - PANEL_WIDTH - EDGE);
  const y = Math.max(EDGE, Math.min(capture.y, screenHeight - height - EDGE));
  const highest = highestValueIndex(rows);
  return {
    mode: "cards",
    visible: rows.length > 0,
    rect: { x, y, w: PANEL_WIDTH, h: height },
    ttlMs: options.ttlMs ?? 20_000,
    style: {
      background: "#100c08c8",
      border: "#8f6a32dd",
      text: "#e8e0d2ff",
      muted: "#a89a85ff",
      accent: "#d29933ff",
      radius: 3,
      padding: 10,
      gap: 5,
    },
    rows: rows.map((row, index) => ({
      label: `${row.quantity > 1 ? `${row.quantity}x ` : ""}${row.name}`,
      value: formatRewardTotal(row) ?? "no price",
      detail: formatRewardEach(row) ?? undefined,
      emphasis: index === highest,
    })),
  };
}
