pub mod ambient;

use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use rodio::{Decoder, DeviceSinkBuilder, Player, Source};

use ambient::Ambient;

const CROSSFADE: Duration = Duration::from_secs(3);
const AMBIENT_LEVEL: f32 = 0.6;

pub enum Event {
    /// A chime and a soft plop, panned from -1 (left) to 1 (right).
    FoodDropped(f32),
    VolumeUp,
    VolumeDown,
    ToggleMute,
    NextTrack,
}

pub enum Status {
    NowPlaying { title: String, artist: Option<String> },
    NoMusic(String),
    Volume { level: f32, muted: bool },
    Error(String),
}

pub struct Audio {
    events: Sender<Event>,
    pub status: Receiver<Status>,
    thread: JoinHandle<()>,
}

impl Audio {
    /// Starts the audio thread. Never blocks on the device: if no output can be
    /// opened, the thread reports `Status::Error` and keeps swallowing events.
    pub fn start(music_dir: &Path) -> Audio {
        let (events, events_rx) = mpsc::channel();
        let (status_tx, status) = mpsc::channel();
        let music_dir = music_dir.to_path_buf();
        let thread = thread::Builder::new()
            .name("koi-audio".into())
            .spawn(move || run(music_dir, events_rx, status_tx))
            .expect("spawn audio thread");
        Audio { events, status, thread }
    }

    pub fn send(&self, event: Event) {
        let _ = self.events.send(event);
    }

    pub fn shutdown(self) {
        drop(self.events);
        let _ = self.thread.join();
    }
}

pub fn seed() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64)
}

struct Track {
    path: PathBuf,
    title: String,
    artist: Option<String>,
}

struct Deck {
    player: Player,
    gain: f32,
    fading_out: bool,
    duration: Option<Duration>,
}

fn load_tracks(dir: &Path) -> Vec<Track> {
    let meta: Vec<serde_json::Value> = std::fs::read_to_string(dir.join("tracks.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("mp3") || x.eq_ignore_ascii_case("ogg")))
        .map(|path| {
            let file_name = path.file_name().and_then(|f| f.to_str()).unwrap_or_default();
            let entry = meta.iter().find(|m| m["file"].as_str() == Some(file_name));
            let stem = path.file_stem().and_then(|f| f.to_str()).unwrap_or_default().to_string();
            Track {
                title: entry.and_then(|m| m["title"].as_str()).map_or(stem, str::to_string),
                artist: entry.and_then(|m| m["artist"].as_str()).map(str::to_string),
                path,
            }
        })
        .collect()
}

fn run(music_dir: PathBuf, events: Receiver<Event>, status: Sender<Status>) {
    let sink = match DeviceSinkBuilder::open_default_sink() {
        Ok(mut sink) => {
            sink.log_on_drop(false);
            sink
        }
        Err(e) => {
            let _ = status.send(Status::Error(format!("no audio output: {e}")));
            while events.recv().is_ok() {}
            return;
        }
    };

    let mut rng = seed();
    let (chimes, chimes_rx) = mpsc::channel();
    let ambient = Player::connect_new(sink.mixer());
    ambient.append(Ambient::new(chimes_rx, rng));

    let mut tracks = load_tracks(&music_dir);
    if tracks.is_empty() {
        let _ = status.send(Status::NoMusic(format!("no mp3/ogg in {}", music_dir.display())));
    }
    let mut next_index = tracks.len();
    let mut decks: Vec<Deck> = Vec::new();
    let mut volume: f32 = 0.7;
    let mut muted = false;
    let mut failures = 0;
    let mut skip = false;
    let mut last_tick = Instant::now();

    loop {
        match events.recv_timeout(Duration::from_millis(20)) {
            Ok(Event::FoodDropped(pan)) => {
                let _ = chimes.send(pan);
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

        let current_ending = decks.last().is_none_or(|d| {
            d.player.empty() || d.duration.is_some_and(|len| d.player.get_pos() + CROSSFADE >= len)
        });
        if !tracks.is_empty() && failures < tracks.len() && (skip || current_ending) {
            if next_index >= tracks.len() {
                let last_played = tracks[tracks.len() - 1].path.clone();
                for i in (1..tracks.len()).rev() {
                    let j = (ambient::rand(&mut rng) * (i + 1) as f32) as usize;
                    tracks.swap(i, j);
                }
                if tracks.len() > 1 && tracks[0].path == last_played {
                    let end = tracks.len() - 1;
                    tracks.swap(0, end);
                }
                next_index = 0;
            }
            let track = &tracks[next_index];
            next_index += 1;
            match File::open(&track.path).map_err(|e| e.to_string()).and_then(|f| Decoder::try_from(f).map_err(|e| e.to_string())) {
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
                    decks.push(Deck { player, gain: 0.0, fading_out: false, duration });
                    let _ = status.send(Status::NowPlaying { title: track.title.clone(), artist: track.artist.clone() });
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
            deck.gain = if deck.fading_out { (deck.gain - step).max(0.0) } else { (deck.gain + step).min(1.0) };
            deck.player.set_volume(deck.gain * master);
        }
        decks.retain(|d| !(d.player.empty() || d.fading_out && d.gain == 0.0));
        ambient.set_volume(master * AMBIENT_LEVEL);
    }
}
