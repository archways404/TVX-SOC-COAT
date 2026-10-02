#!/bin/sh
# Build dist/COAT.app and dist/COAT-macOS.zip.
#
# Universal (Apple Silicon + Intel) when both Rust targets are installed:
#   rustup target add aarch64-apple-darwin x86_64-apple-darwin
# otherwise just this Mac's architecture.
set -eu
cd "$(dirname "$0")/../.."

VERSION=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n 1)
APP=dist/COAT.app
installed=$(rustup target list --installed 2>/dev/null || true)

binaries=""
for target in aarch64-apple-darwin x86_64-apple-darwin; do
  if echo "$installed" | grep -qx "$target"; then
    cargo build --release --locked --target "$target"
    binaries="$binaries target/$target/release/coat"
  fi
done
if [ -z "$binaries" ]; then
  cargo build --release --locked
  binaries="target/release/coat"
fi

rm -rf "$APP" dist/COAT-macOS.zip
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
# shellcheck disable=SC2086 # word splitting is the point
lipo -create -output "$APP/Contents/MacOS/coat" $binaries
sed "s/__VERSION__/$VERSION/g" packaging/macos/Info.plist > "$APP/Contents/Info.plist"
cp packaging/macos/COAT.icns "$APP/Contents/Resources/COAT.icns"

# Ad-hoc signature: required for Apple Silicon to run it at all. It is not a
# Developer ID signature, so Gatekeeper still asks once on first open (see README).
codesign --force --deep --sign - "$APP"

(cd dist && ditto -c -k --keepParent COAT.app COAT-macOS.zip)
echo "Built $APP ($(lipo -archs "$APP/Contents/MacOS/coat")) and dist/COAT-macOS.zip, version $VERSION"
