#!/usr/bin/env bash
# Build Aster.app (release) with its icon: target/release/Aster.app. Drag it to /Applications.
# The app isn't signed or notarized; on first launch, right-click it and choose Open.
#
# Usage: scripts/bundle-macos.sh [--no-build]
set -euo pipefail

APP_ID=io.github.systemicdream.Aster
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)"

if [[ "${1:-}" != "--no-build" ]]; then
    cargo build --release --manifest-path "$ROOT/Cargo.toml" -p aster-app
fi

APP="$ROOT/target/release/Aster.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$ROOT/target/release/aster" "$APP/Contents/MacOS/aster"
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
