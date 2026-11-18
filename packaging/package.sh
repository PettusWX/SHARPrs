#!/usr/bin/env bash
# Package already-built release binaries. Run from the workspace root.
set -euo pipefail
platform=${1:?usage: package.sh macos-universal|windows-x64 BIN_DIR [SECOND_MAC_BIN_DIR]}
bin_dir=${2:?specify a directory containing the release binaries}
dist="$PWD/target/dist"
mkdir -p "$dist"
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
product="$stage/SHARPrs"
mkdir -p "$product"
version=$(cargo metadata --locked --no-deps --format-version=1 | jq -r '.packages[] | select(.name=="sharprs-desktop") | .version')
case "$platform" in
    macos-universal)
        other=${3:?second macOS architecture required}
        app="$product/SHARPrs.app/Contents"
        mkdir -p "$app/MacOS" "$app/Resources"
        lipo -create "$bin_dir/sharprs-desktop" "$other/sharprs-desktop" -output "$app/MacOS/SHARPrs"
        lipo -create "$bin_dir/sharprs" "$other/sharprs" -output "$product/sharprs"
        sed "s/@VERSION@/$version/g" packaging/Info.plist > "$app/Info.plist"
        iconset="$stage/AppIcon.iconset"
        mkdir "$iconset"
        for size in 16 32 128 256 512; do
            sips -z "$size" "$size" packaging/icon.png --out "$iconset/icon_${size}x${size}.png" >/dev/null
            sips -z "$((size*2))" "$((size*2))" packaging/icon.png --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
        done
        iconutil -c icns "$iconset" -o "$app/Resources/AppIcon.icns"
        notices="$app/Resources/Notices"
        ;;
    windows-x64)
        cp "$bin_dir/sharprs-desktop.exe" "$product/SHARPrs.exe"
        # Keep the GUI and CLI filenames distinct on case-insensitive file systems.
        mkdir "$product/cli"
        cp "$bin_dir/sharprs.exe" "$product/cli/"
        if [[ -n "${LLVM_MINGW_ROOT:-}" ]]; then
            cp "$LLVM_MINGW_ROOT/x86_64-w64-mingw32/bin/libunwind.dll" "$product/"
            cp "$product/libunwind.dll" "$product/cli/"
            cp "$LLVM_MINGW_ROOT/LICENSE.TXT" "$product/LLVM-LICENSE.txt"
        fi
        notices="$product/Notices"
        ;;
    *) echo "Unknown package target: $platform" >&2; exit 1 ;;
esac
mkdir -p "$notices"
cp crates/sharprs-core/NOTICE "$notices/SHARPrs-data.txt"
cp examples/data/sharppy/LICENSE.rst "$notices/SHARPpy-example.txt"
cp crates/sharprs-render/fonts/arimo/OFL.txt "$notices/Arimo.txt"
cp crates/sharprs-render/fonts/roboto/LICENSE.txt "$notices/Roboto.txt"
cp README.md LICENSE "$product/"
cp LICENSE "$notices/SHARPrs-LICENSE.txt"
# Include the license metadata and supplied notices for the target dependency tree.
triple=${PACKAGE_TARGET:-$(rustc -vV | sed -n 's/^host: //p')}
cargo metadata --locked --format-version=1 --filter-platform "$triple" > "$stage/meta.json"
cargo tree --locked -p sharprs-cli -p sharprs-desktop --target "$triple" -e normal --prefix none --format '{p}' |
    sed -E 's/ [(].*$//' | sort -u > "$stage/tree"
jq -r '.packages[] | select(.source != null) | [.name,.version,(.license // "See package license"),.manifest_path] | @tsv' "$stage/meta.json" |
while IFS=$'\t' read -r name vers license manifest; do
    grep -Fxq "$name v$vers" "$stage/tree" || continue
    printf '%s %s  %s\n' "$name" "$vers" "$license" >> "$notices/Dependencies.txt"
    crate_dir=$(dirname "$manifest")
    for f in "$crate_dir"/{LICENSE*,LICENCE*,COPYING*,NOTICE*} "$crate_dir"/fonts/*.txt; do
        if [[ -f "$f" ]]; then cp "$f" "$notices/$name-$vers-$(basename "$f").txt"; fi
    done
done
archive="$dist/SHARPrs-$platform.zip"
if [[ "$platform" == macos-universal ]]; then
    codesign --force --sign - "$app/MacOS/SHARPrs"
    codesign --force --sign - "$product/sharprs"
    codesign --force --sign - "$product/SHARPrs.app"
    codesign --verify --strict --all-architectures "$product/SHARPrs.app"
    ditto -c -k --sequesterRsrc --keepParent "$product" "$stage/package.zip"
else
    (cd "$stage" && cmake -E tar cf package.zip --format=zip -- SHARPrs)
fi
mv "$stage/package.zip" "$archive"
echo "$archive"
