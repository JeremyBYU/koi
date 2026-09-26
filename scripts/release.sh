#!/usr/bin/env bash
# Usage: scripts/release.sh [--music]
# Builds dist/koi-<version>-<arch>-linux.tar.gz for x86_64 and aarch64: one stripped binary
# that runs on glibc 2.17 or newer and needs only libasound.so.2 besides libc. Vulkan is
# loaded at run time when present. --music also packs the tracks into dist/koi-music.tar.gz,
# which unpacks a music folder beside the binary.
#
# Needs zig, cargo-zigbuild, jq, the rustup targets x86_64- and aarch64-unknown-linux-gnu,
# and ALSA for the host build (libasound2-dev, or the shim in docs/ARCHITECTURE.md).
#
# The cross builds link against a stub libasound.so with the ALSA symbols a host build uses,
# at their symbol versions, so no aarch64 ALSA is needed. The real library is loaded at run
# time.
set -euo pipefail
cd "$(dirname "$0")/.."

music=0
case "${1:-}" in
  --music) music=1 ;;
  "") ;;
  *) echo "usage: $0 [--music]" >&2; exit 2 ;;
esac

version=$(cargo metadata --no-deps --format-version 1 | jq -r '.packages[] | select(.name == "koi-pond") | .version')
stub=target/alsa-stub
mkdir -p "$stub" dist

echo "host build, for the ALSA symbols"
cargo build --release -p koi-pond
nm -D --undefined-only target/release/koi | awk '$2 ~ /^snd_/ { split($2, s, "@"); print s[1], s[2] }' | sort -u > "$stub/symbols"
awk '{ print "void " $1 "(void) {}" }' "$stub/symbols" > "$stub/stub.c"
awk '{ nodes[$2] = nodes[$2] " " $1 ";" } END { for (v in nodes) print v " { global:" nodes[v] " };" }' "$stub/symbols" > "$stub/libasound.map"

echo "third-party notices"
cargo tree -p koi-pond -e normal --target x86_64-unknown-linux-gnu --prefix none --format '{p}' | awk '!/koi-/ { print $1, substr($2, 2) }' | sort -u > "$stub/crates"
cargo metadata --format-version 1 --filter-platform x86_64-unknown-linux-gnu > "$stub/metadata.json"
python3 - "$stub/crates" "$stub/metadata.json" > "$stub/THIRD-PARTY.md" <<'PY'
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

for arch in x86_64 aarch64; do
  mkdir -p "$stub/$arch"
  zig cc -target "$arch-linux-gnu" -shared -fPIC -Wl,--version-script="$stub/libasound.map" -Wl,-soname,libasound.so.2 -o "$stub/$arch/libasound.so" "$stub/stub.c"
  printf 'libdir=${pcfiledir}\nName: alsa\nDescription: link stub\nVersion: 1.2.0\nLibs: -L${libdir} -lasound\nCflags:\n' > "$stub/$arch/alsa.pc"

  echo "$arch build"
  PKG_CONFIG_PATH="$PWD/$stub/$arch" PKG_CONFIG_ALLOW_CROSS=1 cargo zigbuild --profile dist -p koi-pond --target "$arch-unknown-linux-gnu.2.17"

  name="koi-$version-$arch-linux"
  rm -rf "dist/$name"
  mkdir "dist/$name"
  cp "target/$arch-unknown-linux-gnu/dist/koi" README.md LICENSE-MIT LICENSE-APACHE "$stub/THIRD-PARTY.md" "dist/$name/"
  tar -C dist -czf "dist/$name.tar.gz" "$name"
  rm -r "dist/$name"
done

if [[ $music == 1 ]]; then
  echo "music archive"
  tmp=$(mktemp -d)
  mkdir "$tmp/music"
  scripts/fetch-music.sh --dest "$tmp/music"
  tar -C "$tmp" -czf dist/koi-music.tar.gz music
  rm -r "$tmp"
fi

(cd dist && sha256sum -- *.tar.gz > SHA256SUMS)
ls -l dist
