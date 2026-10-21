"""orbitui logo: a globe with real continents (Natural Earth), a satellite
and the cone of its swath on Brazil; the name alongside.

    pip install pyfiglet
    curl -LO https://raw.githubusercontent.com/nvkelso/natural-earth-vector/master/geojson/ne_110m_land.geojson
    python tools/build_logo.py ne_110m_land.geojson '{"foot":[23,8],"half":2,"sat":[6,1]}'

The optional JSON overrides the layout (view centre, sizes, satellite and
footprint cells). Cells are drawn for GitHub's code blocks (about 1:2.4).
"""
import json
import math
import sys

import pyfiglet

RINGS = []
for f in json.load(open(sys.argv[1], encoding="utf-8"))["features"]:
    g = f["geometry"]
    for p in [g["coordinates"]] if g["type"] == "Polygon" else g["coordinates"]:
        RINGS.extend(p)


def is_land(lat, lon):
    inside = False
    for ring in RINGS:
        j = len(ring) - 1
        for i in range(len(ring)):
            (xa, ya), (xb, yb) = ring[i], ring[j]
            if (ya > lat) != (yb > lat) and lon < (xb - xa) * (lat - ya) / (yb - ya) + xa:
                inside = not inside
            j = i
    return inside


P = dict(lat0=-12, lon0=-42, ry=6.0, rx=14.4, cy=8.5, cx=26.0, h=16, w=44,
         sat=(5, 1), foot=(18, 8), half=3, lit="#", dark=":")
if len(sys.argv) > 2:
    P.update(json.loads(sys.argv[2]))
LAT0, LON0 = math.radians(P["lat0"]), math.radians(P["lon0"])
RY, RX, CY, CX, H, W = P["ry"], P["rx"], P["cy"], P["cx"], P["h"], P["w"]
SUN = (-0.55, 0.50, 0.67)
canvas = [[" "] * W for _ in range(H)]


def put(r, c, ch):
    if 0 <= r < H and 0 <= c < W:
        canvas[r][c] = ch


def unproject(x, y, z):
    sl, cl = math.sin(LAT0), math.cos(LAT0)
    lat = math.asin(max(-1, min(1, y * cl + z * sl)))
    lon = LON0 + math.atan2(x, z * cl - y * sl)
    return math.degrees(lat), math.degrees((lon + math.pi) % (2 * math.pi) - math.pi)


def outline():
    """One character per column on flat stretches, one per row on steep
    ones, picked by where the curve crosses the cell."""
    n = 4000
    pts = [(CX + RX * math.cos(2 * math.pi * k / n), CY - RY * math.sin(2 * math.pi * k / n)) for k in range(n)]
    runs = []
    for i in range(n):
        (x, y), (x2, y2) = pts[i], pts[(i + 1) % n]
        kind = "s" if abs(y2 - y) > abs(x2 - x) * 0.55 else "f"
        if runs and runs[-1][0] == kind:
            runs[-1][1].append(i)
        else:
            runs.append((kind, [i]))
    for kind, idx in runs:
        best = {}
        for i in idx:
            (x, y), (x2, y2) = pts[i], pts[(i + 1) % n]
            if kind == "f":
                key, err = round(x), abs(x - round(x))
                if key not in best or err < best[key][0]:
                    r = math.floor(y + 0.5 - (1e-6 if y > CY else 0.0))
                    frac = y + 0.5 - r
                    ch = "'" if frac < 0.3 else "-" if frac < 0.55 else "." if frac < 0.8 else "_"
                    best[key] = (err, r, key, ch)
            else:
                key, err = round(y), abs(y - round(y))
                if key not in best or err < best[key][0]:
                    dx, dy = x2 - x, y2 - y
                    ratio = abs(dx / dy) if dy else 9
                    ch = "|" if ratio < 0.85 else ("/" if dx * dy < 0 else "\\")
                    best[key] = (err, key, round(x), ch)
        for _, r, c, ch in best.values():
            put(r, c, ch)


for r in range(H):
    for c in range(W):
        x, y = (c - CX) / RX, (CY - r) / RY
        if x * x + y * y > 0.9:
            continue
        z = math.sqrt(1 - x * x - y * y)
        if is_land(*unproject(x, y, z)):
            light = x * SUN[0] + y * SUN[1] + z * SUN[2]
            put(r, c, P["lit"] if light > 0.3 else P["dark"])
outline()

# The swath: two edges from the satellite down to the footprint, in front.
sc, sr = P["sat"]
fc, fr = P["foot"]
for end in (fc - P["half"], fc + P["half"]):
    # One character per row: the cone's edges as dashed diagonals.
    x0, y0, x1, y1 = sc + 1, sr + 1, end, fr - 1
    for r in range(y0, y1 + 1):
        t = (r - y0) / max(y1 - y0, 1)
        c = round(x0 + (x1 - x0) * t)
        slope = (x1 - x0) / max(y1 - y0, 1)
        put(r, c, "\\" if slope > 0.6 else "|" if slope > -0.6 else "/")
for dc, ch in enumerate("=[O]="):
    put(sr, sc - 2 + dc, ch)
put(fr, fc - P["half"] - 1, "[")
for c in range(fc - P["half"], fc + P["half"] + 1):
    put(fr, c, "=")
put(fr, fc + P["half"] + 1, "]")

art = ["".join(row).rstrip() for row in canvas]
while art and not art[-1].strip():
    art.pop()
while art and not art[0].strip():
    art.pop(0)
name = [l.rstrip() for l in pyfiglet.figlet_format("orbitui", font="slant").splitlines() if l.strip()]
right = name + ["", "  when will a satellite image my area?"]
top = max(0, (len(art) - len(right)) // 2)
width = max(len(l) for l in art) + 4
print("\n".join((a.ljust(width) + (right[i - top] if 0 <= i - top < len(right) else "")).rstrip() for i, a in enumerate(art)))
