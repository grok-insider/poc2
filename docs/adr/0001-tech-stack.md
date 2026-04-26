# ADR-0001 — Tech stack: Tauri 2 + Rust + Svelte 5

- Status: Accepted
- Date: 2026-04-26

## Context

The desktop app needs:

- A native-feeling UI on Linux (Hyprland / Wayland).
- A high-performance Rust core (advisor's beam search runs tens of thousands of engine `apply()` calls per re-plan).
- The ability to depend on `pyoe2-craftpath` (MIT, Rust) as a library.
- Clipboard parsing, file watching (`Client.txt`), HTTP for trade APIs.
- Modest binary size — distributed via Nix flake; we want a small closure.

## Options considered

| Stack | Pros | Cons |
|---|---|---|
| **Tauri 2 + Rust + web frontend** | Small binary (~10–20 MB), Rust core for free, modern web tooling, layer-shell overlay possible | WebKitGTK rendering quirks on Linux, IPC layer adds friction |
| Electron + TypeScript | Familiar, huge ecosystem | 100+ MB binaries, slower, RAM-heavy, no Rust integration |
| .NET MAUI / WPF | Strong on Windows | Weak on Linux/Wayland, no Rust integration |
| Native (GTK4 + Rust direct) | Smallest, most native feel | UI complexity higher, no web tooling for forms/charts |

## Frontend framework: Svelte 5 (vs React)

| | Svelte 5 | React |
|---|---|---|
| Bundle | ~10 KB runtime | ~45 KB runtime |
| Reactivity | Runes (compiler-driven) | Hooks (runtime) |
| TS support | Native | Native |
| Devs available | Smaller pool | Larger pool |
| Fit for live data | Excellent (fine-grained) | Good |

Chose **Svelte 5** for:

- Smaller bundle → faster cold start in WebKit on Linux.
- Compiler-driven reactivity matches the advisor's "stream-of-recommendations" pattern naturally.
- Less boilerplate for the kind of forms / panels we'll build.

## Decision

- **Tauri 2** (latest, v2.10+ as of writing) for the desktop shell.
- **Rust 2021 edition**, MSRV 1.82 (we use `Option::is_none_or`).
- **Svelte 5** with runes mode by default. **Vite** for bundling. No SvelteKit (we don't need SSR/routing yet).
- **TypeScript** strict mode in the frontend.
- **pnpm** as the JS package manager.

## Consequences

### Positive
- Rust workspace can directly link `pyoe2-craftpath` once we evaluate it (M3+).
- Clipboard, file watching, HTTP, and OS integration all run in Rust — no JS-side polyfills.
- Tauri's IPC is type-safe via `tauri::generate_handler!` macros.
- Small binary, snappy startup.

### Negative
- WebKitGTK 4.1 has known rendering quirks under Hyprland (HW accel issues on some GPUs). Mitigated by `WEBKIT_DISABLE_DMABUF_RENDERER=1` env if needed.
- Svelte 5 is newer than React 19 — fewer 3rd-party Svelte components for advanced UI patterns. We accept this; the app's UI is mostly custom.
- Tauri 2 capability system is more verbose than v1. Each plugin requires explicit permissions in `capabilities/default.json`.

### Neutral
- We could swap the frontend framework later — Tauri is framework-agnostic. Costs would be limited to the `apps/desktop/src/` directory.
