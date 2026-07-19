import { describe, expect, test } from "bun:test";
import {
  type CapabilityEnv,
  classifySession,
  detectCapabilities,
} from "../src/capture/capabilities";

const env = (over: Partial<CapabilityEnv>): CapabilityEnv => ({
  platform: "linux",
  ...over,
});

describe("classifySession", () => {
  test("win32 is win32 regardless of XDG vars", () => {
    expect(
      classifySession(env({ platform: "win32", XDG_SESSION_TYPE: "wayland" })),
    ).toBe("win32");
  });

  test("linux X11 session", () => {
    expect(classifySession(env({ XDG_SESSION_TYPE: "x11" }))).toBe("linux-x11");
  });

  test("XWayland reports x11 and is treated as x11", () => {
    expect(
      classifySession(
        env({ XDG_SESSION_TYPE: "x11", WAYLAND_DISPLAY: "wayland-0" }),
      ),
    ).toBe("linux-x11");
  });

  test("Hyprland (instance signature) is wlroots", () => {
    expect(
      classifySession(
        env({
          XDG_SESSION_TYPE: "wayland",
          HYPRLAND_INSTANCE_SIGNATURE: "abc123",
        }),
      ),
    ).toBe("linux-wayland-wlroots");
  });

  test("wlroots in XDG_CURRENT_DESKTOP is wlroots", () => {
    expect(
      classifySession(
        env({ XDG_SESSION_TYPE: "wayland", XDG_CURRENT_DESKTOP: "wlroots" }),
      ),
    ).toBe("linux-wayland-wlroots");
  });

  test("sway/river are treated as wlroots", () => {
    expect(
      classifySession(
        env({ XDG_SESSION_TYPE: "wayland", XDG_CURRENT_DESKTOP: "sway" }),
      ),
    ).toBe("linux-wayland-wlroots");
    expect(
      classifySession(
        env({ XDG_SESSION_TYPE: "wayland", XDG_CURRENT_DESKTOP: "river" }),
      ),
    ).toBe("linux-wayland-wlroots");
  });

  test("GNOME Wayland is wayland-other", () => {
    expect(
      classifySession(
        env({ XDG_SESSION_TYPE: "wayland", XDG_CURRENT_DESKTOP: "GNOME" }),
      ),
    ).toBe("linux-wayland-other");
  });

  test("KDE Wayland is wayland-other", () => {
    expect(
      classifySession(
        env({ XDG_SESSION_TYPE: "wayland", XDG_CURRENT_DESKTOP: "KDE" }),
      ),
    ).toBe("linux-wayland-other");
  });

  test("Wayland inferred from WAYLAND_DISPLAY without session type", () => {
    expect(
      classifySession(
        env({ WAYLAND_DISPLAY: "wayland-0", XDG_CURRENT_DESKTOP: "GNOME" }),
      ),
    ).toBe("linux-wayland-other");
  });

  test("no hints at all falls back to x11-like", () => {
    expect(classifySession(env({}))).toBe("linux-x11");
  });
});

describe("detectCapabilities", () => {
  test("win32 → full + silent capture", async () => {
    const caps = await detectCapabilities({ env: env({ platform: "win32" }) });
    expect(caps).toEqual({
      sessionKind: "win32",
      overlayMode: "full",
      silentRegionCapture: true,
      regionPicker: "electron",
      captureBackend: "electron",
    });
  });

  test("linux-x11 → full + silent capture", async () => {
    const caps = await detectCapabilities({
      env: env({ XDG_SESSION_TYPE: "x11" }),
    });
    expect(caps.overlayMode).toBe("full");
    expect(caps.silentRegionCapture).toBe(true);
  });

  test("wlroots without hypr-overlay → degraded, grim capture, NO Electron overlay probe", async () => {
    let probed = false;
    const caps = await detectCapabilities({
      env: env({
        XDG_SESSION_TYPE: "wayland",
        HYPRLAND_INSTANCE_SIGNATURE: "x",
      }),
      probeOverlay: () => {
        probed = true;
        return true;
      },
    });
    expect(caps.overlayMode).toBe("degraded");
    expect(caps.silentRegionCapture).toBe(true);
    expect(caps.captureBackend).toBe("grim");
    expect(probed).toBe(false);
  });

  test("Hyprland with hypr-overlay loaded → compositor plugin overlay", async () => {
    const caps = await detectCapabilities({
      env: env({
        XDG_SESSION_TYPE: "wayland",
        HYPRLAND_INSTANCE_SIGNATURE: "x",
      }),
      probeHyprOverlay: () => true,
    });
    expect(caps.overlayMode).toBe("hyprland-plugin");
    expect(caps.silentRegionCapture).toBe(true);
    expect(caps.regionPicker).toBe("slurp");
    expect(caps.captureBackend).toBe("grim");
  });

  test("XWayland Electron on a Hyprland host keeps silent capture and uses the plugin", async () => {
    const caps = await detectCapabilities({
      env: env({
        XDG_SESSION_TYPE: "x11",
        XDG_CURRENT_DESKTOP: "Hyprland",
        HYPRLAND_INSTANCE_SIGNATURE: "instance",
      }),
      probeHyprOverlay: () => true,
    });
    expect(caps).toEqual({
      sessionKind: "linux-x11",
      overlayMode: "hyprland-plugin",
      silentRegionCapture: true,
      regionPicker: "slurp",
      captureBackend: "grim",
    });
  });

  test("wayland-other with passing probe → full overlay, portal capture", async () => {
    const caps = await detectCapabilities({
      env: env({ XDG_SESSION_TYPE: "wayland", XDG_CURRENT_DESKTOP: "GNOME" }),
      probeOverlay: () => true,
    });
    expect(caps.overlayMode).toBe("full");
    expect(caps.silentRegionCapture).toBe(false);
  });

  test("wayland-other with failing probe → degraded", async () => {
    const caps = await detectCapabilities({
      env: env({ XDG_SESSION_TYPE: "wayland", XDG_CURRENT_DESKTOP: "KDE" }),
      probeOverlay: () => false,
    });
    expect(caps.overlayMode).toBe("degraded");
  });

  test("wayland-other with no probe supplied → degraded", async () => {
    const caps = await detectCapabilities({
      env: env({ XDG_SESSION_TYPE: "wayland", XDG_CURRENT_DESKTOP: "GNOME" }),
    });
    expect(caps.overlayMode).toBe("degraded");
  });

  test("wayland-other with throwing probe → degraded (probe error swallowed)", async () => {
    const caps = await detectCapabilities({
      env: env({ XDG_SESSION_TYPE: "wayland", XDG_CURRENT_DESKTOP: "GNOME" }),
      probeOverlay: () => {
        throw new Error("compositor refused");
      },
    });
    expect(caps.overlayMode).toBe("degraded");
  });

  test("wayland-other with async-rejecting probe → degraded", async () => {
    const caps = await detectCapabilities({
      env: env({ XDG_SESSION_TYPE: "wayland", XDG_CURRENT_DESKTOP: "GNOME" }),
      probeOverlay: () => Promise.reject(new Error("nope")),
    });
    expect(caps.overlayMode).toBe("degraded");
  });
});
