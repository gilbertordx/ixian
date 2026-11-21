#!/usr/bin/env bash
# Rebuilds Ixian from this folder and refreshes the Desktop icon so both stay in sync.
set -euo pipefail
cd "$(dirname "$0")/.."
if [ -d .git ]; then git pull --ff-only; echo "Building: $(git log -1 --oneline)"
else echo "WARNING: this folder is not a git clone, so it cannot pull. Clone https://github.com/gilbertordx/ixian instead."; fi
cargo install --path . --locked --force
bash shortcut/create-desktop-shortcut.sh
echo "Updated. Quit Ixian (q) and open it again to use the new version."
