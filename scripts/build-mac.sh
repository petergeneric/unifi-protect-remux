#!/usr/bin/env bash
set -euo pipefail

# Build script for macOS
# Builds remux-ffi native library, C# Avalonia GUI, and .app bundle
#
# Usage: build-mac.sh [--debug] [CONFIGURATION] [ARCH]
#   --debug: Use debug Rust build (links system FFmpeg, much faster)

DEBUG=false
POSITIONAL=()
for arg in "$@"; do
    case "$arg" in
        --debug) DEBUG=true ;;
        *) POSITIONAL+=("$arg") ;;
    esac
done

CONFIGURATION="${POSITIONAL[0]:-Release}"
ARCH="${POSITIONAL[1]:-$(uname -m)}"

if [ "$DEBUG" = true ]; then
    CONFIGURATION="Debug"
    CARGO_PROFILE="debug"
else
    CARGO_PROFILE="release"
fi

case "$ARCH" in
    arm64|aarch64) RID="osx-arm64" ;;
    x86_64|amd64)  RID="osx-x64" ;;
    *) echo "Unsupported architecture: $ARCH" >&2; exit 1 ;;
esac

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
NATIVE_DIR="$REPO_ROOT/ui/RemuxGui/native/$RID"
PUBLISH_DIR="$REPO_ROOT/publish/$RID"

echo "=== Building remux-ffi (Rust) for $RID ($CARGO_PROFILE) ==="
if [ "$DEBUG" = true ]; then
    cargo build --no-default-features -p remux-ffi
else
    cargo build --release --no-default-features -p remux-ffi
fi

echo "=== Copying native libraries ==="
mkdir -p "$NATIVE_DIR"
cp "$REPO_ROOT/target/$CARGO_PROFILE/libremux_ffi.dylib" "$NATIVE_DIR/"

echo "=== Building C# GUI ==="
dotnet publish "$REPO_ROOT/ui/RemuxGui/RemuxGui.csproj" \
    -c "$CONFIGURATION" \
    -r "$RID" \
    --self-contained \
    -o "$PUBLISH_DIR"

# Detect version from git tags
VERSION=$(git describe --tags --always 2>/dev/null || echo "dev")

echo "=== Creating .app bundle ==="
"$SCRIPT_DIR/create-macos-app.sh" "$PUBLISH_DIR" "$REPO_ROOT/publish" "$VERSION"

echo ""
echo "=== Done ==="
echo "App bundle: $REPO_ROOT/publish/UBV Remux.app"
echo "Raw publish: $PUBLISH_DIR"
