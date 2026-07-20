import { registerHyprOverlayImage } from "./hyprOverlay";

export const PRICE_ICON_IDS = {
  div: "poc2.currency.div",
  ex: "poc2.currency.ex",
} as const;

/** Matches REWARD_TOKENS.iconSize in apps/web/lib/overlay/rewards.ts. */
export const PRICE_ICON_SIZE = 34;

const registeredUrls = new Map<string, string>();
const dataUrlCache = new Map<string, string>();

export interface PriceIconUrls {
  div?: string;
  ex?: string;
}

export interface DecodedIconBitmap {
  width: number;
  height: number;
  data: Buffer;
}

export interface PriceIconLoadDependencies {
  fetchBytes(url: string): Promise<Buffer>;
  decodeBgra(bytes: Buffer, size: number): Promise<DecodedIconBitmap>;
}

export interface PriceIconDependencies extends PriceIconLoadDependencies {
  register(input: {
    id: string;
    width: number;
    height: number;
    rgbaBase64: string;
  }): Promise<boolean>;
}

export interface PriceIconDataUrlDependencies extends PriceIconLoadDependencies {
  /** Optional: produce a data URL from the resized source bytes (default uses Electron). */
  toDataUrl?(bytes: Buffer, size: number): Promise<string | null>;
}

export function isAllowedPriceIconUrl(value: string): boolean {
  try {
    const url = new URL(value);
    return (
      url.protocol === "https:" &&
      (url.hostname === "poe2scout.com" ||
        url.hostname.endsWith(".poe2scout.com") ||
        url.hostname === "poecdn.com" ||
        url.hostname.endsWith(".poecdn.com"))
    );
  } catch {
    return false;
  }
}

/** Convert Electron's premultiplied BGRA bitmap into straight RGBA for v4. */
export function bgraToRgba(data: Uint8Array): Buffer {
  if (data.length % 4 !== 0) throw new Error("BGRA data length must be divisible by four");
  const out = Buffer.allocUnsafe(data.length);
  for (let i = 0; i < data.length; i += 4) {
    const alpha = data[i + 3]!;
    const straight = (channel: number) =>
      alpha === 0 ? 0 : Math.min(255, Math.round((channel * 255) / alpha));
    out[i] = straight(data[i + 2]!);
    out[i + 1] = straight(data[i + 1]!);
    out[i + 2] = straight(data[i]!);
    out[i + 3] = alpha;
  }
  return out;
}

function isValidIconBitmap(image: DecodedIconBitmap): boolean {
  return (
    image.width >= 1 &&
    image.height >= 1 &&
    image.width <= 64 &&
    image.height <= 64 &&
    image.data.length === image.width * image.height * 4
  );
}

async function defaultLoadDependencies(): Promise<PriceIconLoadDependencies> {
  const { nativeImage, net } = await import("electron");
  return {
    async fetchBytes(url) {
      const response = await net.fetch(url);
      if (!response.ok) throw new Error(`price icon fetch failed: ${response.status}`);
      return Buffer.from(await response.arrayBuffer());
    },
    async decodeBgra(bytes, size) {
      const image = nativeImage.createFromBuffer(bytes);
      if (image.isEmpty()) throw new Error("price icon decode failed");
      const resized = image.resize({ width: size, height: size, quality: "best" });
      const dimensions = resized.getSize();
      return { width: dimensions.width, height: dimensions.height, data: resized.toBitmap() };
    },
  };
}

async function defaultDependencies(): Promise<PriceIconDependencies> {
  const load = await defaultLoadDependencies();
  return {
    ...load,
    register: registerHyprOverlayImage,
  };
}

/** Shared load path: allowlisted fetch + square BGRA bitmap. */
export async function loadUnitIconBitmaps(
  urls: PriceIconUrls,
  dependencies?: PriceIconLoadDependencies,
): Promise<Partial<Record<keyof PriceIconUrls, DecodedIconBitmap & { sourceUrl: string }>>> {
  const deps = dependencies ?? (await defaultLoadDependencies());
  const out: Partial<Record<keyof PriceIconUrls, DecodedIconBitmap & { sourceUrl: string }>> = {};
  for (const unit of ["div", "ex"] as const) {
    const url = urls[unit];
    if (!url || !isAllowedPriceIconUrl(url)) continue;
    try {
      const encoded = await deps.fetchBytes(url);
      const image = await deps.decodeBgra(encoded, PRICE_ICON_SIZE);
      if (!isValidIconBitmap(image)) continue;
      out[unit] = { ...image, sourceUrl: url };
    } catch {
      // Decorative only.
    }
  }
  return out;
}

export async function prepareHyprOverlayPriceIcons(
  urls: PriceIconUrls,
  dependencies?: PriceIconDependencies,
): Promise<Partial<Record<keyof PriceIconUrls, string>>> {
  const deps = dependencies ?? (await defaultDependencies());
  const available: Partial<Record<keyof PriceIconUrls, string>> = {};
  for (const unit of ["div", "ex"] as const) {
    const url = urls[unit];
    if (!url || !isAllowedPriceIconUrl(url)) continue;
    const id = PRICE_ICON_IDS[unit];
    if (registeredUrls.get(id) === url) {
      available[unit] = id;
      continue;
    }
    try {
      const encoded = await deps.fetchBytes(url);
      const image = await deps.decodeBgra(encoded, PRICE_ICON_SIZE);
      if (!isValidIconBitmap(image)) continue;
      const ok = await deps.register({
        id,
        width: image.width,
        height: image.height,
        rgbaBase64: bgraToRgba(image.data).toString("base64"),
      });
      if (!ok) continue;
      registeredUrls.set(id, url);
      available[unit] = id;
    } catch {
      // Icons are decorative; positioned text remains usable without them.
    }
  }
  return available;
}

/**
 * Electron full-mode markers: same allowlisted sources as hypr, returned as data URLs.
 */
export async function preparePriceIconDataUrls(
  urls: PriceIconUrls,
  dependencies?: PriceIconDataUrlDependencies,
): Promise<Partial<Record<keyof PriceIconUrls, string>>> {
  const load = dependencies
    ? { fetchBytes: dependencies.fetchBytes, decodeBgra: dependencies.decodeBgra }
    : await defaultLoadDependencies();
  const available: Partial<Record<keyof PriceIconUrls, string>> = {};
  for (const unit of ["div", "ex"] as const) {
    const url = urls[unit];
    if (!url || !isAllowedPriceIconUrl(url)) continue;
    const cached = dataUrlCache.get(url);
    if (cached) {
      available[unit] = cached;
      continue;
    }
    try {
      let dataUrl: string | null = null;
      if (dependencies?.toDataUrl) {
        const bytes = await load.fetchBytes(url);
        dataUrl = await dependencies.toDataUrl(bytes, PRICE_ICON_SIZE);
      } else {
        const { nativeImage } = await import("electron");
        const bytes = await load.fetchBytes(url);
        const image = nativeImage.createFromBuffer(bytes);
        if (image.isEmpty()) continue;
        const resized = image.resize({
          width: PRICE_ICON_SIZE,
          height: PRICE_ICON_SIZE,
          quality: "best",
        });
        dataUrl = resized.toDataURL();
      }
      if (!dataUrl || !dataUrl.startsWith("data:image/")) continue;
      dataUrlCache.set(url, dataUrl);
      available[unit] = dataUrl;
    } catch {
      // Decorative only.
    }
  }
  return available;
}

export function resetHyprOverlayPriceIconCacheForTests(): void {
  registeredUrls.clear();
  dataUrlCache.clear();
}
