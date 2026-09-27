#!/bin/sh
# Installs koi on Linux or macOS: the archive for this system from the latest release,
# checked against the release's SHA256SUMS. In Git Bash, MSYS2 or Cygwin on Windows it runs
# the PowerShell installer instead.
#
#   curl -fsSL https://jeremybyu.github.io/koi/install.sh | sh
#
# To uninstall, pass --uninstall:
#
#   curl -fsSL https://jeremybyu.github.io/koi/install.sh | sh -s -- --uninstall
#
# KOI_INSTALL_DIR picks the folder (default ~/.local/bin). KOI_VERSION picks a release,
# such as 0.5.0, instead of the latest.
set -eu

repo=https://github.com/JeremyBYU/koi
dir=${KOI_INSTALL_DIR:-$HOME/.local/bin}
uninstall=
for arg in "$@"; do
  case "$arg" in
    --uninstall) uninstall=1 ;;
    *) echo "Unknown option $arg. The one option is --uninstall." >&2; exit 2 ;;
  esac
done

case "$(uname -s)" in
  Linux)
    case "$(uname -m)" in
      x86_64 | amd64) target=x86_64-linux ;;
      aarch64 | arm64) target=aarch64-linux ;;
      *) echo "koi has no build for $(uname -m) Linux." >&2; exit 1 ;;
    esac
    ;;
  Darwin) target=macos ;;
  MINGW* | MSYS* | CYGWIN*)
    # Windows, from a Unix-like shell: the PowerShell installer knows where Windows programs
    # go and how to add them to the PATH. PowerShell 7 when it's installed, else the Windows
    # PowerShell that every Windows has.
    ps=$(command -v pwsh || echo powershell.exe)
    if [ -n "$uninstall" ]; then
      export KOI_UNINSTALL=1
    fi
    exec "$ps" -NoProfile -Command "irm https://jeremybyu.github.io/koi/install.ps1 | iex"
    ;;
  *) echo "koi has no build for $(uname -s). See $repo#install." >&2; exit 1 ;;
esac

if [ -n "$uninstall" ]; then
  if [ -e "$dir/koi" ]; then
    rm -f "$dir/koi"
    echo "Removed $dir/koi"
  else
    echo "There is no koi in $dir. If you installed it with KOI_INSTALL_DIR, set it again."
  fi
  # What koi writes by itself: the remembered scene and volume, and the loudness cache.
  rm -rf "${XDG_STATE_HOME:-$HOME/.local/state}/koi-pond" "${XDG_CACHE_HOME:-$HOME/.cache}/koi-pond"
  # What you wrote or downloaded stays: config.toml, your themes and the music pack.
  kept=
  for folder in "${XDG_CONFIG_HOME:-$HOME/.config}/koi-pond" "${XDG_DATA_HOME:-$HOME/.local/share}/koi-pond"; do
    if [ -d "$folder" ]; then
      kept="$kept \"$folder\""
    fi
  done
  if [ -n "$kept" ]; then
    echo "Kept your settings, themes and music. To remove them too: rm -r$kept"
  fi
  exit 0
fi

if [ -n "${KOI_VERSION:-}" ]; then
  version=${KOI_VERSION#v}
else
  # The latest release's page redirects to its tag, so no API call is needed.
  latest=$(curl -fsSLI -o /dev/null -w '%{url_effective}' "$repo/releases/latest")
  version=${latest##*/v}
fi
case "$version" in
  [0-9]*) ;;
  *) echo "Could not find the latest koi release." >&2; exit 1 ;;
esac

archive=koi-$version-$target.tar.gz
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
# Ctrl-C, or the reader of our output going away, exits through the EXIT trap too.
trap 'exit 1' HUP INT PIPE TERM

echo "Downloading koi $version for $target"
curl -fsSL -o "$work/$archive" "$repo/releases/download/v$version/$archive"
curl -fsSL -o "$work/SHA256SUMS" "$repo/releases/download/v$version/SHA256SUMS"

expected=$(grep " $archive\$" "$work/SHA256SUMS" | cut -d ' ' -f 1)
if command -v sha256sum >/dev/null 2>&1; then
  actual=$(sha256sum "$work/$archive" | cut -d ' ' -f 1)
else
  actual=$(shasum -a 256 "$work/$archive" | cut -d ' ' -f 1)
fi
if [ -z "$expected" ] || [ "$expected" != "$actual" ]; then
  echo "The download does not match the release's SHA256SUMS, so nothing was installed." >&2
  exit 1
fi

tar -xzf "$work/$archive" -C "$work"
mkdir -p "$dir"
# Move rather than copy over, so a koi that is running keeps its old file.
cp "$work/koi-$version-$target/koi" "$dir/koi.new"
chmod +x "$dir/koi.new"
mv -f "$dir/koi.new" "$dir/koi"
echo "Installed koi $version to $dir/koi"

case ":$PATH:" in
  *":$dir:"*) ;;
  *) echo "$dir is not on your PATH. Add this line to your shell's startup file: export PATH=\"$dir:\$PATH\"" ;;
esac
if [ "$target" != macos ] && ! { { ldconfig -p || /sbin/ldconfig -p; } 2>/dev/null | grep -q libasound.so.2; }; then
  echo "koi needs libasound.so.2 for sound, and it was not found. Install your system's alsa-lib (libasound2 on Debian and Ubuntu)."
fi
echo "Run it with: koi"
