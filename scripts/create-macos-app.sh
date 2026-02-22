#!/usr/bin/env bash
set -euo pipefail

# create-macos-app.sh — Create a macOS .app bundle from a .NET publish directory.
#
# Usage: create-macos-app.sh <publish-dir> <output-dir> [version]
#
# Creates "UBV Remux.app" inside <output-dir>.

BUNDLE_ID="works.peter.ubv-remux"
APP_NAME="UBV Remux"

PUBLISH_DIR="${1:?Usage: create-macos-app.sh <publish-dir> <output-dir> [version]}"
OUTPUT_DIR="${2:?Usage: create-macos-app.sh <publish-dir> <output-dir> [version]}"
VERSION="${3:-0.0.0}"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
APP_DIR="$OUTPUT_DIR/${APP_NAME}.app"

echo "Creating ${APP_NAME}.app (v${VERSION})..."

rm -rf "$APP_DIR"
mkdir -p "$APP_DIR/Contents/MacOS"
mkdir -p "$APP_DIR/Contents/Resources"

# Copy the publish directory into MacOS/, excluding debug symbol bundles
rsync -a --exclude '*.dSYM' "$PUBLISH_DIR/" "$APP_DIR/Contents/MacOS/"

# Copy app icon
if [[ -f "$SCRIPT_DIR/../assets/appicon.icns" ]]; then
    cp "$SCRIPT_DIR/../assets/appicon.icns" "$APP_DIR/Contents/Resources/appicon.icns"
fi

# Info.plist
cat > "$APP_DIR/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>
    <string>${APP_NAME}</string>
    <key>CFBundleDisplayName</key>
    <string>${APP_NAME}</string>
    <key>CFBundleIdentifier</key>
    <string>${BUNDLE_ID}</string>
    <key>CFBundleVersion</key>
    <string>${VERSION}</string>
    <key>CFBundleShortVersionString</key>
    <string>${VERSION}</string>
    <key>CFBundleExecutable</key>
    <string>RemuxGui</string>
    <key>CFBundleIconFile</key>
    <string>appicon</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>LSMinimumSystemVersion</key>
    <string>11.0</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>NSSupportsAutomaticGraphicsSwitching</key>
    <true/>
    <key>CFBundleDocumentTypes</key>
    <array>
        <dict>
            <key>CFBundleTypeName</key>
            <string>UBV Video File</string>
            <key>CFBundleTypeExtensions</key>
            <array>
                <string>ubv</string>
            </array>
            <key>CFBundleTypeRole</key>
            <string>Viewer</string>
            <key>LSHandlerRank</key>
            <string>Default</string>
        </dict>
    </array>
</dict>
</plist>
PLIST

echo "Created: $APP_DIR"
