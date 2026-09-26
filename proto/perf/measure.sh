#!/usr/bin/env bash
# Usage: measure.sh <label> <outdir> <command...>
# Opens the command in a fixed-size Ghostty window (XWayland so it can be captured),
# drops food twice, then reports Ghostty CPU, game CPU (percent of one core, 10 s avg),
# distinct frames in a 3 s window recording, and saves a screenshot.
set -u
label=$1; out=$2; shift 2
mkdir -p "$out"
GDK_BACKEND=x11 ghostty --title="koi-$label" --window-width=160 --window-height=45 -e "$@" >/dev/null 2>&1 &
gpid=$!
sleep 3
win=$(DISPLAY=:1 xdotool search --name "koi-$label" | head -1)
game=$(pgrep -n -f "$1")
DISPLAY=:1 xdotool mousemove --window "$win" 700 600 click 1 sleep 0.5 mousemove --window "$win" 1200 900 click 1
tmpg=$(mktemp); tmpc=$(mktemp)
( pidstat -p "$gpid" 10 1 | awk '/Average/{print $8}' > "$tmpg" ) & p1=$!
( pidstat -p "$game" 10 1 | awk '/Average/{print $8}' > "$tmpc" ) & p2=$!
wait "$p1" "$p2"
DISPLAY=:1 xdotool mousemove --window "$win" 900 700 click 1
DISPLAY=:1 ffmpeg -loglevel quiet -y -f x11grab -window_id "$win" -framerate 60 -i :1 -t 3 -c:v ffv1 "$out/$label.mkv"
total=$(ffprobe -v error -count_frames -show_entries stream=nb_read_frames -of csv=p=0 "$out/$label.mkv")
distinct=$(ffmpeg -i "$out/$label.mkv" -vf mpdecimate=hi=64:lo=32:frac=0.05 -f null - 2>&1 | grep -o 'frame= *[0-9]*' | tail -1 | tr -dc 0-9)
ffmpeg -loglevel quiet -y -i "$out/$label.mkv" -vf "select=eq(n\,$((total/2)))" -frames:v 1 "$out/$label.png"
kill "$gpid" 2>/dev/null; sleep 1
echo "{\"label\":\"$label\",\"ghostty_cpu\":$(cat $tmpg),\"game_cpu\":$(cat $tmpc),\"captured\":$total,\"distinct\":$distinct,\"screenshot\":\"$out/$label.png\"}"
rm -f "$tmpg" "$tmpc"
