#!/usr/bin/env bash
set -euo pipefail

# sign-and-notarize.sh — Sign and notarize macOS release archives locally.
#
# Signs CLI binaries in the release tarballs and builds signed .dmg
# installers from the unsigned GUI tarballs.
#
# Usage: macos-notarise.sh [--debug] <tarball>...
#
#   --debug   Skip signing, notarization and stapling (test the repack/dmg flow only)
#
# Tarballs are classified by filename:
#   unifi-protect-remux-macos-*.tar.gz  → CLI archive (signed and repacked)
#   gui-unsigned-macos-*.tar.gz         → GUI archive (signed, .dmg built)

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
    echo "Usage: $(basename "$0") [--debug] <tarball>..." >&2
    exit 1
fi

# --- Classify input files ---

CLI_BINARIES=(remux ubv-info ubv-anonymise)
APP_NAME="UBV Remux"
KEYCHAIN_SERVICE="unifi-protect-remux-notarize"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

declare -a CLI_PATHS=()
declare -a GUI_PATHS=()
declare -a GUI_ARCHS=()

for f in "${INPUT_FILES[@]}"; do
    if [[ ! -f "$f" ]]; then
        echo "ERROR: File not found: $f" >&2
        exit 1
    fi
    base="$(basename "$f")"
    case "$base" in
        unifi-protect-remux-macos-*.tar.gz)
            CLI_PATHS+=("$f")
            ;;
        gui-unsigned-macos-*.tar.gz)
            # Extract architecture from filename: gui-unsigned-macos-<arch>.tar.gz
            arch="${base#gui-unsigned-macos-}"
            arch="${arch%.tar.gz}"
            GUI_PATHS+=("$f")
            GUI_ARCHS+=("$arch")
            ;;
        *)
            echo "ERROR: Unrecognised tarball: $base" >&2
            echo "Expected unifi-protect-remux-macos-*.tar.gz or gui-unsigned-macos-*.tar.gz" >&2
            exit 1
            ;;
    esac
done

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

# --- Process CLI archives ---

declare -a CLI_UNPACK_DIRS=()

for archive_path in "${CLI_PATHS[@]}"; do
    echo ""
    echo "=== Processing CLI archive: $(basename "$archive_path") ==="

    unpack_dir="$WORKDIR/cli-${#CLI_UNPACK_DIRS[@]}"
    mkdir -p "$unpack_dir"
    CLI_UNPACK_DIRS+=("$unpack_dir")

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
declare -a GUI_UNPACK_DIRS=()

for i in "${!GUI_PATHS[@]}"; do
    archive_path="${GUI_PATHS[$i]}"
    arch="${GUI_ARCHS[$i]}"

    echo ""
    echo "=== Processing GUI archive: $(basename "$archive_path") ==="

    unpack_dir="$WORKDIR/gui-$arch"
    mkdir -p "$unpack_dir"
    GUI_UNPACK_DIRS+=("$unpack_dir")

    echo "Unpacking..."
    tar xzf "$archive_path" -C "$unpack_dir"

    APP_DIR="$unpack_dir/${APP_NAME}.app"
    if [[ ! -d "$APP_DIR" ]]; then
        echo "ERROR: Expected ${APP_NAME}.app in $(basename "$archive_path")" >&2
        exit 1
    fi

    APP_BUNDLE_PATHS+=("$APP_DIR")

    if [[ "$DEBUG" == false ]]; then
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

    for i in "${!CLI_UNPACK_DIRS[@]}"; do
        cp -a "${CLI_UNPACK_DIRS[$i]}" "$NOTARIZE_STAGING/cli-$i"
    done

    for i in "${!GUI_ARCHS[@]}"; do
        arch="${GUI_ARCHS[$i]}"
        app_dir="${APP_BUNDLE_PATHS[$i]}"
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

    echo ""
    echo "=== Stapling ==="
    for app_dir in "${APP_BUNDLE_PATHS[@]}"; do
        echo "  Stapling $(basename "$app_dir")..."
        xcrun stapler staple "$app_dir"
    done
fi

# --- Repack CLI archives ---

if [[ ${#CLI_PATHS[@]} -gt 0 ]]; then
    echo ""
    echo "=== Repacking CLI archives ==="

    for i in "${!CLI_PATHS[@]}"; do
        archive_path="$(cd "$(dirname "${CLI_PATHS[$i]}")" && pwd)/$(basename "${CLI_PATHS[$i]}")"
        unpack_dir="${CLI_UNPACK_DIRS[$i]}"
        backup="${archive_path}.unsigned"

        echo "Backing up original to $(basename "$archive_path").unsigned"
        mv "$archive_path" "$backup"

        echo "Creating signed $(basename "$archive_path")"
        tar czf "$archive_path" -C "$unpack_dir" .

        echo "Done: $(basename "$archive_path")"
    done
fi

# --- Build GUI .dmg files ---

if [[ ${#GUI_ARCHS[@]} -gt 0 ]]; then
    VOLICON="$SCRIPT_DIR/../assets/appicon.icns"

    echo ""
    echo "=== Building .dmg files ==="

    for i in "${!GUI_ARCHS[@]}"; do
        arch="${GUI_ARCHS[$i]}"
        app_dir="${APP_BUNDLE_PATHS[$i]}"
        dmg_name="gui-macos-${arch}.dmg"
        output_dir="$(cd "$(dirname "${GUI_PATHS[$i]}")" && pwd)"
        dmg_path="$output_dir/$dmg_name"

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

    # Notarize .dmg files (skip in debug mode)
    if [[ "$DEBUG" == false ]]; then
        echo ""
        echo "=== Notarizing .dmg files ==="

        for i in "${!GUI_ARCHS[@]}"; do
            arch="${GUI_ARCHS[$i]}"
            dmg_name="gui-macos-${arch}.dmg"
            output_dir="$(cd "$(dirname "${GUI_PATHS[$i]}")" && pwd)"
            dmg_path="$output_dir/$dmg_name"

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
if [[ "$DEBUG" == false && ${#CLI_PATHS[@]} -gt 0 ]]; then
    echo "Original unsigned CLI archives saved with .unsigned suffix."
fi
