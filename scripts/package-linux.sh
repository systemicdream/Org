#!/usr/bin/env bash
# Package a release build of Aster for Linux, in target/dist:
#   aster_<version>_<arch>.deb          Debian, Ubuntu (needs dpkg-deb)
#   aster-<version>-1.<arch>.rpm        Fedora (needs rpmbuild)
#   aster-<version>-linux-<arch>.tar.gz any distribution (has an install.sh)
#
# Build on the oldest distribution you want to support: the binary needs a glibc at least as
# new as the build machine's. (Releases are built on Ubuntu 22.04: glibc 2.35.)
#
# Usage: scripts/package-linux.sh [--no-build] [deb] [rpm] [tar]   (default: all three)
set -euo pipefail

APP_ID=io.github.systemicdream.Aster
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)"
SUMMARY="Notes, to-dos and plans in plain Org files"
HOMEPAGE="https://github.com/systemicdream/Aster"
DIST="$ROOT/target/dist"
BUILD=1
FORMATS=()
for arg in "$@"; do
    case "$arg" in
        --no-build) BUILD=0 ;;
        deb | rpm | tar) FORMATS+=("$arg") ;;
        *) echo "unknown argument: $arg" >&2; exit 2 ;;
    esac
done
[[ ${#FORMATS[@]} -eq 0 ]] && FORMATS=(deb rpm tar)

if [[ $BUILD -eq 1 ]]; then
    cargo build --release --locked --manifest-path "$ROOT/Cargo.toml" -p aster-app
fi
BIN="$ROOT/target/release/aster"
ARCH="$(uname -m)"
mkdir -p "$DIST"

# Lay out the files under a prefix: <dest>/bin, <dest>/share/...
stage() {
    local dest="$1"
    install -Dm755 "$BIN" "$dest/bin/aster"
    install -Dm644 "$ROOT/packaging/linux/$APP_ID.desktop" "$dest/share/applications/$APP_ID.desktop"
    install -Dm644 "$ROOT/packaging/linux/$APP_ID.metainfo.xml" "$dest/share/metainfo/$APP_ID.metainfo.xml"
    for icon in "$ROOT"/packaging/linux/icons/hicolor/*/apps/"$APP_ID".png; do
        local size
        size="$(basename "$(dirname "$(dirname "$icon")")")"
        install -Dm644 "$icon" "$dest/share/icons/hicolor/$size/apps/$APP_ID.png"
    done
    install -Dm644 "$ROOT/LICENSE" "$dest/share/licenses/aster/LICENSE"
}

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

for format in "${FORMATS[@]}"; do
    case "$format" in
    deb)
        case "$ARCH" in x86_64) DEB_ARCH=amd64 ;; aarch64) DEB_ARCH=arm64 ;; *) DEB_ARCH="$ARCH" ;; esac
        PKG="$TMP/deb"
        stage "$PKG/usr"
        mkdir -p "$PKG/usr/share/doc/aster"
        mv "$PKG/usr/share/licenses/aster/LICENSE" "$PKG/usr/share/doc/aster/copyright"
        rm -rf "$PKG/usr/share/licenses"
        mkdir -p "$PKG/DEBIAN"
        # Linked: libc, libgcc, libxkbcommon. Loaded at runtime: Wayland and Vulkan.
        cat > "$PKG/DEBIAN/control" <<CONTROL
Package: aster
Version: $VERSION
Architecture: $DEB_ARCH
Maintainer: Aster <$HOMEPAGE>
Installed-Size: $(du -sk "$PKG/usr" | cut -f1)
Depends: libc6 (>= 2.35), libgcc-s1, libxkbcommon0, libwayland-client0, libvulkan1
Recommends: mesa-vulkan-drivers
Section: editors
Priority: optional
Homepage: $HOMEPAGE
Description: $SUMMARY
 Aster is a notes app built on Org mode, without needing to learn Emacs.
 Notes are plain .org files in a folder you choose. Requires a Wayland session.
CONTROL
        dpkg-deb --root-owner-group --build "$PKG" "$DIST/aster_${VERSION}_${DEB_ARCH}.deb" >/dev/null
        echo "Built $DIST/aster_${VERSION}_${DEB_ARCH}.deb"
        ;;
    rpm)
        RPMTOP="$TMP/rpm"
        mkdir -p "$RPMTOP"/{BUILD,RPMS,SOURCES,SPECS,SRPMS}
        stage "$RPMTOP/files"
        cat > "$RPMTOP/SPECS/aster.spec" <<SPEC
Name:           aster
Version:        $VERSION
Release:        1
Summary:        $SUMMARY
License:        MIT
URL:            $HOMEPAGE
# Loaded at runtime, so not found by the automatic dependency scan.
Requires:       libwayland-client
Requires:       vulkan-loader
Recommends:     mesa-vulkan-drivers

%description
Aster is a notes app built on Org mode, without needing to learn Emacs. Notes are plain
.org files in a folder you choose. Requires a Wayland session.

%global debug_package %{nil}
%global __strip /bin/true

%install
mkdir -p %{buildroot}/usr
cp -a $RPMTOP/files/. %{buildroot}/usr/

%files
/usr/bin/aster
/usr/share/applications/$APP_ID.desktop
/usr/share/metainfo/$APP_ID.metainfo.xml
/usr/share/icons/hicolor/*/apps/$APP_ID.png
%license /usr/share/licenses/aster/LICENSE
SPEC
        rpmbuild --quiet --define "_topdir $RPMTOP" --define "source_date_epoch_from_changelog 0" -bb "$RPMTOP/SPECS/aster.spec"
        cp "$RPMTOP"/RPMS/*/aster-"$VERSION"-1.*.rpm "$DIST/"
        echo "Built $(ls "$DIST"/aster-"$VERSION"-1.*.rpm)"
        ;;
    tar)
        NAME="aster-$VERSION-linux-$ARCH"
        stage "$TMP/$NAME"
        cat > "$TMP/$NAME/install.sh" <<'INSTALL'
#!/usr/bin/env bash
# Install Aster for the current user (~/.local), or elsewhere: PREFIX=/usr/local sudo ./install.sh
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PREFIX="${PREFIX:-$HOME/.local}"
install -Dm755 "$HERE/bin/aster" "$PREFIX/bin/aster"
(cd "$HERE/share" && find . -type f -exec install -Dm644 {} "$PREFIX/share/{}" \;)
# Point the launcher at the installed binary, whether or not it's on PATH.
sed -i "s|^Exec=aster |Exec=$PREFIX/bin/aster |" "$PREFIX/share/applications/io.github.systemicdream.Aster.desktop"
command -v update-desktop-database >/dev/null && update-desktop-database "$PREFIX/share/applications" || true
command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q -t "$PREFIX/share/icons/hicolor" || true
echo "Installed Aster to $PREFIX/bin/aster"
INSTALL
        chmod +x "$TMP/$NAME/install.sh"
        tar -C "$TMP" -czf "$DIST/$NAME.tar.gz" "$NAME"
        echo "Built $DIST/$NAME.tar.gz"
        ;;
    esac
done
