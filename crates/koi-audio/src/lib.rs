//! Music with per-track loudness normalization, the generated ambient layer, the food chimes
//! and the petting sound, all on a `koi-audio` thread.

#![warn(missing_docs)]

use koi_sim::FoodKind;
use rodio::{ChannelCount, Decoder, DeviceSinkBuilder, Player, SampleRate, Source};
use std::collections::HashMap;
use std::f32::consts::{FRAC_PI_4, TAU};
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const CROSSFADE: Duration = Duration::from_secs(3);
/// -22 dBFS gated RMS: the middle of the bundled tracks, which range from -17 to -39.
const TARGET_RMS: f32 = 0.0794;
/// -1 dBFS. Quiet tracks with loud peaks get less gain rather than clipping.
const PEAK_CEILING: f32 = 0.891;
const MAX_GAIN: f32 = 8.0;

/// Settings read once at start.
pub struct Settings {
    /// Every mp3 and ogg here plays in shuffled order. An optional tracks.json there gives
    /// titles.
    pub music_dir: PathBuf,
    /// Music volume, 0 to 1.
    pub volume: f32,
    /// Start muted.
    pub muted: bool,
    /// The ambient layer's volume as a fraction of the music volume.
    pub ambient_volume: f32,
    /// Chime when food lands, and bloop when a koi is petted.
    pub chime: bool,
    /// Play every track at about the same loudness. Gains are measured in the background
    /// and cached in `$XDG_CACHE_HOME/koi-pond/loudness.json` (or `~/.cache`).
    pub normalize: bool,
}

/// A request from the game to the audio thread.
pub enum Event {
    /// Food of this kind landed: its chime and splash, panned from -1 (left) to 1 (right).
    /// Every chime is on the ambient layer's pentatonic scale, so any mix stays in key.
    Chime(FoodKind, f32),
    /// A hand went into the water for a koi: a low bloop and a faint high bell, in key with
    /// the chimes, panned as `Chime`. At most one every 1.5 s sounds.
    Pet(f32),
    /// Raise the volume by 0.1.
    VolumeUp,
    /// Lower the volume by 0.1.
    VolumeDown,
    /// Mute or unmute everything.
    ToggleMute,
    /// Crossfade to the next track.
    NextTrack,
}

/// News from the audio thread, for the HUD's music stone, the state file, the stats line
/// and error toasts.
pub enum Status {
    /// A track started.
    NowPlaying {
        /// From tracks.json, or the file name.
        title: String,
    },
    /// Why nothing plays: there is no audio output.
    NoMusic(String),
    /// The volume changed.
    Volume {
        /// Music volume, 0 to 1.
        level: f32,
        /// Whether everything is muted.
        muted: bool,
    },
    /// Something failed: no output device, or a track that would not open or decode.
    Error(String),
}

/// The handle to the audio thread.
pub struct Audio {
    events: Sender<Event>,
    /// Status messages, in order. Poll it with `try_recv`.
    pub status: Receiver<Status>,
    thread: JoinHandle<()>,
}

impl Audio {
    /// Starts the audio thread. `send` never blocks the caller; all audio work happens on
    /// that thread, and a missing output device only produces `Status::Error`.
    pub fn start(config: Settings) -> Audio {
        let (events, events_rx) = mpsc::channel();
        let (status_tx, status) = mpsc::channel();
        let thread = thread::Builder::new()
            .name("koi-audio".into())
            .spawn(move || run(config, events_rx, status_tx))
            .expect("spawn audio thread");
        Audio { events, status, thread }
    }

    /// Queues `event` for the audio thread.
    pub fn send(&self, event: Event) {
        let _ = self.events.send(event);
    }

    /// Stops the audio thread and waits for it to finish.
    pub fn shutdown(self) {
        drop(self.events);
        let _ = self.thread.join();
    }
}

/// Played when the music folder has none: three of the game's tracks, re-encoded to Ogg
/// Vorbis to keep the binary small.
const BUILT_IN: [(&str, &[u8]); 3] = [
    ("Clear Waters", include_bytes!("../../../assets/music/builtin/clear-waters.ogg")),
    ("Fresh Air", include_bytes!("../../../assets/music/builtin/fresh-air.ogg")),
    ("Kalimba Relaxation Music", include_bytes!("../../../assets/music/builtin/kalimba-relaxation-music.ogg")),
];

struct Track {
    /// A built-in track's path is only its name, for the loudness cache.
    path: PathBuf,
    title: String,
    built_in: Option<&'static [u8]>,
}

struct Deck {
    player: Player,
    fade: f32,
    loudness_gain: f32,
    fading_out: bool,
    duration: Option<Duration>,
}

fn load_tracks(dir: &Path) -> Vec<Track> {
    let meta: Vec<serde_json::Value> = fs::read_to_string(dir.join("tracks.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    let Ok(entries) = fs::read_dir(dir) else { return Vec::new() };
    entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("mp3") || x.eq_ignore_ascii_case("ogg")))
        .map(|path| {
            let file_name = path.file_name().and_then(|f| f.to_str()).unwrap_or_default();
            let entry = meta.iter().find(|m| m["file"].as_str() == Some(file_name));
            let stem = path.file_stem().and_then(|f| f.to_str()).unwrap_or_default().to_string();
            Track {
                title: entry.and_then(|m| m["title"].as_str()).map_or(stem, str::to_string),
                path,
                built_in: None,
            }
        })
        .collect()
}

fn shuffle(tracks: &mut [Track], rng: &mut u64) {
    for i in (1..tracks.len()).rev() {
        let j = (rand(rng) * (i + 1) as f32) as usize;
        tracks.swap(i, j.min(i));
    }
}

fn run(config: Settings, events: Receiver<Event>, status: Sender<Status>) {
    let sink = match DeviceSinkBuilder::open_default_sink() {
        Ok(mut sink) => {
            sink.log_on_drop(false);
            sink
        }
        Err(e) => {
            let _ = status.send(Status::Error(format!("no audio output: {e}")));
            let _ = status.send(Status::NoMusic(format!("no audio output: {e}")));
            while events.recv().is_ok() {}
            return;
        }
    };

    let mut rng = SystemTime::now().duration_since(UNIX_EPOCH).map_or(1, |d| d.subsec_nanos().into()) | 1;
    let (chimes, chimes_rx) = mpsc::channel();
    let ambient = Player::connect_new(sink.mixer());
    ambient.append(Ambient::new(chimes_rx, rng));

    let mut tracks = load_tracks(&config.music_dir);
    if tracks.is_empty() {
        tracks = BUILT_IN.iter().map(|&(title, data)| Track { path: PathBuf::from(format!("built-in:{title}")), title: title.to_string(), built_in: Some(data) }).collect();
    }
    shuffle(&mut tracks, &mut rng);

    // A track starts only once its gain is known. Measuring runs on its own thread in play
    // order, so the first track is ready in well under a second even with a cold cache.
    let mut gains: HashMap<PathBuf, f32> = HashMap::new();
    let (gains_tx, gains_rx) = mpsc::channel();
    if config.normalize {
        let paths: Vec<(PathBuf, Option<&'static [u8]>)> = tracks.iter().map(|t| (t.path.clone(), t.built_in)).collect();
        let spawned = thread::Builder::new().name("koi-loudness".into()).spawn(move || measure_all(paths, gains_tx));
        if let Err(e) = spawned {
            let _ = status.send(Status::Error(format!("loudness thread: {e}")));
        }
    } else {
        gains.extend(tracks.iter().map(|t| (t.path.clone(), 1.0)));
    }

    let mut next_index = 0;
    let mut decks: Vec<Deck> = Vec::new();
    let mut volume = config.volume.clamp(0.0, 1.0);
    let mut muted = config.muted;
    let mut failures = 0;
    let mut skip = false;
    let mut last_tick = Instant::now();

    loop {
        match events.recv_timeout(Duration::from_millis(20)) {
            Ok(event @ (Event::Chime(..) | Event::Pet(_))) => {
                if config.chime {
                    let _ = chimes.send(event);
                }
            }
            Ok(Event::VolumeUp) => {
                volume = (volume + 0.1).min(1.0);
                let _ = status.send(Status::Volume { level: volume, muted });
            }
            Ok(Event::VolumeDown) => {
                volume = (volume - 0.1).max(0.0);
                let _ = status.send(Status::Volume { level: volume, muted });
            }
            Ok(Event::ToggleMute) => {
                muted = !muted;
                let _ = status.send(Status::Volume { level: volume, muted });
            }
            Ok(Event::NextTrack) => skip = true,
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        gains.extend(gains_rx.try_iter());

        if next_index >= tracks.len() && !tracks.is_empty() {
            let last_played = tracks[tracks.len() - 1].path.clone();
            shuffle(&mut tracks, &mut rng);
            if tracks.len() > 1 && tracks[0].path == last_played {
                let end = tracks.len() - 1;
                tracks.swap(0, end);
            }
            next_index = 0;
        }
        let current_ending = decks.last().is_none_or(|d| {
            d.player.empty() || d.duration.is_some_and(|len| d.player.get_pos() + CROSSFADE >= len)
        });
        if let Some(track) = tracks.get(next_index)
            && let Some(&loudness_gain) = gains.get(&track.path)
            && failures < tracks.len()
            && (skip || current_ending)
        {
            next_index += 1;
            match open(&track.path, track.built_in) {
                Ok(decoder) => {
                    skip = false;
                    failures = 0;
                    for deck in &mut decks {
                        deck.fading_out = true;
                    }
                    let player = Player::connect_new(sink.mixer());
                    player.set_volume(0.0);
                    let duration = decoder.total_duration();
                    player.append(decoder);
                    decks.push(Deck { player, fade: 0.0, loudness_gain, fading_out: false, duration });
                    let _ = status.send(Status::NowPlaying { title: track.title.clone() });
                }
                Err(e) => {
                    failures += 1;
                    let _ = status.send(Status::Error(format!("{}: {e}", track.path.display())));
                }
            }
        }

        let step = last_tick.elapsed().as_secs_f32() / CROSSFADE.as_secs_f32();
        last_tick = Instant::now();
        let master = if muted { 0.0 } else { volume };
        for deck in &mut decks {
            deck.fade = if deck.fading_out { (deck.fade - step).max(0.0) } else { (deck.fade + step).min(1.0) };
            deck.player.set_volume(deck.fade * deck.loudness_gain * master);
        }
        decks.retain(|d| !(d.player.empty() || d.fading_out && d.fade == 0.0));
        ambient.set_volume(master * config.ambient_volume);
    }
}

/// Sends a gain for every path, in order. Gains are cached by path and file size in
/// $XDG_CACHE_HOME/koi-pond/loudness.json, because decoding a whole track takes about a second.
fn measure_all(paths: Vec<(PathBuf, Option<&'static [u8]>)>, gains: Sender<(PathBuf, f32)>) {
    let cache_file = std::env::var_os("XDG_CACHE_HOME")
        .filter(|d| !d.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .map(|d| d.join("koi-pond").join("loudness.json"));
    let mut cache: HashMap<String, (u64, f32)> = cache_file
        .as_ref()
        .and_then(|f| fs::read_to_string(f).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    for (path, built_in) in paths {
        let key = path.to_string_lossy().into_owned();
        let bytes = match built_in {
            Some(data) => u64::try_from(data.len()).unwrap_or(0),
            None => fs::metadata(&path).map_or(0, |m| m.len()),
        };
        let gain = match cache.get(&key) {
            Some(&(cached_bytes, gain)) if cached_bytes == bytes => gain,
            _ => match measure(&path, built_in) {
                Some(gain) => {
                    cache.insert(key, (bytes, gain));
                    if let Some(file) = &cache_file
                        && let Some(dir) = file.parent()
                        && let Ok(json) = serde_json::to_string_pretty(&cache)
                    {
                        let tmp = file.with_extension("json.tmp");
                        let _ = fs::create_dir_all(dir).and_then(|()| fs::write(&tmp, json)).and_then(|()| fs::rename(&tmp, file));
                    }
                    gain
                }
                // The player hits the same decode error and reports it.
                None => 1.0,
            },
        };
        if gains.send((path, gain)).is_err() {
            return;
        }
    }
}

/// Decodes a music file, or a built-in track from its bytes.
fn open(path: &Path, built_in: Option<&'static [u8]>) -> Result<Box<dyn Source + Send>, String> {
    let decoded: Box<dyn Source + Send> = match built_in {
        Some(data) => Box::new(Decoder::builder().with_data(std::io::Cursor::new(data)).with_byte_len(u64::try_from(data.len()).unwrap_or(0)).with_mime_type("audio/ogg").build().map_err(|e| e.to_string())?),
        None => Box::new(Decoder::try_from(File::open(path).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?),
    };
    Ok(decoded)
}

/// Gain that brings the track's gated RMS to TARGET_RMS without pushing its peak past
/// PEAK_CEILING. 400 ms blocks below -70 dBFS (silence, fades) are left out of the mean.
fn measure(path: &Path, built_in: Option<&'static [u8]>) -> Option<f32> {
    let decoder = open(path, built_in).ok()?;
    let block_len = decoder.sample_rate().get() as usize * usize::from(decoder.channels().get()) * 2 / 5;
    let (mut block, mut in_block, mut peak) = (0.0f64, 0, 0.0f32);
    let (mut sum, mut blocks) = (0.0f64, 0);
    for sample in decoder {
        peak = peak.max(sample.abs());
        block += f64::from(sample * sample);
        in_block += 1;
        if in_block == block_len {
            let mean_square = block / in_block as f64;
            if mean_square > 1e-7 {
                sum += mean_square;
                blocks += 1;
            }
            (block, in_block) = (0.0, 0);
        }
    }
    if blocks == 0 || peak == 0.0 {
        return Some(1.0);
    }
    let rms = (sum / f64::from(blocks)).sqrt() as f32;
    Some((TARGET_RMS / rms).min(PEAK_CEILING / peak).min(MAX_GAIN))
}

const AMBIENT_RATE: u32 = 48_000;
const SR: f32 = AMBIENT_RATE as f32;

// Yo scale (D E G A B), the pentatonic used in Japanese folk songs. Octave 4.
const YO_SCALE: [f32; 5] = [293.66, 329.63, 392.00, 440.00, 493.88];

enum Stage {
    Rise,
    Hold,
    Fall,
    Rest,
}

struct PadVoice {
    freq: f32,
    phase: f32,
    pan: f32,
    env: f32,
    stage: Stage,
    timer: u32,
}

struct Droplet {
    phase: f32,
    age: u32,
    freq: f32,
    pan: f32,
    amp: f32,
}

struct Chime {
    phases: [f32; 3],
    /// Samples to wait before it sounds, so the treat's second note follows the first.
    delay: u32,
    age: u32,
    freq: f32,
    pan: f32,
    amp: f32,
    /// Frequency ratio, level and decay time in seconds of each partial.
    partials: [(f32, f32, f32); 3],
}

/// A struck bell: long, with inharmonic overtones.
const BELL: [(f32, f32, f32); 3] = [(1.0, 1.0, 2.8), (2.76, 0.25, 1.1), (5.4, 0.07, 0.45)];
/// A wooden knock: short, with a quick bright click on top.
const WOOD: [(f32, f32, f32); 3] = [(1.0, 1.0, 0.4), (3.9, 0.2, 0.08), (9.2, 0.05, 0.03)];

struct Reverb {
    combs: Vec<(Vec<f32>, usize, f32)>,
    allpasses: Vec<(Vec<f32>, usize)>,
}

impl Reverb {
    fn new(spread: usize) -> Reverb {
        let scale = SR / 44_100.0;
        Reverb {
            combs: [1116, 1188, 1277, 1356, 1422, 1491]
                .iter()
                .map(|&n| (vec![0.0; ((n + spread) as f32 * scale) as usize], 0, 0.0))
                .collect(),
            allpasses: [556, 441, 341]
                .iter()
                .map(|&n| (vec![0.0; ((n + spread) as f32 * scale) as usize], 0))
                .collect(),
        }
    }

    fn process(&mut self, input: f32) -> f32 {
        let mut out = 0.0;
        for (buf, i, filt) in &mut self.combs {
            let y = buf[*i];
            *filt = y * 0.6 + *filt * 0.4;
            buf[*i] = input + *filt * 0.86;
            *i = (*i + 1) % buf.len();
            out += y;
        }
        for (buf, i) in &mut self.allpasses {
            let y = buf[*i];
            buf[*i] = out + y * 0.5;
            *i = (*i + 1) % buf.len();
            out = y - out;
        }
        out
    }
}

/// xorshift64, returning a value in [0, 1).
fn rand(state: &mut u64) -> f32 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    (*state >> 40) as f32 / (1u64 << 24) as f32
}

/// The generative ambient layer: a slow pentatonic pad over a low drone,
/// soft water noise, occasional drops, and the `Chime` and `Pet` events sent over `chimes`.
struct Ambient {
    chimes: Receiver<Event>,
    rng: u64,
    frame: u64,
    right: Option<f32>,
    drone_phases: [f32; 2],
    voices: Vec<PadVoice>,
    brown: [f32; 2],
    brown_slow: [f32; 2],
    swell: f32,
    swell_target: f32,
    swell_timer: u32,
    drops: Vec<Droplet>,
    next_drop: u32,
    chime_voices: Vec<Chime>,
    /// The frame of the last petting sound.
    last_pet: Option<u64>,
    reverbs: [Reverb; 2],
}

impl Ambient {
    fn new(chimes: Receiver<Event>, seed: u64) -> Ambient {
        Ambient {
            chimes,
            rng: seed | 1,
            frame: 0,
            right: None,
            drone_phases: [0.0; 2],
            voices: (0..4)
                .map(|i| PadVoice {
                    freq: 0.0,
                    phase: 0.0,
                    pan: 0.0,
                    env: 0.0,
                    stage: Stage::Rest,
                    timer: (i as f32 * 3.5 * SR) as u32,
                })
                .collect(),
            brown: [0.0; 2],
            brown_slow: [0.0; 2],
            swell: 0.5,
            swell_target: 0.5,
            swell_timer: 0,
            drops: Vec::with_capacity(16),
            next_drop: 3 * AMBIENT_RATE,
            chime_voices: Vec::with_capacity(16),
            last_pet: None,
            reverbs: [Reverb::new(0), Reverb::new(23)],
        }
    }

    /// One note from the yo scale for `kind`, and its splash. Pellets keep the original
    /// chime; flakes sit an octave up and quieter, petals are a faint high bell, seeds a low
    /// wooden knock, and the treat a warm rising pair of notes.
    fn chime(&mut self, kind: FoodKind, pan: f32) {
        let note = (rand(&mut self.rng) * 5.0) as usize % 5;
        let high = if rand(&mut self.rng) < 0.6 { 2.0 } else { 1.0 };
        let bell = |freq: f32, amp: f32, delay: u32| Chime { phases: [0.0; 3], delay, age: 0, freq, pan, amp, partials: BELL };
        let (voices, splash) = match kind {
            FoodKind::Pellets => (vec![bell(YO_SCALE[note] * high, 0.16, 0)], (500.0, 300.0, 0.12)),
            FoodKind::Flakes => (vec![bell(YO_SCALE[note] * high * 2.0, 0.09, 0)], (900.0, 300.0, 0.03)),
            FoodKind::Petals => (vec![bell(YO_SCALE[note] * 4.0, 0.05, 0)], (1400.0, 300.0, 0.015)),
            FoodKind::Seeds => (vec![Chime { partials: WOOD, ..bell(YO_SCALE[note] * 0.5, 0.2, 0) }], (250.0, 150.0, 0.1)),
            FoodKind::Treat => {
                let low = note % 3;
                (vec![bell(YO_SCALE[low], 0.13, 0), bell(YO_SCALE[low + 2], 0.13, (0.18 * SR) as u32)], (400.0, 150.0, 0.14))
            }
        };
        for voice in voices {
            if self.chime_voices.len() == self.chime_voices.capacity() {
                self.chime_voices.remove(0);
            }
            self.chime_voices.push(voice);
        }
        if self.drops.len() == self.drops.capacity() {
            self.drops.remove(0);
        }
        let (low, range, amp) = splash;
        let freq = low + rand(&mut self.rng) * range;
        self.drops.push(Droplet { phase: 0.0, age: 0, freq, pan, amp });
    }

    /// A low bloop and a faint bell two octaves up, unless one sounded in the last 1.5 s.
    fn pet(&mut self, pan: f32) {
        if self.last_pet.is_some_and(|at| self.frame - at < u64::from(AMBIENT_RATE) * 3 / 2) {
            return;
        }
        self.last_pet = Some(self.frame);
        let note = (rand(&mut self.rng) * 5.0) as usize % 5;
        if self.chime_voices.len() == self.chime_voices.capacity() {
            self.chime_voices.remove(0);
        }
        self.chime_voices.push(Chime { phases: [0.0; 3], delay: (0.06 * SR) as u32, age: 0, freq: YO_SCALE[note] * 4.0, pan, amp: 0.04, partials: BELL });
        if self.drops.len() == self.drops.capacity() {
            self.drops.remove(0);
        }
        self.drops.push(Droplet { phase: 0.0, age: 0, freq: YO_SCALE[note] * 0.5, pan, amp: 0.12 });
    }

    fn render_frame(&mut self) -> (f32, f32) {
        if self.frame.is_multiple_of(256) {
            while let Ok(event) = self.chimes.try_recv() {
                match event {
                    Event::Chime(kind, pan) => self.chime(kind, pan.clamp(-1.0, 1.0)),
                    Event::Pet(pan) => self.pet(pan.clamp(-1.0, 1.0)),
                    _ => {}
                }
            }
        }
        let t = self.frame as f32 / SR;
        self.frame += 1;

        let (mut dry_l, mut dry_r) = (0.0, 0.0);
        let (mut send_l, mut send_r) = (0.0, 0.0);

        let drone_breath = 0.75 + 0.25 * (TAU * 0.021 * t).sin();
        self.drone_phases[0] = (self.drone_phases[0] + TAU * 73.42 / SR) % TAU;
        self.drone_phases[1] = (self.drone_phases[1] + TAU * 110.0 / SR) % TAU;
        let drone = drone_breath
            * (0.05 * self.drone_phases[0].sin() + 0.012 * (2.0 * self.drone_phases[0]).sin() + 0.025 * self.drone_phases[1].sin());
        dry_l += drone;
        dry_r += drone;
        send_l += drone * 0.3;
        send_r += drone * 0.3;

        for (v, voice) in self.voices.iter_mut().enumerate() {
            match voice.stage {
                Stage::Rest if voice.timer == 0 => {
                    let octave = if rand(&mut self.rng) < 0.3 { 2.0 } else { 1.0 };
                    voice.freq = YO_SCALE[(rand(&mut self.rng) * 5.0) as usize % 5] * 0.5 * octave;
                    voice.pan = rand(&mut self.rng) * 1.2 - 0.6;
                    voice.stage = Stage::Rise;
                }
                Stage::Rest => voice.timer -= 1,
                Stage::Rise => {
                    voice.env += 1.0 / (6.0 * SR);
                    if voice.env >= 1.0 {
                        voice.env = 1.0;
                        voice.stage = Stage::Hold;
                        voice.timer = ((5.0 + 6.0 * rand(&mut self.rng)) * SR) as u32;
                    }
                }
                Stage::Hold => {
                    voice.timer -= 1;
                    if voice.timer == 0 {
                        voice.stage = Stage::Fall;
                    }
                }
                Stage::Fall => {
                    voice.env -= 1.0 / (9.0 * SR);
                    if voice.env <= 0.0 {
                        voice.env = 0.0;
                        voice.stage = Stage::Rest;
                        voice.timer = ((2.0 + 7.0 * rand(&mut self.rng)) * SR) as u32;
                    }
                }
            }
            if voice.env > 0.0 {
                let vibrato = 1.0 + 0.0015 * (TAU * 0.2 * t + v as f32).sin();
                voice.phase = (voice.phase + TAU * voice.freq * vibrato / SR) % TAU;
                let shaped = voice.env * voice.env * (3.0 - 2.0 * voice.env);
                let s = shaped * 0.035 * (voice.phase.sin() + 0.2 * (2.0 * voice.phase).sin() + 0.05 * (3.0 * voice.phase).sin());
                let angle = (voice.pan + 1.0) * FRAC_PI_4;
                dry_l += s * angle.cos();
                dry_r += s * angle.sin();
                send_l += s * angle.cos() * 0.8;
                send_r += s * angle.sin() * 0.8;
            }
        }

        if self.swell_timer == 0 {
            self.swell_target = 0.25 + 0.75 * rand(&mut self.rng);
            self.swell_timer = ((5.0 + 6.0 * rand(&mut self.rng)) * SR) as u32;
        }
        self.swell_timer -= 1;
        self.swell += (self.swell_target - self.swell) / (4.0 * SR);
        for ch in 0..2 {
            let white = rand(&mut self.rng) * 2.0 - 1.0;
            self.brown[ch] = self.brown[ch] * 0.995 + white * 0.05;
            self.brown_slow[ch] += 0.004 * (self.brown[ch] - self.brown_slow[ch]);
            let water = (self.brown[ch] - self.brown_slow[ch]) * self.swell * 0.07;
            if ch == 0 {
                dry_l += water;
            } else {
                dry_r += water;
            }
        }

        if self.next_drop == 0 {
            let freq = 700.0 + rand(&mut self.rng) * 900.0;
            let pan = rand(&mut self.rng) * 1.6 - 0.8;
            let amp = 0.025 + rand(&mut self.rng) * 0.03;
            if self.drops.len() < self.drops.capacity() {
                self.drops.push(Droplet { phase: 0.0, age: 0, freq, pan, amp });
            }
            self.next_drop = ((4.0 + 10.0 * rand(&mut self.rng)) * SR) as u32;
        }
        self.next_drop -= 1;
        for drop in &mut self.drops {
            let age = drop.age as f32 / SR;
            let freq = drop.freq * (1.0 + 0.7 * (1.0 - (-age / 0.02).exp()));
            drop.phase = (drop.phase + TAU * freq / SR) % TAU;
            let env = (age / 0.002).min(1.0) * (-age / 0.045).exp();
            let s = drop.amp * env * drop.phase.sin();
            let angle = (drop.pan + 1.0) * FRAC_PI_4;
            dry_l += s * angle.cos() * 0.6;
            dry_r += s * angle.sin() * 0.6;
            send_l += s * angle.cos();
            send_r += s * angle.sin();
            drop.age += 1;
        }
        self.drops.retain(|d| d.age < AMBIENT_RATE / 2);

        for chime in &mut self.chime_voices {
            if chime.delay > 0 {
                chime.delay -= 1;
                continue;
            }
            let age = chime.age as f32 / SR;
            let attack = (age / 0.004).min(1.0);
            let mut s = 0.0;
            for (i, (ratio, level, decay)) in chime.partials.iter().enumerate() {
                chime.phases[i] = (chime.phases[i] + TAU * chime.freq * ratio / SR) % TAU;
                s += level * (-age / decay).exp() * chime.phases[i].sin();
            }
            s *= chime.amp * attack;
            let angle = (chime.pan + 1.0) * FRAC_PI_4;
            dry_l += s * angle.cos();
            dry_r += s * angle.sin();
            send_l += s * angle.cos() * 0.9;
            send_r += s * angle.sin() * 0.9;
            chime.age += 1;
        }
        self.chime_voices.retain(|c| c.age < 8 * AMBIENT_RATE);

        let wet_l = self.reverbs[0].process(send_l * 0.08);
        let wet_r = self.reverbs[1].process(send_r * 0.08);
        ((dry_l + wet_l * 0.5).tanh(), (dry_r + wet_r * 0.5).tanh())
    }
}

impl Iterator for Ambient {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        if let Some(r) = self.right.take() {
            return Some(r);
        }
        let (l, r) = self.render_frame();
        self.right = Some(r);
        Some(l)
    }
}

impl Source for Ambient {
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> ChannelCount {
        const { ChannelCount::new(2).unwrap() }
    }

    fn sample_rate(&self) -> SampleRate {
        const { SampleRate::new(AMBIENT_RATE).unwrap() }
    }

    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every built-in track decodes all the way through and gets a loudness gain.
    #[test]
    fn built_in_tracks_decode() {
        for (title, data) in BUILT_IN {
            let decoder = open(Path::new(title), Some(data)).expect("decodes");
            assert!(decoder.total_duration().is_some_and(|d| d > Duration::from_secs(60)), "{title}");
            assert!(measure(Path::new(title), Some(data)).is_some_and(|gain| gain > 0.0), "{title}");
        }
    }

    /// Every kind's chime lands on the yo scale in some octave, so any mix of foods stays in
    /// key with the pad. Petals are the quietest and highest, seeds the lowest, and the treat
    /// is two notes, the second after the first. Petting rings in key too, softly.
    #[test]
    fn chimes_stay_in_key_and_in_character() {
        let in_key = |freq: f32| YO_SCALE.iter().any(|&n| ((freq / n).log2() - (freq / n).log2().round()).abs() < 1e-4);
        let (_tx, rx) = mpsc::channel();
        let mut ambient = Ambient::new(rx, 11);
        for kind in FoodKind::ALL {
            for _ in 0..40 {
                ambient.chime_voices.clear();
                ambient.chime(kind, 0.0);
                let voices = &ambient.chime_voices;
                assert!(voices.iter().all(|v| in_key(v.freq)), "{kind:?} chimed off key: {:?}", voices.iter().map(|v| v.freq).collect::<Vec<_>>());
                let top = voices.iter().map(|v| v.freq).fold(0.0, f32::max);
                match kind {
                    FoodKind::Petals => assert!(top > 1000.0 && voices[0].amp < 0.1),
                    FoodKind::Seeds => assert!(top < 250.0),
                    FoodKind::Treat => assert!(voices.len() == 2 && voices[1].freq > voices[0].freq && voices[1].delay > 0),
                    FoodKind::Pellets | FoodKind::Flakes => assert!(voices.len() == 1),
                }
            }
        }
        // A second pet soon after is quiet.
        ambient.chime_voices.clear();
        ambient.drops.clear();
        ambient.pet(0.0);
        ambient.pet(0.0);
        let voices = &ambient.chime_voices;
        assert!(voices.len() == 1 && in_key(voices[0].freq) && voices[0].freq > 1000.0 && voices[0].amp < 0.05, "pet bell {:?}", voices.iter().map(|v| (v.freq, v.amp)).collect::<Vec<_>>());
        assert!(ambient.drops.len() == 1 && ambient.drops[0].freq < 250.0, "pet bloop");
    }
}
