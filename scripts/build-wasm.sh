#!/usr/bin/env bash
# Back-compat wrapper — the build lives in scripts/build-wasm.mjs (cross-platform).
exec bun "$(dirname "$0")/build-wasm.mjs" "$@"
