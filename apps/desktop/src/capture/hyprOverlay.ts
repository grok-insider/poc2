import { execFile } from "node:child_process";
import type { Socket } from "node:net";
import { join } from "node:path";

import {
  startHyprOverlayEventSocket,
  startHyprOverlaySelectionSocket,
} from "./hyprOverlaySelection";

export { parseHyprOverlayEventLine } from "./hyprOverlaySelection";

const HYPRCTL_TIMEOUT_MS = 800;
const HYPRCTL_MAX_BUFFER = 64 * 1024;
const HYPR_OVERLAY_PAYLOAD_BYTES = 16 * 1024;
const HYPR_OVERLAY_IMAGE_PAYLOAD_BYTES = 24 * 1024;

export type HyprOverlayMode = "cards" | "menu" | "selection";

export interface HyprOverlayRect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface HyprOverlayPayloadRect {
  x: number;
  y: number;
  w?: number;
  h?: number;
  width?: number;
  height?: number;
}

export interface HyprOverlayRow {
  kind?: "text" | "header" | "separator" | "columns";
  label?: string;
  value?: string;
  detail?: string;
  emphasis?: boolean;
  color?: string;
  valueColor?: string;
  detailColor?: string;
  bg?: string;
  size?: number;
  align?: "left" | "center" | "right";
  top?: number;
  height?: number;
  iconId?: string;
  iconSize?: number;
  iconGap?: number;
  cells?: Array<{
    text: string;
    color?: string;
    bg?: string;
    size?: number;
    align?: "left" | "center" | "right";
    weight?: number;
  }>;
}

export interface HyprOverlayPayload {
  mode?: HyprOverlayMode;
  visible?: boolean;
  rect: HyprOverlayPayloadRect;
  ttlMs?: number;
  style?: {
    font?: string;
    fontSize?: number;
    radius?: number;
    padding?: number;
    gap?: number;
    background?: string;
    border?: string;
    text?: string;
    muted?: string;
    accent?: string;
  };
  rows?: HyprOverlayRow[];
  menu?: {
    title?: string;
    subtitle?: string;
    footer?: string;
    preview?: string;
    budget?: string;
    activeTab?: string;
    focusIndex?: number;
    tabs?: Array<{ id: string; label: string }>;
    controls?: Array<{
      id: string;
      tab?: string;
      label: string;
      value?: string;
      detail?: string;
      kind?: "toggle" | "cycle" | "action" | "text";
      placeholder?: string;
      options?: string[];
      cursor?: number;
      maxLength?: number;
      selected?: boolean;
      disabled?: boolean;
    }>;
    inputFocused?: boolean;
  };
  interactive?: {
    enabled?: boolean;
    pointer?: boolean;
    keyboard?: boolean;
    text?: boolean;
    passthroughOutside?: boolean;
    dismissOnOutside?: boolean;
    overlayId?: string;
  };
  selection?: {
    draft?: HyprOverlayRect;
    border?: string;
    borderWidth?: number;
    hint?: string;
    hintColor?: string;
    hintSize?: number;
  };
}

export type HyprOverlaySelectionPayload = Omit<HyprOverlayPayload, "mode"> & {
  mode?: "selection";
};

export interface HyprOverlayLimits {
  payloadBytes: number;
  rows: number;
  tabs: number;
  controls: number;
  options: number;
  cells: number;
  textBytes: number;
  ttlMs: number;
  images: number;
  imageDimension: number;
  imageIdBytes: number;
  imageBytesTotal: number;
  imagePayloadBytes: number;
}

export interface HyprOverlayStatus {
  loaded: boolean;
  protocolVersion: number | null;
  capabilities: string[];
  limits: Partial<HyprOverlayLimits>;
  visible?: boolean;
  mode?: HyprOverlayMode;
  generation?: number;
  rows?: number;
  controls?: number;
  focusIndex?: number;
  inputFocused?: boolean;
  hoverIndex?: number | null;
  interactive?: boolean;
  images?: { count: number; bytes: number };
  eventSeq?: number;
  ttlMs?: number;
  rect?: HyprOverlayRect;
}

export interface HyprOverlayImageInput {
  id: string;
  width: number;
  height: number;
  rgbaBase64: string;
}

export interface HyprctlResult {
  stdout: string;
  stderr: string;
}

export type HyprctlRunner = (args: string[]) => Promise<HyprctlResult>;

export type HyprOverlaySocketFactory = (socketPath: string) => Socket;

export interface HyprOverlaySelectionWaitOptions {
  timeoutMs?: number;
  signal?: AbortSignal;
  env?: NodeJS.ProcessEnv;
  runner?: HyprctlRunner;
  socketFactory?: HyprOverlaySocketFactory;
}

export interface HyprOverlaySelectionListener {
  promise: Promise<HyprOverlayRect | null>;
  close(): void;
}

export interface HyprOverlayEvent {
  seq?: number;
  type: string;
  overlayId: string;
  controlId?: string;
  value?: string;
  selected?: boolean;
  selectedIds?: string[];
  selectedIdsTruncated?: boolean;
  /** Enriched from menu-output when the event envelope omits them. */
  activeTab?: string;
  focusIndex?: number;
  rect?: HyprOverlayRect;
}

/** Stable overlay id for the Search Regex interactive menu. */
export const REGEX_OVERLAY_ID = "poc2-regex";

export interface HyprOverlayEventSessionOptions {
  signal?: AbortSignal;
  env?: NodeJS.ProcessEnv;
  runner?: HyprctlRunner;
  socketFactory?: HyprOverlaySocketFactory;
  onEvent: (event: HyprOverlayEvent) => void;
}

export interface HyprOverlayEventSession {
  close(): void;
}

export interface HyprOverlayMenuOutput {
  mode?: string;
  overlayId?: string;
  activeTab?: string;
  focusIndex?: number;
  inputFocused?: boolean;
  selected?: Array<{ id: string; selected?: boolean }>;
}

function defaultRunner(args: string[]): Promise<HyprctlResult> {
  return new Promise((resolve, reject) => {
    execFile(
      "hyprctl",
      args,
      {
        encoding: "utf8",
        timeout: HYPRCTL_TIMEOUT_MS,
        maxBuffer: HYPRCTL_MAX_BUFFER,
        windowsHide: true,
      },
      (error, stdout, stderr) => {
        if (error) {
          reject(error);
          return;
        }
        resolve({ stdout, stderr });
      },
    );
  });
}

function hasPluginInList(stdout: string): boolean {
  return stdout.toLowerCase().includes("hyproverlay");
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function finiteNumber(value: unknown): number | undefined {
  return typeof value === "number" && Number.isFinite(value) ? value : undefined;
}

function nonNegativeInteger(value: unknown): number | undefined {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 0
    ? value
    : undefined;
}

function parseRect(value: unknown, requirePositive: boolean): HyprOverlayRect | undefined {
  if (!isRecord(value)) return undefined;
  const x = finiteNumber(value.x);
  const y = finiteNumber(value.y);
  const w = finiteNumber(value.w);
  const h = finiteNumber(value.h);
  if (x === undefined || y === undefined || w === undefined || h === undefined) {
    return undefined;
  }
  if (requirePositive ? w <= 0 || h <= 0 : w < 0 || h < 0) return undefined;
  return { x, y, w, h };
}

const LIMIT_KEYS: Array<keyof HyprOverlayLimits> = [
  "payloadBytes",
  "rows",
  "tabs",
  "controls",
  "options",
  "cells",
  "textBytes",
  "ttlMs",
  "images",
  "imageDimension",
  "imageIdBytes",
  "imageBytesTotal",
  "imagePayloadBytes",
];

export function parseHyprOverlayStatus(stdout: string): HyprOverlayStatus | null {
  try {
    const parsed: unknown = JSON.parse(stdout);
    if (!isRecord(parsed) || typeof parsed.loaded !== "boolean") return null;

    const limits: Partial<HyprOverlayLimits> = {};
    if (isRecord(parsed.limits)) {
      for (const key of LIMIT_KEYS) {
        const value = nonNegativeInteger(parsed.limits[key]);
        if (value !== undefined) limits[key] = value;
      }
    }

    const protocolVersion = nonNegativeInteger(parsed.protocolVersion) ?? null;
    const capabilities = Array.isArray(parsed.capabilities)
      ? parsed.capabilities.filter((value): value is string => typeof value === "string")
      : [];
    const mode =
      parsed.mode === "cards" || parsed.mode === "menu" || parsed.mode === "selection"
        ? parsed.mode
        : undefined;
    const images = isRecord(parsed.images)
      ? {
          count: nonNegativeInteger(parsed.images.count),
          bytes: nonNegativeInteger(parsed.images.bytes),
        }
      : undefined;
    const hoverIndex =
      parsed.hoverIndex === null ? null : nonNegativeInteger(parsed.hoverIndex);

    return {
      loaded: parsed.loaded,
      protocolVersion,
      capabilities,
      limits,
      ...(typeof parsed.visible === "boolean" ? { visible: parsed.visible } : {}),
      ...(mode ? { mode } : {}),
      ...(nonNegativeInteger(parsed.generation) !== undefined
        ? { generation: nonNegativeInteger(parsed.generation) }
        : {}),
      ...(nonNegativeInteger(parsed.rows) !== undefined
        ? { rows: nonNegativeInteger(parsed.rows) }
        : {}),
      ...(nonNegativeInteger(parsed.controls) !== undefined
        ? { controls: nonNegativeInteger(parsed.controls) }
        : {}),
      ...(nonNegativeInteger(parsed.focusIndex) !== undefined
        ? { focusIndex: nonNegativeInteger(parsed.focusIndex) }
        : {}),
      ...(typeof parsed.inputFocused === "boolean"
        ? { inputFocused: parsed.inputFocused }
        : {}),
      ...(hoverIndex !== undefined ? { hoverIndex } : {}),
      ...(typeof parsed.interactive === "boolean"
        ? { interactive: parsed.interactive }
        : {}),
      ...(images?.count !== undefined && images.bytes !== undefined
        ? { images: { count: images.count, bytes: images.bytes } }
        : {}),
      ...(nonNegativeInteger(parsed.eventSeq) !== undefined
        ? { eventSeq: nonNegativeInteger(parsed.eventSeq) }
        : {}),
      ...(nonNegativeInteger(parsed.ttlMs) !== undefined
        ? { ttlMs: nonNegativeInteger(parsed.ttlMs) }
        : {}),
      ...(parseRect(parsed.rect, false) ? { rect: parseRect(parsed.rect, false) } : {}),
    };
  } catch {
    return null;
  }
}

export async function getHyprOverlayStatus(
  runner: HyprctlRunner = defaultRunner,
): Promise<HyprOverlayStatus | null> {
  try {
    const status = await runner(["-j", "hyproverlay", "status"]);
    return parseHyprOverlayStatus(status.stdout);
  } catch {
    return null;
  }
}

export async function detectHyprOverlay(
  runner: HyprctlRunner = defaultRunner,
): Promise<boolean> {
  try {
    const list = await runner(["plugin", "list"]);
    if (!hasPluginInList(list.stdout)) return false;
    return (await getHyprOverlayStatus(runner))?.loaded === true;
  } catch {
    return false;
  }
}

export async function sendHyprOverlay(
  payload: HyprOverlayPayload,
  runner: HyprctlRunner = defaultRunner,
): Promise<boolean> {
  try {
    const json = JSON.stringify(payload);
    if (Buffer.byteLength(json, "utf8") > HYPR_OVERLAY_PAYLOAD_BYTES) return false;
    const res = await runner(["hyproverlay", "set-json", json]);
    return res.stdout.trim() === "ok";
  } catch {
    return false;
  }
}

export function sendHyprOverlaySelection(
  payload: HyprOverlaySelectionPayload,
  runner: HyprctlRunner = defaultRunner,
): Promise<boolean> {
  return sendHyprOverlay({ ...payload, mode: "selection" }, runner);
}

export function isValidHyprOverlayImageId(id: unknown, allowAll = false): id is string {
  return (
    typeof id === "string" &&
    (allowAll || id !== "all") &&
    /^[A-Za-z0-9._:-]{1,64}$/.test(id)
  );
}

export function isValidHyprOverlayImageInput(
  input: unknown,
): input is HyprOverlayImageInput {
  if (!isRecord(input) || !isValidHyprOverlayImageId(input.id)) return false;
  const { width, height, rgbaBase64 } = input;
  if (
    !Number.isInteger(width) ||
    !Number.isInteger(height) ||
    typeof width !== "number" ||
    typeof height !== "number" ||
    width < 1 ||
    height < 1 ||
    width > 64 ||
    height > 64 ||
    typeof rgbaBase64 !== "string"
  ) {
    return false;
  }

  const decodedBytes = width * height * 4;
  const encodedBytes = 4 * Math.ceil(decodedBytes / 3);
  if (
    rgbaBase64.length !== encodedBytes ||
    !/^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/.test(
      rgbaBase64,
    )
  ) {
    return false;
  }
  const decoded = Buffer.from(rgbaBase64, "base64");
  return decoded.length === decodedBytes && decoded.toString("base64") === rgbaBase64;
}

export async function registerHyprOverlayImage(
  input: HyprOverlayImageInput,
  runner: HyprctlRunner = defaultRunner,
): Promise<boolean> {
  if (!isValidHyprOverlayImageInput(input)) return false;
  try {
    const json = JSON.stringify(input);
    if (Buffer.byteLength(json, "utf8") > HYPR_OVERLAY_IMAGE_PAYLOAD_BYTES) {
      return false;
    }
    const res = await runner(["hyproverlay", "image-set-json", json]);
    return res.stdout.trim() === "ok";
  } catch {
    return false;
  }
}

export async function clearHyprOverlayImage(
  id: string,
  runner: HyprctlRunner = defaultRunner,
): Promise<boolean> {
  if (id !== "all" && !isValidHyprOverlayImageId(id)) return false;
  try {
    const res = await runner(["hyproverlay", "image-clear", id]);
    const output = res.stdout.trim();
    return output === "ok" || output === "noop";
  } catch {
    return false;
  }
}

interface HyprlandMonitor {
  x?: unknown;
  y?: unknown;
  width?: unknown;
  height?: unknown;
  scale?: unknown;
  transform?: unknown;
  disabled?: unknown;
}

export function parseHyprlandMonitorBounds(raw: string): HyprOverlayRect | null {
  try {
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) return null;

    let left = Infinity;
    let top = Infinity;
    let right = -Infinity;
    let bottom = -Infinity;
    for (const candidate of parsed) {
      if (!isRecord(candidate)) continue;
      const monitor = candidate as HyprlandMonitor;
      if (monitor.disabled === true) continue;
      const x = finiteNumber(monitor.x);
      const y = finiteNumber(monitor.y);
      const width = finiteNumber(monitor.width);
      const height = finiteNumber(monitor.height);
      const scale = monitor.scale === undefined ? 1 : finiteNumber(monitor.scale);
      const transform = nonNegativeInteger(monitor.transform) ?? 0;
      if (
        x === undefined ||
        y === undefined ||
        width === undefined ||
        height === undefined ||
        scale === undefined ||
        width <= 0 ||
        height <= 0 ||
        scale <= 0
      ) {
        continue;
      }
      const swapsAxes = transform % 2 === 1;
      const logicalWidth = (swapsAxes ? height : width) / scale;
      const logicalHeight = (swapsAxes ? width : height) / scale;
      left = Math.min(left, x);
      top = Math.min(top, y);
      right = Math.max(right, x + logicalWidth);
      bottom = Math.max(bottom, y + logicalHeight);
    }

    return Number.isFinite(left)
      ? { x: left, y: top, w: right - left, h: bottom - top }
      : null;
  } catch {
    return null;
  }
}

export async function virtualDesktopBounds(
  runner: HyprctlRunner = defaultRunner,
): Promise<HyprOverlayRect | null> {
  try {
    const monitors = await runner(["monitors", "-j"]);
    return parseHyprlandMonitorBounds(monitors.stdout);
  } catch {
    return null;
  }
}

function validInstanceSignature(signature: unknown): signature is string {
  return (
    typeof signature === "string" &&
    signature.length > 0 &&
    signature.length <= 256 &&
    signature !== "." &&
    signature !== ".." &&
    !signature.includes("/") &&
    !signature.includes("\\") &&
    !signature.includes("\0")
  );
}

export function parseHyprlandInstanceSignatures(raw: string): string[] {
  try {
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed
      .map((value) => (isRecord(value) ? value.instance : undefined))
      .filter(validInstanceSignature);
  } catch {
    return [];
  }
}

export function hyprlandSocket2Path(
  runtimeDir: string | undefined,
  signature: string | undefined,
): string | null {
  if (!runtimeDir || !validInstanceSignature(signature)) return null;
  return join(runtimeDir, "hypr", signature, ".socket2.sock");
}

export async function resolveHyprlandSocket2Path(
  env: NodeJS.ProcessEnv = process.env,
  runner: HyprctlRunner = defaultRunner,
): Promise<string | null> {
  let signature = validInstanceSignature(env.HYPRLAND_INSTANCE_SIGNATURE)
    ? env.HYPRLAND_INSTANCE_SIGNATURE
    : undefined;
  if (!signature) {
    try {
      const instances = await runner(["instances", "-j"]);
      signature = parseHyprlandInstanceSignatures(instances.stdout)[0];
    } catch {
      return null;
    }
  }
  return hyprlandSocket2Path(env.XDG_RUNTIME_DIR, signature);
}

export async function startHyprOverlaySelectionListener(
  overlayId: string,
  options: HyprOverlaySelectionWaitOptions = {},
): Promise<HyprOverlaySelectionListener> {
  const socketPath = await resolveHyprlandSocket2Path(
    options.env,
    options.runner ?? defaultRunner,
  );
  if (!socketPath) throw new Error("Hyprland socket2 path is unavailable");
  return startHyprOverlaySelectionSocket(socketPath, overlayId, options);
}

/**
 * Long-lived socket2 session for interactive menu events (regex, etc.).
 * Does not auto-close on submit — caller closes when the menu is dismissed.
 */
export async function startHyprOverlayEventSession(
  overlayId: string,
  options: HyprOverlayEventSessionOptions,
): Promise<HyprOverlayEventSession> {
  const socketPath = await resolveHyprlandSocket2Path(
    options.env,
    options.runner ?? defaultRunner,
  );
  if (!socketPath) throw new Error("Hyprland socket2 path is unavailable");
  return startHyprOverlayEventSocket(socketPath, overlayId, {
    onEvent: options.onEvent,
    signal: options.signal,
    socketFactory: options.socketFactory,
  });
}

/** Opt a visible interactive menu into keyboard capture (plugin menu focus). */
export async function focusHyprOverlay(
  runner: HyprctlRunner = defaultRunner,
): Promise<boolean> {
  try {
    const res = await runner(["hyproverlay", "focus"]);
    return res.stdout.trim() === "ok";
  } catch {
    return false;
  }
}

/**
 * Full menu selection/focus snapshot. Use when event `selectedIds` were truncated
 * (Hyprland caps event JSON at 1024 bytes).
 */
export async function fetchHyprOverlayMenuOutput(
  runner: HyprctlRunner = defaultRunner,
): Promise<HyprOverlayMenuOutput | null> {
  try {
    // Plugin always returns JSON for menu-output (no hyprctl -j needed).
    const res = await runner(["hyproverlay", "menu-output"]);
    const parsed: unknown = JSON.parse(res.stdout);
    if (!isRecord(parsed)) return null;
    const selected = Array.isArray(parsed.selected)
      ? parsed.selected.flatMap((row) => {
          if (!isRecord(row) || typeof row.id !== "string") return [];
          return [{ id: row.id, selected: row.selected === true }];
        })
      : undefined;
    return {
      ...(typeof parsed.mode === "string" ? { mode: parsed.mode } : {}),
      ...(typeof parsed.overlayId === "string" ? { overlayId: parsed.overlayId } : {}),
      ...(typeof parsed.activeTab === "string" ? { activeTab: parsed.activeTab } : {}),
      ...(nonNegativeInteger(parsed.focusIndex) !== undefined
        ? { focusIndex: nonNegativeInteger(parsed.focusIndex) }
        : {}),
      ...(typeof parsed.inputFocused === "boolean"
        ? { inputFocused: parsed.inputFocused }
        : {}),
      ...(selected ? { selected } : {}),
    };
  } catch {
    return null;
  }
}

/** True when a payload is the interactive Search Regex menu we own. */
export function isInteractiveRegexMenuPayload(payload: unknown): boolean {
  if (!isRecord(payload)) return false;
  if (payload.mode !== "menu" || payload.visible === false) return false;
  const interactive = payload.interactive;
  if (!isRecord(interactive) || interactive.enabled !== true) return false;
  return interactive.overlayId === REGEX_OVERLAY_ID;
}

export async function waitForHyprOverlaySelection(
  overlayId: string,
  options: HyprOverlaySelectionWaitOptions = {},
): Promise<HyprOverlayRect | null> {
  try {
    const listener = await startHyprOverlaySelectionListener(overlayId, options);
    return await listener.promise;
  } catch {
    return null;
  }
}

export async function hideHyprOverlay(
  runner: HyprctlRunner = defaultRunner,
): Promise<boolean> {
  try {
    const res = await runner(["hyproverlay", "hide"]);
    return res.stdout.trim() === "ok";
  } catch {
    return false;
  }
}
