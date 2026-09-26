#!/usr/bin/env bash
# Downloads every track listed in assets/music/tracks.json that is not already there.
# --force downloads them all again. --dry-run only prints what it would fetch.
set -euo pipefail

dir="$(cd "$(dirname "$0")/.." && pwd)/assets/music"
force=0
dry_run=0
for arg in "$@"; do
  case "$arg" in
    --force) force=1 ;;
    --dry-run) dry_run=1 ;;
    *) echo "usage: $0 [--force] [--dry-run]" >&2; exit 2 ;;
  esac
done
command -v jq >/dev/null || { echo "fetch-music.sh needs jq" >&2; exit 1; }

jq -r '.[] | "\(.file)\t\(.download // "")"' "$dir/tracks.json" | while IFS=$'\t' read -r file url; do
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
