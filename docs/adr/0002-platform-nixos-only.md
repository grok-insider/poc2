# ADR-0002 — Platform: NixOS + Hyprland only (v1)

- Status: Accepted
- Date: 2026-04-26

## Context

The author runs NixOS with Hyprland (Wayland compositor). PoE2 runs under Proton/Wine on Linux. Targeting only this platform for v1 lets us:

- Ship a single declarative `flake.nix` package.
- Use Wayland-native primitives (`wlr-layer-shell` for overlay, `wl-clipboard` for clipboard).
- Skip cross-platform abstraction layers in v1.

## Tradeoffs

### What we lose

- ~99% of PoE2 players (Windows, macOS users).
- The "free marketing" of being cross-platform.
- Several tools (e.g. anti-cheat-aware overlays) that work on Windows but not Linux.

### What we gain

- A simpler dev environment — one OS, one compositor, one display server.
- The ability to ship a flake and skip installer / signing / notarization complexity.
- Wayland-first layer-shell overlay is technically cleaner than the global-hotkey + always-on-top hacks needed on X11/Win32.
- Scope reduction — v1 is a months-shorter project than a cross-platform v1.

## Architectural implications

- **No `cfg(target_os)` branches in v1**. We assume Linux + Wayland.
- **Hyprland window rules** are documented as part of installation (the user adds `windowrulev2 = float, class:^(poc2-desktop)$` to `~/.config/hypr/hyprland.conf`).
- **Overlay** uses `gtk4-layer-shell` via Tauri's runtime, OR a separate Rust process using `smithay-client-toolkit` directly. To be decided in M7.
- **PoE2 detection** uses `Client.txt` watching. Path is the user's Steam Wine prefix; configurable via settings.
- **CI** runs on Ubuntu (closest analogue to NixOS for GitHub Actions). The flake itself is checked via `nix flake check`.

## Decision

v1 ships only for NixOS + Hyprland. Cross-platform support is deferred to v2+.

We commit to **not adding** code that hardcodes Linux assumptions in ways that prevent future cross-platform extension. Examples:

- File-system paths use `directories` crate / `xdg` conventions, not hardcoded `~/.config`.
- HTTP / IPC code is OS-agnostic.
- Clipboard / overlay / `Client.txt` watching are isolated behind traits with `linux_wayland::*` impls.

## Consequences

- The `flake.nix` is the canonical install path. README points users at `nix run .#poc2-desktop` (post-M8).
- Bug reports from non-NixOS users are politely closed with a "v1 NixOS only" template.
- The `bundle.targets` in `tauri.conf.json` is `["deb", "appimage"]` for users on other Linux distros willing to try, but unsupported.
