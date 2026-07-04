// Compositor capability gate for the screen-region capture + price overlay
// (ADR-0013). Decides, per session, whether we can run a genuine
// click-through transparent overlay window ("full") or must fall back to an
// in-app panel ("degraded"), and whether a region screen-grab can be taken
// silently (no portal/permission prompt).
//
// The hard env classification is pure and unit-tested; the GNOME/KDE Wayland
// branch needs a one-time *runtime* probe (does an offscreen transparent,
// always-on-top, click-through BrowserWindow actually behave?). The probe is
// injected so tests never touch Electron — see `detectCapabilities`.

export type SessionKind =
  | "win32"
  | "linux-x11"
  | "linux-wayland-wlroots"
  | "linux-wayland-other";

export type OverlayMode = "full" | "degraded" | "hyprland-plugin";

export interface Capabilities {
  /** Region capture can be taken without a per-grab permission prompt. */
  silentRegionCapture: boolean;
  /** Whether a real click-through overlay window is usable. */
  overlayMode: OverlayMode;
  /** Classified session, for diagnostics + the renderer's fallback copy. */
  sessionKind: SessionKind;
}

/** The subset of process.env we read. Injectable for tests. */
export interface CapabilityEnv {
  platform: NodeJS.Platform;
  XDG_SESSION_TYPE?: string;
  XDG_CURRENT_DESKTOP?: string;
  WAYLAND_DISPLAY?: string;
  HYPRLAND_INSTANCE_SIGNATURE?: string;
}

/**
 * Pure env → session classification. No I/O, no Electron. The Wayland
 * GNOME/KDE case ("linux-wayland-other") is the only one that subsequently
 * needs a runtime probe; every other kind decides without one.
 */
export function classifySession(env: CapabilityEnv): SessionKind {
  if (env.platform === "win32") return "win32";
  if (env.platform !== "linux") {
    // macOS is out of scope (ADR-0010); treat anything non-linux/non-win32 as
    // "other" so it lands in the conservative degraded path by default.
    return "linux-wayland-other";
  }

  const sessionType = (env.XDG_SESSION_TYPE ?? "").toLowerCase();
  // X11 (real or XWayland) gives us silent desktopCapturer + a working
  // override-redirect overlay. XWayland reports XDG_SESSION_TYPE=x11.
  if (sessionType === "x11") return "linux-x11";

  const isWayland =
    sessionType === "wayland" || Boolean(env.WAYLAND_DISPLAY) && sessionType !== "x11";
  if (!isWayland) {
    // No session-type hint and no Wayland display: assume X11-like (the
    // desktopCapturer + override-redirect path), which is the safe legacy case.
    return "linux-x11";
  }

  const desktop = (env.XDG_CURRENT_DESKTOP ?? "").toLowerCase();
  const isWlroots =
    Boolean(env.HYPRLAND_INSTANCE_SIGNATURE) ||
    desktop.includes("hyprland") ||
    desktop.includes("wlroots") ||
    desktop.includes("sway") ||
    desktop.includes("river");
  if (isWlroots) return "linux-wayland-wlroots";

  return "linux-wayland-other";
}

/**
 * Result of the runtime overlay probe. `true` ⇒ the transparent click-through
 * always-on-top window took effect; `false` ⇒ the compositor refused it.
 */
export type OverlayProbe = () => boolean | Promise<boolean>;
export type HyprOverlayProbe = () => boolean | Promise<boolean>;

export interface DetectOptions {
  env?: CapabilityEnv;
  /**
   * Runtime probe for the GNOME/KDE Wayland case. Omitted (or returning a
   * rejected/falsey value) ⇒ degraded. Never invoked for the other session
   * kinds, so unit tests of the pure branches need not supply it.
   */
  probeOverlay?: OverlayProbe;
  /** Runtime probe for the generic hypr-overlay compositor plugin. */
  probeHyprOverlay?: HyprOverlayProbe;
}

function defaultEnv(): CapabilityEnv {
  return {
    platform: process.platform,
    XDG_SESSION_TYPE: process.env.XDG_SESSION_TYPE,
    XDG_CURRENT_DESKTOP: process.env.XDG_CURRENT_DESKTOP,
    WAYLAND_DISPLAY: process.env.WAYLAND_DISPLAY,
    HYPRLAND_INSTANCE_SIGNATURE: process.env.HYPRLAND_INSTANCE_SIGNATURE,
  };
}

/**
 * Decide capabilities for the current (or supplied) session.
 *
 * Decision table (ADR-0013):
 *   - win32                     → full,     silent capture
 *   - linux-x11 (incl XWayland) → full,     silent capture
 *   - linux-wayland-wlroots     → degraded, portal capture   (no probe)
 *   - linux-wayland-other       → probe: full if it passes else degraded;
 *                                 capture goes through the xdg portal either way
 */
export async function detectCapabilities(
  opts: DetectOptions = {},
): Promise<Capabilities> {
  const env = opts.env ?? defaultEnv();
  const sessionKind = classifySession(env);

  switch (sessionKind) {
    case "win32":
    case "linux-x11":
      return {
        sessionKind,
        overlayMode: "full",
        silentRegionCapture: true,
      };
    case "linux-wayland-wlroots":
      // Hyprland/wlroots: prefer the compositor plugin when it is loaded.
      // Without it, layer-shell stays deferred and Electron's transparent
      // click-through window is unreliable, so we degrade conservatively.
      {
        let pluginLoaded = false;
        if (opts.probeHyprOverlay) {
          try {
            pluginLoaded = (await opts.probeHyprOverlay()) === true;
          } catch {
            pluginLoaded = false;
          }
        }
        return {
          sessionKind,
          overlayMode: pluginLoaded ? "hyprland-plugin" : "degraded",
          silentRegionCapture: false,
        };
      }
    case "linux-wayland-other": {
      let probePassed = false;
      if (opts.probeOverlay) {
        try {
          probePassed = (await opts.probeOverlay()) === true;
        } catch {
          probePassed = false;
        }
      }
      return {
        sessionKind,
        overlayMode: probePassed ? "full" : "degraded",
        // Wayland always needs the xdg-desktop-portal for screen capture; the
        // first grant prompts, then the token is reused — not "silent".
        silentRegionCapture: false,
      };
    }
  }
}
