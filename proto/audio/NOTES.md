# koi-audio

Audio prototype for the pond. It plays a folder of music with crossfades and
mixes a quiet generated ambient layer underneath. The game will drive it
through a small event API.

## Run it

```sh
cd proto/audio
cargo build --release

# live: plays assets/music (relative to the current directory) over the ambient layer
./target/release/koi-audio --music ../../assets/music

# offline: N seconds of the ambient layer plus five chimes, no speakers needed
./target/release/koi-audio --render-seconds 60 --out out/ambient-60s.wav
```

Keys in live mode: `n` next track, `space` chime (random pan), `+`/`-`
volume, `m` mute, `q` quit. With no music folder or no mp3/ogg in it, you
hear the ambient layer alone.

The render prints peak and RMS. A 60 s render comes out at about -8 dBFS
peak and -25 dBFS RMS, and renders about 120 times faster than real time.
`out/ambient-60s.wav` is a ready-made render to listen to.

## Music folder

Every `.mp3` and `.ogg` in the folder plays in shuffled order. When the
list runs out it reshuffles, and never starts the new round with the track
that just ended. Files that fail to decode are reported and skipped.

Titles come from an optional `tracks.json` in the same folder, matched by
file name. Other fields are ignored, so it can carry license data too:

```json
[
  {"file": "rain-garden.ogg", "title": "Rain Garden", "artist": "Someone",
   "license": "CC-BY 4.0", "url": "https://..."}
]
```

Without an entry, the title is the file name without its extension.

## API for the game

```rust
let audio = koi_audio::Audio::start(Path::new("assets/music"));
audio.send(Event::FoodDropped(pan));   // pan -1.0 left .. 1.0 right
audio.send(Event::NextTrack);          // also VolumeUp, VolumeDown, ToggleMute
while let Ok(s) = audio.status.try_recv() { /* NowPlaying, NoMusic, Volume, Error */ }
audio.shutdown();
```

`send` is a push onto an unbounded `mpsc` channel, so the render loop never
waits on audio. All audio work happens on a `koi-audio` thread: it opens the
device, owns the rodio players, and ticks crossfades every 20 ms. rodio's
own output thread pulls the samples. If no output device opens, the thread
reports `Status::Error` and keeps draining events, so the game runs silent
instead of failing.

## How it sounds

Everything is built from the yo scale (D E G A B), the major pentatonic of
Japanese folk music, so any mix of notes and chimes stays consonant.

- Drone: D2 and A2 sines that breathe over about 50 s.
- Pad: four voices. Each picks a note, swells in over 6 s, holds 5 to 11 s,
  fades over 9 s, rests 2 to 9 s, then picks again. Slight vibrato, random
  pan.
- Water: two independent band-limited brown noise channels whose level
  drifts slowly.
- Drops: every 4 to 14 s, a short sine that bends upward, like a bubble.
- Chime (`FoodDropped`): a bell with three inharmonic partials (1, 2.76,
  5.4) and a plop, panned with an equal-power law.
- A small Freeverb-style reverb (6 combs, 3 allpasses per side) under the
  pad, drops and chimes.

It is all hand-written oscillators and noise in `src/ambient.rs`, about 300
lines. I skipped fundsp: for this handful of voices, plain code was shorter
than learning its graph API, and every part is visible in one file.

Music crossfades take 3 s. The next track starts when the current one is
3 s from its end, using the decoder's reported duration. A file with no
known duration plays to the end and the next one fades in from silence.

## ALSA workaround

cpal needs `libasound2-dev` to link, and it is not installed. The runtime
library `libasound.so.2` is. Instead of installing anything system-wide,
the crate carries a link-only shim:

- `.alsa-shim/alsa.pc` and `.alsa-shim/libasound.so`, a symlink to
  `/usr/lib/x86_64-linux-gnu/libasound.so.2`. Both are git-ignored.
- `.cargo/config.toml` puts `.alsa-shim` on `PKG_CONFIG_PATH`.

alsa-sys ships pre-generated bindings, so it needs only the linker flag,
not the headers. The proper fix is:

```sh
sudo apt install libasound2-dev
```

After that, delete `.alsa-shim` and `.cargo/config.toml`. On another machine
without the package, recreate the shim with:

```sh
mkdir -p .alsa-shim
ln -s /usr/lib/x86_64-linux-gnu/libasound.so.2 .alsa-shim/libasound.so
printf 'libdir=${pcfiledir}\nName: alsa\nDescription: shim\nVersion: 1.2.0\nLibs: -L${libdir} -lasound\nCflags:\n' > .alsa-shim/alsa.pc
```

## Verified

- `cargo build --release` and `cargo clippy --release` are clean.
- Live mode in a pty: opens a PipeWire stream ("PipeWire ALSA
  [koi-audio]"), prints now playing with the tracks.json title and artist,
  volume and mute respond, `n` crossfades, tracks advance on their own
  about 3 s before the end, a broken file is skipped without eating the
  `n`, and `q` exits cleanly within a second.
- A 600 s render stays stable (RMS -25 to -28 dBFS every 2 s, no DC, no
  clicks).

## Not verified

- I cannot listen, so the mix is tuned by numbers only. Levels to try by
  ear: `AMBIENT_LEVEL` in `src/lib.rs` (0.6 of master), the drone amplitudes
  and the chime `amp`.
- There is no real music in `assets/music` yet. Crossfades were tested with
  10 s ffmpeg sine tones in mp3 and ogg.
