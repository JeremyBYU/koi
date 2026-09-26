#!/usr/bin/env bash
# Usage: scripts/release.sh [--music]
# Builds the release archives for the machine it runs on, into dist/:
#
# On Linux, koi-<version>-x86_64-linux.tar.gz and koi-<version>-aarch64-linux.tar.gz: one
# stripped binary each that runs on glibc 2.17 or newer and needs only libasound.so.2 besides
# libc. Vulkan is loaded at run time when present. Needs zig, cargo-zigbuild, jq, the rustup
# targets x86_64- and aarch64-unknown-linux-gnu, and ALSA for the host build (libasound2-dev,
# or the shim in docs/ARCHITECTURE.md). The cross builds link against a stub libasound.so with
# the ALSA symbols a host build uses, at their symbol versions, so no aarch64 ALSA is needed.
#
# On macOS, koi-<version>-macos.tar.gz: one universal binary for Apple silicon and Intel.
# Needs jq and the rustup targets aarch64- and x86_64-apple-darwin.
#
# --music also packs the full music set into dist/koi-music.tar.gz, which unpacks a music
# folder: beside the binary, or in ~/.local/share/koi-pond.
set -euo pipefail
cd "$(dirname "$0")/.."

music=0
case "${1:-}" in
  --music) music=1 ;;
  "") ;;
  *) echo "usage: $0 [--music]" >&2; exit 2 ;;
esac

version=$(cargo metadata --no-deps --format-version 1 | jq -r '.packages[] | select(.name == "koi-pond") | .version')
work=target/release-work
mkdir -p "$work" dist

# Writes $work/THIRD-PARTY.md: the license of every crate in a build for target $1.
notices() {
  cargo tree -p koi-pond -e normal --target "$1" --prefix none --format '{p}' | awk '!/koi-/ { print $1, substr($2, 2) }' | sort -u > "$work/crates"
  cargo metadata --format-version 1 --filter-platform "$1" > "$work/metadata.json"
  python3 - "$work/crates" "$work/metadata.json" > "$work/THIRD-PARTY.md" <<'PY'
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
}

# Packs the binary $1 with the docs into dist/$2.tar.gz.
pack() {
  rm -rf "dist/$2"
  mkdir "dist/$2"
  cp "$1" README.md LICENSE-MIT LICENSE-APACHE "$work/THIRD-PARTY.md" "dist/$2/"
  tar -C dist -czf "dist/$2.tar.gz" "$2"
  rm -r "dist/$2"
}

case "$(uname -s)" in
  Linux)
    echo "host build, for the ALSA symbols"
    cargo build --release -p koi-pond
    nm -D --undefined-only target/release/koi | awk '$2 ~ /^snd_/ { split($2, s, "@"); print s[1], s[2] }' | sort -u > "$work/symbols"
    awk '{ print "void " $1 "(void) {}" }' "$work/symbols" > "$work/stub.c"
    awk '{ nodes[$2] = nodes[$2] " " $1 ";" } END { for (v in nodes) print v " { global:" nodes[v] " };" }' "$work/symbols" > "$work/libasound.map"
    notices x86_64-unknown-linux-gnu
    for arch in x86_64 aarch64; do
      mkdir -p "$work/$arch"
      zig cc -target "$arch-linux-gnu" -shared -fPIC -Wl,--version-script="$work/libasound.map" -Wl,-soname,libasound.so.2 -o "$work/$arch/libasound.so" "$work/stub.c"
      printf 'libdir=${pcfiledir}\nName: alsa\nDescription: link stub\nVersion: 1.2.0\nLibs: -L${libdir} -lasound\nCflags:\n' > "$work/$arch/alsa.pc"
      echo "$arch build"
      PKG_CONFIG_PATH="$PWD/$work/$arch" PKG_CONFIG_ALLOW_CROSS=1 cargo zigbuild --profile dist -p koi-pond --target "$arch-unknown-linux-gnu.2.17"
      pack "target/$arch-unknown-linux-gnu/dist/koi" "koi-$version-$arch-linux"
    done
    ;;
  Darwin)
    notices aarch64-apple-darwin
    for arch in aarch64 x86_64; do
      echo "$arch build"
      cargo build --profile dist -p koi-pond --target "$arch-apple-darwin"
    done
    lipo -create -output "$work/koi" target/aarch64-apple-darwin/dist/koi target/x86_64-apple-darwin/dist/koi
    pack "$work/koi" "koi-$version-macos"
    ;;
  *) echo "release.sh builds on Linux or macOS" >&2; exit 1 ;;
esac

if [[ $music == 1 ]]; then
  echo "music archive"
  tmp=$(mktemp -d)
  mkdir "$tmp/music"
  scripts/fetch-music.sh --dest "$tmp/music"
  tar -C "$tmp" -czf dist/koi-music.tar.gz music
  rm -r "$tmp"
fi

ls -l dist
