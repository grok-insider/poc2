import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  // Fully static, client-side app: the Rust engine runs in the browser via
  // WebAssembly, so there is no server runtime. `export` emits a static site
  // that can be served from any host or opened locally.
  output: "export",
  reactStrictMode: true,
  images: { unoptimized: true },
  // The wasm is fetched at runtime from /public/wasm by the engine worker, so
  // no special bundler wasm handling is needed.
};

export default nextConfig;
