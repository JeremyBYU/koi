#!/usr/bin/env bash
# Usage: scripts/vshot.sh <out-dir> <seconds> <command...>
# Runs the command in Ghostty on a private Xvfb display so nothing appears on the
# real screen, then saves out-dir/shot.png and a 3 s out-dir/clip.mkv.
# XDG_CONFIG_HOME points at a config with audio off so tests never play through the
# speakers (a --config flag passed by the caller overrides this). XDG_STATE_HOME keeps
# the remembered theme out of the real ~/.local/state. TMUX is unset because the new Ghostty
# window is not inside the tmux this may be run from.
# Ghostty renders with software GL and the game's wgpu uses lavapipe (software Vulkan), so
# nothing touches the NVIDIA GPU that drives the real display. CPU numbers are not
# representative of real use.
set -u
out=$1; secs=$2; shift 2
mkdir -p "$out/config/koi-pond"
printf '[audio]\nenabled = false\n' > "$out/config/koi-pond/config.toml"
disp=:$((70 + RANDOM % 20))
Xvfb "$disp" -screen 0 2400x1600x24 -nolisten tcp >"$out/xvfb.log" 2>&1 &
xvfb=$!
sleep 1
env -u WAYLAND_DISPLAY -u TMUX -u TMUX_PANE DISPLAY="$disp" GDK_BACKEND=x11 LIBGL_ALWAYS_SOFTWARE=1 XDG_CONFIG_HOME="$out/config" XDG_STATE_HOME="$out/state" \
  VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json \
  ghostty --gtk-single-instance=false --title=koi-vshot --window-width=160 --window-height=45 -e "$@" >"$out/ghostty.log" 2>&1 &
ghostty=$!
sleep "$secs"
win=$(DISPLAY="$disp" xdotool search --name koi-vshot | head -1)
DISPLAY="$disp" import -window "$win" "$out/shot.png"
DISPLAY="$disp" ffmpeg -loglevel error -y -f x11grab -window_id "$win" -framerate 60 -i "$disp" -t 3 -c:v ffv1 "$out/clip.mkv"
kill "$ghostty" 2>/dev/null
sleep 1
kill "$xvfb" 2>/dev/null
