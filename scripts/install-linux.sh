#!/usr/bin/env bash
# Build Aster and install it for the current user: the binary in ~/.local/bin, plus the
# desktop entry and icons so it shows up in the app launcher with its icon (Wayland matches
# the window to the desktop entry by its app id, io.github.systemicdream.Aster).
#
# Usage: scripts/install-linux.sh [--no-build]
set -euo pipefail

APP_ID=io.github.systemicdream.Aster
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN_DIR="${XDG_BIN_HOME:-$HOME/.local/bin}"
DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}"

if [[ "${1:-}" != "--no-build" ]]; then
    cargo build --release --manifest-path "$ROOT/Cargo.toml" -p aster-app
fi

install -Dm755 "$ROOT/target/release/aster" "$BIN_DIR/aster"
# Point the launcher at the installed binary, whether or not ~/.local/bin is on PATH.
mkdir -p "$DATA_DIR/applications"
sed "s|^Exec=aster |Exec=$BIN_DIR/aster |" "$ROOT/packaging/linux/$APP_ID.desktop" \
    > "$DATA_DIR/applications/$APP_ID.desktop"

install -Dm644 "$ROOT/packaging/linux/$APP_ID.metainfo.xml" "$DATA_DIR/metainfo/$APP_ID.metainfo.xml"

for icon in "$ROOT"/packaging/linux/icons/hicolor/*/apps/"$APP_ID".png; do
    size_dir="$(basename "$(dirname "$(dirname "$icon")")")"
    install -Dm644 "$icon" "$DATA_DIR/icons/hicolor/$size_dir/apps/$APP_ID.png"
done

command -v update-desktop-database >/dev/null && update-desktop-database "$DATA_DIR/applications" || true
command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q -t "$DATA_DIR/icons/hicolor" || true

echo "Installed Aster to $BIN_DIR/aster"
