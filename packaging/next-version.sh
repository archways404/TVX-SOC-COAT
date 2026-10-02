#!/usr/bin/env bash
# Print the version the next release should get.
#
# Starts from the newest vX.Y.Z tag and bumps it based on the commits since:
#   "#major" or "BREAKING CHANGE" in a commit message  → major (1.4.2 → 2.0.0)
#   a commit starting with "feat", or "#minor"          → minor (1.4.2 → 1.5.0)
#   anything else                                       → patch (1.4.2 → 1.4.3)
# The version in Cargo.toml is a floor: set it higher by hand to jump (e.g. to 1.0.0).
# With no tags yet, the Cargo.toml version is used as is.
set -euo pipefail
cd "$(dirname "$0")/.."

floor=$(sed -n 's/^version = "\(.*\)"\r\{0,1\}$/\1/p' Cargo.toml | head -n 1)
latest=$(git tag --list 'v[0-9]*.[0-9]*.[0-9]*' --sort=-v:refname | head -n 1)

if [ -z "$latest" ]; then
  echo "$floor"
  exit 0
fi

IFS=. read -r major minor patch <<< "${latest#v}"
messages=$(git log --format=%B "$latest"..HEAD)
if grep -qE '#major|BREAKING CHANGE' <<< "$messages"; then
  major=$((major + 1)); minor=0; patch=0
elif grep -qE '^feat|#minor' <<< "$messages"; then
  minor=$((minor + 1)); patch=0
else
  patch=$((patch + 1))
fi
next="$major.$minor.$patch"

highest=$(printf '%s\n%s\n' "$next" "$floor" | sort -V | tail -n 1)
echo "$highest"
