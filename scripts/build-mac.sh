#!/usr/bin/env bash
set -euo pipefail

# Build script for macOS
# Builds remux-ffi native library, C# Avalonia GUI, and .app bundle

CONFIGURATION="${1:-Release}"
ARCH="${2:-$(uname -m)}"

case "$ARCH" in
    arm64|aarch64) RID="osx-arm64" ;;
    x86_64|amd64)  RID="osx-x64" ;;
    *) echo "Unsupported architecture: $ARCH" >&2; exit 1 ;;
esac

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
NATIVE_DIR="$REPO_ROOT/remux-gui-cs/RemuxGui/native/$RID"
PUBLISH_DIR="$REPO_ROOT/publish/$RID"

echo "=== Building remux-ffi (Rust) for $RID ==="
cargo build --release --no-default-features -p remux-ffi

echo "=== Copying native libraries ==="
mkdir -p "$NATIVE_DIR"
cp "$REPO_ROOT/target/release/libremux_ffi.dylib" "$NATIVE_DIR/"

echo "=== Building C# GUI ==="
dotnet publish "$REPO_ROOT/remux-gui-cs/RemuxGui/RemuxGui.csproj" \
    -c "$CONFIGURATION" \
    -r "$RID" \
    --self-contained \
    -o "$PUBLISH_DIR"

# Detect version from Cargo.toml
VERSION=$(grep '^version = ' "$REPO_ROOT/Cargo.toml" | head -1 | sed 's/.*"\(.*\)".*/\1/')

echo "=== Creating .app bundle ==="
"$SCRIPT_DIR/create-macos-app.sh" "$PUBLISH_DIR" "$REPO_ROOT/publish" "$VERSION"

echo ""
echo "=== Done ==="
echo "App bundle: $REPO_ROOT/publish/UBV Remux.app"
echo "Raw publish: $PUBLISH_DIR"
