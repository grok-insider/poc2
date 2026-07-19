"use client";

/// WebSocket client for the `poc2-capture` daemon (ADR-0011).
///
/// The daemon runs on the desktop (`poc2-capture serve`, loopback only) and
/// pushes hotkey-captured items: a Hyprland bind injects the game's own
/// Ctrl+C, the daemon reads the clipboard and broadcasts the item text (or
/// a cursor-region screenshot in OCR mode). The browser can't read the
/// clipboard without focus — this push is what makes capture feel like an
/// overlay instead of "alt-tab and paste".
///
/// Connection is strictly best-effort: silent retries with backoff, no
/// errors surfaced unless the user looks at Settings → Capture. The web app
/// works fully without the daemon.

export type CaptureBridgeEvent =
  | { type: "hello"; version: string }
  | { type: "item-text"; text: string; advanced?: boolean }
  | { type: "item-image"; png_base64: string }
  | { type: "capture-error"; message: string };

export type CaptureStatus = "connected" | "disconnected";

export interface CaptureBridgeHandlers {
  onEvent: (ev: CaptureBridgeEvent) => void;
  onStatus: (status: CaptureStatus, daemonVersion?: string) => void;
}

const URL_WS = "ws://127.0.0.1:17771/ws";
const BACKOFF_START_MS = 3_000;
const BACKOFF_MAX_MS = 30_000;

/** Connect (and keep reconnecting) to the capture daemon. Returns a stop fn. */
export function startCaptureBridge(handlers: CaptureBridgeHandlers): () => void {
  if (typeof window === "undefined" || typeof WebSocket === "undefined") {
    return () => {};
  }

  let ws: WebSocket | null = null;
  let stopped = false;
  let backoff = BACKOFF_START_MS;
  let retryTimer: ReturnType<typeof setTimeout> | null = null;

  const connect = () => {
    if (stopped) return;
    try {
      ws = new WebSocket(URL_WS);
    } catch {
      scheduleRetry();
      return;
    }
    ws.onopen = () => {
      backoff = BACKOFF_START_MS;
    };
    ws.onmessage = (e) => {
      try {
        const ev = JSON.parse(String(e.data)) as CaptureBridgeEvent;
        if (ev.type === "hello") {
          handlers.onStatus("connected", ev.version);
        }
        handlers.onEvent(ev);
      } catch {
        /* malformed frame — ignore */
      }
    };
    ws.onclose = () => {
      handlers.onStatus("disconnected");
      scheduleRetry();
    };
    ws.onerror = () => {
      // onclose follows; nothing to do (and nothing to log — the daemon
      // simply isn't running for browser-only users).
    };
  };

  const scheduleRetry = () => {
    if (stopped) return;
    if (retryTimer) clearTimeout(retryTimer);
    retryTimer = setTimeout(() => {
      backoff = Math.min(backoff * 2, BACKOFF_MAX_MS);
      connect();
    }, backoff);
  };

  connect();

  return () => {
    stopped = true;
    if (retryTimer) clearTimeout(retryTimer);
    ws?.close();
  };
}

/** Decode the daemon's base64 PNG into a Blob for the OCR pipeline. */
export function pngBase64ToBlob(b64: string): Blob {
  const bin = atob(b64);
  const bytes = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
  return new Blob([bytes], { type: "image/png" });
}
