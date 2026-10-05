#!/usr/bin/env bash
# Tests for next-version.sh, in a throwaway git repository.
set -euo pipefail
here=$(cd "$(dirname "$0")/.." && pwd)
repo=$(mktemp -d)
trap 'rm -rf "$repo"' EXIT
mkdir -p "$repo/packaging"
cp "$here/packaging/next-version.sh" "$repo/packaging/"
printf '[package]\nname = "coat"\nversion = "0.2.0"\n' > "$repo/Cargo.toml"
cd "$repo"
git init -q && git config user.email test@example.test && git config user.name test
git add -A && git commit -qm "initial"

failures=0
expect() {   # expect <description> <expected> <args…>
  local what=$1 want=$2; shift 2
  local got; got=$(./packaging/next-version.sh "$@")
  if [ "$got" = "$want" ]; then echo "ok   $what: $got"; else echo "FAIL $what: got $got, want $want"; failures=$((failures + 1)); fi
}
commit() { git commit -q --allow-empty -m "$1"; }

expect "no tags yet" 0.2.0
expect "no tags yet, preview" 0.2.0-preview.1 --preview
git tag v0.2.0
commit "fix: a typo";                   expect "plain fix → patch" 0.2.1
expect "first preview of 0.2.1" 0.2.1-preview.1 --preview
git tag v0.2.1-preview.1
expect "second preview counts up" 0.2.1-preview.2 --preview
git tag v0.2.1-preview.2
expect "previews don't move stable" 0.2.1
commit "Add a view #Minor";             expect "#Minor (any case) → minor" 0.3.0
expect "preview restarts at .1 for a new version" 0.3.0-preview.1 --preview
commit "feat: new thing";               expect "feat → minor" 0.3.0
commit "Merge pull request #7 from x/y #MAJOR"; expect "#MAJOR → major" 1.0.0
git tag v1.0.0
commit "chore: tidy #patch";            expect "#patch → patch" 1.0.1
commit "BREAKING CHANGE: new format";   expect "BREAKING CHANGE → major" 2.0.0
sed -i.bak 's/^version = "0.2.0"/version = "3.0.0"/' Cargo.toml && rm -f Cargo.toml.bak
expect "Cargo.toml raises the floor" 3.0.0
expect "floor applies to previews" 3.0.0-preview.1 --preview

[ "$failures" -eq 0 ] && echo "all version tests passed" || { echo "$failures version test(s) failed"; exit 1; }
