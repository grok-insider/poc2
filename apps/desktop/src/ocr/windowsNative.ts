import { spawn, type ChildProcessWithoutNullStreams } from "node:child_process";
import { existsSync } from "node:fs";
import path from "node:path";
import { createInterface, type Interface as ReadlineInterface } from "node:readline";

export interface NativeOcrLine {
  text: string;
  confidence: number;
  boundingBox: { x: number; y: number; width: number; height: number };
}

export interface NativeOcrResult {
  text: string;
  lines: NativeOcrLine[];
}

export interface NativeOcrStatus {
  available: boolean;
  backend: "windows-media-ocr";
  helperPath: string | null;
  lastError: string | null;
}

export interface NativeOcrController {
  recognize(dataUrl: string, language?: string): Promise<NativeOcrResult | null>;
  status(): NativeOcrStatus;
  stop(): void;
}

interface HelperResponse {
  version: number;
  id?: string;
  ok: boolean;
  result?: {
    text: string;
    lines: Array<{
      text: string;
      confidence: number;
      bounding_box: { x: number; y: number; width: number; height: number };
    }>;
  };
  error?: { message?: string };
}

interface PendingRequest {
  resolve: (result: NativeOcrResult | null) => void;
  reject: (error: Error) => void;
  timeout: ReturnType<typeof setTimeout>;
}

export interface WindowsOcrPathOptions {
  platform: NodeJS.Platform;
  resourcesPath: string;
  appPath: string;
  override?: string;
}

export function resolveWindowsOcrHelperPath(options: WindowsOcrPathOptions): string | null {
  if (options.platform !== "win32") return null;
  const executable = "poc2-windows-ocr.exe";
  const candidates = [
    options.override,
    path.join(options.resourcesPath, "windows-ocr", executable),
    path.join(options.appPath, "native", "windows-ocr", executable),
    path.resolve(options.appPath, "..", "..", "target", "release", executable),
    path.resolve(options.appPath, "..", "..", "target", "debug", executable),
  ].filter((candidate): candidate is string => typeof candidate === "string" && candidate.length > 0);
  return candidates.find(existsSync) ?? null;
}

class WindowsOcrProcess implements NativeOcrController {
  private child: ChildProcessWithoutNullStreams | null = null;
  private lines: ReadlineInterface | null = null;
  private readonly pending = new Map<string, PendingRequest>();
  private nextId = 1;
  private lastError: string | null = null;

  constructor(private readonly helperPath: string | null) {}

  status(): NativeOcrStatus {
    return {
      available: this.helperPath !== null,
      backend: "windows-media-ocr",
      helperPath: this.helperPath,
      lastError: this.lastError,
    };
  }

  async recognize(dataUrl: string, language = "en-US"): Promise<NativeOcrResult | null> {
    if (!this.helperPath) return null;
    const comma = dataUrl.indexOf(",");
    if (comma < 0 || !dataUrl.slice(0, comma).includes(";base64")) {
      throw new Error("native OCR requires a base64 data URL");
    }
    const imageBase64 = dataUrl.slice(comma + 1);
    if (imageBase64.length > 15_000_000) {
      throw new Error("native OCR image exceeds the helper request limit");
    }

    try {
      const child = this.ensureChild();
      const id = String(this.nextId++);
      const response = new Promise<NativeOcrResult | null>((resolve, reject) => {
        const timeout = setTimeout(() => {
          this.pending.delete(id);
          reject(new Error("Windows OCR helper timed out"));
          this.stop();
        }, 5_000);
        this.pending.set(id, { resolve, reject, timeout });
      });
      child.stdin.write(`${JSON.stringify({
        version: 1,
        id,
        type: "recognize",
        imageBase64,
        language,
      })}\n`);
      return await response;
    } catch (error) {
      this.lastError = error instanceof Error ? error.message : String(error);
      throw error;
    }
  }

  stop(): void {
    this.lines?.close();
    this.lines = null;
    const child = this.child;
    this.child = null;
    if (child && !child.killed) child.kill();
    this.rejectPending(new Error("Windows OCR helper stopped"));
  }

  private ensureChild(): ChildProcessWithoutNullStreams {
    if (this.child && !this.child.killed) return this.child;
    if (!this.helperPath) throw new Error("Windows OCR helper is unavailable");

    const child = spawn(this.helperPath, [], {
      stdio: ["pipe", "pipe", "pipe"],
      windowsHide: true,
    });
    this.child = child;
    this.lines = createInterface({ input: child.stdout });
    this.lines.on("line", (line) => this.handleLine(line));
    child.stderr.setEncoding("utf8");
    child.stderr.on("data", (chunk: string) => {
      const message = chunk.trim();
      if (message) this.lastError = message.slice(-1_000);
    });
    child.once("error", (error) => {
      this.lastError = error.message;
      this.child = null;
      this.rejectPending(error);
    });
    child.once("close", (code) => {
      this.child = null;
      this.lines?.close();
      this.lines = null;
      if (this.pending.size > 0) {
        this.rejectPending(new Error(`Windows OCR helper exited with code ${code ?? "unknown"}`));
      }
    });
    return child;
  }

  private handleLine(line: string): void {
    let response: HelperResponse;
    try {
      response = JSON.parse(line) as HelperResponse;
    } catch {
      this.lastError = "Windows OCR helper returned invalid JSON";
      return;
    }
    if (!response.id) return;
    const pending = this.pending.get(response.id);
    if (!pending) return;
    this.pending.delete(response.id);
    clearTimeout(pending.timeout);
    if (!response.ok || !response.result) {
      pending.reject(new Error(response.error?.message ?? "Windows OCR recognition failed"));
      return;
    }
    pending.resolve({
      text: response.result.text,
      lines: response.result.lines.map((lineResult) => ({
        text: lineResult.text,
        confidence: lineResult.confidence,
        boundingBox: lineResult.bounding_box,
      })),
    });
  }

  private rejectPending(error: Error): void {
    for (const pending of this.pending.values()) {
      clearTimeout(pending.timeout);
      pending.reject(error);
    }
    this.pending.clear();
  }
}

export function createWindowsOcrController(options: WindowsOcrPathOptions): NativeOcrController {
  return new WindowsOcrProcess(resolveWindowsOcrHelperPath(options));
}
