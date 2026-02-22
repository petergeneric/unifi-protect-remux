#!/usr/bin/env bash
set -euo pipefail

# sign-and-notarize.sh — Sign and notarize macOS release archives locally.
#
# Signs binaries, notarizes with Apple, and produces release-ready output.
#
# Usage: macos-notarise.sh [--debug] <archive>...
#
#   --debug   Skip signing, notarization and stapling (test the repack/dmg flow only)
#
# Accepts .tar.gz and .zip archives. Each archive is classified by contents:
#   Contains a .app bundle → GUI archive (signed, packaged as .dmg)
#   Otherwise              → CLI archive (binaries signed, repacked)
#
# Any "-unsigned" in the input filename is removed in the output filename.

# --- Parse arguments ---

DEBUG=false
declare -a INPUT_FILES=()

while [[ $# -gt 0 ]]; do
    case "$1" in
        --debug) DEBUG=true; shift ;;
        *) INPUT_FILES+=("$1"); shift ;;
    esac
done

if [[ ${#INPUT_FILES[@]} -eq 0 ]]; then
    echo "Usage: $(basename "$0") [--debug] <archive>..." >&2
    exit 1
fi

# --- Helpers ---

unpack() {
    local archive="$1" dest="$2"
    case "$archive" in
        *.tar.gz) tar xzf "$archive" -C "$dest" ;;
        *.zip)    unzip -qo "$archive" -d "$dest" ;;
        *)        echo "ERROR: Unsupported archive format: $archive" >&2; exit 1 ;;
    esac

    # GitHub Actions artifact downloads wrap the file in a zip.
    # If the result is a single .tar.gz, unpack that too.
    local inner
    inner=$(find "$dest" -maxdepth 1 -name '*.tar.gz' -type f)
    if [[ $(echo "$inner" | wc -l) -eq 1 && -n "$inner" ]]; then
        local count
        count=$(find "$dest" -maxdepth 1 -mindepth 1 | wc -l)
        if [[ "$count" -eq 1 ]]; then
            echo "  Unwrapping inner archive: $(basename "$inner")"
            tar xzf "$inner" -C "$dest"
            rm "$inner"
        fi
    fi
}

repack() {
    local src_dir="$1" dest="$2"
    case "$dest" in
        *.tar.gz) tar czf "$dest" -C "$src_dir" . ;;
        *.zip)    (cd "$src_dir" && zip -qr "$dest" .) ;;
        *)        echo "ERROR: Unsupported archive format: $dest" >&2; exit 1 ;;
    esac
}

# Remove "-unsigned" from a filename
strip_unsigned() {
    echo "$1" | sed 's/-unsigned//g'
}

# --- Configuration ---

APP_NAME="UBV Remux"
KEYCHAIN_SERVICE="unifi-protect-remux-notarize"
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

# --- Unpack and classify archives ---

CLI_COUNT=0
GUI_COUNT=0

for archive in "${INPUT_FILES[@]}"; do
    if [[ ! -f "$archive" ]]; then
        echo "ERROR: File not found: $archive" >&2
        exit 1
    fi

    echo ""
    echo "=== Unpacking $(basename "$archive") ==="

    unpack_dir="$WORKDIR/archive-${CLI_COUNT}-${GUI_COUNT}"
    mkdir -p "$unpack_dir"
    unpack "$archive" "$unpack_dir"

    # Classify by contents: look for a .app bundle
    app_dir=$(find "$unpack_dir" -maxdepth 1 -name '*.app' -type d | head -1)

    if [[ -n "$app_dir" ]]; then
        echo "  Detected GUI archive (found $(basename "$app_dir"))"
        eval "GUI_INPUT_${GUI_COUNT}=\"$archive\""
        eval "GUI_DIR_${GUI_COUNT}=\"$unpack_dir\""
        eval "APP_BUNDLE_${GUI_COUNT}=\"$app_dir\""
        GUI_COUNT=$((GUI_COUNT + 1))
    else
        echo "  Detected CLI archive"
        eval "CLI_INPUT_${CLI_COUNT}=\"$archive\""
        eval "CLI_DIR_${CLI_COUNT}=\"$unpack_dir\""
        CLI_COUNT=$((CLI_COUNT + 1))
    fi
done

# --- Sign CLI binaries ---

if [[ "$DEBUG" == false && $CLI_COUNT -gt 0 ]]; then
    for i in $(seq 0 $((CLI_COUNT - 1))); do
        eval "unpack_dir=\"\$CLI_DIR_${i}\""
        eval "input=\"\$CLI_INPUT_${i}\""
        echo ""
        echo "=== Signing CLI binaries in $(basename "$input") ==="

        find "$unpack_dir" -maxdepth 1 -type f -perm +111 | while read -r bin; do
            echo "  Signing $(basename "$bin")..."
            codesign --force --options runtime --sign "$IDENTITY" --timestamp "$bin"
            codesign --verify --verbose "$bin"
        done
    done
fi

# --- Sign GUI .app bundles ---

if [[ "$DEBUG" == false && $GUI_COUNT -gt 0 ]]; then
    for i in $(seq 0 $((GUI_COUNT - 1))); do
        eval "app_dir=\"\$APP_BUNDLE_${i}\""
        echo ""
        echo "=== Signing $(basename "$app_dir") ==="

        echo "  Signing native libraries..."
        find "$app_dir/Contents/MacOS" -name '*.dylib' -exec \
            codesign --force --options runtime --sign "$IDENTITY" --timestamp \
            --entitlements "$ENTITLEMENTS" {} \;

        echo "  Signing app bundle..."
        codesign --force --options runtime --sign "$IDENTITY" --timestamp \
            --entitlements "$ENTITLEMENTS" \
            "$app_dir"
        codesign --verify --verbose "$app_dir"
    done
fi

# --- Notarization (skip in debug mode) ---

if [[ "$DEBUG" == false ]]; then
    echo ""
    echo "=== Notarizing ==="
    NOTARIZE_ZIP="$WORKDIR/notarize-submission.zip"

    NOTARIZE_STAGING="$WORKDIR/notarize-staging"
    mkdir -p "$NOTARIZE_STAGING"

    for i in $(seq 0 $((CLI_COUNT - 1))); do
        eval "cli_dir=\"\$CLI_DIR_${i}\""
        cp -a "$cli_dir" "$NOTARIZE_STAGING/cli-$i"
    done

    for i in $(seq 0 $((GUI_COUNT - 1))); do
        eval "app_dir=\"\$APP_BUNDLE_${i}\""
        mkdir -p "$NOTARIZE_STAGING/gui-$i"
        cp -a "$app_dir" "$NOTARIZE_STAGING/gui-$i/"
    done

    ditto -c -k "$NOTARIZE_STAGING" "$NOTARIZE_ZIP"

    echo "Submitting to Apple notary service..."
    xcrun notarytool submit "$NOTARIZE_ZIP" \
        --apple-id "$APPLE_ID" \
        --password "$APPLE_APP_SPECIFIC_PASSWORD" \
        --team-id "$APPLE_TEAM_ID" \
        --wait

    echo "Notarization complete."

    if [[ $GUI_COUNT -gt 0 ]]; then
        echo ""
        echo "=== Stapling ==="
        for i in $(seq 0 $((GUI_COUNT - 1))); do
            eval "app_dir=\"\$APP_BUNDLE_${i}\""
            echo "  Stapling $(basename "$app_dir")..."
            xcrun stapler staple "$app_dir"
        done
    fi
fi

# --- Repack CLI archives ---

if [[ $CLI_COUNT -gt 0 ]]; then
    echo ""
    echo "=== Repacking CLI archives ==="

    for i in $(seq 0 $((CLI_COUNT - 1))); do
        eval "input=\"\$CLI_INPUT_${i}\""
        eval "unpack_dir=\"\$CLI_DIR_${i}\""
        archive_path="$(cd "$(dirname "$input")" && pwd)/$(basename "$input")"
        output_path="$(dirname "$archive_path")/$(strip_unsigned "$(basename "$archive_path")")"

        if [[ "$archive_path" != "$output_path" ]]; then
            echo "  Creating $(basename "$output_path")"
        else
            backup="${archive_path}.unsigned"
            echo "  Backing up original to $(basename "$backup")"
            mv "$archive_path" "$backup"
        fi

        repack "$unpack_dir" "$output_path"
        echo "  Done: $(basename "$output_path")"
    done
fi

# --- Build GUI .dmg files ---

if [[ $GUI_COUNT -gt 0 ]]; then
    VOLICON="$SCRIPT_DIR/../assets/appicon.icns"

    echo ""
    echo "=== Building .dmg files ==="

    DMG_COUNT=0

    for i in $(seq 0 $((GUI_COUNT - 1))); do
        eval "app_dir=\"\$APP_BUNDLE_${i}\""
        eval "input=\"\$GUI_INPUT_${i}\""
        archive_path="$(cd "$(dirname "$input")" && pwd)/$(basename "$input")"
        output_dir="$(dirname "$archive_path")"

        # Derive .dmg name: strip extension and -unsigned, then add .dmg
        base="$(basename "$archive_path")"
        case "$base" in
            *.tar.gz) stem="${base%.tar.gz}" ;;
            *.zip)    stem="${base%.zip}" ;;
        esac
        dmg_name="$(strip_unsigned "$stem").dmg"
        dmg_path="$output_dir/$dmg_name"
        eval "DMG_PATH_${DMG_COUNT}=\"$dmg_path\""
        DMG_COUNT=$((DMG_COUNT + 1))

        echo "  Building $dmg_path"

        dmg_staging="$WORKDIR/dmg-$i"
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
            "$dmg_path" \
            "$dmg_staging"

        echo "  Done: $dmg_name"
    done

    # Notarize .dmg files (skip in debug mode)
    if [[ "$DEBUG" == false && $DMG_COUNT -gt 0 ]]; then
        echo ""
        echo "=== Notarizing .dmg files ==="

        for i in $(seq 0 $((DMG_COUNT - 1))); do
            eval "dmg_path=\"\$DMG_PATH_${i}\""
            dmg_name="$(basename "$dmg_path")"

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
fi

echo ""
echo "All done."
