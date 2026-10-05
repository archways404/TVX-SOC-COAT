#!/usr/bin/env bash
# Print the version the next release should get.
#
#   ./packaging/next-version.sh             next stable version (master), e.g. 1.5.0
#   ./packaging/next-version.sh --preview   next preview version (preview branch), e.g. 1.5.0-preview.3
#
# Starts from the newest stable tag (vX.Y.Z) and bumps it based on the commit messages since
# then. Upper or lower case doesn't matter:
#   "#major" or "BREAKING CHANGE"            → major (1.4.2 → 2.0.0)
#   "#minor", or a message starting "feat"   → minor (1.4.2 → 1.5.0)
#   "#patch", or anything else               → patch (1.4.2 → 1.4.3)
# The version in Cargo.toml is a floor: set it higher by hand to jump (e.g. to 1.0.0). With no
# stable tag yet, the Cargo.toml version is used as is.
#
# A preview version is the version master would release next, plus "-preview.N", where N is one
# more than the highest preview already tagged for that version.
set -euo pipefail
cd "$(dirname "$0")/.."

channel=stable
[ "${1:-}" = "--preview" ] && channel=preview

floor=$(sed -n 's/^version = "\([0-9]*\.[0-9]*\.[0-9]*\).*"\r\{0,1\}$/\1/p' Cargo.toml | head -n 1)
latest=$(git tag --list 'v*' | grep -E '^v[0-9]+\.[0-9]+\.[0-9]+$' | sort -V | tail -n 1 || true)

if [ -z "$latest" ]; then
  next=$floor
else
  IFS=. read -r major minor patch <<< "${latest#v}"
  messages=$(git log --format=%B "$latest"..HEAD)
  if grep -qiE '#major|BREAKING CHANGE' <<< "$messages"; then
    major=$((major + 1)); minor=0; patch=0
  elif grep -qiE '^feat|#minor' <<< "$messages"; then
    minor=$((minor + 1)); patch=0
  else
    patch=$((patch + 1))
  fi
  next=$(printf '%s\n%s\n' "$major.$minor.$patch" "$floor" | sort -V | tail -n 1)
fi

if [ "$channel" = stable ]; then
  echo "$next"
  exit 0
fi

highest=$(git tag --list "v$next-preview.*" | sed -n "s/^v$next-preview\.\([0-9][0-9]*\)$/\1/p" | sort -n | tail -n 1)
echo "$next-preview.$(( ${highest:-0} + 1 ))"
