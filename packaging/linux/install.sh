#!/usr/bin/env bash
#
# Installs HeadroomLab for the current user. Run from inside an unpacked
# release archive:
#
#   ./install.sh
#
# Everything goes under ~/.local, so no root is needed and the updater — which
# replaces the binary in place — keeps working afterwards.

set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN_DIR="${XDG_BIN_HOME:-$HOME/.local/bin}"
DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}"

echo "==> Installing the binary to $BIN_DIR"
mkdir -p "$BIN_DIR"
install -m 755 "$HERE/HeadroomLab" "$BIN_DIR/HeadroomLab"

echo "==> Installing icons to $DATA_DIR/icons/hicolor"
for dir in "$HERE"/hicolor/*/; do
  size="$(basename "$dir")"
  target="$DATA_DIR/icons/hicolor/$size/apps"
  mkdir -p "$target"
  install -m 644 "$dir/apps/headroomlab.png" "$target/headroomlab.png"
done

echo "==> Installing the desktop entry to $DATA_DIR/applications"
mkdir -p "$DATA_DIR/applications"
install -m 644 "$HERE/headroomlab.desktop" "$DATA_DIR/applications/headroomlab.desktop"

# Without these the launcher can keep showing a stale (or generic) icon until
# the next login, which reads as the icon simply not working.
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
  gtk-update-icon-cache -q -t -f "$DATA_DIR/icons/hicolor" 2>/dev/null || true
fi
if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database -q "$DATA_DIR/applications" 2>/dev/null || true
fi

echo
echo "Installed. Launch it from your application menu, or run: HeadroomLab"
case ":$PATH:" in
  *":$BIN_DIR:"*) ;;
  *) echo "Note: $BIN_DIR is not on your PATH — add it to use the 'HeadroomLab' command." ;;
esac
