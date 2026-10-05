#!/bin/sh
# Build dist/COAT.app and dist/COAT_v<version>.zip, or with COAT_CHANNEL=preview,
# dist/COAT Preview.app and dist/COAT_v<version>_PREVIEW.zip (a separate app with its own name,
# bundle id and green icon, so it can be installed next to COAT).
#
# Universal (Apple Silicon + Intel) when both Rust targets are installed:
#   rustup target add aarch64-apple-darwin x86_64-apple-darwin
# otherwise just this Mac's architecture.
set -eu
cd "$(dirname "$0")/../.."

VERSION=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n 1)
if [ "${COAT_CHANNEL:-stable}" = preview ]; then
  NAME="COAT Preview" ID=se.telavox.coat.preview ICON=COAT-Preview ZIP="COAT_v${VERSION}_PREVIEW.zip"
else
  NAME="COAT" ID=se.telavox.coat ICON=COAT ZIP="COAT_v${VERSION}.zip"
fi
APP="dist/$NAME.app"
installed=$(rustup target list --installed 2>/dev/null || true)

binaries=""
# COAT_NATIVE_ONLY=1 builds just this Mac's architecture (quicker; used by the update test).
[ -n "${COAT_NATIVE_ONLY:-}" ] && installed=""
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

rm -rf "$APP" "dist/$ZIP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
# shellcheck disable=SC2086 # word splitting is the point
lipo -create -output "$APP/Contents/MacOS/coat" $binaries
# macOS wants plain 1.2.3 here; the full version (e.g. 1.2.3-preview.4) is inside the program.
sed -e "s/__VERSION__/${VERSION%%-*}/g" -e "s/__NAME__/$NAME/g" -e "s/__ID__/$ID/g" -e "s/__ICON__/$ICON/g" \
  packaging/macos/Info.plist > "$APP/Contents/Info.plist"
cp "packaging/macos/$ICON.icns" "$APP/Contents/Resources/$ICON.icns"

# Ad-hoc signature: required for Apple Silicon to run it at all. It is not a
# Developer ID signature, so Gatekeeper still asks once on first open (see README).
codesign --force --deep --sign - "$APP"

(cd dist && ditto -c -k --keepParent "$NAME.app" "$ZIP")
echo "Built $APP ($(lipo -archs "$APP/Contents/MacOS/coat")) and dist/$ZIP, version $VERSION"
