#!/usr/bin/env bash
set -euo pipefail

# End-to-end sanity script:
#   1. Generate a `testsrc2` pattern (has a built-in running timecode and
#      frame counter in the top-left) + 1 kHz test tone via ffmpeg.
#   2. Feed it to `create-ubv` to produce samples/synthetic.ubv.
#   3. Run `remux` on that .ubv to produce samples/synthetic-out.mp4.
#
# Handy for eyeballing the whole pipeline after making pipeline changes —
# the timecode overlay makes frame drops / timing issues obvious.
#
# Requirements: ffmpeg on PATH, cargo, system FFmpeg for the debug build
# (linked via pkg-config).

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SAMPLES_DIR="$REPO_ROOT/samples"
UBV_OUT="$SAMPLES_DIR/synthetic.ubv"
MP4_OUT="$SAMPLES_DIR/synthetic-out.mp4"

TMPDIR="$(mktemp -d)"
trap 'rm -rf "$TMPDIR"' EXIT
SOURCE_MP4="$TMPDIR/source.mp4"
REMUX_OUT_DIR="$TMPDIR/remux-out"
mkdir -p "$SAMPLES_DIR" "$REMUX_OUT_DIR"

echo "=== Generating testsrc2 pattern + 1 kHz tone (5s, 640x480 @ 30fps) ==="
# testsrc2 renders a running HH:MM:SS.mmm timecode and frame counter in the
# top-left corner natively — no freetype/drawtext required.
#
# `-g 30` + scenecut=0 forces a keyframe every second (5 GOPs over 5s) so the
# remux pipeline exercises keyframe-boundary handling, not just a single GOP.
ffmpeg -y -loglevel error \
    -f lavfi -i "testsrc2=size=640x480:rate=30" \
    -f lavfi -i "sine=frequency=1000:sample_rate=48000" \
    -t 5 \
    -c:v libx264 -pix_fmt yuv420p -profile:v baseline -level 3.0 \
    -g 30 -x264-params "scenecut=0" \
    -c:a aac -b:a 128k \
    -shortest -movflags +faststart \
    "$SOURCE_MP4"

echo "=== Building create-ubv and remux (debug, system FFmpeg) ==="
# --release swaps to the statically-compiled FFmpeg (slower build, no system
# dep) if your system FFmpeg isn't compatible with ffmpeg-next.
cd "$REPO_ROOT"
cargo build --no-default-features -p remux -p create-ubv

echo "=== Synthesising $UBV_OUT ==="
"$REPO_ROOT/target/debug/create-ubv" "$SOURCE_MP4" "$UBV_OUT"

echo "=== Remuxing to $MP4_OUT ==="
# `remux` names outputs <base>_<timecode>.mp4 and does not let us override
# the final filename, so we render into a tempdir and rename into place.
"$REPO_ROOT/target/debug/remux" --output-folder "$REMUX_OUT_DIR" "$UBV_OUT"

produced=("$REMUX_OUT_DIR"/*.mp4)
if [ "${#produced[@]}" -ne 1 ]; then
    echo "Expected exactly one MP4 from remux, got ${#produced[@]}:" >&2
    printf '  %s\n' "${produced[@]}" >&2
    exit 1
fi
mv "${produced[0]}" "$MP4_OUT"

echo ""
echo "=== Round-trip complete ==="
echo "  UBV:        $UBV_OUT"
echo "  Output MP4: $MP4_OUT"
