"""Download current elements (OMM JSON) for every built-in satellite into
assets/elements_snapshot.json, the set orbitui uses until it can download.

    python tools/build_snapshot.py

NORAD ids are read from src/catalog.rs. Celestrak groups are fetched first,
then each missing satellite by id, slowly, as Celestrak asks.
"""
import json
import re
import time
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
GP = "https://celestrak.org/NORAD/elements/gp.php"
GROUPS = ["resource", "weather", "stations"]


def get(query):
    req = urllib.request.Request(f"{GP}?{query}&FORMAT=json", headers={"User-Agent": "orbitui-snapshot"})
    with urllib.request.urlopen(req, timeout=60) as r:
        body = r.read().decode()
    return json.loads(body) if body.lstrip().startswith("[") else []


def main():
    source = (ROOT / "src" / "catalog.rs").read_text(encoding="utf-8")
    ids = [int(n) for n in re.findall(r"\bsat\((\d+),", source)]
    found = {}
    for group in GROUPS:
        for record in get(f"GROUP={group}"):
            if record["NORAD_CAT_ID"] in ids:
                found[record["NORAD_CAT_ID"]] = record
    for norad in ids:
        if norad not in found:
            time.sleep(1)
            for record in get(f"CATNR={norad}"):
                found[norad] = record
    missing = [n for n in ids if n not in found]
    records = [found[n] for n in ids if n in found]
    out = ROOT / "assets" / "elements_snapshot.json"
    out.write_text(json.dumps(records, separators=(",", ":")) + "\n", encoding="utf-8")
    print(f"{out}: {len(records)} satellites; missing: {missing or 'none'}")


if __name__ == "__main__":
    main()
