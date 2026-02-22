#!/usr/bin/env bash
set -euo pipefail

# sign-and-notarize.sh — Sign and notarize macOS release archives locally.
#
# Builds a proper .app bundle from the C# Avalonia GUI (RemuxGui/),
# signs all binaries, notarizes with Apple, and staples the ticket
# to the .app bundle.
#
# Expects to find one or both of:
#   unifi-protect-remux-macos-aarch64.tar.gz
#   unifi-protect-remux-macos-x86_64.tar.gz

# --- Configuration ---

KEYCHAIN_SERVICE="unifi-protect-remux-notarize"
BUNDLE_ID="works.peter.ubv-remux"
APP_NAME="UBV Remux"
ARCHIVES=(
    "unifi-protect-remux-macos-x86_64.tar.gz"
    "unifi-protect-remux-macos-aarch64.tar.gz"
)
CLI_BINARIES=(remux ubv-info ubv-anonymise)

SEARCH_DIR="${1:-.}"

# --- Retrieve credentials from keychain ---

get_keychain_value() {
    security find-generic-password -s "$KEYCHAIN_SERVICE" -a "$1" -w 2>/dev/null \
        || { echo "ERROR: Could not find keychain entry for account '$1' under service '$KEYCHAIN_SERVICE'" >&2; exit 1; }
}

echo "Retrieving credentials from keychain..."
APPLE_ID="$(get_keychain_value APPLE_ID)"
APPLE_APP_SPECIFIC_PASSWORD="$(get_keychain_value APPLE_APP_SPECIFIC_PASSWORD)"
APPLE_TEAM_ID="$(get_keychain_value APPLE_TEAM_ID)"
echo "  Apple ID: $APPLE_ID"
echo "  Team ID:  $APPLE_TEAM_ID"

# --- Find signing identity ---

IDENTITY=$(security find-identity -v -p codesigning | grep "Developer ID Application" | head -1 | sed 's/.*"\(.*\)".*/\1/')
if [[ -z "$IDENTITY" ]]; then
    echo "ERROR: No 'Developer ID Application' certificate found in keychain." >&2
    echo "Install your Developer ID certificate in Keychain Access first." >&2
    exit 1
fi
echo "  Signing identity: $IDENTITY"

# --- Find archives ---

found_any=false
for archive in "${ARCHIVES[@]}"; do
    if [[ -f "$SEARCH_DIR/$archive" ]]; then
        found_any=true
    fi
done

if [[ "$found_any" == false ]]; then
    echo "ERROR: No matching archives found in $SEARCH_DIR" >&2
    echo "Expected: ${ARCHIVES[*]}" >&2
    exit 1
fi

# --- Detect version from remux CLI binary ---

detect_version() {
    local bin="$1"
    # Try to extract version string from the binary; fall back to "0.0.0"
    local ver
    ver=$(strings "$bin" 2>/dev/null | grep -oE '^[0-9]+\.[0-9]+\.[0-9]+$' | head -1) || true
    if [[ -z "$ver" ]]; then
        ver="0.0.0"
    fi
    echo "$ver"
}

# --- Locate shared .app bundle creation script ---

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CREATE_APP="$SCRIPT_DIR/create-macos-app.sh"
if [[ ! -x "$CREATE_APP" ]]; then
    echo "ERROR: $CREATE_APP not found or not executable" >&2
    exit 1
fi

# --- Create entitlements file ---

WORKDIR="$(mktemp -d)"
trap 'rm -rf "$WORKDIR"' EXIT

ENTITLEMENTS="$WORKDIR/entitlements.plist"
cat > "$ENTITLEMENTS" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>com.apple.security.cs.allow-jit</key>
    <true/>
    <key>com.apple.security.cs.allow-unsigned-executable-memory</key>
    <true/>
</dict>
</plist>
PLIST

# --- Process each archive ---

# Track .app bundle paths for stapling after notarization
declare -a APP_BUNDLE_PATHS=()

for archive in "${ARCHIVES[@]}"; do
    archive_path="$SEARCH_DIR/$archive"
    if [[ ! -f "$archive_path" ]]; then
        echo "Skipping $archive (not found)"
        continue
    fi

    echo ""
    echo "=== Processing $archive ==="

    slug="${archive%.tar.gz}"
    unpack_dir="$WORKDIR/$slug"
    output_dir="$WORKDIR/${slug}-output"
    mkdir -p "$unpack_dir" "$output_dir"

    echo "Unpacking..."
    tar xzf "$archive_path" -C "$unpack_dir"

    # Detect version from the remux CLI binary
    VERSION="$(detect_version "$unpack_dir/remux")"
    echo "  Detected version: $VERSION"

    # Create .app bundle from the RemuxGui publish directory
    "$CREATE_APP" "$unpack_dir/RemuxGui" "$output_dir" "$VERSION"
    APP_DIR="$output_dir/${APP_NAME}.app"
    APP_BUNDLE_PATHS+=("$APP_DIR")

    # Sign all native binaries inside the .app bundle (dylibs first, then main executable)
    echo "  Signing native libraries in ${APP_NAME}.app..."
    find "$APP_DIR/Contents/MacOS" -name '*.dylib' -exec \
        codesign --force --options runtime --sign "$IDENTITY" --timestamp \
        --entitlements "$ENTITLEMENTS" {} \;

    echo "  Signing ${APP_NAME}.app..."
    codesign --force --options runtime --sign "$IDENTITY" --timestamp \
        --entitlements "$ENTITLEMENTS" \
        "$APP_DIR"
    codesign --verify --verbose "$APP_DIR"

    # Sign CLI binaries and copy to output
    for cli_bin in "${CLI_BINARIES[@]}"; do
        src="$unpack_dir/$cli_bin"
        if [[ -f "$src" ]]; then
            echo "  Signing $cli_bin..."
            codesign --force --options runtime --sign "$IDENTITY" --timestamp "$src"
            codesign --verify --verbose "$src"
            cp "$src" "$output_dir/"
        fi
    done
done

# --- Submit everything for notarization in one request ---

echo ""
echo "=== Notarizing ==="
NOTARIZE_ZIP="$WORKDIR/notarize-submission.zip"

# Build a zip containing all output directories
NOTARIZE_STAGING="$WORKDIR/notarize-staging"
mkdir -p "$NOTARIZE_STAGING"
for archive in "${ARCHIVES[@]}"; do
    slug="${archive%.tar.gz}"
    output_dir="$WORKDIR/${slug}-output"
    [[ -d "$output_dir" ]] || continue
    cp -a "$output_dir" "$NOTARIZE_STAGING/$slug"
done
ditto -c -k "$NOTARIZE_STAGING" "$NOTARIZE_ZIP"

echo "Submitting to Apple notary service..."
xcrun notarytool submit "$NOTARIZE_ZIP" \
    --apple-id "$APPLE_ID" \
    --password "$APPLE_APP_SPECIFIC_PASSWORD" \
    --team-id "$APPLE_TEAM_ID" \
    --wait

echo "Notarization complete."

# --- Staple notarization tickets to .app bundles ---

echo ""
echo "=== Stapling ==="
for app_dir in "${APP_BUNDLE_PATHS[@]}"; do
    echo "  Stapling $(basename "$app_dir")..."
    xcrun stapler staple "$app_dir"
done

# --- Repack archives ---

echo ""
echo "=== Repacking archives ==="

for archive in "${ARCHIVES[@]}"; do
    slug="${archive%.tar.gz}"
    output_dir="$WORKDIR/${slug}-output"
    [[ -d "$output_dir" ]] || continue

    archive_path="$(cd "$SEARCH_DIR" && pwd)/$archive"
    backup="${archive_path}.unsigned"

    echo "Backing up original to ${archive}.unsigned"
    mv "$archive_path" "$backup"

    echo "Creating signed $archive"
    tar czf "$archive_path" -C "$output_dir" .

    echo "Done: $archive"
done

echo ""
echo "All done. Signed archives are in $SEARCH_DIR"
echo "Original unsigned archives saved with .unsigned suffix."
echo ""
echo "Each archive now contains:"
echo "  ${APP_NAME}.app/  — signed, notarized, stapled GUI application"
for cli_bin in "${CLI_BINARIES[@]}"; do
    echo "  $cli_bin          — signed and notarized CLI tool"
done
