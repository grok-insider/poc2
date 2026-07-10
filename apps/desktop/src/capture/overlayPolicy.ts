import type { OverlayMode } from "./capabilities";

export type OverlayCommandAction =
  | "reward-scan"
  | "reward-watch-start"
  | "reward-watch-stop"
  | "price-check"
  | "regex-open"
  | "regex-next"
  | "regex-prev"
  | "regex-tab-next"
  | "regex-tab-prev"
  | "regex-toggle"
  | "regex-copy"
  | "regex-apply";

/** Reward scans capture first; every other full-mode command can render now. */
export function showElectronBeforeCommand(
  mode: OverlayMode | undefined,
  action: OverlayCommandAction,
): boolean {
  return mode === "full" && !action.startsWith("reward-");
}

export function isOverlayRendererReady(url: string, loadingMainFrame: boolean): boolean {
  return !loadingMainFrame && /\/overlay\/index\.html(?:$|[?#])/.test(url);
}
