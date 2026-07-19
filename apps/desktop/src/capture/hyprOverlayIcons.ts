import { registerHyprOverlayImage } from "./hyprOverlay";

export const PRICE_ICON_IDS = {
  div: "poc2.currency.div",
  ex: "poc2.currency.ex",
} as const;

const ICON_SIZE = 34;
const registeredUrls = new Map<string, string>();

export interface PriceIconUrls {
  div?: string;
  ex?: string;
}

export interface PriceIconDependencies {
  fetchBytes(url: string): Promise<Buffer>;
  decodeBgra(bytes: Buffer, size: number): Promise<{ width: number; height: number; data: Buffer }>;
  register(input: {
    id: string;
    width: number;
    height: number;
    rgbaBase64: string;
  }): Promise<boolean>;
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

async function defaultDependencies(): Promise<PriceIconDependencies> {
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
    register: registerHyprOverlayImage,
  };
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
      const image = await deps.decodeBgra(encoded, ICON_SIZE);
      if (
        image.width < 1 ||
        image.height < 1 ||
        image.width > 64 ||
        image.height > 64 ||
        image.data.length !== image.width * image.height * 4
      ) {
        continue;
      }
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

export function resetHyprOverlayPriceIconCacheForTests(): void {
  registeredUrls.clear();
}
