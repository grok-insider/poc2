{
  description = "Path of Crafting 2 (poc2) — PoE2 crafting advisor for NixOS + Hyprland";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };

        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rust-analyzer" "clippy" "rustfmt" ];
          targets = [ "x86_64-unknown-linux-gnu" ];
        };

        # Tauri 2 system dependencies
        tauriDeps = with pkgs; [
          # Webview
          webkitgtk_4_1
          libsoup_3
          # Core
          openssl
          pkg-config
          gtk3
          # Tray support
          libayatana-appindicator
          # SVG icons
          librsvg
          # File dialogs / GIO
          glib
          glib-networking
          # Cairo / Pango / GDK
          cairo
          pango
          gdk-pixbuf
          atk
          harfbuzz
        ];

        # Wayland / Hyprland overlay support
        waylandDeps = with pkgs; [
          wayland
          wayland-protocols
          wayland-scanner
          gtk4-layer-shell
          gtk-layer-shell
          libxkbcommon
        ];

        # In-game integration
        integrationDeps = with pkgs; [
          wl-clipboard         # clipboard reading on Wayland
          inotify-tools        # Client.txt monitoring
        ];

        nativeBuildInputs = with pkgs; [
          rustToolchain
          nodejs_22
          pnpm
          cargo-tauri
          gcc
          gnumake
          cmake
          pkg-config
          # Linters / formatters
          taplo                # TOML formatter
          # Useful helpers
          jq
          ripgrep
          fd
          bacon                # Rust auto-rebuild
        ];

      in
      {
        devShells.default = pkgs.mkShell {
          inherit nativeBuildInputs;

          buildInputs = tauriDeps ++ waylandDeps ++ integrationDeps;

          # Tauri / WebKit needs WEBKIT_DISABLE_COMPOSITING_MODE=1 on some Linux setups
          # to avoid blank-window issues; toggle if you hit them.
          shellHook = ''
            export PKG_CONFIG_PATH="${pkgs.lib.makeSearchPath "lib/pkgconfig" (tauriDeps ++ waylandDeps)}:$PKG_CONFIG_PATH"
            export LD_LIBRARY_PATH="${pkgs.lib.makeLibraryPath (tauriDeps ++ waylandDeps)}:$LD_LIBRARY_PATH"
            export GIO_MODULE_DIR="${pkgs.glib-networking}/lib/gio/modules/"
            # Avoid Tauri webview rendering bugs under Hyprland with HW accel:
            # export WEBKIT_DISABLE_COMPOSITING_MODE=1
            # export WEBKIT_DISABLE_DMABUF_RENDERER=1

            echo "poc2 dev shell ready"
            echo "  rustc:  $(rustc --version)"
            echo "  cargo:  $(cargo --version)"
            echo "  node:   $(node --version)"
            echo "  pnpm:   $(pnpm --version)"
          '';
        };

        # Placeholder package — wired up after M2 when crates are real
        packages.default = pkgs.hello;
      });
}
