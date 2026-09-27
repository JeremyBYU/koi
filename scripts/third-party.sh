#!/usr/bin/env bash
# Usage: scripts/third-party.sh PACKAGE TARGET > THIRD-PARTY.md
# Prints the license of every crate in a build of PACKAGE for TARGET, for shipping beside it.
# Needs python3 (or PYTHON).
set -euo pipefail
cd "$(dirname "$0")/.."
work=$(mktemp -d)
trap 'rm -r "$work"' EXIT
cargo tree -p "$1" -e normal --target "$2" --prefix none --format '{p}' | awk '!/koi-/ { print $1, substr($2, 2) }' | sort -u > "$work/crates"
cargo metadata --format-version 1 --filter-platform "$2" > "$work/metadata.json"
PYTHONUTF8=1 "${PYTHON:-python3}" - "$work/crates" "$work/metadata.json" <<'PY'
import json, pathlib, sys
wanted = {tuple(line.split()) for line in open(sys.argv[1])}
packages = sorted((p for p in json.load(open(sys.argv[2]))["packages"] if (p["name"], p["version"]) in wanted), key=lambda p: p["name"])
print("# Third-party software\n\nkoi is built from these crates. Their licenses follow.\n")
for p in packages:
    print(f"- {p['name']} {p['version']}: {p['license']}")
for p in packages:
    root = pathlib.Path(p["manifest_path"]).parent
    texts = sorted(f for f in root.iterdir() if f.is_file() and f.name.upper().startswith(("LICENSE", "LICENCE", "COPYING", "NOTICE")))
    print(f"\n## {p['name']} {p['version']}\n")
    for f in texts:
        print(f"### {f.name}\n\n```\n{f.read_text(errors='replace').strip()}\n```\n")
    if not texts:
        print(f"License: {p['license']} (no license file in the crate)\n")
PY
