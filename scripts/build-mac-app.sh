#!/usr/bin/env bash
# Builds likh.app (Apple silicon and Intel in one binary) and likh-macos.dmg
# into target/macos. macOS only: it needs lipo, iconutil, codesign and hdiutil.
#
# Signing: with MAC_SIGNING_IDENTITY (a "Developer ID Application: ..." in the
# keychain) the app is signed for the hardened runtime, ready to notarize.
# Without it, it's signed ad hoc: it runs, and macOS asks people to confirm
# it the first time (right-click, Open), as for any unsigned download.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

version="$(sed -n 's/^version = "\(.*\)"$/\1/p' app/Cargo.toml | head -1)"
short="${version%%-*}"
out="target/macos"
app="$out/likh.app"

for target in aarch64-apple-darwin x86_64-apple-darwin; do
  rustup target add "$target" >/dev/null
  cargo build -p likh --release --locked --target "$target"
done

rm -rf "$out"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
lipo -create -output "$app/Contents/MacOS/likh" \
  target/aarch64-apple-darwin/release/likh target/x86_64-apple-darwin/release/likh
sed -e "s/__VERSION__/$version/" -e "s/__SHORT_VERSION__/$short/" installer/macos/Info.plist > "$app/Contents/Info.plist"
plutil -lint "$app/Contents/Info.plist" >/dev/null
"$app/Contents/MacOS/likh" icon "$out/likh.iconset"
iconutil -c icns "$out/likh.iconset" -o "$app/Contents/Resources/likh.icns"

if [ -n "${MAC_SIGNING_IDENTITY:-}" ]; then
  codesign --force --timestamp --options runtime --entitlements installer/macos/likh.entitlements \
    --sign "$MAC_SIGNING_IDENTITY" "$app"
else
  codesign --force --sign - "$app"
fi
codesign --verify --strict "$app"

stage="$out/dmg"
mkdir -p "$stage"
cp -R "$app" "$stage/"
ln -s /Applications "$stage/Applications"
hdiutil create -volname "likh" -srcfolder "$stage" -ov -format UDZO "$out/likh-macos.dmg" >/dev/null
rm -rf "$stage" "$out/likh.iconset"
echo "$root/$out/likh-macos.dmg"
