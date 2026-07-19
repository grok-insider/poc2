import { createConnection, type Socket } from "node:net";

import type {
  HyprOverlayEvent,
  HyprOverlayEventSession,
  HyprOverlayRect,
  HyprOverlaySelectionListener,
  HyprOverlaySelectionWaitOptions,
  HyprOverlaySocketFactory,
} from "./hyprOverlay";

const HYPR_OVERLAY_EVENT_BYTES = 64 * 1024;
const DEFAULT_SELECTION_TIMEOUT_MS = 30_000;

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function nonNegativeInteger(value: unknown): number | undefined {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 0
    ? value
    : undefined;
}

function parseRect(value: unknown): HyprOverlayRect | undefined {
  if (!isRecord(value)) return undefined;
  const { x, y, w, h } = value;
  if (
    typeof x !== "number" ||
    !Number.isFinite(x) ||
    typeof y !== "number" ||
    !Number.isFinite(y) ||
    typeof w !== "number" ||
    !Number.isFinite(w) ||
    typeof h !== "number" ||
    !Number.isFinite(h) ||
    w <= 0 ||
    h <= 0
  ) {
    return undefined;
  }
  return { x, y, w, h };
}

export function parseHyprOverlayEventLine(line: string): HyprOverlayEvent | null {
  const prefix = "hyproverlay>>";
  const normalized = line.endsWith("\r") ? line.slice(0, -1) : line;
  if (!normalized.startsWith(prefix)) return null;
  try {
    const parsed: unknown = JSON.parse(normalized.slice(prefix.length));
    if (
      !isRecord(parsed) ||
      typeof parsed.type !== "string" ||
      typeof parsed.overlayId !== "string"
    ) {
      return null;
    }
    const rect = parseRect(parsed.rect);
    return {
      type: parsed.type,
      overlayId: parsed.overlayId,
      ...(nonNegativeInteger(parsed.seq) !== undefined
        ? { seq: nonNegativeInteger(parsed.seq) }
        : {}),
      ...(typeof parsed.controlId === "string" ? { controlId: parsed.controlId } : {}),
      ...(typeof parsed.value === "string" ? { value: parsed.value } : {}),
      ...(typeof parsed.selected === "boolean" ? { selected: parsed.selected } : {}),
      ...(Array.isArray(parsed.selectedIds) &&
      parsed.selectedIds.every((value) => typeof value === "string")
        ? { selectedIds: parsed.selectedIds as string[] }
        : {}),
      ...(typeof parsed.selectedIdsTruncated === "boolean"
        ? { selectedIdsTruncated: parsed.selectedIdsTruncated }
        : {}),
      ...(rect ? { rect } : {}),
    };
  } catch {
    return null;
  }
}

function defaultSocketFactory(socketPath: string): Socket {
  return createConnection(socketPath);
}

/** Resolves only after socket2 is connected, so callers can then send the payload safely. */
export async function startHyprOverlaySelectionSocket(
  socketPath: string,
  overlayId: string,
  options: Pick<
    HyprOverlaySelectionWaitOptions,
    "timeoutMs" | "signal" | "socketFactory"
  >,
): Promise<HyprOverlaySelectionListener> {
  if (!overlayId) throw new Error("overlayId is required");
  if (options.signal?.aborted) throw new Error("selection listener aborted");

  const timeoutMs = options.timeoutMs ?? DEFAULT_SELECTION_TIMEOUT_MS;
  if (!Number.isFinite(timeoutMs) || timeoutMs < 0) {
    throw new Error("timeoutMs must be a non-negative finite number");
  }

  const socket = (options.socketFactory ?? defaultSocketFactory)(socketPath);
  let settled = false;
  let connected = false;
  let pending = "";
  let resolveResult!: (value: HyprOverlayRect | null) => void;
  let resolveReady!: () => void;
  let rejectReady!: (reason: Error) => void;
  const promise = new Promise<HyprOverlayRect | null>((resolve) => {
    resolveResult = resolve;
  });
  const ready = new Promise<void>((resolve, reject) => {
    resolveReady = resolve;
    rejectReady = reject;
  });

  let timer: NodeJS.Timeout | undefined;
  const onAbort = () => settle(null, new Error("selection listener aborted"));
  const settle = (result: HyprOverlayRect | null, beforeReadyError?: Error) => {
    if (settled) return;
    settled = true;
    if (timer) clearTimeout(timer);
    options.signal?.removeEventListener("abort", onAbort);
    if (!connected) rejectReady(beforeReadyError ?? new Error("selection socket closed"));
    resolveResult(result);
    socket.destroy();
  };

  socket.once("connect", () => {
    if (settled) return;
    connected = true;
    resolveReady();
  });
  socket.on("data", (chunk) => {
    if (settled) return;
    pending += chunk.toString("utf8");
    if (Buffer.byteLength(pending, "utf8") > HYPR_OVERLAY_EVENT_BYTES) {
      pending = "";
      return;
    }
    let newline = pending.indexOf("\n");
    while (newline >= 0) {
      const line = pending.slice(0, newline);
      pending = pending.slice(newline + 1);
      const event = parseHyprOverlayEventLine(line);
      if (event?.overlayId === overlayId) {
        if (event.type === "dismiss") {
          settle(null);
          return;
        }
        if (event.type === "submit" && event.rect) {
          settle(event.rect);
          return;
        }
      }
      newline = pending.indexOf("\n");
    }
  });
  socket.once("error", (error) => settle(null, error));
  socket.once("end", () => settle(null));
  socket.once("close", () => settle(null));
  timer = setTimeout(
    () => settle(null, new Error("selection listener timed out")),
    timeoutMs,
  );
  options.signal?.addEventListener("abort", onAbort, { once: true });
  if (options.signal?.aborted) onAbort();

  await ready;
  return {
    promise,
    close: () => settle(null),
  };
}

/**
 * Long-lived socket2 subscription for interactive menu/selection events.
 * Filters by overlayId; stays open until close() or abort. Does not settle on
 * submit/dismiss — the app decides when the menu session ends.
 */
export async function startHyprOverlayEventSocket(
  socketPath: string,
  overlayId: string,
  options: {
    onEvent: (event: HyprOverlayEvent) => void;
    signal?: AbortSignal;
    socketFactory?: HyprOverlaySocketFactory;
  },
): Promise<HyprOverlayEventSession> {
  if (!overlayId) throw new Error("overlayId is required");
  if (options.signal?.aborted) throw new Error("event session aborted");

  const socket = (options.socketFactory ?? defaultSocketFactory)(socketPath);
  let closed = false;
  let connected = false;
  let pending = "";
  let resolveReady!: () => void;
  let rejectReady!: (reason: Error) => void;
  const ready = new Promise<void>((resolve, reject) => {
    resolveReady = resolve;
    rejectReady = reject;
  });

  const close = () => {
    if (closed) return;
    closed = true;
    options.signal?.removeEventListener("abort", onAbort);
    if (!connected) rejectReady(new Error("event session closed"));
    socket.destroy();
  };
  const onAbort = () => close();

  socket.once("connect", () => {
    if (closed) return;
    connected = true;
    resolveReady();
  });
  socket.on("data", (chunk) => {
    if (closed) return;
    pending += chunk.toString("utf8");
    if (Buffer.byteLength(pending, "utf8") > HYPR_OVERLAY_EVENT_BYTES) {
      pending = "";
      return;
    }
    let newline = pending.indexOf("\n");
    while (newline >= 0) {
      const line = pending.slice(0, newline);
      pending = pending.slice(newline + 1);
      const event = parseHyprOverlayEventLine(line);
      if (event?.overlayId === overlayId) {
        options.onEvent(event);
      }
      newline = pending.indexOf("\n");
    }
  });
  socket.once("error", (error) => {
    if (!connected) rejectReady(error);
    close();
  });
  socket.once("end", () => close());
  socket.once("close", () => close());
  options.signal?.addEventListener("abort", onAbort, { once: true });
  if (options.signal?.aborted) onAbort();

  await ready;
  return { close };
}
