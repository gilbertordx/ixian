#!/usr/bin/env bash
# Puts a "Launchpad" icon on your Desktop that opens the menu in a terminal window.
set -euo pipefail
BIN="$(command -v launchpad || true)"
[ -n "$BIN" ] || { echo "launchpad is not installed yet: run  cargo install --path .  first"; exit 1; }
DESK="$(xdg-user-dir DESKTOP 2>/dev/null || echo "$HOME/Desktop")"
mkdir -p "$DESK"
if [ "$(uname)" = "Darwin" ]; then
  F="$DESK/Launchpad.command"
  printf '#!/bin/bash\nexec "%s"\n' "$BIN" > "$F"; chmod +x "$F"
else
  F="$DESK/Launchpad.desktop"
  printf '[Desktop Entry]\nType=Application\nName=Launchpad\nComment=Open and test my projects\nExec=%s\nTerminal=true\nIcon=utilities-terminal\n' "$BIN" > "$F"
  chmod +x "$F"; command -v gio >/dev/null && gio set "$F" metadata::trusted true 2>/dev/null || true
fi
echo "Created $F"
