#!/usr/bin/env bash
# Clones every reference repo into example-repos/ with --depth 1.
# Idempotent: skips repos that already exist.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"
TARGET_DIR="$ROOT_DIR/example-repos"

mkdir -p "$TARGET_DIR"

declare -A REPOS=(
  ["repoe-fork"]="https://github.com/repoe-fork/repoe.git"
  ["poe-tool-dev-dat-schema"]="https://github.com/poe-tool-dev/dat-schema.git"
  ["pyoe2-craftpath"]="https://github.com/WladHD/pyoe2-craftpath.git"
  ["POE2_HTC"]="https://github.com/Dboire9/POE2_HTC.git"
  ["XileHUD-poe_overlay"]="https://github.com/XileHUD/poe_overlay.git"
  ["Exiled-Exchange-2"]="https://github.com/Kvan7/Exiled-Exchange-2.git"
  ["awakened-poe-trade"]="https://github.com/SnosMe/awakened-poe-trade.git"
  ["ggpk-explorer"]="https://github.com/juddisjudd/ggpk-explorer.git"
  ["poe-dat-viewer"]="https://github.com/SnosMe/poe-dat-viewer.git"
  ["LocalIdentity-poe2-data"]="https://github.com/LocalIdentity/poe2-data.git"
)

cd "$TARGET_DIR"

for name in "${!REPOS[@]}"; do
  url="${REPOS[$name]}"
  if [[ -d "$name/.git" ]]; then
    echo "[skip] $name already cloned"
  else
    echo "[clone] $name <- $url"
    git clone --depth 1 "$url" "$name"
  fi
done

echo
echo "Done. Total size:"
du -sh "$TARGET_DIR" 2>/dev/null || true
