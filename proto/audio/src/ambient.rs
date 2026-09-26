use std::f32::consts::{FRAC_PI_4, TAU};
use std::sync::mpsc::Receiver;
use std::time::Duration;

use rodio::{ChannelCount, SampleRate, Source};

pub const SAMPLE_RATE: u32 = 48_000;
const SR: f32 = SAMPLE_RATE as f32;

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
    age: u32,
    freq: f32,
    pan: f32,
    amp: f32,
}

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

pub fn rand(state: &mut u64) -> f32 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    (*state >> 40) as f32 / (1u64 << 24) as f32
}

/// The generative ambient layer: a slow pentatonic pad over a low drone,
/// soft water noise, occasional drops, and chimes sent over `chimes`.
/// Each value received on `chimes` is a pan from -1 (left) to 1 (right).
pub struct Ambient {
    chimes: Receiver<f32>,
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
    reverbs: [Reverb; 2],
}

impl Ambient {
    pub fn new(chimes: Receiver<f32>, seed: u64) -> Ambient {
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
            next_drop: 3 * SAMPLE_RATE,
            chime_voices: Vec::with_capacity(16),
            reverbs: [Reverb::new(0), Reverb::new(23)],
        }
    }

    fn chime(&mut self, pan: f32) {
        let freq = YO_SCALE[(rand(&mut self.rng) * 5.0) as usize % 5] * if rand(&mut self.rng) < 0.6 { 2.0 } else { 1.0 };
        if self.chime_voices.len() == self.chime_voices.capacity() {
            self.chime_voices.remove(0);
        }
        self.chime_voices.push(Chime { phases: [0.0; 3], age: 0, freq, pan, amp: 0.16 });
        if self.drops.len() == self.drops.capacity() {
            self.drops.remove(0);
        }
        let freq = 500.0 + rand(&mut self.rng) * 300.0;
        self.drops.push(Droplet { phase: 0.0, age: 0, freq, pan, amp: 0.12 });
    }

    fn render_frame(&mut self) -> (f32, f32) {
        if self.frame.is_multiple_of(256) {
            while let Ok(pan) = self.chimes.try_recv() {
                self.chime(pan.clamp(-1.0, 1.0));
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
            * (0.05 * self.drone_phases[0].sin()
                + 0.012 * (2.0 * self.drone_phases[0]).sin()
                + 0.025 * self.drone_phases[1].sin());
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
                let s = shaped
                    * 0.035
                    * (voice.phase.sin() + 0.2 * (2.0 * voice.phase).sin() + 0.05 * (3.0 * voice.phase).sin());
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
        self.drops.retain(|d| d.age < SAMPLE_RATE / 2);

        for chime in &mut self.chime_voices {
            let age = chime.age as f32 / SR;
            let attack = (age / 0.004).min(1.0);
            let mut s = 0.0;
            for (i, (ratio, level, decay)) in [(1.0, 1.0, 2.8), (2.76, 0.25, 1.1), (5.4, 0.07, 0.45)].iter().enumerate() {
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
        self.chime_voices.retain(|c| c.age < 8 * SAMPLE_RATE);

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
        const { SampleRate::new(SAMPLE_RATE).unwrap() }
    }

    fn total_duration(&self) -> Option<Duration> {
        None
    }
}
