//! Music with per-track loudness normalization, and `koi-synth`'s generated ambient layer with
//! its food chimes and petting sound, all on a `koi-audio` thread.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

use koi_sim::FoodKind;
use koi_synth::{Ambient, rand};
use rodio::{ChannelCount, Decoder, DeviceSinkBuilder, Player, SampleRate, Source};
use std::collections::HashMap;
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
    /// and cached in `loudness_cache`.
    pub normalize: bool,
    /// The file that keeps measured gains between runs. None measures every run.
    pub loudness_cache: Option<PathBuf>,
}

/// A request from the game to the audio thread.
pub enum Event {
    /// Food of this kind landed: its chime and splash, panned from -1 (left) to 1 (right).
    /// Every chime is on the pentatonic yo scale, so any mix stays in key.
    Chime(FoodKind, f32),
    /// A hand went into the water for a koi: a low bloop and a faint high bell, in key with
    /// the chimes, panned as `Chime`. At most one every 1.5 s sounds.
    Pet(f32),
    /// Whether the scene is at night, when the crickets sing.
    Night(bool),
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
        let thread = thread::Builder::new().name("koi-audio".into()).spawn(move || run(config, events_rx, status_tx)).expect("spawn audio thread");
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
    let meta: Vec<serde_json::Value> = fs::read_to_string(dir.join("tracks.json")).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
    let Ok(entries) = fs::read_dir(dir) else { return Vec::new() };
    entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("mp3") || x.eq_ignore_ascii_case("ogg")))
        .map(|path| {
            let file_name = path.file_name().and_then(|f| f.to_str()).unwrap_or_default();
            let entry = meta.iter().find(|m| m["file"].as_str() == Some(file_name));
            let stem = path.file_stem().and_then(|f| f.to_str()).unwrap_or_default().to_string();
            Track { title: entry.and_then(|m| m["title"].as_str()).map_or(stem, str::to_string), path, built_in: None }
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
            return;
        }
    };

    let mut rng = SystemTime::now().duration_since(UNIX_EPOCH).map_or(1, |d| d.subsec_nanos().into()) | 1;
    let (chimes, chimes_rx) = mpsc::channel();
    let ambient = Player::connect_new(sink.mixer());
    ambient.append(AmbientSource { ambient: Ambient::new(rng), events: chimes_rx, frame: 0, right: None });

    let mut tracks = load_tracks(&config.music_dir);
    if tracks.is_empty() {
        tracks = BUILT_IN
            .iter()
            .map(|&(title, data)| Track { path: PathBuf::from(format!("built-in:{title}")), title: title.to_string(), built_in: Some(data) })
            .collect();
    }
    shuffle(&mut tracks, &mut rng);
    // Every session opens with Clear Waters, when it is there; the rest follow shuffled.
    if let Some(i) = tracks.iter().position(|t| t.title == "Clear Waters") {
        tracks[..=i].rotate_right(1);
    }

    // A track starts only once its gain is known. Measuring runs on its own thread in play
    // order, so the first track is ready in well under a second even with a cold cache.
    let mut gains: HashMap<PathBuf, f32> = HashMap::new();
    let (gains_tx, gains_rx) = mpsc::channel();
    if config.normalize {
        let paths: Vec<(PathBuf, Option<&'static [u8]>)> = tracks.iter().map(|t| (t.path.clone(), t.built_in)).collect();
        let cache_file = config.loudness_cache.clone();
        let spawned = thread::Builder::new().name("koi-loudness".into()).spawn(move || measure_all(paths, cache_file, gains_tx));
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
            Ok(event @ Event::Night(_)) => {
                let _ = chimes.send(event);
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
        let current_ending = decks.last().is_none_or(|d| d.player.empty() || d.duration.is_some_and(|len| d.player.get_pos() + CROSSFADE >= len));
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
/// `cache_file`, because decoding a whole track takes about a second.
fn measure_all(paths: Vec<(PathBuf, Option<&'static [u8]>)>, cache_file: Option<PathBuf>, gains: Sender<(PathBuf, f32)>) {
    let mut cache: HashMap<String, (u64, f32)> =
        cache_file.as_ref().and_then(|f| fs::read_to_string(f).ok()).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
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
        Some(data) => Box::new(
            Decoder::builder()
                .with_data(std::io::Cursor::new(data))
                .with_byte_len(u64::try_from(data.len()).unwrap_or(0))
                .with_mime_type("audio/ogg")
                .build()
                .map_err(|e| e.to_string())?,
        ),
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

/// The ambient layer as a rodio source, fed the `Chime`, `Pet` and `Night` events sent over
/// `events`.
struct AmbientSource {
    ambient: Ambient,
    events: Receiver<Event>,
    frame: u64,
    right: Option<f32>,
}

impl Iterator for AmbientSource {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        if let Some(r) = self.right.take() {
            return Some(r);
        }
        if self.frame.is_multiple_of(256) {
            while let Ok(event) = self.events.try_recv() {
                match event {
                    Event::Chime(kind, pan) => self.ambient.chime(kind, pan),
                    Event::Pet(pan) => self.ambient.pet(pan),
                    Event::Night(night) => self.ambient.night = night,
                    _ => {}
                }
            }
        }
        self.frame += 1;
        let (l, r) = self.ambient.render_frame();
        self.right = Some(r);
        Some(l)
    }
}

impl Source for AmbientSource {
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> ChannelCount {
        const { ChannelCount::new(2).unwrap() }
    }

    fn sample_rate(&self) -> SampleRate {
        const { SampleRate::new(koi_synth::RATE).unwrap() }
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
}
