#!/usr/bin/env bash
# package-linux.sh <AppName> <display-name> <bin-name> <bundle-id>
# Example: bash scripts/package-linux.sh pigment Pigment pigment-gpui com.prism-suite.pigment
set -euo pipefail

APP_NAME="${1:?Usage: package-linux.sh <app_name> <DisplayName> <bin> <bundle-id>}"
DISPLAY_NAME="${2:?}"
BIN="${3:?}"
BUNDLE_ID="${4:?}"
VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')
BRANDING="assets/branding"
ARCH=$(uname -m | sed 's/x86_64/amd64/;s/aarch64/arm64/')

echo "Building $DISPLAY_NAME $VERSION (release)..."
cargo build --release -p "$BIN"

DEB_ROOT="dist/deb-staging"
rm -rf "$DEB_ROOT" && mkdir -p \
    "$DEB_ROOT/DEBIAN" \
    "$DEB_ROOT/usr/bin" \
    "$DEB_ROOT/usr/share/applications" \
    "$DEB_ROOT/usr/share/icons/hicolor/256x256/apps" \
    "$DEB_ROOT/usr/share/icons/hicolor/512x512/apps" \
    "$DEB_ROOT/usr/share/doc/$APP_NAME"

cp "target/release/$BIN" "$DEB_ROOT/usr/bin/$APP_NAME"

for size in 256 512; do
  SRC="$BRANDING/png/${APP_NAME}-${size}.png"
  [ -f "$SRC" ] || SRC="$BRANDING/${APP_NAME}-master.png"
  if command -v convert &>/dev/null; then
    convert "$SRC" -resize "${size}x${size}" \
        "$DEB_ROOT/usr/share/icons/hicolor/${size}x${size}/apps/${APP_NAME}.png"
  else
    cp "$SRC" "$DEB_ROOT/usr/share/icons/hicolor/${size}x${size}/apps/${APP_NAME}.png"
  fi
done

cat > "$DEB_ROOT/usr/share/applications/${APP_NAME}.desktop" <<DESKTOP
[Desktop Entry]
Name=$DISPLAY_NAME
Comment=Part of the Prism creative suite
Exec=$APP_NAME
Icon=$APP_NAME
Type=Application
Categories=Graphics;
StartupNotify=true
DESKTOP

cat > "$DEB_ROOT/DEBIAN/control" <<CTRL
Package: $APP_NAME
Version: $VERSION
Architecture: $ARCH
Maintainer: Prism Suite <stanleykwaminaotabil@gmail.com>
Description: $DISPLAY_NAME — part of the Prism creative suite
Homepage: https://github.com/KwaminaWhyte/prism-suite
Section: graphics
Priority: optional
CTRL

cat > "$DEB_ROOT/DEBIAN/postinst" <<'POST'
#!/bin/sh
update-desktop-database -q /usr/share/applications || true
gtk-update-icon-cache -q /usr/share/icons/hicolor || true
POST
chmod 755 "$DEB_ROOT/DEBIAN/postinst"

cp README.md "$DEB_ROOT/usr/share/doc/$APP_NAME/README.md" 2>/dev/null || true

DEB="dist/${APP_NAME}_${VERSION}_${ARCH}.deb"
dpkg-deb --build "$DEB_ROOT" "$DEB"

TAR="dist/${APP_NAME}-${VERSION}-linux-${ARCH}.tar.gz"
tar -czf "$TAR" -C "target/release" "$BIN"

echo ""
echo "Done:"
echo "  .deb : $DEB"
echo "  .tar : $TAR"
