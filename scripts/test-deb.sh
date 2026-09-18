#!/usr/bin/env bash
# Run as root inside a disposable Debian/Ubuntu container.

set -euo pipefail
export DEBIAN_FRONTEND=noninteractive

deb=$(realpath "${1:?Missing package file}")
fixture=$(realpath "${2:?Missing UBV fixture}")

apt-get update
apt-get install -y --no-install-recommends "$deb"

for binary in remux ubv-info ubv-anonymise; do
    "$binary" --help > /dev/null
done
remux --version

work_dir=$(mktemp -d)
trap 'rm -rf "$work_dir"' EXIT

remux \
    --fail-fast=true \
    --output-folder "$work_dir" \
    "$fixture"

outputs=("$work_dir"/*.mp4)
test -s "${outputs[0]}"

# Install FFmpeg only after remuxing, to expose missing package dependencies.
apt-get install -y --no-install-recommends ffmpeg
for output in "${outputs[@]}"; do
    ffmpeg -v error -xerror -i "$output" -f null -
done

apt-get purge -y unifi-protect-remux

for binary in remux ubv-info ubv-anonymise; do
    test ! -e "/usr/bin/$binary"
done
