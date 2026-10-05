#!/bin/sh
# Regenerate every icon file from the 1024 px PNGs (macOS only: uses sips + iconutil).
#
#   icon.svg               → icon-1024.png                the detailed coat, used from 64 px up
#   icon-small.svg         → icon-small-1024.png          bolder and simpler, used at 16–32 px
#   icon-preview.svg       → icon-preview-1024.png        the same for COAT Preview (green, PREVIEW band)
#   icon-small-preview.svg → icon-small-preview-1024.png
#
# Export the SVGs to those PNGs first (any SVG tool, or open them in a browser and screenshot
# at 1024×1024 with a transparent background), then run this script.
set -eu
cd "$(dirname "$0")"

# make_set <detailed 1024 png> <simple 1024 png> <icns> <ico> <favicon> <logo>
make_set() {
  big=$1 small=$2
  pick() { if [ "$1" -le 32 ]; then echo "$small"; else echo "$big"; fi; }

  # macOS: .icns
  rm -rf COAT.iconset && mkdir COAT.iconset
  for s in 16 32 128 256 512; do
    sips -z "$s" "$s" "$(pick "$s")" --out "COAT.iconset/icon_${s}x${s}.png" >/dev/null
    d=$((s * 2))
    sips -z "$d" "$d" "$(pick "$d")" --out "COAT.iconset/icon_${s}x${s}@2x.png" >/dev/null
  done
  iconutil -c icns COAT.iconset -o "$3"
  rm -rf COAT.iconset

  # Windows: .ico (PNG-compressed entries, which Windows Vista and later read)
  rm -rf ico-tmp && mkdir ico-tmp
  for s in 16 24 32 48 64 256; do
    sips -z "$s" "$s" "$(pick "$s")" --out "ico-tmp/$s.png" >/dev/null
  done
  python3 - "$4" <<'EOF'
import struct, sys
sizes = [16, 24, 32, 48, 64, 256]
pngs = [open(f"ico-tmp/{s}.png", "rb").read() for s in sizes]
offset, entries, data = 6 + 16 * len(sizes), b"", b""
for size, png in zip(sizes, pngs):
    entries += struct.pack("<BBBBHHII", size % 256, size % 256, 0, 0, 1, 32, len(png), offset + len(data))
    data += png
open(sys.argv[1], "wb").write(struct.pack("<HHH", 0, 1, len(sizes)) + entries + data)
EOF
  rm -rf ico-tmp

  # Web UI: favicon and landing-page logo (compiled into the binary)
  sips -z 64 64 "$small" --out "$5" >/dev/null
  sips -z 160 160 "$big" --out "$6" >/dev/null
  echo "regenerated $3 $4 $5 $6"
}

make_set icon-1024.png icon-small-1024.png macos/COAT.icns windows/coat.ico favicon-64.png logo-160.png
make_set icon-preview-1024.png icon-small-preview-1024.png macos/COAT-Preview.icns windows/coat-preview.ico \
         favicon-preview-64.png logo-preview-160.png
