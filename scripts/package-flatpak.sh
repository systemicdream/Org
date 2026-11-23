#!/usr/bin/env bash
# Build Aster's Flatpak and export it as a single-file bundle:
#   target/dist/Aster-<version>-<arch>.flatpak
# Install it with: flatpak install --user Aster-<version>-<arch>.flatpak
#
# Needs flatpak and flatpak-builder; the runtime and SDK come from Flathub (added as a user
# remote if missing).
set -euo pipefail

APP_ID=io.github.systemicdream.Aster
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)"
ARCH="$(flatpak --default-arch)"
WORK="$ROOT/target/flatpak"
DIST="$ROOT/target/dist"

flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
flatpak-builder --user --install-deps-from=flathub --force-clean --disable-rofiles-fuse \
    --state-dir="$WORK/state" --repo="$WORK/repo" \
    "$WORK/build" "$ROOT/packaging/flatpak/$APP_ID.yml"

mkdir -p "$DIST"
# --runtime-repo: installing the bundle fetches the runtime from Flathub if needed.
flatpak build-bundle --runtime-repo=https://dl.flathub.org/repo/flathub.flatpakrepo \
    "$WORK/repo" "$DIST/Aster-$VERSION-$ARCH.flatpak" "$APP_ID"
echo "Built $DIST/Aster-$VERSION-$ARCH.flatpak"
