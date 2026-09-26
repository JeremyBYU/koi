use std::io::Write;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use crossterm::event::{self, Event as TermEvent, KeyCode, KeyEventKind};
use crossterm::terminal;
use koi_audio::ambient::{self, Ambient, SAMPLE_RATE};
use koi_audio::{Audio, Event, Status};

const USAGE: &str = "usage: koi-audio [--music DIR] | --render-seconds N --out FILE.wav";

fn main() {
    let mut music = PathBuf::from("assets/music");
    let mut render_seconds: Option<u32> = None;
    let mut out: Option<PathBuf> = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match (arg.as_str(), args.next()) {
            ("--music", Some(v)) => music = PathBuf::from(v),
            ("--render-seconds", Some(v)) => render_seconds = Some(v.parse().expect(USAGE)),
            ("--out", Some(v)) => out = Some(PathBuf::from(v)),
            _ => {
                eprintln!("{USAGE}");
                std::process::exit(2);
            }
        }
    }

    match (render_seconds, out) {
        (Some(seconds), Some(out)) => render(seconds, &out),
        (None, None) => play(music),
        _ => {
            eprintln!("{USAGE}");
            std::process::exit(2);
        }
    }
}

fn render(seconds: u32, out: &PathBuf) {
    let (chimes, chimes_rx) = mpsc::channel();
    let mut ambient = Ambient::new(chimes_rx, 7);
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: SAMPLE_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(out, spec).expect("create wav");
    let chime_frames: Vec<u32> = [2.0, 5.0, 8.5, 12.0, 16.0].iter().map(|s| (s * SAMPLE_RATE as f32) as u32).collect();
    let pans = [-0.8, 0.8, 0.0, -0.4, 0.5];
    let (mut peak, mut sum_sq) = (0.0f32, 0.0f64);
    for frame in 0..seconds * SAMPLE_RATE {
        if let Some(i) = chime_frames.iter().position(|&f| f == frame) {
            let _ = chimes.send(pans[i]);
        }
        for _ in 0..2 {
            let s = ambient.next().unwrap_or(0.0);
            peak = peak.max(s.abs());
            sum_sq += (s * s) as f64;
            writer.write_sample((s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16).expect("write wav");
        }
    }
    writer.finalize().expect("finish wav");
    let rms = (sum_sq / (2 * seconds * SAMPLE_RATE).max(1) as f64).sqrt();
    println!(
        "wrote {} ({seconds} s, chimes at 2, 5, 8.5, 12, 16 s) peak {:.1} dBFS, rms {:.1} dBFS",
        out.display(),
        20.0 * peak.max(1e-9).log10(),
        20.0 * rms.max(1e-9).log10()
    );
}

fn play(music: PathBuf) {
    let audio = Audio::start(&music);
    let mut rng = koi_audio::seed();
    terminal::enable_raw_mode().expect("raw mode");
    print!("koi-audio  n next  space chime  +/- volume  m mute  q quit\r\n");
    loop {
        while let Ok(status) = audio.status.try_recv() {
            match status {
                Status::NowPlaying { title, artist: Some(artist) } => print!("now playing: {title} by {artist}\r\n"),
                Status::NowPlaying { title, artist: None } => print!("now playing: {title}\r\n"),
                Status::NoMusic(why) => print!("{why}; ambient layer only\r\n"),
                Status::Volume { level, muted: true } => print!("muted (volume {:.0}%)\r\n", level * 100.0),
                Status::Volume { level, muted: false } => print!("volume {:.0}%\r\n", level * 100.0),
                Status::Error(e) => print!("audio: {e}\r\n"),
            }
        }
        let _ = std::io::stdout().flush();
        if !event::poll(Duration::from_millis(50)).unwrap_or(false) {
            continue;
        }
        let Ok(TermEvent::Key(key)) = event::read() else { continue };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => break,
            KeyCode::Char('n') => audio.send(Event::NextTrack),
            KeyCode::Char(' ') => audio.send(Event::FoodDropped(ambient::rand(&mut rng) * 2.0 - 1.0)),
            KeyCode::Char('+') | KeyCode::Char('=') => audio.send(Event::VolumeUp),
            KeyCode::Char('-') => audio.send(Event::VolumeDown),
            KeyCode::Char('m') => audio.send(Event::ToggleMute),
            _ => {}
        }
    }
    terminal::disable_raw_mode().expect("restore terminal");
    audio.shutdown();
}
