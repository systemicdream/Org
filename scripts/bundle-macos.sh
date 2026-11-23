#!/usr/bin/env bash
# Build Aster.app (release) with its icon: target/release/Aster.app. Drag it to /Applications.
# The app is ad-hoc signed, not notarized (no Apple Developer account): see the README for
# opening it the first time.
#
# Usage: scripts/bundle-macos.sh [--no-build] [--universal] [--dmg]
#   --universal  one app for Apple silicon and Intel (needs both rustup targets)
#   --dmg        also make target/dist/Aster-<version>-macos-<arch>.dmg
set -euo pipefail

APP_ID=io.github.systemicdream.Aster
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)"
BUILD=1 UNIVERSAL=0 DMG=0
for arg in "$@"; do
    case "$arg" in
        --no-build) BUILD=0 ;;
        --universal) UNIVERSAL=1 ;;
        --dmg) DMG=1 ;;
        *) echo "unknown argument: $arg" >&2; exit 2 ;;
    esac
done

cargo_build() {
    cargo build --release --locked --manifest-path "$ROOT/Cargo.toml" -p aster-app "$@"
}
if [[ $UNIVERSAL -eq 1 ]]; then
    TARGETS=(aarch64-apple-darwin x86_64-apple-darwin)
    if [[ $BUILD -eq 1 ]]; then
        for t in "${TARGETS[@]}"; do cargo_build --target "$t"; done
    fi
    BIN="$ROOT/target/release/aster-universal"
    mkdir -p "$ROOT/target/release"
    lipo -create -output "$BIN" \
        "$ROOT/target/aarch64-apple-darwin/release/aster" \
        "$ROOT/target/x86_64-apple-darwin/release/aster"
    ARCH=universal
else
    [[ $BUILD -eq 1 ]] && cargo_build
    BIN="$ROOT/target/release/aster"
    ARCH="$(uname -m)"
fi

APP="$ROOT/target/release/Aster.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN" "$APP/Contents/MacOS/aster"
cp "$ROOT/packaging/macos/AppIcon.icns" "$APP/Contents/Resources/AppIcon.icns"
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>Aster</string>
    <key>CFBundleDisplayName</key><string>Aster</string>
    <key>CFBundleIdentifier</key><string>$APP_ID</string>
    <key>CFBundleExecutable</key><string>aster</string>
    <key>CFBundleIconFile</key><string>AppIcon</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>$VERSION</string>
    <key>CFBundleVersion</key><string>$VERSION</string>
    <key>LSMinimumSystemVersion</key><string>11.0</string>
    <key>LSApplicationCategoryType</key><string>public.app-category.productivity</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>CFBundleDocumentTypes</key>
    <array>
        <dict>
            <key>CFBundleTypeName</key><string>Org document</string>
            <key>CFBundleTypeRole</key><string>Editor</string>
            <key>CFBundleTypeExtensions</key><array><string>org</string></array>
            <key>LSHandlerRank</key><string>Alternate</string>
        </dict>
    </array>
</dict>
</plist>
PLIST
# Ad-hoc sign so macOS treats the bundle as one app (no developer identity needed).
codesign --force --deep --sign - "$APP" >/dev/null 2>&1 || true
echo "Built $APP"

if [[ $DMG -eq 1 ]]; then
    DIST="$ROOT/target/dist"
    DMG_PATH="$DIST/Aster-$VERSION-macos-$ARCH.dmg"
    STAGE="$(mktemp -d)"
    trap 'rm -rf "$STAGE"' EXIT
    cp -R "$APP" "$STAGE/"
    ln -s /Applications "$STAGE/Applications"
    mkdir -p "$DIST"
    rm -f "$DMG_PATH"
    hdiutil create -quiet -volname "Aster $VERSION" -srcfolder "$STAGE" -fs HFS+ \
        -format UDZO -ov "$DMG_PATH"
    echo "Built $DMG_PATH"
fi
