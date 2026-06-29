{
  description = "Path of Crafting 2 (poc2) — PoE2 crafting advisor (Rust engine + WebAssembly web app)";

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
          # wasm32 target for the Next.js web app, which runs the engine in the
          # browser via WebAssembly (crates/poc2-wasm).
          targets = [ "x86_64-unknown-linux-gnu" "wasm32-unknown-unknown" ];
        };

        nativeBuildInputs = with pkgs; [
          rustToolchain
          # Web app toolchain (Next.js 16 + React 19) — Bun is the package
          # manager + script runner; nodejs kept for tooling compatibility.
          bun
          nodejs_22
          # Desktop shell (apps/desktop, ADR-0010): NixOS dev-runs use this
          # electron; npm's downloaded binary is non-FHS and never runs here.
          electron
          # WebAssembly toolchain (crates/poc2-wasm → apps/web)
          wasm-pack
          wasm-bindgen-cli
          binaryen             # wasm-opt
          # C toolchain for native crates that need it
          gcc
          gnumake
          cmake
          pkg-config
          # Linters / formatters / helpers
          taplo                # TOML formatter
          jq
          ripgrep
          fd
          bacon                # Rust auto-rebuild
        ];

        # OpenSSL is kept available for crates that opt into networking
        # (poc2-market's `net` feature is off by default and not used by the
        # web build, but leaving openssl here keeps `--features net` buildable).
        buildInputs = with pkgs; [ openssl ];

      in
      {
        devShells.default = pkgs.mkShell {
          inherit nativeBuildInputs buildInputs;

          shellHook = ''
            export PKG_CONFIG_PATH="${pkgs.lib.makeSearchPath "lib/pkgconfig" buildInputs}:$PKG_CONFIG_PATH"
            export LD_LIBRARY_PATH="${pkgs.lib.makeLibraryPath buildInputs}:$LD_LIBRARY_PATH"

            echo "poc2 dev shell ready"
            echo "  rustc:  $(rustc --version)"
            echo "  cargo:  $(cargo --version)"
            echo "  bun:    $(bun --version)"
            echo "  node:   $(node --version)"
          '';
        };

        packages.default = pkgs.hello;
      });
}
