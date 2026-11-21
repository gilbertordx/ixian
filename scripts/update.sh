#!/usr/bin/env bash
# Rebuilds Sietch from this folder and refreshes the Desktop icon so both stay in sync.
set -euo pipefail
cd "$(dirname "$0")/.."
[ -d .git ] && git pull --ff-only
cargo install --path . --locked --force
bash shortcut/create-desktop-shortcut.sh
echo "Updated. Quit Sietch (q) and open it again to use the new version."
