// Windows Ctrl+C injection via uiohook-napi (same library Awakened PoE
// Trade uses). Optional dependency: loaded lazily and only on win32, so
// Linux/NixOS installs never touch the native module.

type UiohookModule = {
  uIOhook: { keyTap(key: number, modifiers?: number[]): void };
  UiohookKey: Record<string, number>;
};

let cached: UiohookModule | null | undefined;

function loadUiohook(): UiohookModule | null {
  if (cached !== undefined) return cached;
  try {
    // eslint-disable-next-line @typescript-eslint/no-require-imports
    cached = require("uiohook-napi") as UiohookModule;
  } catch {
    cached = null;
  }
  return cached;
}

/** Inject Ctrl+C (or Ctrl+Alt+C for advanced mods). Returns tool used. */
export async function injectCopy(advanced: boolean): Promise<string | null> {
  const mod = loadUiohook();
  if (!mod) return null;
  const { uIOhook, UiohookKey } = mod;
  const mods = advanced
    ? [UiohookKey.Ctrl!, UiohookKey.Alt!]
    : [UiohookKey.Ctrl!];
  uIOhook.keyTap(UiohookKey.C!, mods);
  return "uiohook-napi";
}
