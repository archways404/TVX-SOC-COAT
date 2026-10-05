#!/usr/bin/env bash
# End-to-end test of automatic updates, on macOS or Windows (Git Bash).
#
# Builds an "old" COAT (0.0.1) and a "new" one (0.0.2), publishes the new one as a fake
# GitHub release on this computer, starts the old one, and checks that it finds the
# update, downloads and verifies it, swaps itself, restarts as 0.0.2, and cleans up.
# Uses its own port and settings folder, so a COAT you have running isn't touched.
set -euo pipefail
cd "$(dirname "$0")/.."

case "$(uname -s)" in
  Darwin) os=mac ;;
  MINGW*|MSYS*|CYGWIN*) os=windows ;;
  *) echo "update test: only runs on macOS and Windows"; exit 0 ;;
esac

old=0.0.1 new=0.0.2 port=7350 release_port=8765
work=$(mktemp -d)
api="http://127.0.0.1:$port"
cp Cargo.toml "$work/Cargo.toml.orig"; cp Cargo.lock "$work/Cargo.lock.orig"
server_pid=""
cleanup() {
  curl -fsS -X POST "$api/api/quit" >/dev/null 2>&1 || true
  if [ -n "$server_pid" ]; then kill "$server_pid" 2>/dev/null || true; wait "$server_pid" 2>/dev/null || true; fi
  cp "$work/Cargo.toml.orig" Cargo.toml; cp "$work/Cargo.lock.orig" Cargo.lock
  rm -rf "$work"
}
trap cleanup EXIT

export COAT_RELEASE_BUILD=1 COAT_NATIVE_ONLY=1
mkdir -p "$work/apps" "$work/release"

build() {   # build <version>
  ./packaging/set-version.sh "$1" >/dev/null
  if [ "$os" = mac ]; then ./packaging/macos/build-app.sh >/dev/null; else cargo build --release --locked -q; fi
}

echo "== building the old version ($old)"
build "$old"
if [ "$os" = mac ]; then
  cp -R dist/COAT.app "$work/apps/COAT.app"; installed="$work/apps/COAT.app/Contents/MacOS/coat"; asset=COAT-macOS.zip
else
  cp target/release/coat.exe "$work/apps/COAT.exe"; installed="$work/apps/COAT.exe"; asset=COAT.exe
fi

echo "== building the new version ($new) and publishing it as a fake release"
build "$new"
if [ "$os" = mac ]; then cp dist/COAT-macOS.zip "$work/release/"; else cp target/release/coat.exe "$work/release/COAT.exe"; fi
( cd "$work/release"
  if command -v sha256sum >/dev/null; then sha256sum "$asset"; else shasum -a 256 "$asset"; fi > SHA256SUMS.txt
  cat > release.json <<JSON
{"tag_name": "v$new", "html_url": "http://127.0.0.1:$release_port/",
 "assets": [{"name": "$asset", "browser_download_url": "http://127.0.0.1:$release_port/$asset"},
            {"name": "SHA256SUMS.txt", "browser_download_url": "http://127.0.0.1:$release_port/SHA256SUMS.txt"}]}
JSON
)
python=$(command -v python3 || command -v python)
"$python" -m http.server "$release_port" --bind 127.0.0.1 --directory "$work/release" >/dev/null 2>&1 &
server_pid=$!

json() { "$python" -c "import sys,json; print(json.load(sys.stdin).get('$1',''))"; }
wait_for() {   # wait_for <seconds> <description> <command…>
  local deadline=$((SECONDS + $1)) what=$2; shift 2
  until "$@"; do
    if [ "$SECONDS" -ge "$deadline" ]; then echo "FAILED: timed out waiting for $what"; exit 1; fi
    sleep 1
  done
}

echo "== starting the old version"
export COAT_PORT=$port COAT_NO_BROWSER=1 COAT_SETTINGS_DIR="$work/settings"
export COAT_UPDATE_URL="http://127.0.0.1:$release_port/release.json"
"$installed"
version_is() { [ "$(curl -fsS "$api/api/ping" 2>/dev/null | json version)" = "$1" ]; }
wait_for 15 "the old version to answer" version_is "$old"

echo "== checking for the update (it downloads by itself)"
curl -fsS -X POST "$api/api/update/check" >/dev/null
state_is() { [ "$(curl -fsS "$api/api/update" 2>/dev/null | json state)" = "$1" ]; }
wait_for 60 "the update to be downloaded and verified" state_is ready

echo "== installing: COAT swaps itself and restarts"
curl -fsS -X POST "$api/api/update/install" >/dev/null
wait_for 60 "the new version to answer" version_is "$new"

echo "== checking what's on disk"
on_disk=$("$installed" --version)
[ "$on_disk" = "coat $new" ] || { echo "FAILED: installed program reports '$on_disk'"; exit 1; }
leftovers_gone() { ! ls -A "$work/apps" | grep -qiE '\.COAT-old|\.old\.exe|\.new\.exe|\.COAT-update'; }
wait_for 20 "the old copy to be cleaned up" leftovers_gone

echo "PASS: $old updated itself to $new"
