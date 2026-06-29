import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  // Fully static, client-side app: the Rust engine runs in the browser via
  // WebAssembly, so there is no server runtime. `export` emits a static site
  // that can be served from any host or opened locally.
  output: "export",
  // Directory-style export (overlay/index.html, calibrate/index.html) so the
  // Electron app:// scheme can load /overlay/index.html and /calibrate/index.html
  // for the ADR-0013 overlay + calibration windows. Keeps asset URLs
  // origin-relative.
  trailingSlash: true,
  reactStrictMode: true,
  images: { unoptimized: true },
  // The wasm is fetched at runtime from /public/wasm by the engine worker, so
  // no special bundler wasm handling is needed.
};

export default nextConfig;
