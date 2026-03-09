#!/usr/bin/env bash
set -euo pipefail

# Build script for native SwiftUI macOS app
#
# Usage: build-swift.sh [--release]
#   Default is debug build (links system FFmpeg via pkg-config, fast).
#   --release: statically compiles FFmpeg from source (slow).

RELEASE=false
for arg in "$@"; do
    case "$arg" in
        --release) RELEASE=true ;;
        *) echo "Unknown argument: $arg" >&2; exit 1 ;;
    esac
done

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MACOS_DIR="$REPO_ROOT/ui/macos"

if [ "$RELEASE" = true ]; then
    CARGO_FLAGS="--release"
    CARGO_PROFILE="release"
    XCODE_CONFIG="Release"
else
    CARGO_FLAGS="--no-default-features"
    CARGO_PROFILE="debug"
    XCODE_CONFIG="Debug"
fi

echo "=== Building remux-ffi (Rust, $CARGO_PROFILE) ==="
cargo build -p remux-ffi $CARGO_FLAGS

echo "=== Copying dylib ==="
mkdir -p "$MACOS_DIR/lib"
cp "$REPO_ROOT/target/$CARGO_PROFILE/libremux_ffi.dylib" "$MACOS_DIR/lib/"

# Derive version from git tags (same source as Rust's GIT_VERSION)
GIT_VERSION="$(git -C "$REPO_ROOT" describe --tags --always 2>/dev/null || echo "0.0.0")"
# Strip leading 'v' and any pre-release suffix for CFBundleShortVersionString (e.g. v4.2.1-3-gabcdef -> 4.2.1)
MARKETING_VERSION="$(echo "$GIT_VERSION" | sed 's/^v//; s/-.*//')"
echo "=== Version: $MARKETING_VERSION (from $GIT_VERSION) ==="

echo "=== Building SwiftUI app ($XCODE_CONFIG) ==="
xcodebuild \
    -project "$MACOS_DIR/RemuxGui.xcodeproj" \
    -scheme RemuxGui \
    -configuration "$XCODE_CONFIG" \
    -derivedDataPath "$MACOS_DIR/build" \
    MARKETING_VERSION="$MARKETING_VERSION" \
    build

APP_PATH="$(find "$MACOS_DIR/build" -name 'UBV Remux.app' -type d | head -1)"

if [ -z "$APP_PATH" ]; then
    echo "ERROR: UBV Remux.app not found in build output" >&2
    exit 1
fi

echo ""
echo "=== Done ==="
echo "$APP_PATH"
