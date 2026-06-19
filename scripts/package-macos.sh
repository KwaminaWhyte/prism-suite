#!/usr/bin/env bash
# package-macos.sh <AppName> <bin-name> <bundle-id>
# Example: bash scripts/package-macos.sh Pigment pigment-gpui com.prism-suite.pigment
set -euo pipefail

APP_NAME="${1:?Usage: package-macos.sh <AppName> <bin> <bundle-id>}"
BIN="${2:?}"
BUNDLE_ID="${3:?}"
APP_LOWER=$(echo "$APP_NAME" | tr '[:upper:]' '[:lower:]')
VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')
BRANDING="assets/branding"

echo "Building $APP_NAME $VERSION (release)..."
cargo build --release -p "$BIN"

BINARY="target/release/$BIN"
APP_DIR="dist/$APP_NAME.app"
CONTENTS="$APP_DIR/Contents"

echo "Generating .icns icon..."
ICONSET="dist/${APP_NAME}.iconset"
rm -rf "$ICONSET" && mkdir -p "$ICONSET"
for size in 16 32 64 128 256 512; do
  sips -z $size $size "$BRANDING/png/${APP_LOWER}-$size.png" \
      --out "$ICONSET/icon_${size}x${size}.png" &>/dev/null || \
  sips -z $size $size "$BRANDING/${APP_LOWER}-master.png" \
      --out "$ICONSET/icon_${size}x${size}.png" &>/dev/null
  double=$((size * 2))
  sips -z $double $double "$BRANDING/png/${APP_LOWER}-$(( size <= 256 ? size * 2 : 512 )).png" \
      --out "$ICONSET/icon_${size}x${size}@2x.png" &>/dev/null || \
  sips -z $double $double "$BRANDING/${APP_LOWER}-master.png" \
      --out "$ICONSET/icon_${size}x${size}@2x.png" &>/dev/null
done
iconutil -c icns "$ICONSET" -o "dist/$APP_NAME.icns" 2>/dev/null || true

echo "Assembling .app bundle..."
rm -rf "$APP_DIR"
mkdir -p "$CONTENTS/MacOS" "$CONTENTS/Resources"

cp "$BINARY" "$CONTENTS/MacOS/$APP_NAME"
[ -f "dist/$APP_NAME.icns" ] && cp "dist/$APP_NAME.icns" "$CONTENTS/Resources/AppIcon.icns"

cat > "$CONTENTS/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>             <string>$APP_NAME</string>
    <key>CFBundleDisplayName</key>      <string>$APP_NAME</string>
    <key>CFBundleIdentifier</key>       <string>$BUNDLE_ID</string>
    <key>CFBundleVersion</key>          <string>$VERSION</string>
    <key>CFBundleShortVersionString</key><string>$VERSION</string>
    <key>CFBundleExecutable</key>       <string>$APP_NAME</string>
    <key>CFBundleIconFile</key>         <string>AppIcon</string>
    <key>CFBundlePackageType</key>      <string>APPL</string>
    <key>CFBundleSignature</key>        <string>????</string>
    <key>LSMinimumSystemVersion</key>   <string>13.0</string>
    <key>NSHighResolutionCapable</key>  <true/>
    <key>NSSupportsAutomaticGraphicsSwitching</key><true/>
    <key>NSPrincipalClass</key>         <string>NSApplication</string>
</dict>
</plist>
PLIST

echo "Creating .dmg..."
DMG="dist/${APP_NAME}-${VERSION}-macos.dmg"
hdiutil create -volname "$APP_NAME" -srcfolder "$APP_DIR" \
    -ov -format UDZO "$DMG" 2>/dev/null

echo ""
echo "Done:"
echo "  App bundle : $APP_DIR"
echo "  Disk image : $DMG"
