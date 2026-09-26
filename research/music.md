# Music: can it play my YouTube playlists?

## Recommendation

Short answer: the game can list your YouTube playlists and start one, but it
cannot stream YouTube audio itself without breaking YouTube's terms. So split
the job:

1. **Built-in default: generative ambient audio**, synthesized in-process in
   Rust (`fundsp` for synthesis, `cpal` or `rodio` for output). No network,
   no accounts, no terms to worry about. It can react to the pond: a soft
   chime when food lands, a low swell when fish gather.
2. **Local files**: a music folder played with `rodio` + `symphonia`. Fully
   allowed, cheap to add.
3. **"My YouTube playlists", the compliant way**: the game uses the YouTube
   Data API (read-only OAuth) to list your playlists, opens the chosen one in
   your browser (YouTube or YouTube Music, the official player), and then
   shows "now playing" and handles play/pause/next through MPRIS on D-Bus.
   Audio comes from the browser tab. This follows the rules.
4. **Not recommended to ship**: `yt-dlp` + `mpv` piping YouTube audio into the
   game. It works technically but breaks YouTube's Terms of Service and the
   API policies. If you want it for yourself, make it a user-configured
   "external player command" rather than a built-in feature. The same goes
   for Spotify via `librespot`.

Build order: generative audio first (it also proves the audio thread does not
disturb the 60 fps loop), then MPRIS now-playing, then the playlist picker.

## What YouTube allows

### Listing your playlists: allowed

- `playlists.list` with `mine=true` returns the signed-in user's playlists.
  It needs an OAuth token; the read-only scope
  `https://www.googleapis.com/auth/youtube.readonly` is enough.
  `playlistItems.list` returns the tracks.
- Each list call costs 1 quota unit. The default quota is 10,000 units per
  day, far more than a pond game needs.
- For a CLI, use the OAuth "installed app" flow (open a browser, receive the
  code on a loopback port). You register your own Google Cloud project and
  client ID. An unverified app is fine for personal use with yourself added
  as a test user.

Sources:
- https://developers.google.com/youtube/v3/docs/playlists/list
- https://developers.google.com/youtube/v3/getting-started (quota: "10,000
  units per day combined for all other endpoints"; list reads "usually cost
  1 unit")

### Playing audio from YouTube in a terminal: not allowed

The YouTube Terms of Service forbid it on two counts:

- "You are not allowed to access, reproduce, download, ... or otherwise use
  any part of the Service or any Content except: (a) as expressly authorized
  by the Service; or (b) with prior written permission from YouTube."
- "You are not allowed to access the Service using any automated means
  (such as robots, botnets or scrapers) ..."

If the game uses the Data API, it is an "API Client" and the Developer
Policies apply too. They forbid exactly what an audio-only terminal player
does:

- III.E.1: must not "download, import, backup, cache, or store copies of
  YouTube audiovisual content" without written approval.
- III.I.7: must not "separate, isolate, or modify the audio or video
  components of any YouTube audiovisual content."
- III.I.9: must not play content "from a background player, meaning a player
  that is not displayed in the page, tab, or screen that the user is
  viewing."

A terminal app that plays only the audio of a YouTube video hits all three.
YouTube Premium does not change this; Premium's background play and
downloads are features of the official apps, not a license for third-party
clients.

Sources:
- https://www.youtube.com/static?template=terms
- https://developers.google.com/youtube/terms/developer-policies

### What the practical tools do

- **yt-dlp + mpv.** `mpv <youtube playlist url>` calls `yt-dlp` to resolve
  streams, and `--no-video` gives audio only. Since yt-dlp 2025.11.12, full
  YouTube support needs an external JavaScript runtime (Deno by default) plus
  the bundled `yt-dlp-ejs` solver; without it, formats go missing. It breaks
  whenever YouTube changes its player, so expect regular `yt-dlp` updates.
  Neither `mpv`, `yt-dlp`, nor `deno` is installed on this machine now.
  Status under the terms: violates the ToS clauses above. Enforcement against
  individual listeners is rare, but the risk lands on your Google account,
  and it is not something to build into the game as a feature.
  - https://github.com/yt-dlp/yt-dlp/issues/15012
- **ytmusicapi** (Python, unofficial). Emulates the YouTube Music web client
  with your cookies or OAuth to browse and edit your library. It does not
  stream audio. Reverse-engineered, so the same ToS problem, and it is
  Python, which does not fit a Rust binary.
  - https://github.com/sigma67/ytmusicapi

### The compliant YouTube path: browser + MPRIS

1. List playlists via the Data API (allowed).
2. Open `https://music.youtube.com/playlist?list=<id>` (or the
   youtube.com equivalent) with `xdg-open`. Playback happens in the official
   player in a visible tab. That is normal use of YouTube.
3. Chrome and Firefox publish media sessions over MPRIS. The game reads
   title/artist and sends play/pause/next with the `mpris` crate (2.1.0), or
   by shelling out to `playerctl` (already installed here).

This gives "my playlists, with a now-playing line in the pond" without any
terms problem. Trade-off: a browser tab must stay open, and ads play if you
do not have Premium.

The same MPRIS control works for the Spotify desktop app, local players, or
anything else that exposes MPRIS, so it doubles as a generic "whatever you
are already playing" integration.

tmux note: MPRIS uses the session D-Bus. Inside a long-lived tmux session,
`DBUS_SESSION_BUS_ADDRESS` can be stale if the tmux server started under a
different login. If MPRIS finds no players inside tmux but does outside it,
that variable is the cause. Audio itself is unaffected by tmux, since it
goes to PipeWire/PulseAudio, not the terminal. Also decide what happens on
tmux detach: the game keeps running, so in-process audio keeps playing.

## Fallbacks

### Generative ambient audio (recommended default)

- `fundsp` 0.23 (Jan 2026): audio graph and synthesis (oscillators, filters,
  reverb, noise). Good for pads, water noise, soft plucks.
- `cpal` 0.18 (Aug 2026): raw audio output. `rodio` 0.22 (Mar 2026) sits on
  top of cpal and adds mixing and file playback.
- Run synthesis on cpal's audio callback thread. The render loop sends events
  (food dropped, fish count near food) over a lock-free channel. The audio
  thread must never wait on the render thread, or both glitch.
- Fully allowed, offline, and it fits the idea better than a playlist: the
  pond can make its own sound.

### Local files

- `rodio` + `symphonia` 0.6 (Aug 2026) decode MP3, FLAC, OGG/Vorbis, WAV,
  AAC. Point the game at a folder, shuffle it. Allowed for music you own.

### Spotify

- **librespot** (Rust, 0.8.0, Nov 2025) is an open Spotify Connect client.
  Its own README says: "Using this code to connect to Spotify's API is
  probably forbidden by them. Use at your own risk." It needs Premium, and
  Spotify server changes keep breaking it. Spotify's User Guidelines forbid
  "circumventing any technology used by Spotify" and use of "automated
  means ... to view, access or collect information." Same verdict as
  yt-dlp: works, not permitted, fragile.
  - https://github.com/librespot-org/librespot
  - https://www.spotify.com/us/legal/user-guidelines/
  - https://community.spotify.com/t5/Spotify-for-Developers/Spotify-keeps-breaking-librespot-and-go-librespot/td-p/7298958
- **Allowed Spotify route**: the official desktop app plays; the game controls
  it via MPRIS (simplest) or the Web API player endpoints. The Web API needs
  Premium for playback control, and since Feb 11, 2026 new Development Mode
  client IDs require the developer to have Premium, with one client ID and up
  to five users.
  - https://developer.spotify.com/documentation/web-api/reference/start-a-users-playback
  - https://developer.spotify.com/blog/2026-02-06-update-on-developer-access-and-platform-security

### Internet radio

- Technically trivial: `symphonia` can decode an HTTP MP3/AAC stream, or hand
  the URL to `mpv`.
- Check each station's terms. SomaFM (Drone Zone would suit a koi pond)
  says: "We can't grant permission for third-party SomaFM clients or
  applications, even noncommercial ones," which covers games. So do not
  bundle station URLs or branding. Letting the user paste any stream URL they
  already listen to is a personal player setting, not a SomaFM client.
  - https://somafm.com/contact/tos.html

## Summary table

| Option | Works | Allowed | Effort | Notes |
|---|---|---|---|---|
| Generative audio (fundsp + cpal) | Yes | Yes | Medium | Offline, reacts to the pond |
| Local files (rodio + symphonia) | Yes | Yes | Low | |
| YouTube: API list + browser + MPRIS | Yes | Yes | Medium | Tab stays open; ads without Premium |
| MPRIS "now playing" for any player | Yes | Yes | Low | Also covers Spotify desktop |
| User-supplied radio URL | Yes | Depends on station | Low | Do not bundle stations |
| yt-dlp + mpv | Yes, breaks often | No (YouTube ToS) | Low | Needs Deno; account risk |
| librespot | Yes, breaks often | No (Spotify rules) | Medium | Premium only |
