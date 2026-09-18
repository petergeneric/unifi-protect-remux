#!/usr/bin/env bash
# Usage: bash scripts/package-deb.sh <binary-dir> <output-dir> <release-tag>

set -euo pipefail

binary_dir=$(cd "${1:?Missing binary directory}" && pwd)
mkdir -p "${2:?Missing output directory}"
output_dir=$(cd "$2" && pwd)

release_tag=${3:?Missing release tag}
version=${release_tag#v}
if [[ ! $version =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?(\+[0-9A-Za-z.-]+)?$ ]]; then
    echo "Expected a SemVer release tag, got: $version" >&2
    exit 1
fi

# Debian's ~ sorts prereleases before the final version.
upstream=${version%%+*}
metadata=${version#"$upstream"}
version="${upstream/-/\~}${metadata}"
dpkg --validate-version "$version"

arch=$(dpkg --print-architecture)
case "$arch" in
    amd64|arm64) ;;
    *)
        echo "Unsupported architecture: $arch" >&2
        exit 1
        ;;
esac

repo_dir=$(cd "$(dirname "$0")/.." && pwd)
work_dir=$(mktemp -d)
trap 'rm -rf "$work_dir"' EXIT

package=unifi-protect-remux
root="$work_dir/package"
doc="$root/usr/share/doc/$package"

mkdir -p "$root/DEBIAN" "$doc"
cp -R "$repo_dir/packaging/debian" "$work_dir/debian"

# dpkg-gencontrol requires changelog metadata; release notes live on GitHub.
cat > "$work_dir/debian/changelog" <<EOF
$package ($version) unstable; urgency=medium

  * Release notes: https://github.com/petergeneric/unifi-protect-remux/releases/tag/$release_tag

 -- Peter Wright <code@peter.works>  $(LC_ALL=C date -R)
EOF

while read -r binary destination; do
    machine=$(LC_ALL=C readelf -h "$binary_dir/$binary")
    case "$arch:$machine" in
        amd64:*'Advanced Micro Devices X86-64'*|arm64:*AArch64*) ;;
        *)
            echo "Wrong ELF architecture: $binary" >&2
            exit 1
            ;;
    esac

    install -D -m 755 "$binary_dir/$binary" "$root/$destination/$binary"
done < "$work_dir/debian/install"

install -m 644 "$repo_dir/LICENSE.txt" "$doc/copyright"
gzip -n -9 -c "$repo_dir/README.md" > "$doc/README.md.gz"
gzip -n -9 -c "$work_dir/debian/changelog" > "$doc/changelog.Debian.gz"

(
    cd "$work_dir"
    dpkg-shlibdeps "$root"/usr/bin/*
    dpkg-gencontrol -P"$root"
)

dpkg-deb --build --root-owner-group -Zxz \
    "$root" \
    "$output_dir/${package}_${version}_${arch}.deb"
