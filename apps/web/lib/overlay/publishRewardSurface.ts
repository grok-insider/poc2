import type {
  CaptureRect,
  DesktopCapabilities,
  Poc2DesktopBridge,
} from "@/lib/desktop";
import type { RewardSurfaceModel } from "@/lib/overlay/rewards";
import { toHyprPayload } from "@/lib/overlay/rewards";

export type OverlayMode = DesktopCapabilities["overlayMode"];

export interface PublishRewardSurfaceResult {
  /** False when the compositor transport rejected the payload. */
  renderOk: boolean;
}

/**
 * Transport port for reward results: one call site after OCR, adapters per mode.
 * Does not touch React state — the page applies the model locally for Electron paint.
 */
export async function publishRewardSurface(
  bridge: Poc2DesktopBridge,
  mode: OverlayMode | null | undefined,
  model: RewardSurfaceModel,
): Promise<PublishRewardSurfaceResult> {
  const active = mode ?? "degraded";

  if (active === "hyprland-plugin") {
    const ok = await bridge.hyprOverlayRender(toHyprPayload(model));
    return { renderOk: ok };
  }

  if (active === "full") {
    if (model.kind === "positioned" || model.kind === "stack") {
      await bridge.overlaySetContentBounds?.(model.strip);
    }
    await bridge.overlayShow();
    return { renderOk: true };
  }

  // degraded: in-app / small electron fallback — no content bounds
  await bridge.overlayShow();
  return { renderOk: true };
}

/** Hide whichever reward surface is active. */
export async function hideRewardSurface(
  bridge: Poc2DesktopBridge,
  mode: OverlayMode | null | undefined,
  captureHint?: CaptureRect | null,
): Promise<void> {
  if (mode === "hyprland-plugin") {
    await bridge.hyprOverlayRender({
      mode: "cards",
      visible: false,
      rect: {
        x: captureHint?.x ?? 0,
        y: captureHint?.y ?? 0,
        w: 1,
        h: 1,
      },
    });
    return;
  }
  await bridge.overlayHide();
}
