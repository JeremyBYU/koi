//! The generated sound under the music: a low drone and the water's rumble, drops, crickets
//! at night and now and then a shishi-odoshi, and the food chimes and the petting sound. Plain Rust with no clock, thread or
//! device, so the terminal plays it through its audio thread and the web page through an
//! AudioWorklet, from the same code.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

use koi_sim::FoodKind;
use std::f32::consts::{FRAC_PI_4, TAU};

/// Frames per second of `Ambient::render_frame`.
pub const RATE: u32 = 48_000;
const SR: f32 = RATE as f32;

// Yo scale (D E G A B), the pentatonic used in Japanese folk songs. Octave 4.
const YO_SCALE: [f32; 5] = [293.66, 329.63, 392.00, 440.00, 493.88];

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

/// A cricket: a high pitch chirped in a few short syllables, each on its own slow rhythm.
struct Cricket {
    freq: f32,
    phase: f32,
    pan: f32,
    /// Seconds to its next chirp, and how far into a chirp it is (none between chirps).
    next: f32,
    within: Option<f32>,
    syllables: u32,
}

/// A 2-pole band-pass (RBJ, 0 dB peak), for the shishi-odoshi's trickle and knock.
struct Bandpass {
    b0: f32,
    a1: f32,
    a2: f32,
    x: [f32; 2],
    y: [f32; 2],
}

impl Bandpass {
    fn new(freq: f32, q: f32) -> Bandpass {
        let w = TAU * freq / SR;
        let alpha = w.sin() / (2.0 * q);
        let a0 = 1.0 + alpha;
        Bandpass { b0: alpha / a0, a1: -2.0 * w.cos() / a0, a2: (1.0 - alpha) / a0, x: [0.0; 2], y: [0.0; 2] }
    }

    fn run(&mut self, x: f32) -> f32 {
        let y = self.b0 * (x - self.x[1]) - self.a1 * self.y[0] - self.a2 * self.y[1];
        self.x = [x, self.x[0]];
        self.y = [y, self.y[0]];
        y
    }
}

/// The level of each layer, from the listening test of 2026-09-26: the pad left out, the
/// drone a little quieter, and the drops brought forward. The rumble came forward too, then
/// was halved after listening in the game, where it stood out.
const DRONE: f32 = 0.81;
const WATER: f32 = 0.073;
const CRICKET: f32 = 0.02;
const SHISHI: f32 = 0.216;

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
            combs: [1116, 1188, 1277, 1356, 1422, 1491].iter().map(|&n| (vec![0.0; ((n + spread) as f32 * scale) as usize], 0, 0.0)).collect(),
            allpasses: [556, 441, 341].iter().map(|&n| (vec![0.0; ((n + spread) as f32 * scale) as usize], 0)).collect(),
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
pub fn rand(state: &mut u64) -> f32 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    (*state >> 40) as f32 / (1u64 << 24) as f32
}

/// The generative ambient layer: a low drone and the water's rumble under drops, crickets at
/// night, a shishi-odoshi now and then, and the food chimes and petting sound it is asked for.
pub struct Ambient {
    /// The level of the bed, 1 by default: the drone and the rumble, the continuous low part,
    /// which small speakers play worst. `?ambient=` on the web page sets it.
    pub bed: f32,
    /// Whether it is night in the pond, when the crickets sing. They fade in and out.
    pub night: bool,
    rng: u64,
    frame: u64,
    drone_phases: [f32; 2],
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
    crickets: Vec<Cricket>,
    /// The crickets' level, fading to 1 at night and to 0 by day over 4 s.
    cricket_level: f32,
    /// Seconds to the next knock, and how long since the last one began (none once it has
    /// died away).
    shishi_in: f32,
    shishi_since: Option<f32>,
    trickle: Bandpass,
    knock: Bandpass,
    shishi_phases: [f32; 2],
    reverbs: [Reverb; 2],
}

impl Ambient {
    /// The pond by day. `seed` picks the notes and the crickets.
    pub fn new(seed: u64) -> Ambient {
        let mut rng = seed | 1;
        let crickets = [-0.8f32, 0.1, 0.75]
            .map(|pan| Cricket { freq: 4200.0 + 800.0 * rand(&mut rng), phase: 0.0, pan, next: rand(&mut rng), within: None, syllables: 3 })
            .into();
        Ambient {
            bed: 1.0,
            night: false,
            rng,
            frame: 0,
            drone_phases: [0.0; 2],
            brown: [0.0; 2],
            brown_slow: [0.0; 2],
            swell: 0.5,
            swell_target: 0.5,
            swell_timer: 0,
            drops: Vec::with_capacity(16),
            next_drop: 3 * RATE,
            chime_voices: Vec::with_capacity(16),
            last_pet: None,
            crickets,
            cricket_level: 0.0,
            shishi_in: 30.0,
            shishi_since: None,
            trickle: Bandpass::new(2000.0, 4.0),
            knock: Bandpass::new(1200.0, 2.0),
            shishi_phases: [0.0; 2],
            reverbs: [Reverb::new(0), Reverb::new(23)],
        }
    }

    /// Food of `kind` landed: one note from the yo scale for it, and its splash, panned from -1
    /// (left) to 1 (right). Pellets keep the original chime; flakes sit an octave up and
    /// quieter, petals are a faint high bell, seeds a low wooden knock, and the treat a warm
    /// rising pair of notes. Every chime is on the yo scale, so any mix stays in key.
    pub fn chime(&mut self, kind: FoodKind, pan: f32) {
        let pan = pan.clamp(-1.0, 1.0);
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

    /// A hand went into the water for a koi: a low bloop and a faint bell two octaves up,
    /// panned as `chime`, unless one sounded in the last 1.5 s.
    pub fn pet(&mut self, pan: f32) {
        let pan = pan.clamp(-1.0, 1.0);
        if self.last_pet.is_some_and(|at| self.frame - at < u64::from(RATE) * 3 / 2) {
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

    /// The next stereo frame, left and right, at `RATE` frames a second.
    pub fn render_frame(&mut self) -> (f32, f32) {
        let t = self.frame as f32 / SR;
        self.frame += 1;

        let (mut dry_l, mut dry_r) = (0.0, 0.0);
        let (mut send_l, mut send_r) = (0.0, 0.0);

        let drone_breath = 0.75 + 0.25 * (TAU * 0.021 * t).sin();
        self.drone_phases[0] = (self.drone_phases[0] + TAU * 73.42 / SR) % TAU;
        self.drone_phases[1] = (self.drone_phases[1] + TAU * 110.0 / SR) % TAU;
        let drone = drone_breath
            * (0.05 * self.drone_phases[0].sin() + 0.012 * (2.0 * self.drone_phases[0]).sin() + 0.025 * self.drone_phases[1].sin())
            * DRONE
            * self.bed;
        dry_l += drone;
        dry_r += drone;
        send_l += drone * 0.3;
        send_r += drone * 0.3;

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
            let water = (self.brown[ch] - self.brown_slow[ch]) * self.swell * WATER * self.bed;
            if ch == 0 {
                dry_l += water;
            } else {
                dry_r += water;
            }
        }

        if self.next_drop == 0 {
            let freq = 700.0 + rand(&mut self.rng) * 900.0;
            let pan = rand(&mut self.rng) * 1.6 - 0.8;
            let amp = 0.11 + rand(&mut self.rng) * 0.13;
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
        self.drops.retain(|d| d.age < RATE / 2);

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
        self.chime_voices.retain(|c| c.age < 8 * RATE);

        // Crickets: three or four 16 ms syllables 70 ms apart, a chirp every 0.8 to 1.3 s,
        // each cricket on its own rhythm. They always run, so night changes only the level.
        let fade = (if self.night { 1.0 } else { -1.0 }) / (4.0 * SR);
        self.cricket_level = (self.cricket_level + fade).clamp(0.0, 1.0);
        for c in &mut self.crickets {
            c.next -= 1.0 / SR;
            if c.next <= 0.0 {
                c.within = Some(0.0);
                c.syllables = 3 + u32::from(rand(&mut self.rng) < 0.5);
                c.next = 0.8 + 0.5 * rand(&mut self.rng);
            }
            let jitter = 60.0 * rand(&mut self.rng) - 30.0;
            c.phase = (c.phase + TAU * (c.freq + jitter) / SR) % TAU;
            if let Some(t) = c.within {
                let (syllable, into) = ((t / 0.07) as u32, t % 0.07);
                let env = if into < 0.016 { 0.5 - 0.5 * (TAU * into / 0.016).cos() } else { 0.0 };
                let s = c.phase.sin() * env * CRICKET * self.cricket_level;
                let angle = (c.pan + 1.0) * FRAC_PI_4;
                dry_l += s * angle.cos();
                dry_r += s * angle.sin();
                c.within = (syllable < c.syllables).then_some(t + 1.0 / SR);
            }
        }

        // The shishi-odoshi, every 45 to 90 s: a thin trickle filling the bamboo for 1.5 s, then
        // a hollow knock (a short click and two damped modes near 300 and 570 Hz), half wet in
        // the reverb for the garden's echo.
        self.shishi_in -= 1.0 / SR;
        let mut shishi = 0.0;
        if self.shishi_in < 1.5 && self.shishi_in > 0.0 {
            shishi += self.trickle.run(rand(&mut self.rng) * 2.0 - 1.0) * 0.05 * (1.0 - self.shishi_in / 1.5);
        }
        if self.shishi_in <= 0.0 {
            self.shishi_since = Some(0.0);
            self.shishi_phases = [0.0; 2];
            self.shishi_in = 45.0 + 45.0 * rand(&mut self.rng);
        }
        if let Some(t) = self.shishi_since {
            let click = if t < 0.004 { self.knock.run(rand(&mut self.rng) * 2.0 - 1.0) * (1.0 - t / 0.004) } else { 0.0 };
            self.shishi_phases[0] = (self.shishi_phases[0] + TAU * 300.0 / SR) % TAU;
            self.shishi_phases[1] = (self.shishi_phases[1] + TAU * 570.0 / SR) % TAU;
            shishi += click * 0.8 + self.shishi_phases[0].sin() * (-t / 0.15).exp() * 0.5 + self.shishi_phases[1].sin() * (-t / 0.08).exp() * 0.3;
            self.shishi_since = (t < 1.0).then_some(t + 1.0 / SR);
        }
        let angle = 1.25 * FRAC_PI_4;
        let shishi = shishi * SHISHI;
        dry_l += shishi * angle.cos();
        dry_r += shishi * angle.sin();
        send_l += shishi * angle.cos() * 3.75;
        send_r += shishi * angle.sin() * 3.75;

        let wet_l = self.reverbs[0].process(send_l * 0.08);
        let wet_r = self.reverbs[1].process(send_r * 0.08);
        ((dry_l + wet_l * 0.5).tanh(), (dry_r + wet_r * 0.5).tanh())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every kind's chime lands on the yo scale in some octave, so any mix of foods stays in
    /// key. Petals are the quietest and highest, seeds the lowest, and the treat
    /// is two notes, the second after the first. Petting rings in key too, softly.
    #[test]
    fn chimes_stay_in_key_and_in_character() {
        let in_key = |freq: f32| YO_SCALE.iter().any(|&n| ((freq / n).log2() - (freq / n).log2().round()).abs() < 1e-4);
        let mut ambient = Ambient::new(11);
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
        assert!(
            voices.len() == 1 && in_key(voices[0].freq) && voices[0].freq > 1000.0 && voices[0].amp < 0.05,
            "pet bell {:?}",
            voices.iter().map(|v| (v.freq, v.amp)).collect::<Vec<_>>()
        );
        assert!(ambient.drops.len() == 1 && ambient.drops[0].freq < 250.0, "pet bloop");
    }

    /// Turning the bed off takes out the drone and the rumble, the steady sound, and leaves
    /// the drops, chimes and petting sound.
    #[test]
    fn the_bed_is_the_steady_sound() {
        let rms = |bed: f32| {
            let mut ambient = Ambient::new(3);
            ambient.bed = bed;
            let sum: f32 = (0..(10.0 * SR) as usize).map(|_| ambient.render_frame()).map(|(l, r)| l * l + r * r).sum();
            (sum / (20.0 * SR)).sqrt()
        };
        assert!(rms(0.0) < 0.3 * rms(1.0), "with the bed off: {} against {}", rms(0.0), rms(1.0));
        let mut ambient = Ambient::new(3);
        ambient.bed = 0.0;
        let loudest =
            |ambient: &mut Ambient, secs: f32| (0..(secs * SR) as usize).map(|_| ambient.render_frame()).fold(0.0f32, |m, (l, r)| m.max(l.abs()).max(r.abs()));
        loudest(&mut ambient, 2.0);
        ambient.chime(FoodKind::Pellets, 0.0);
        assert!(loudest(&mut ambient, 0.5) > 0.05, "a chime sounds");
        ambient.pet(0.0);
        assert!(loudest(&mut ambient, 0.5) > 0.02, "a pet sounds");
    }

    /// The crickets sing only at night: the same pond at night differs from it by day by their
    /// chirps alone, and once day comes back they fade away entirely.
    #[test]
    fn crickets_sing_at_night() {
        let (mut day, mut night) = (Ambient::new(5), Ambient::new(5));
        night.night = true;
        let mut apart = 0.0f32;
        for _ in 0..(8.0 * SR) as usize {
            let (a, b) = (day.render_frame(), night.render_frame());
            apart = apart.max((a.0 - b.0).abs()).max((a.1 - b.1).abs());
        }
        assert!(apart > 0.005, "no crickets at night: {apart}");
        night.night = false;
        for _ in 0..(4.0 * SR) as usize {
            day.render_frame();
            night.render_frame();
        }
        assert!((0..(4.0 * SR) as usize).all(|_| day.render_frame() == night.render_frame()), "crickets after night");
    }
}
