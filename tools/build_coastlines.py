"""Convert Natural Earth coastline GeoJSON into orbitui's compact binary format.

Format (little endian): b"SWC1", u32 line count, then per line a u32 point
count followed by (i16 lon, i16 lat) pairs in hundredths of a degree.

    python tools/build_coastlines.py ne_110m_coastline.geojson assets/coast110.bin
"""
import json
import struct
import sys


def main(src, dst):
    with open(src, encoding="utf-8") as f:
        data = json.load(f)
    lines = []
    for feat in data["features"]:
        geom = feat["geometry"]
        parts = [geom["coordinates"]] if geom["type"] == "LineString" else geom["coordinates"]
        for part in parts:
            if len(part) >= 2:
                lines.append(part)
    with open(dst, "wb") as out:
        out.write(b"SWC1")
        out.write(struct.pack("<I", len(lines)))
        for line in lines:
            out.write(struct.pack("<I", len(line)))
            for lon, lat in line:
                out.write(struct.pack("<hh", round(lon * 100), round(lat * 100)))
    print(f"{dst}: {len(lines)} lines, {sum(map(len, lines))} points")


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
