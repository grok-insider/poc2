import { execFileSync } from "node:child_process";
import { readdirSync } from "node:fs";

interface HyprInstance {
  instance?: unknown;
  wl_socket?: unknown;
}

export function waylandDisplayFromInstances(
  raw: string,
  signature: string | undefined,
): string | null {
  try {
    const instances = JSON.parse(raw) as HyprInstance[];
    if (!Array.isArray(instances)) return null;
    const exact = signature
      ? instances.find((item) => item.instance === signature)
      : undefined;
    const candidate = exact ?? instances[0];
    return typeof candidate?.wl_socket === "string" && candidate.wl_socket
      ? candidate.wl_socket
      : null;
  } catch {
    return null;
  }
}

/** Recover the compositor socket when an XWayland launcher strips it. */
export function resolveWaylandDisplay(): string | null {
  if (process.env.WAYLAND_DISPLAY) return process.env.WAYLAND_DISPLAY;
  try {
    const output = execFileSync("hyprctl", ["instances", "-j"], {
      encoding: "utf8",
      timeout: 800,
      windowsHide: true,
    });
    const display = waylandDisplayFromInstances(
      output,
      process.env.HYPRLAND_INSTANCE_SIGNATURE,
    );
    if (display) return display;
  } catch {
    // Fall through to the runtime-dir socket scan.
  }
  try {
    const runtime = process.env.XDG_RUNTIME_DIR;
    if (!runtime) return null;
    return (
      readdirSync(runtime)
        .filter((name) => /^wayland-\d+$/.test(name))
        .sort()
        .at(-1) ?? null
    );
  } catch {
    return null;
  }
}

export function withWaylandDisplay(env: NodeJS.ProcessEnv): NodeJS.ProcessEnv {
  const display = resolveWaylandDisplay();
  return display ? { ...env, WAYLAND_DISPLAY: display } : env;
}
