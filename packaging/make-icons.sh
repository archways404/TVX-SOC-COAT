#!/bin/sh
# Regenerate every icon file from the two 1024 px PNGs (macOS only: uses sips + iconutil).
#
#   icon.svg       → icon-1024.png        the detailed coat, used from 64 px up
#   icon-small.svg → icon-small-1024.png  bolder and simpler, used at 16–32 px
#
# Export the SVGs to those PNGs first (any SVG tool, or open them in a browser and screenshot
# at 1024×1024 with a transparent background), then run this script.
set -eu
cd "$(dirname "$0")"

pick() { if [ "$1" -le 32 ]; then echo icon-small-1024.png; else echo icon-1024.png; fi; }

# macOS: COAT.icns
rm -rf COAT.iconset && mkdir COAT.iconset
for s in 16 32 128 256 512; do
  sips -z "$s" "$s" "$(pick "$s")" --out "COAT.iconset/icon_${s}x${s}.png" >/dev/null
  d=$((s * 2))
  sips -z "$d" "$d" "$(pick "$d")" --out "COAT.iconset/icon_${s}x${s}@2x.png" >/dev/null
done
iconutil -c icns COAT.iconset -o macos/COAT.icns
rm -rf COAT.iconset

# Windows: coat.ico (PNG-compressed entries, which Windows Vista and later read)
rm -rf ico-tmp && mkdir ico-tmp
for s in 16 24 32 48 64 256; do
  sips -z "$s" "$s" "$(pick "$s")" --out "ico-tmp/$s.png" >/dev/null
done
python3 - <<'EOF'
import struct
sizes = [16, 24, 32, 48, 64, 256]
pngs = [open(f"ico-tmp/{s}.png", "rb").read() for s in sizes]
offset, entries, data = 6 + 16 * len(sizes), b"", b""
for size, png in zip(sizes, pngs):
    entries += struct.pack("<BBBBHHII", size % 256, size % 256, 0, 0, 1, 32, len(png), offset + len(data))
    data += png
open("windows/coat.ico", "wb").write(struct.pack("<HHH", 0, 1, len(sizes)) + entries + data)
EOF
rm -rf ico-tmp

# Web UI: favicon and landing-page logo (compiled into the binary)
sips -z 64 64 icon-small-1024.png --out favicon-64.png >/dev/null
sips -z 160 160 icon-1024.png --out logo-160.png >/dev/null

echo "icons regenerated: macos/COAT.icns windows/coat.ico favicon-64.png logo-160.png"
