#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
app="$root/dist/Veil Display.app"

cargo build --release --manifest-path "$root/Cargo.toml"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
install -m 0755 "$root/target/release/cocoa-way" "$app/Contents/MacOS/Veil Display"
install -m 0644 "$root/packaging/VeilDisplay-Info.plist" "$app/Contents/Info.plist"
iconwork=$(mktemp -d "${TMPDIR:-/tmp}/veil-display.XXXXXX")
iconset="$iconwork/VeilDisplay.iconset"
mkdir -p "$iconset"
trap 'rm -rf "$iconwork"' EXIT
for size in 16 32 128 256 512; do
  sips -z "$size" "$size" "$root/assets/icon.png" --out "$iconset/icon_${size}x${size}.png" >/dev/null
  retina=$((size * 2))
  sips -z "$retina" "$retina" "$root/assets/icon.png" --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$iconset" -o "$app/Contents/Resources/VeilDisplay.icns"

# Ad-hoc signing is suitable for local development. Distribution builds should
# replace '-' with the Developer ID Application identity used for the Veil DMG.
codesign --force --deep --sign "${VEIL_CODESIGN_IDENTITY:--}" "$app"
echo "$app"
