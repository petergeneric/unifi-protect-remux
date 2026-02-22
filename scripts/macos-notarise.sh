#!/usr/bin/env bash
set -euo pipefail

# sign-and-notarize.sh — Sign and notarize macOS release archives locally.
#
# Signs CLI binaries in the release tarballs and builds signed .dmg
# installers from the unsigned GUI tarballs.
#
# Usage: macos-notarise.sh [--debug] [search_dir]
#
#   --debug   Skip signing, notarization and stapling (test the repack/dmg flow only)
#
# Expects to find some or all of:
#   unifi-protect-remux-macos-aarch64.tar.gz   (CLI tools)
#   unifi-protect-remux-macos-x86_64.tar.gz    (CLI tools)
#   gui-unsigned-macos-aarch64.tar.gz           (.app bundle)
#   gui-unsigned-macos-x86_64.tar.gz            (.app bundle)

# --- Parse arguments ---

DEBUG=false
SEARCH_DIR="."

while [[ $# -gt 0 ]]; do
    case "$1" in
        --debug) DEBUG=true; shift ;;
        *) SEARCH_DIR="$1"; shift ;;
    esac
done

# --- Configuration ---

KEYCHAIN_SERVICE="unifi-protect-remux-notarize"
BUNDLE_ID="works.peter.ubv-remux"
APP_NAME="UBV Remux"

CLI_ARCHIVES=(
    "unifi-protect-remux-macos-x86_64.tar.gz"
    "unifi-protect-remux-macos-aarch64.tar.gz"
)
GUI_ARCHIVES=(
    "gui-unsigned-macos-x86_64.tar.gz:x86_64"
    "gui-unsigned-macos-aarch64.tar.gz:aarch64"
)
CLI_BINARIES=(remux ubv-info ubv-anonymise)

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# --- Retrieve credentials and signing identity (skip in debug mode) ---

if [[ "$DEBUG" == true ]]; then
    echo "DEBUG MODE: skipping signing, notarization and stapling"
    IDENTITY=""
else
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

    IDENTITY=$(security find-identity -v -p codesigning | grep "Developer ID Application" | head -1 | sed 's/.*"\(.*\)".*/\1/')
    if [[ -z "$IDENTITY" ]]; then
        echo "ERROR: No 'Developer ID Application' certificate found in keychain." >&2
        echo "Install your Developer ID certificate in Keychain Access first." >&2
        exit 1
    fi
    echo "  Signing identity: $IDENTITY"
fi

# --- Check for archives ---

found_any=false
for archive in "${CLI_ARCHIVES[@]}"; do
    [[ -f "$SEARCH_DIR/$archive" ]] && found_any=true
done
for entry in "${GUI_ARCHIVES[@]}"; do
    archive="${entry%%:*}"
    [[ -f "$SEARCH_DIR/$archive" ]] && found_any=true
done

if [[ "$found_any" == false ]]; then
    echo "ERROR: No matching archives found in $SEARCH_DIR" >&2
    exit 1
fi

# --- Create working directory ---

WORKDIR="$(mktemp -d)"
trap 'rm -rf "$WORKDIR"' EXIT

# --- Create entitlements file ---

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

# --- Process CLI archives ---

for archive in "${CLI_ARCHIVES[@]}"; do
    archive_path="$SEARCH_DIR/$archive"
    if [[ ! -f "$archive_path" ]]; then
        echo "Skipping $archive (not found)"
        continue
    fi

    echo ""
    echo "=== Processing CLI archive: $archive ==="

    slug="${archive%.tar.gz}"
    unpack_dir="$WORKDIR/$slug"
    mkdir -p "$unpack_dir"

    echo "Unpacking..."
    tar xzf "$archive_path" -C "$unpack_dir"

    if [[ "$DEBUG" == false ]]; then
        for cli_bin in "${CLI_BINARIES[@]}"; do
            src="$unpack_dir/$cli_bin"
            if [[ -f "$src" ]]; then
                echo "  Signing $cli_bin..."
                codesign --force --options runtime --sign "$IDENTITY" --timestamp "$src"
                codesign --verify --verbose "$src"
            fi
        done
    fi
done

# --- Process GUI archives ---

declare -a APP_BUNDLE_PATHS=()
declare -a GUI_OUTPUT_DIRS=()
declare -a GUI_ARCHS=()

for entry in "${GUI_ARCHIVES[@]}"; do
    archive="${entry%%:*}"
    arch="${entry##*:}"
    archive_path="$SEARCH_DIR/$archive"

    if [[ ! -f "$archive_path" ]]; then
        echo "Skipping $archive (not found)"
        continue
    fi

    echo ""
    echo "=== Processing GUI archive: $archive ==="

    unpack_dir="$WORKDIR/gui-$arch"
    mkdir -p "$unpack_dir"

    echo "Unpacking..."
    tar xzf "$archive_path" -C "$unpack_dir"

    APP_DIR="$unpack_dir/${APP_NAME}.app"
    if [[ ! -d "$APP_DIR" ]]; then
        echo "ERROR: Expected ${APP_NAME}.app in $archive" >&2
        exit 1
    fi

    APP_BUNDLE_PATHS+=("$APP_DIR")
    GUI_OUTPUT_DIRS+=("$unpack_dir")
    GUI_ARCHS+=("$arch")

    if [[ "$DEBUG" == false ]]; then
        # Sign all native libraries inside the .app bundle (dylibs first, then main executable)
        echo "  Signing native libraries in ${APP_NAME}.app..."
        find "$APP_DIR/Contents/MacOS" -name '*.dylib' -exec \
            codesign --force --options runtime --sign "$IDENTITY" --timestamp \
            --entitlements "$ENTITLEMENTS" {} \;

        echo "  Signing ${APP_NAME}.app..."
        codesign --force --options runtime --sign "$IDENTITY" --timestamp \
            --entitlements "$ENTITLEMENTS" \
            "$APP_DIR"
        codesign --verify --verbose "$APP_DIR"
    fi
done

# --- Notarization (skip in debug mode) ---

if [[ "$DEBUG" == false ]]; then
    echo ""
    echo "=== Notarizing ==="
    NOTARIZE_ZIP="$WORKDIR/notarize-submission.zip"

    NOTARIZE_STAGING="$WORKDIR/notarize-staging"
    mkdir -p "$NOTARIZE_STAGING"

    # Add signed CLI binaries
    for archive in "${CLI_ARCHIVES[@]}"; do
        slug="${archive%.tar.gz}"
        unpack_dir="$WORKDIR/$slug"
        [[ -d "$unpack_dir" ]] || continue
        cp -a "$unpack_dir" "$NOTARIZE_STAGING/$slug"
    done

    # Add signed .app bundles
    for i in "${!GUI_ARCHS[@]}"; do
        arch="${GUI_ARCHS[$i]}"
        app_dir="${APP_BUNDLE_PATHS[$i]}"
        [[ -d "$app_dir" ]] || continue
        mkdir -p "$NOTARIZE_STAGING/gui-$arch"
        cp -a "$app_dir" "$NOTARIZE_STAGING/gui-$arch/"
    done

    ditto -c -k "$NOTARIZE_STAGING" "$NOTARIZE_ZIP"

    echo "Submitting to Apple notary service..."
    xcrun notarytool submit "$NOTARIZE_ZIP" \
        --apple-id "$APPLE_ID" \
        --password "$APPLE_APP_SPECIFIC_PASSWORD" \
        --team-id "$APPLE_TEAM_ID" \
        --wait

    echo "Notarization complete."

    # Staple notarization tickets to .app bundles
    echo ""
    echo "=== Stapling ==="
    for app_dir in "${APP_BUNDLE_PATHS[@]}"; do
        echo "  Stapling $(basename "$app_dir")..."
        xcrun stapler staple "$app_dir"
    done
fi

# --- Repack CLI archives ---

echo ""
echo "=== Repacking CLI archives ==="

for archive in "${CLI_ARCHIVES[@]}"; do
    slug="${archive%.tar.gz}"
    unpack_dir="$WORKDIR/$slug"
    [[ -d "$unpack_dir" ]] || continue

    archive_path="$(cd "$SEARCH_DIR" && pwd)/$archive"
    backup="${archive_path}.unsigned"

    echo "Backing up original to ${archive}.unsigned"
    mv "$archive_path" "$backup"

    echo "Creating signed $archive"
    tar czf "$archive_path" -C "$unpack_dir" .

    echo "Done: $archive"
done

# --- Build GUI .dmg files ---

VOLICON="$SCRIPT_DIR/../assets/appicon.icns"

echo ""
echo "=== Building .dmg files ==="

for i in "${!GUI_ARCHS[@]}"; do
    arch="${GUI_ARCHS[$i]}"
    app_dir="${APP_BUNDLE_PATHS[$i]}"
    dmg_name="gui-macos-${arch}.dmg"
    dmg_path="$(cd "$SEARCH_DIR" && pwd)/$dmg_name"

    echo "Building $dmg_name..."

    dmg_staging="$WORKDIR/dmg-$arch"
    mkdir -p "$dmg_staging"
    cp -a "$app_dir" "$dmg_staging/"

    create-dmg \
        --volname "$APP_NAME" \
        --volicon "$VOLICON" \
        --window-size 600 400 \
        --icon-size 128 \
        --icon "${APP_NAME}.app" 150 185 \
        --app-drop-link 450 185 \
        --hide-extension "${APP_NAME}.app" \
        --no-internet-enable \
        --skip-jenkins \
        "$dmg_path" \
        "$dmg_staging"

    echo "Done: $dmg_name"
done

# --- Notarize .dmg files (skip in debug mode) ---

if [[ "$DEBUG" == false && ${#GUI_ARCHS[@]} -gt 0 ]]; then
    echo ""
    echo "=== Notarizing .dmg files ==="

    for i in "${!GUI_ARCHS[@]}"; do
        arch="${GUI_ARCHS[$i]}"
        dmg_name="gui-macos-${arch}.dmg"
        dmg_path="$(cd "$SEARCH_DIR" && pwd)/$dmg_name"

        echo "Submitting $dmg_name to Apple notary service..."
        xcrun notarytool submit "$dmg_path" \
            --apple-id "$APPLE_ID" \
            --password "$APPLE_APP_SPECIFIC_PASSWORD" \
            --team-id "$APPLE_TEAM_ID" \
            --wait

        echo "  Stapling $dmg_name..."
        xcrun stapler staple "$dmg_path"

        echo "Done: $dmg_name"
    done
fi

echo ""
echo "All done. Output is in $SEARCH_DIR"
if [[ "$DEBUG" == false ]]; then
    echo "Original unsigned CLI archives saved with .unsigned suffix."
fi
