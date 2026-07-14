#!/usr/bin/env bash
set -euo pipefail

BIN="ctrl-vitals"
DESKTOP="scripts/ctrl-vitals.desktop"

echo "==> Installing $BIN..."

if [ "${1:-}" = "--system" ]; then
    echo "    System-wide install (requires sudo)"
    sudo install -m 755 "target/release/$BIN" /usr/local/bin/
    sudo install -m 644 "$DESKTOP" /usr/local/share/applications/
    echo "    Done. Launch from application menu or run: $BIN"
else
    echo "    User install to ~/.local"
    mkdir -p "$HOME/.local/bin" "$HOME/.local/share/applications"
    install -m 755 "target/release/$BIN" "$HOME/.local/bin/"
    install -m 644 "$DESKTOP" "$HOME/.local/share/applications/"
    echo "    Done. Make sure ~/.local/bin is in your PATH."
    echo "    Launch from application menu or run: $BIN"
fi
