#!/usr/bin/env bash
# Stamp a version into Cargo.toml and Cargo.lock for a release build (not committed).
# Everything else (the binary, COAT.exe's file details, the Mac app's Info.plist)
# reads the version from there.
set -euo pipefail
cd "$(dirname "$0")/.."

version=${1:?usage: set-version.sh X.Y.Z}
if ! [[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$ ]]; then
  echo "not a version: $version" >&2
  exit 1
fi

# The package's own version is the first line-leading `version =` in Cargo.toml.
# -i.bak (then remove the backup): some Windows Perl builds refuse in-place edits without one.
perl -0pi.bak -e 's/^version = "[^"]*"/version = "'"$version"'"/m' Cargo.toml
perl -0pi.bak -e 's/(\[\[package\]\]\r?\nname = "coat"\r?\nversion = )"[^"]*"/$1"'"$version"'"/' Cargo.lock
rm -f Cargo.toml.bak Cargo.lock.bak

grep -q "^version = \"$version\"" Cargo.toml || { echo "failed to set Cargo.toml version" >&2; exit 1; }
grep -A1 '^name = "coat"' Cargo.lock | grep -q "version = \"$version\"" || { echo "failed to set Cargo.lock version" >&2; exit 1; }
echo "version set to $version"
