#!/usr/bin/env bash
# Puts a "Sietch" icon on your Desktop that opens the menu in a terminal window.
set -euo pipefail
BIN="$(command -v sietch || true)"
[ -n "$BIN" ] || { echo "sietch is not installed yet: run  cargo install --path .  first"; exit 1; }
DESK="$(xdg-user-dir DESKTOP 2>/dev/null || echo "$HOME/Desktop")"
mkdir -p "$DESK"
rm -f "$DESK/Launchpad.desktop" "$DESK/Launchpad.command"   # the old name
if [ "$(uname)" = "Darwin" ]; then
  F="$DESK/Sietch.command"
  printf '#!/bin/bash\nexec "%s"\n' "$BIN" > "$F"; chmod +x "$F"
else
  F="$DESK/Sietch.desktop"
  printf '[Desktop Entry]\nType=Application\nName=Sietch\nComment=Open and test my projects\nExec=%s\nTerminal=true\nIcon=utilities-terminal\n' "$BIN" > "$F"
  chmod +x "$F"; command -v gio >/dev/null && gio set "$F" metadata::trusted true 2>/dev/null || true
fi
echo "Created $F"
