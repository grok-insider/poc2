export interface ScanDiagnostics {
  updatedAt: string;
  transport: "full" | "degraded" | "hyprland-plugin";
  captureWidth?: number;
  captureHeight?: number;
  selectedCrop?: number;
  selectedScale?: number;
  ocrBackend?: "windows-media-ocr" | "tesseract-fast" | "tesseract-fallback";
  captureMs?: number;
  decodeMs?: number;
  fastOcrMs?: number;
  fallbackOcrMs?: number;
  totalMs?: number;
  rawText?: string;
  rawRows?: string[];
  resolvedRows?: string[];
  lineRows?: string[];
  pluginProtocol?: number;
  pluginCapabilities?: string[];
  renderOk?: boolean;
  watcherEnabled?: boolean;
  error?: string;
}

let latest: ScanDiagnostics | null = null;

export function setScanDiagnostics(raw: unknown): ScanDiagnostics | null {
  if (!raw || typeof raw !== "object") return null;
  const value = raw as Partial<ScanDiagnostics>;
  if (
    typeof value.updatedAt !== "string" ||
    !["full", "degraded", "hyprland-plugin"].includes(value.transport ?? "")
  ) {
    return null;
  }
  latest = {
    updatedAt: value.updatedAt,
    transport: value.transport!,
    captureWidth: finite(value.captureWidth),
    captureHeight: finite(value.captureHeight),
    selectedCrop: finite(value.selectedCrop),
    selectedScale: finite(value.selectedScale),
    ocrBackend: ["windows-media-ocr", "tesseract-fast", "tesseract-fallback"].includes(
      value.ocrBackend ?? "",
    ) ? value.ocrBackend : undefined,
    captureMs: finite(value.captureMs),
    decodeMs: finite(value.decodeMs),
    fastOcrMs: finite(value.fastOcrMs),
    fallbackOcrMs: finite(value.fallbackOcrMs),
    totalMs: finite(value.totalMs),
    rawText: typeof value.rawText === "string" ? value.rawText.slice(0, 2_000) : undefined,
    rawRows: strings(value.rawRows),
    resolvedRows: strings(value.resolvedRows),
    lineRows: strings(value.lineRows),
    pluginProtocol: finite(value.pluginProtocol),
    pluginCapabilities: strings(value.pluginCapabilities),
    renderOk: typeof value.renderOk === "boolean" ? value.renderOk : undefined,
    watcherEnabled:
      typeof value.watcherEnabled === "boolean" ? value.watcherEnabled : undefined,
    error: typeof value.error === "string" ? value.error.slice(0, 500) : undefined,
  };
  return latest;
}

export function getScanDiagnostics(): ScanDiagnostics | null {
  return latest ? { ...latest } : null;
}

function finite(value: unknown): number | undefined {
  return typeof value === "number" && Number.isFinite(value) ? value : undefined;
}

function strings(value: unknown): string[] | undefined {
  if (!Array.isArray(value)) return undefined;
  return value
    .filter((item): item is string => typeof item === "string")
    .slice(0, 20)
    .map((item) => item.slice(0, 160));
}
