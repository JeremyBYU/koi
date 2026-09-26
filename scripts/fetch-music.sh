#!/usr/bin/env bash
# Downloads every track in assets/music/tracks.json that is not already there, into
# $XDG_DATA_HOME/koi-pond/music (or ~/.local/share/koi-pond/music), where koi looks first.
# The track list and credits are copied beside them.
# --dest DIR downloads into DIR instead. --force downloads them all again. --dry-run only
# prints what it would fetch.
set -euo pipefail

src="$(cd "$(dirname "$0")/.." && pwd)/assets/music"
dir="${XDG_DATA_HOME:-$HOME/.local/share}/koi-pond/music"
force=0
dry_run=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --dest) dir="${2:?--dest needs a directory}"; shift ;;
    --force) force=1 ;;
    --dry-run) dry_run=1 ;;
    *) echo "usage: $0 [--dest DIR] [--force] [--dry-run]" >&2; exit 2 ;;
  esac
  shift
done
command -v jq >/dev/null || { echo "fetch-music.sh needs jq" >&2; exit 1; }
command -v curl >/dev/null || { echo "fetch-music.sh needs curl" >&2; exit 1; }

if [[ $dry_run == 0 ]]; then
  mkdir -p "$dir"
  if [[ "$(cd "$dir" && pwd)" != "$src" ]]; then
    cp "$src/tracks.json" "$src/CREDITS.md" "$dir/"
  fi
fi
echo "music folder: $dir"
jq -r '.[] | "\(.file)\t\(.download // "")"' "$src/tracks.json" | while IFS=$'\t' read -r file url; do
  if [[ -f "$dir/$file" && $force == 0 ]]; then
    echo "have  $file"
  elif [[ -z "$url" ]]; then
    echo "skip  $file (no download URL in tracks.json)" >&2
  elif [[ $dry_run == 1 ]]; then
    echo "would fetch $file from $url"
  else
    echo "fetch $file"
    curl -fL --retry 3 -o "$dir/$file.part" "$url"
    mv "$dir/$file.part" "$dir/$file"
  fi
done
