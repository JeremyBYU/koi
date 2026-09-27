#!/usr/bin/env bash
# Usage: scripts/build-site.sh [out-dir]
# Builds the web page into out-dir (default target/site): the pages in site/, koi-web
# compiled to wasm with its JavaScript bindings, the three built-in tracks, and the guide to
# the code from docs/guide at guide/. Serve it with any static server, such as
# `python3 -m http.server -d target/site`.
#
# Needs the rustup target wasm32-unknown-unknown, wasm-bindgen-cli at the version of the
# wasm-bindgen crate in Cargo.lock, ffmpeg and jq. wasm-opt (binaryen), when on the PATH,
# optimises the wasm further.
#
# The tracks are the Ogg Vorbis files built into the binary, with an AAC copy of each for
# browsers without Vorbis (Safari before 18.4). The page lists both and the browser fetches
# the one it plays. music/tracks.json gives each its title and a gain that brings it to
# -20 LUFS, less where that would push its true peak over -1 dBFS, as the terminal evens out
# its tracks.
set -euo pipefail
cd "$(dirname "$0")/.."
out=${1:-target/site}

want=$(awk '/^name = "wasm-bindgen"$/ { getline; gsub(/[^0-9.]/, ""); print; exit }' Cargo.lock)
have=$(wasm-bindgen --version | awk '{ print $2 }')
if [[ $have != "$want" ]]; then
  echo "wasm-bindgen $have does not match the crate's $want: cargo install --locked wasm-bindgen-cli --version $want" >&2
  exit 1
fi

cargo build --profile web -p koi-web --target wasm32-unknown-unknown
mkdir -p "$out/pkg" "$out/music"
cp site/* "$out/"
wasm-bindgen --target web --no-typescript --out-dir "$out/pkg" target/wasm32-unknown-unknown/web/koi_web.wasm
if command -v wasm-opt >/dev/null; then
  # Current rustc emits these features, which wasm-opt leaves off unless told.
  wasm-opt -O3 --enable-bulk-memory --enable-nontrapping-float-to-int --enable-sign-ext --enable-mutable-globals --enable-reference-types --enable-multivalue \
    -o "$out/pkg/koi_web_bg.wasm" "$out/pkg/koi_web_bg.wasm"
fi

tracks=()
for ogg in assets/music/builtin/*.ogg; do
  name=$(basename "$ogg" .ogg)
  cp "$ogg" "$out/music/"
  ffmpeg -loglevel error -y -i "$ogg" -vn -c:a aac -b:a 96k "$out/music/$name.m4a"
  read -r lufs peak < <(ffmpeg -hide_banner -nostats -i "$ogg" -af ebur128=peak=true -f null - 2>&1 |
    awk '/^ *I:/ { i = $2 } /^ *Peak:/ { p = $2 } END { print i, p }')
  title=$(jq -r --arg file "$name.mp3" '.[] | select(.file == $file) | .title' assets/music/tracks.json)
  tracks+=("$(jq -n --arg title "$title" --arg file "$name" --argjson lufs "$lufs" --argjson peak "$peak" \
    '{ title: $title, file: $file, gain: (pow(10; ([-20 - $lufs, -1 - $peak] | min) / 20) * 1000 | round / 1000) }')")
done
printf '%s\n' "${tracks[@]}" | jq -s . > "$out/music/tracks.json"
cp assets/music/CREDITS.md "$out/music/"
scripts/third-party.sh koi-web wasm32-unknown-unknown > "$out/THIRD-PARTY.md"
mkdir -p "$out/guide"
cp docs/guide/index.html "$out/guide/"
ls -l "$out" "$out/pkg"
