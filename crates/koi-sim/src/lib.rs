//! The pond simulation: koi steering, moods and feeding, and the food they chase. Everything
//! is in water pixels, the coarse grid the water surface uses, and advances in fixed `DT`
//! steps. Nothing here touches a GPU, a terminal or audio, so it runs in plain unit tests.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

use std::f32::consts::{PI, TAU};

/// The fixed simulation step in seconds. Water and koi advance in these steps whatever the
/// frame rate.
pub const DT: f32 = 1.0 / 60.0;

/// A disturbance of the water surface, in water pixels. `amount` can be negative (a wake).
#[derive(Clone, Copy)]
pub struct Splash {
    /// Centre, from the left edge.
    pub x: f32,
    /// Centre, from the top edge.
    pub y: f32,
    /// Radius of the raised cosine bump.
    pub radius: f32,
    /// Height added at the centre.
    pub amount: f32,
}

/// A koi's shadow on the pond floor, in water pixels.
#[derive(Clone, Copy)]
pub struct Shadow {
    /// Centre of the koi, from the left edge.
    pub x: f32,
    /// Centre of the koi, from the top edge.
    pub y: f32,
    /// The heading as a unit vector, x part: +1 swims right.
    pub dir_x: f32,
    /// The heading as a unit vector, y part: +1 swims down.
    pub dir_y: f32,
    /// Body length.
    pub len: f32,
    /// How deep the koi is, as `Pose::depth`. A deeper koi is nearer the floor, so its
    /// shadow falls closer and sharper.
    pub depth: f32,
}

/// What a drop of food is. Each kind has its own look, handful, spread, cap on pieces in the
/// water, float and sink times, notice range and koi reaction. There is no hunger meter and nothing to score: uneaten food fades.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FoodKind {
    /// Ochre beads, 1 to 3 per drop. The nearest koi glide in at feeding speed and gulp.
    Pellets,
    /// Pale flecks, 8 to 12 scattered wide, drifting. Several koi come at cruise speed and
    /// nibble, so nobody darts.
    Flakes,
    /// Petals, 3 to 5, drifting. They never sink and are never eaten: one koi rises, mouths a
    /// petal once and loses interest.
    Petals,
    /// Dark seeds, 3 to 6, sinking from the start in a slow spiral. Koi follow them down
    /// (`Pose::depth`) and come back up.
    Seeds,
    /// One large slice, one in the pond at a time. Every koi drifts over and circles it,
    /// taking turns to take a bite until it is gone.
    Treat,
}

impl FoodKind {
    /// Every kind in HUD order, so `ALL[n]` is the food on key `n + 1`.
    pub const ALL: [FoodKind; 5] = [FoodKind::Pellets, FoodKind::Flakes, FoodKind::Petals, FoodKind::Seeds, FoodKind::Treat];

    /// The kind's position in `ALL`. Sprite sets are numbered by it.
    pub fn index(self) -> usize {
        match self {
            FoodKind::Pellets => 0,
            FoodKind::Flakes => 1,
            FoodKind::Petals => 2,
            FoodKind::Seeds => 3,
            FoodKind::Treat => 4,
        }
    }

    /// The HUD label: "pellets", "flakes", "petals", "seeds" or "treat".
    pub fn name(self) -> &'static str {
        match self {
            FoodKind::Pellets => "pellets",
            FoodKind::Flakes => "flakes",
            FoodKind::Petals => "petals",
            FoodKind::Seeds => "seeds",
            FoodKind::Treat => "treat",
        }
    }

    fn spec(self) -> Spec {
        match self {
            FoodKind::Pellets => Spec {
                pieces: (1, 3),
                spread: 0.02,
                cap: 12,
                floats: 20.0,
                fades: 5.0,
                sinks: true,
                drift: 0.0,
                notice: 10.0,
                speed: FEED_CAP,
                filling: 0.15,
                splash: (0.035, 1.2),
            },
            FoodKind::Flakes => Spec {
                pieces: (8, 12),
                spread: 0.09,
                cap: 36,
                floats: 30.0,
                fades: 8.0,
                sinks: true,
                drift: 0.004,
                notice: 12.0,
                speed: 0.45,
                filling: 0.04,
                splash: (0.05, 0.4),
            },
            FoodKind::Petals => Spec {
                pieces: (3, 5),
                spread: 0.05,
                cap: 15,
                floats: 40.0,
                fades: 20.0,
                sinks: false,
                drift: 0.006,
                notice: 6.0,
                speed: 0.4,
                filling: 0.0,
                splash: (0.03, 0.3),
            },
            FoodKind::Seeds => Spec {
                pieces: (3, 6),
                spread: 0.03,
                cap: 18,
                floats: 0.0,
                fades: 10.0,
                sinks: true,
                drift: 0.0,
                notice: 8.0,
                speed: 0.45,
                filling: 0.08,
                splash: (0.03, 1.5),
            },
            FoodKind::Treat => Spec {
                pieces: (1, 1),
                spread: 0.0,
                cap: 1,
                floats: 54.0,
                fades: 6.0,
                sinks: false,
                drift: 0.0,
                notice: f32::INFINITY,
                speed: 0.5,
                filling: 0.1,
                splash: (0.06, 0.4),
            },
        }
    }
}

/// One kind's numbers. Distances are in body lengths (BL) unless they say otherwise.
struct Spec {
    /// Pieces per drop, lowest and highest.
    pieces: (usize, usize),
    /// Furthest a piece lands from the drop point, as a fraction of the pond height.
    spread: f32,
    /// Most pieces of this kind in the water at once. A drop at the cap lands fewer or none.
    cap: usize,
    /// Seconds on the surface before the ending starts.
    floats: f32,
    /// Seconds the ending takes: sinking out of sight, or fading on the surface.
    fades: f32,
    /// Whether the ending sinks (koi can follow it down) or fades on the surface.
    sinks: bool,
    /// Drift speed on the surface, as a fraction of the pond height per second.
    drift: f32,
    /// Koi further than this never notice it.
    notice: f32,
    /// Top approach speed in BL/s. At or under `CRUISE_CAP` the koi come calmly: cruise gait,
    /// cruise spacing, gentle turns, so they never dart.
    speed: f32,
    /// How much a bite lowers the eater's hunger.
    filling: f32,
    /// The landing splash: radius as a fraction of the pond height, and height.
    splash: (f32, f32),
}

/// Bites in a whole treat.
const TREAT_BITES: u8 = 8;
/// Seconds between bites of the treat, so the circling koi take turns.
const TREAT_TURN: f32 = 2.5;
/// Radius, in BL, of the ring the koi swim round a treat.
const TREAT_RING: f32 = 1.4;
/// Seconds a seed takes for one turn of its spiral, and the spiral's widest radius as a
/// fraction of the pond height.
const SPIRAL_TURN: f32 = 5.0;
const SPIRAL_RADIUS: f32 = 0.015;
/// Turn rate of a koi coming calmly to food: between cruising and feeding.
const CALM_FEED_TURN: f32 = 70.0 * PI / 180.0;
/// How deep a koi follows sinking food, as a fraction of the food's own depth.
const DIVE: f32 = 0.8;

// Behaviour, in body lengths (BL) and seconds, from how real koi swim in a calm pond.
const CRUISE: f32 = 0.35;
const CRUISE_CAP: f32 = 0.5;
const FEED_CAP: f32 = 1.2;
const CRUISE_TURN: f32 = 40.0 * PI / 180.0;
const FEED_TURN: f32 = 110.0 * PI / 180.0;
const CRUISE_TAU: f32 = 0.6;
const FEED_TAU: f32 = 0.25;
const DRAG: f32 = 0.4;
/// Extra drag from spread pectoral fins when a koi is faster than it wants to be.
const BRAKE: f32 = 1.5;
/// How fast a splash is felt, in BL per second. Faster than the visible ring, as koi sense
/// the pressure wave with their lateral line, but slow enough that the pond reacts in turn.
const NOTICE_SPEED: f32 = 3.0;
const EAT_RANGE: f32 = 0.3;
/// A petted koi glides to the hand no faster than this, in BL/s: well under `CRUISE_CAP`.
const NUZZLE_SPEED: f32 = 0.4;
/// How close, in BL, a nuzzling koi's mouth is to the hand to count as touching it.
const NUZZLE_REACH: f32 = 0.3;
/// Seconds between the soft mouthings at the hand.
const MOUTHING: f32 = 0.6;
/// Seconds a bubble shows, from surfacing to popping.
pub const BUBBLE_LIFE: f32 = 1.6;

/// Joints in a koi's spine: a follow-the-leader chain, one BL from head to tail joint.
pub const JOINTS: usize = 8;
const JOINT_BEND: f32 = PI / 10.0;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Mood {
    Cruise,
    Rest {
        left: f32,
    },
    Approach {
        food: u32,
    },
    Eat {
        left: f32,
    },
    Linger {
        left: f32,
        x: f32,
        y: f32,
    },
    /// Nuzzling a hand at (`x`, `y`). `left` counts down, and `near` up, only while the
    /// mouth touches it.
    Nuzzle {
        left: f32,
        near: f32,
        x: f32,
        y: f32,
    },
}

/// A koi's colour pattern. The renderer paints each one differently.
#[derive(Clone, Copy)]
pub enum Variety {
    /// Red patches on white.
    Kohaku,
    /// Red and white with small black spots.
    Sanke,
    /// Black with red and white patches.
    Showa,
    /// Metallic gold, with a faint scale net.
    Ogon,
    /// Blue-grey scale net with orange flanks.
    Asagi,
}

/// One koi. Its state is private to the simulation apart from what a renderer needs to
/// paint it once: its size, pattern and pattern seed.
pub struct Koi {
    /// Centre of the body, in water pixels.
    x: f32,
    y: f32,
    /// Velocity along the heading, in water pixels per second.
    vx: f32,
    vy: f32,
    /// Tail-beat phase in radians, in [0, TAU).
    phase: f32,
    /// Body length (BL) in water pixels. Fixed for the koi's life.
    pub len: f32,
    /// Colour pattern.
    pub variety: Variety,
    /// Seeds the pattern noise and the koi's personal reaction times.
    pub seed: u32,
    heading: f32,
    speed: f32,
    turn: f32,
    steer: (f32, f32),
    gait: f32,
    energy: f32,
    wander: f32,
    pace: f32,
    whim: f32,
    sociable: f32,
    hunger: f32,
    mood: Mood,
    rethink: f32,
    rest_in: f32,
    depth: f32,
    spine: [(f32, f32); JOINTS],
    /// 0 to 1: rises while the koi nuzzles a hand and fades after.
    joy: f32,
    /// 0 to 1: how often it has been petted lately. `p` prefers a fond koi.
    fond: f32,
    /// Seconds until it may blow bubbles again.
    bubble_in: f32,
}

impl Koi {
    /// Everything a renderer needs to draw this koi as it is now.
    pub fn pose(&self) -> Pose {
        Pose { x: self.x, y: self.y, len: self.len, phase: self.phase, energy: self.energy, depth: self.depth, joy: self.joy, spine: self.spine }
    }
}

/// Everything a drawn koi depends on, in water pixels. Frames show a blend of the last two
/// simulation steps, so motion stays even when frames and steps drift apart.
#[derive(Clone, Copy)]
pub struct Pose {
    /// Centre of the body.
    pub x: f32,
    /// Centre of the body.
    pub y: f32,
    /// Body length (BL).
    pub len: f32,
    /// Tail-beat phase in radians, in [0, TAU).
    pub phase: f32,
    /// How hard the koi is swimming, about 0.1 resting to 1 bursting. Sets the tail-beat
    /// amplitude and how far the pectoral fins are tucked.
    pub energy: f32,
    /// How far below the surface the koi has dived, 0 at the surface to 1 at the depth
    /// where sinking food goes out of sight. Koi dive only while following sinking food, and
    /// never all the way. A renderer dims the koi with depth.
    pub depth: f32,
    /// How pleased the koi is at being petted, 0 to 1. The tail flutters and the head
    /// shimmers with it.
    pub joy: f32,
    /// Joint positions from head to tail, `len / (JOINTS - 1)` apart.
    pub spine: [(f32, f32); JOINTS],
}

impl Pose {
    /// The pose a fraction `t` of the way from `self` to `next`. The phase takes the short
    /// way round.
    pub fn lerp(&self, next: &Pose, t: f32) -> Pose {
        let at = |a: f32, b: f32| a + (b - a) * t;
        Pose {
            x: at(self.x, next.x),
            y: at(self.y, next.y),
            len: next.len,
            phase: (self.phase + wrap(next.phase - self.phase) * t).rem_euclid(TAU),
            energy: at(self.energy, next.energy),
            depth: at(self.depth, next.depth),
            joy: at(self.joy, next.joy),
            spine: std::array::from_fn(|i| (at(self.spine[i].0, next.spine[i].0), at(self.spine[i].1, next.spine[i].1))),
        }
    }
}

/// One piece of food in the water. It floats, then sinks or fades, and is gone.
pub struct Food {
    /// The kind, which sets its sprite, its ending and how the koi take it.
    pub kind: FoodKind,
    /// Where it is now. Flakes and petals drift, and seeds spiral as they sink.
    pub x: f32,
    /// Where it is now.
    pub y: f32,
    /// Seconds since it landed.
    pub age: f32,
    id: u32,
    /// Where it landed. Seeds spiral round it.
    origin: (f32, f32),
    /// Bites left: `TREAT_BITES` for a treat, 1 for anything else.
    bites: u8,
    /// A petal a koi has mouthed. Nobody comes for it again.
    mouthed: bool,
}

impl Food {
    /// How much of it shows, 1 on landing down to 0 when it is gone. It falls as the piece
    /// sinks or fades. For the treat it is how much of the slice is left, so the sprite
    /// shrinks bite by bite rather than fading.
    pub fn fade(&self) -> f32 {
        let spec = self.kind.spec();
        let life = (1.0 - (self.age - spec.floats) / spec.fades).clamp(0.0, 1.0);
        match self.kind {
            FoodKind::Treat => life * f32::from(self.bites) / f32::from(TREAT_BITES),
            _ => life,
        }
    }

    /// How far it has sunk, 0 on the surface to 1 out of sight. Petals and the treat stay at 0.
    pub fn depth(&self) -> f32 {
        let spec = self.kind.spec();
        if spec.sinks { ((self.age - spec.floats) / spec.fades).clamp(0.0, 1.0) } else { 0.0 }
    }
}

/// A bubble a petted koi blew at the hand. It swells, fades and pops with a tiny splash.
pub struct Bubble {
    /// Where it is.
    pub x: f32,
    /// Where it is.
    pub y: f32,
    /// Seconds since it surfaced. Negative while it is still on its way up, unseen.
    pub age: f32,
}

/// The koi and the food, in water pixels, the simulation's grid. The water is drawn on the
/// same grid in a painted theme and on its own art grid in a pixel theme, and converts.
/// Splashes they cause collect in `splashes` until the caller hands them to the water.
pub struct School {
    w: f32,
    h: f32,
    /// The koi, in a fixed order: index `k` is the same koi for the life of the school.
    pub fish: Vec<Koi>,
    /// Food in the water, every kind together. Eaten, sunk and faded pieces are removed.
    pub food: Vec<Food>,
    /// Bubbles on the surface, and on their way up. Popped ones are removed.
    pub bubbles: Vec<Bubble>,
    /// Splashes caused since the caller last drained this. Food landing and being eaten
    /// makes rings; every swimming koi adds a small wake each step.
    pub splashes: Vec<Splash>,
    /// Multiplies every swimming speed. 1 is a calm koi's pace.
    pub speed: f32,
    /// Divides turn rates and stretches steering smoothing and the gait. Higher is lazier.
    pub calmness: f32,
    next_food: u32,
    /// The koi whose turn it is to bite the treat, and the last one that did.
    nibbler: Option<usize>,
    last_nibbler: usize,
    /// Seconds until the next koi may go in for a bite of the treat.
    next_bite: f32,
    rng: u64,
}

/// The nearest point to (`x`, `y`) at least `margin` clear of the rim stones. The shore is
/// koi-render's `Layout` without its wobble: a rounded rectangle inset 0.07 of the short
/// side, its corners 0.3 round. The stones on the waterline and the wobble reach 0.12 in.
fn open_water(w: f32, h: f32, margin: f32, x: f32, y: f32) -> (f32, f32) {
    let u = w.min(h);
    let (hx, hy) = ((w / 2.0 - 0.37 * u).max(0.0), (h / 2.0 - 0.37 * u).max(0.0));
    let (cx, cy) = (w / 2.0 + (x - w / 2.0).clamp(-hx, hx), h / 2.0 + (y - h / 2.0).clamp(-hy, hy));
    let reach = (0.3 * u - 0.12 * u - margin).max(0.0);
    let d = (x - cx).hypot(y - cy);
    if d <= reach { (x, y) } else { (cx + (x - cx) / d * reach, cy + (y - cy) / d * reach) }
}

fn rand(state: &mut u64) -> f32 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    (*state >> 40) as f32 / (1u64 << 24) as f32
}

/// A fixed pseudo-random value in [0, 1) for `n`.
fn hash(n: u32) -> f32 {
    let mut x = n.wrapping_mul(0x9E37_79B9) ^ 0x7F4A_7C15;
    x ^= x >> 16;
    x = x.wrapping_mul(0x7FEB_352D);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846C_A68B);
    x ^= x >> 16;
    (x >> 8) as f32 / (1u32 << 24) as f32
}

fn smooth(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    smooth(((x - a) / (b - a)).clamp(0.0, 1.0))
}

/// Smooth value noise in [0, 1] with one lattice cell per unit, fixed by `seed`. Koi wander
/// along it, and the renderer paints their patterns with it.
pub fn noise2(seed: u32, x: f32, y: f32) -> f32 {
    let (ix, iy) = (x.floor(), y.floor());
    let (fx, fy) = (smooth(x - ix), smooth(y - iy));
    let (ix, iy) = (ix as i32 as u32, iy as i32 as u32);
    let at = |dx: u32, dy: u32| hash(seed ^ ix.wrapping_add(dx).wrapping_mul(0x27D4_EB2F) ^ iy.wrapping_add(dy).wrapping_mul(0x1656_67B1));
    let top = at(0, 0) + (at(1, 0) - at(0, 0)) * fx;
    let bottom = at(0, 1) + (at(1, 1) - at(0, 1)) * fx;
    top + (bottom - top) * fy
}

fn wrap(angle: f32) -> f32 {
    (angle + PI).rem_euclid(TAU) - PI
}

impl School {
    /// `count` koi spread over a `w` x `h` pond, away from the banks. The same `seed` gives
    /// the same school.
    pub fn new(w: usize, h: usize, count: usize, seed: u64) -> School {
        let varieties = [Variety::Kohaku, Variety::Sanke, Variety::Showa, Variety::Ogon, Variety::Asagi];
        let (w, h) = (w as f32, h as f32);
        let mut rng = seed | 1;
        let mut fish = Vec::new();
        for k in 0..count {
            let heading = rand(&mut rng) * TAU - PI;
            let len = h * (0.14 + 0.05 * rand(&mut rng));
            let (mut x, mut y) = (0.0, 0.0);
            for _ in 0..20 {
                (x, y) = (w * (0.2 + 0.6 * rand(&mut rng)), h * (0.2 + 0.6 * rand(&mut rng)));
                if fish.iter().all(|o: &Koi| (o.x - x).hypot(o.y - y) > 1.5 * len) {
                    break;
                }
            }
            let speed = CRUISE * len;
            let link = len / (JOINTS - 1) as f32;
            let head = (x + heading.cos() * len * 0.5, y + heading.sin() * len * 0.5);
            fish.push(Koi {
                x,
                y,
                vx: heading.cos() * speed,
                vy: heading.sin() * speed,
                phase: rand(&mut rng) * TAU,
                len,
                variety: varieties[k % varieties.len()],
                seed: (rand(&mut rng) * 16_777_216.0) as u32,
                heading,
                speed,
                turn: 0.0,
                steer: (heading.cos(), heading.sin()),
                gait: rand(&mut rng),
                energy: 0.3,
                wander: rand(&mut rng) * 100.0,
                pace: 0.85 + 0.3 * rand(&mut rng),
                whim: 0.8 + 0.4 * rand(&mut rng),
                sociable: 0.8 + 0.4 * rand(&mut rng),
                hunger: 0.5 + 0.5 * rand(&mut rng),
                mood: Mood::Cruise,
                rethink: rand(&mut rng) * 0.5,
                rest_in: 20.0 + 60.0 * rand(&mut rng),
                depth: 0.0,
                spine: std::array::from_fn(|i| (head.0 - heading.cos() * link * i as f32, head.1 - heading.sin() * link * i as f32)),
                joy: 0.0,
                fond: 0.0,
                bubble_in: 0.0,
            });
        }
        School {
            w,
            h,
            fish,
            food: Vec::new(),
            bubbles: Vec::new(),
            splashes: Vec::new(),
            speed: 1.0,
            calmness: 1.0,
            next_food: 0,
            nibbler: None,
            last_nibbler: 0,
            next_bite: 0.0,
            rng,
        }
    }

    /// Drops a handful of `kind` around (`x`, `y`) with a splash, and returns how many pieces
    /// landed. A point on or near the rim stones moves to the nearest open water first. At the
    /// kind's cap on pieces in the water fewer land, and with none the water only gets a soft
    /// tap: the caller should then skip the chime.
    pub fn drop_food(&mut self, kind: FoodKind, x: f32, y: f32) -> usize {
        let spec = kind.spec();
        let (x, y) = open_water(self.w, self.h, spec.spread * self.h, x, y);
        let mut pieces = spec.pieces.0;
        for _ in spec.pieces.0..spec.pieces.1 {
            if rand(&mut self.rng) < 0.5 {
                pieces += 1;
            }
        }
        let room = spec.cap.saturating_sub(self.food.iter().filter(|p| p.kind == kind).count());
        let landed = room.min(pieces);
        if landed == 0 {
            self.splashes.push(Splash { x, y, radius: self.h * 0.02, amount: 0.3 });
            return 0;
        }
        for _ in 0..landed {
            let angle = rand(&mut self.rng) * TAU;
            let r = spec.spread * self.h * rand(&mut self.rng).sqrt();
            let (px, py) = (x + angle.cos() * r, y + angle.sin() * r);
            let bites = if kind == FoodKind::Treat { TREAT_BITES } else { 1 };
            self.food.push(Food { kind, x: px, y: py, age: 0.0, id: self.next_food, origin: (px, py), bites, mouthed: false });
            self.next_food = self.next_food.wrapping_add(1);
        }
        if kind == FoodKind::Treat {
            self.nibbler = None;
            self.next_bite = TREAT_TURN;
        }
        self.splashes.push(Splash { x, y, radius: self.h * spec.splash.0, amount: spec.splash.1 });
        landed
    }

    /// Drops a handful of `kind` anywhere in open water, clear of the rim stones. Returns
    /// where, and how many pieces landed, as `drop_food` does.
    pub fn drop_food_random(&mut self, kind: FoodKind) -> (f32, f32, usize) {
        let margin = kind.spec().spread * self.h;
        loop {
            let (x, y) = (self.w * rand(&mut self.rng), self.h * rand(&mut self.rng));
            if open_water(self.w, self.h, margin, x, y) == (x, y) {
                return (x, y, self.drop_food(kind, x, y));
            }
        }
    }

    /// Puts a hand in the water at (`x`, `y`) for a koi to nuzzle for `secs` of touching it,
    /// and returns which koi, leaving any food it was after. It is the koi whose spine passes
    /// within `reach` BL of the hand, the nearest counted in BL and shortened by how
    /// fond the koi is. `reach` of 0.25 takes a press on a koi's body, and infinity the
    /// nearest koi anywhere. With none, nothing happens. Otherwise any earlier hand is let go,
    /// the water rings softly, and a few curious koi nearby come to watch.
    pub fn pet(&mut self, x: f32, y: f32, reach: f32, secs: f32) -> Option<usize> {
        let (k, _) = self
            .fish
            .iter()
            .enumerate()
            .filter_map(|(k, f)| {
                let d = f
                    .spine
                    .windows(2)
                    .map(|s| {
                        let ((ax, ay), (tx, ty)) = (s[0], (s[1].0 - s[0].0, s[1].1 - s[0].1));
                        let t = (((x - ax) * tx + (y - ay) * ty) / (tx * tx + ty * ty).max(1e-6)).clamp(0.0, 1.0);
                        (x - ax - tx * t).hypot(y - ay - ty * t)
                    })
                    .fold(f32::INFINITY, f32::min)
                    / f.len;
                (d <= reach).then_some((k, d / (1.0 + f.fond)))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))?;
        self.let_go();
        self.splashes.push(Splash { x, y, radius: self.h * 0.03, amount: 0.35 });
        for (j, o) in self.fish.iter_mut().enumerate() {
            let (dx, dy) = (o.x - x, o.y - y);
            let d = dx.hypot(dy).max(0.01);
            if j != k && matches!(o.mood, Mood::Cruise | Mood::Rest { .. }) && d < 4.0 * o.len && rand(&mut self.rng) < 0.3 {
                o.mood = Mood::Linger { left: 4.0 + 4.0 * rand(&mut self.rng), x: x + dx / d * 1.2 * o.len, y: y + dy / d * 1.2 * o.len };
            }
        }
        let f = &mut self.fish[k];
        f.mood = Mood::Nuzzle { left: secs, near: 0.0, x, y };
        f.fond = (f.fond + 0.25).min(1.0);
        Some(k)
    }

    /// Moves the hand a koi is nuzzling, kept to open water, and the koi follows it.
    pub fn move_hand(&mut self, x: f32, y: f32) {
        let hand = open_water(self.w, self.h, 0.0, x, y);
        for f in &mut self.fish {
            if let Mood::Nuzzle { x, y, .. } = &mut f.mood {
                (*x, *y) = hand;
            }
        }
    }

    /// Takes the hand out of the water. The koi that was nuzzling it lingers there a while.
    pub fn let_go(&mut self) {
        for f in &mut self.fish {
            if let Mood::Nuzzle { x, y, .. } = f.mood {
                f.mood = Mood::Linger { left: 4.0 + 4.0 * rand(&mut self.rng), x, y };
            }
        }
    }

    /// The fastest koi's speed as a multiple of the calm cruising speed.
    pub fn max_speed_ratio(&self) -> f32 {
        self.fish.iter().map(|f| f.speed / (CRUISE * self.speed.max(0.05) * f.len)).fold(0.0, f32::max)
    }

    /// One shadow per koi, for the water to paint on the floor.
    pub fn shadows(&self) -> Vec<Shadow> {
        self.fish.iter().map(|f| Shadow { x: f.x, y: f.y, dir_x: f.heading.cos(), dir_y: f.heading.sin(), len: f.len, depth: f.depth }).collect()
    }

    /// Advances everything by one `DT`: food ages, drifts and sinks, each koi picks a mood and
    /// steers, and the closest koi in reach eats, mouths or bites each piece.
    pub fn step(&mut self) {
        let inset = 0.07 * self.w.min(self.h);
        for food in &mut self.food {
            food.age += DT;
            let spec = food.kind.spec();
            if spec.drift > 0.0 {
                let heading = noise2(food.id, food.age * 0.05, 0.5) * 2.0 * TAU;
                let step = spec.drift * self.h * DT;
                // Drift never carries a piece onto the rim stones, though it may land there.
                food.x = (food.x + heading.cos() * step).clamp(inset.min(food.x), (self.w - inset).max(food.x));
                food.y = (food.y + heading.sin() * step).clamp(inset.min(food.y), (self.h - inset).max(food.y));
            }
            if food.kind == FoodKind::Seeds {
                let turn = if hash(food.id) < 0.5 { 1.0 } else { -1.0 };
                let angle = TAU * hash(food.id ^ 0x68E3_1DA4) + turn * TAU * food.age / SPIRAL_TURN;
                let r = SPIRAL_RADIUS * self.h * food.depth().sqrt();
                food.x = food.origin.0 + angle.cos() * r;
                food.y = food.origin.1 + angle.sin() * r;
            }
        }
        self.food.retain(|p| p.age < p.kind.spec().floats + p.kind.spec().fades);
        for b in &mut self.bubbles {
            b.age += DT;
            if b.age >= BUBBLE_LIFE {
                self.splashes.push(Splash { x: b.x, y: b.y, radius: self.h * 0.012, amount: 0.08 });
            }
        }
        self.bubbles.retain(|b| b.age < BUBBLE_LIFE);

        // The circling koi take turns: once the last bite has settled, the next koi round the
        // ring after the last one to bite goes in.
        self.next_bite -= DT;
        match self.food.iter().find(|p| p.kind == FoodKind::Treat) {
            Some(treat) => {
                let circling = |k: usize| {
                    let f = &self.fish[k];
                    f.mood == Mood::Approach { food: treat.id } && (treat.x - f.x).hypot(treat.y - f.y) < (TREAT_RING + 1.0) * f.len
                };
                if self.nibbler.is_some_and(|k| self.fish[k].mood != Mood::Approach { food: treat.id }) {
                    self.nibbler = None;
                }
                if self.nibbler.is_none() && self.next_bite <= 0.0 {
                    let n = self.fish.len();
                    self.nibbler = (1..=n).map(|i| (self.last_nibbler + i) % n).find(|&k| circling(k));
                }
            }
            None => self.nibbler = None,
        }
        let nibbler = self.nibbler;

        let others: Vec<(f32, f32, f32, f32, f32, Option<u32>)> =
            self.fish.iter().map(|f| (f.x, f.y, f.vx, f.vy, f.len, if let Mood::Approach { food } = f.mood { Some(food) } else { None })).collect();
        let mut resting = self.fish.iter().filter(|f| matches!(f.mood, Mood::Rest { .. })).count();
        let (w, h, knob, calm) = (self.w, self.h, self.speed.max(0.05), self.calmness.max(0.1));
        let settle = |tau: f32| 1.0 - (-DT / tau).exp();

        for (k, f) in self.fish.iter_mut().enumerate() {
            let bl = f.len;
            let fwd = (f.heading.cos(), f.heading.sin());
            let mouth = (f.x + fwd.0 * 0.45 * bl, f.y + fwd.1 * 0.45 * bl);
            f.hunger = (f.hunger + DT / 300.0).min(1.0);
            let at_hand = matches!(f.mood, Mood::Nuzzle { x, y, .. } if (mouth.0 - x).hypot(mouth.1 - y) < NUZZLE_REACH * bl);

            f.mood = match f.mood {
                Mood::Rest { left } if left > DT => Mood::Rest { left: left - DT },
                Mood::Eat { left } if left > DT => Mood::Eat { left: left - DT },
                Mood::Eat { .. } => Mood::Linger { left: 4.0 + 4.0 * rand(&mut self.rng), x: mouth.0, y: mouth.1 },
                Mood::Linger { left, x, y } if left > DT => Mood::Linger { left: left - DT, x, y },
                Mood::Rest { .. } | Mood::Linger { .. } => Mood::Cruise,
                Mood::Nuzzle { left, near, x, y } if at_hand && left > DT => Mood::Nuzzle { left: left - DT, near: near + DT, x, y },
                Mood::Nuzzle { x, y, .. } if at_hand => Mood::Linger { left: 4.0 + 4.0 * rand(&mut self.rng), x, y },
                mood => mood,
            };
            // Mouthing the hand now and then, and once in a while a few bubbles.
            if let Mood::Nuzzle { near, .. } = f.mood
                && at_hand
            {
                if near % MOUTHING < DT {
                    self.splashes.push(Splash { x: mouth.0, y: mouth.1, radius: 0.06 * bl, amount: 0.15 });
                }
                if near >= 0.4 && f.bubble_in <= 0.0 {
                    // Just past the snout, where they show against the water.
                    for n in 0..3u8 {
                        let (angle, r) = (rand(&mut self.rng) * TAU, 0.06 * bl * rand(&mut self.rng).sqrt());
                        self.bubbles.push(Bubble {
                            x: mouth.0 + fwd.0 * 0.15 * bl + angle.cos() * r,
                            y: mouth.1 + fwd.1 * 0.15 * bl + angle.sin() * r,
                            age: -0.3 * f32::from(n),
                        });
                    }
                    f.bubble_in = 8.0;
                }
            }
            f.bubble_in -= DT;
            f.joy += (if at_hand { 1.0 } else { 0.0 } - f.joy) * settle(if at_hand { 0.4 } else { 1.0 });
            // Settled, so the renderers can skip the shimmer rather than draw a trace of it.
            if f.joy < 1e-3 {
                f.joy = 0.0;
            }
            f.fond = (f.fond - DT / 600.0).max(0.0);
            if f.mood == Mood::Cruise {
                f.rest_in -= DT;
                if f.rest_in <= 0.0 {
                    if resting < 2 {
                        f.mood = Mood::Rest { left: 3.0 + 5.0 * rand(&mut self.rng) };
                        resting += 1;
                    }
                    f.rest_in = 30.0 + 60.0 * rand(&mut self.rng);
                }
            }

            // Food is noticed when its splash reaches the fish, plus a personal reaction time.
            // A well-fed koi mostly ignores it. Each fish re-scores the food twice a second.
            let current = if let Mood::Approach { food } = f.mood { Some(food) } else { None };
            let target_gone = current.is_some_and(|id| !self.food.iter().any(|p| p.id == id));
            f.rethink -= DT;
            if (f.rethink <= 0.0 || target_gone) && !matches!(f.mood, Mood::Eat { .. } | Mood::Nuzzle { .. }) {
                f.rethink = 0.5;
                let best = self
                    .food
                    .iter()
                    .filter_map(|p| {
                        let d = (p.x - f.x).hypot(p.y - f.y);
                        let key = f.seed ^ p.id.wrapping_mul(0x2545_F491);
                        let reaction = 0.3 + 0.9 * hash(key);
                        let felt = p.age >= d / (NOTICE_SPEED * knob * bl) + reaction;
                        let rivals = others.iter().enumerate().filter(|&(j, o)| j != k && o.5 == Some(p.id)).count();
                        // The treat draws everyone. A pellet draws one koi, and a petal one curious koi, once.
                        let keen = match p.kind {
                            FoodKind::Treat => true,
                            FoodKind::Petals => !p.mouthed && (rivals == 0 || current == Some(p.id)) && hash(key ^ 0x5bd1_e995) < 0.5,
                            FoodKind::Pellets => (rivals == 0 || current == Some(p.id)) && (f.hunger >= 0.2 || hash(key ^ 0x5bd1_e995) < 0.3),
                            _ => f.hunger >= 0.2 || hash(key ^ 0x5bd1_e995) < 0.3,
                        };
                        // The treat outranks everything else, so the whole pond goes to it.
                        let score = if p.kind == FoodKind::Treat {
                            f32::NEG_INFINITY
                        } else {
                            d + 2.0 * bl * rivals as f32 - if current == Some(p.id) { bl } else { 0.0 }
                        };
                        (keen && felt && d < p.kind.spec().notice * bl).then_some((p.id, score))
                    })
                    .min_by(|a, b| a.1.total_cmp(&b.1));
                match (best, f.mood) {
                    (Some((id, _)), Mood::Approach { .. }) => f.mood = Mood::Approach { food: id },
                    (Some((id, _)), _) => {
                        f.mood = Mood::Approach { food: id };
                        f.gait = 0.0;
                    }
                    (None, Mood::Approach { .. }) => f.mood = Mood::Linger { left: 4.0 + 4.0 * rand(&mut self.rng), x: mouth.0, y: mouth.1 },
                    (None, _) => {}
                }
            }
            let target = if let Mood::Approach { food } = f.mood { self.food.iter().find(|p| p.id == food) } else { None };
            let feeding = matches!(f.mood, Mood::Approach { .. } | Mood::Eat { .. } | Mood::Nuzzle { .. });
            // Only food worth a rush brings out the feeding gait, turns and tight spacing.
            let eager = target.is_some_and(|p| p.kind.spec().speed > CRUISE_CAP);
            let dive = match f.mood {
                Mood::Approach { .. } => target.map_or(0.0, Food::depth) * DIVE,
                Mood::Eat { .. } => f.depth,
                _ => 0.0,
            };
            f.depth += (dive - f.depth) * settle(1.2);

            // Steering: each behaviour adds a desired direction, weighted.
            f.wander += DT * 0.15 * f.whim / calm;
            let theta = (noise2(f.seed, f.wander, 0.0) * 2.0 - 1.0) * PI;
            let wander = {
                let (tx, ty) = (fwd.0 * 2.0 + (f.heading + theta).cos() * 0.8, fwd.1 * 2.0 + (f.heading + theta).sin() * 0.8);
                let n = tx.hypot(ty);
                (tx / n, ty / n)
            };
            let (mut want, target_speed) = match f.mood {
                Mood::Cruise => (wander, CRUISE * f.pace),
                Mood::Rest { .. } => (wander, 0.03),
                Mood::Approach { .. } => match target {
                    // Round the treat on a ring, all the same way, until it is this koi's turn.
                    Some(p) if p.kind == FoodKind::Treat && nibbler != Some(k) => {
                        let (dx, dy) = (p.x - f.x, p.y - f.y);
                        let d = dx.hypot(dy).max(0.01);
                        let (rx, ry) = (dx / d, dy / d);
                        let pull = ((d - TREAT_RING * bl) / bl).clamp(-1.0, 1.0) * 1.5;
                        ((-ry + rx * pull, rx + ry * pull), if d > 2.0 * TREAT_RING * bl { p.kind.spec().speed } else { 0.25 })
                    }
                    _ => {
                        let (p, top) = target.map_or((mouth, CRUISE_CAP), |p| ((p.x, p.y), p.kind.spec().speed));
                        let (dx, dy) = (p.0 - f.x, p.1 - f.y);
                        let d = dx.hypot(dy).max(0.01);
                        // A rush brakes hard over the last BL and a half; a calm approach glides in.
                        let ramp = if eager { ((d - 0.45 * bl) / (1.5 * bl)).clamp(0.15, 1.0) } else { ((d - 0.45 * bl) / (0.75 * bl)).clamp(0.5, 1.0) };
                        ((dx / d * 3.0, dy / d * 3.0), top * ramp)
                    }
                },
                Mood::Eat { .. } => (fwd, 0.1),
                Mood::Linger { x, y, .. } => {
                    let (dx, dy) = (x - f.x, y - f.y);
                    let d = dx.hypot(dy).max(0.01);
                    let pull = (d / (0.5 * bl)).min(1.0) * 0.8;
                    ((wander.0 * 0.6 + dx / d * pull, wander.1 * 0.6 + dy / d * pull), 0.2)
                }
                // Touching, it faces the hand and eases its mouth up to it.
                Mood::Nuzzle { x, y, .. } if at_hand => {
                    let (hx, hy) = (x - f.x, y - f.y);
                    let h = hx.hypot(hy).max(0.01);
                    ((hx / h * 3.0, hy / h * 3.0), 0.8 * ((x - mouth.0) * fwd.0 + (y - mouth.1) * fwd.1).max(0.0) / bl)
                }
                // Otherwise the body heads for a mouth's length short of the hand, then turns to
                // face it, so a koi pressed on its back swims round rather than circling over.
                Mood::Nuzzle { x, y, .. } => {
                    let (hx, hy) = (x - f.x, y - f.y);
                    let h = hx.hypot(hy).max(0.01);
                    let (sx, sy) = (hx - hx / h * 0.45 * bl, hy - hy / h * 0.45 * bl);
                    let s = sx.hypot(sy).max(0.001);
                    let far = (s / (0.2 * bl)).min(1.0);
                    let (wx, wy) = (sx / s * far + hx / h * (1.0 - far), sy / s * far + hy / h * (1.0 - far));
                    let n = wx.hypot(wy).max(0.001);
                    ((wx / n * 3.0, wy / n * 3.0), NUZZLE_SPEED * (s / (0.75 * bl)).clamp(0.3, 1.0) * (0.3 + 0.7 * ((wx * fwd.0 + wy * fwd.1) / n).max(0.0)))
                }
            };
            let target_speed = target_speed * knob;

            let (sep_r, sep_w) = if eager || matches!(f.mood, Mood::Nuzzle { .. }) { (0.6, 0.8) } else { (1.2, 1.5) };
            let (mut align, mut centre, mut yield_to, mut crowded) = ((0.0, 0.0), (0.0, 0.0), 1.0f32, false);
            for (j, &(ox, oy, ovx, ovy, olen, _)) in others.iter().enumerate() {
                let (dx, dy) = (ox - f.x, oy - f.y);
                let d = dx.hypot(dy) / bl;
                // Koi ignore a fish right behind them, which breaks up head-on standoffs.
                if j == k || d < 0.001 || (dx * fwd.0 + dy * fwd.1) / (d * bl) < -0.7 {
                    continue;
                }
                // A crowded koi swims off at cruising pace rather than drifting beside the other.
                if d < 0.5 {
                    crowded = true;
                    if matches!(f.mood, Mood::Rest { .. }) {
                        f.mood = Mood::Cruise;
                    }
                }
                if d < sep_r {
                    let push = sep_w * (1.0 - d / sep_r) * olen / bl;
                    want.0 -= dx / (d * bl) * push;
                    want.1 -= dy / (d * bl) * push;
                }
                // Look ahead to the closest approach, so crossing koi veer early instead of
                // meeting and then pushing apart.
                let (rvx, rvy) = (ovx - f.vx, ovy - f.vy);
                let closing = rvx * rvx + rvy * rvy;
                if closing > 1e-6 {
                    let t = (-(dx * rvx + dy * rvy) / closing).clamp(0.0, 3.0);
                    let (cx, cy) = (dx + rvx * t, dy + rvy * t);
                    let miss = cx.hypot(cy).max(0.001) / bl;
                    if t > 0.0 && miss < sep_r {
                        let push = 3.0 * sep_w * (1.0 - miss / sep_r) * (1.0 - t / 3.0);
                        want.0 -= cx / (miss * bl) * push;
                        want.1 -= cy / (miss * bl) * push;
                        // Head on, the miss direction is noise, so both keep to their right.
                        if ovx * fwd.0 + ovy * fwd.1 < -0.5 * ovx.hypot(ovy) {
                            want.0 -= fwd.1 * push * 0.7;
                            want.1 += fwd.0 * push * 0.7;
                        }
                        // Slowing down tightens the turn, so a koi about to meet one ahead holds back.
                        if d > 0.5 && dx * fwd.0 + dy * fwd.1 > 0.0 {
                            yield_to = yield_to.min(0.3 + 0.7 * (miss / sep_r).max(t / 3.0));
                        }
                    }
                }
                if !feeding && d < 3.0 {
                    align = (align.0 + ovx, align.1 + ovy);
                }
                if !feeding && d < 5.0 {
                    centre = (centre.0 + dx, centre.1 + dy);
                }
            }
            let align_len = align.0.hypot(align.1);
            if align_len > 0.001 {
                want.0 += align.0 / align_len * 0.25 * f.sociable;
                want.1 += align.1 / align_len * 0.25 * f.sociable;
            }
            let target_speed = if crowded && !feeding { target_speed.max(CRUISE * f.pace * knob) } else { target_speed } * yield_to;
            let centre_len = centre.0.hypot(centre.1);
            if centre_len > 0.001 {
                want.0 += centre.0 / centre_len * 0.15 * f.sociable;
                want.1 += centre.1 / centre_len * 0.15 * f.sociable;
            }

            // Containment: look ahead and follow the bank, turning the way the fish already leans.
            // The water layer paints rim stones around the edge; keep to the open water.
            let (inset, corner) = (0.07 * w.min(h) + 0.3 * bl, 1.5 * bl);
            let sdf = |px: f32, py: f32| {
                let qx = (px - w / 2.0).abs() - (w / 2.0 - inset - corner);
                let qy = (py - h / 2.0).abs() - (h / 2.0 - inset - corner);
                qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - corner
            };
            let probe = (f.x + fwd.0 * 1.5 * bl, f.y + fwd.1 * 1.5 * bl);
            let d = sdf(probe.0, probe.1).max(sdf(f.x, f.y));
            if d > -1.2 * bl {
                let (gx, gy) = (sdf(probe.0 + 1.0, probe.1) - sdf(probe.0 - 1.0, probe.1), sdf(probe.0, probe.1 + 1.0) - sdf(probe.0, probe.1 - 1.0));
                let g = gx.hypot(gy).max(0.001);
                let n = (gx / g, gy / g);
                let t = if -n.1 * fwd.0 + n.0 * fwd.1 >= 0.0 { (-n.1, n.0) } else { (n.1, -n.0) };
                let strength = smoothstep(-1.2 * bl, 0.0, d) * 2.5;
                want.0 += (t.0 - n.0 * 0.6) * strength;
                want.1 += (t.1 - n.1 * 0.6) * strength;
            }

            // Low-pass the wish, then turn towards it no faster than the cap.
            let (tau, turn_cap) = if eager || matches!(f.mood, Mood::Nuzzle { .. }) {
                (FEED_TAU, FEED_TURN)
            } else if feeding {
                (2.0 * FEED_TAU, CALM_FEED_TURN)
            } else {
                (CRUISE_TAU * calm, CRUISE_TURN)
            };
            let turn_cap = turn_cap / calm;
            let a = settle(tau);
            f.steer.0 += (want.0 - f.steer.0) * a;
            f.steer.1 += (want.1 - f.steer.1) * a;
            if f.steer.0.hypot(f.steer.1) > 0.001 {
                let error = wrap(f.steer.1.atan2(f.steer.0) - f.heading);
                f.turn += ((error * 2.0).clamp(-turn_cap, turn_cap) - f.turn) * settle(0.3);
            }
            f.turn = f.turn.clamp(-turn_cap, turn_cap);
            f.heading = wrap(f.heading + f.turn * DT);

            // Burst and coast: a few strong tail beats at the start of each cycle, then a glide.
            let (period, thrust) = if eager { (1.2, 1.6 * knob) } else { (2.5 * calm, 0.6 * knob) };
            f.gait = (f.gait + DT / period).fract();
            let now = f.speed / bl;
            let ratio = ((target_speed * DRAG + (target_speed - now)) / thrust).clamp(0.0, 0.85);
            let bursting = f.gait < ratio;
            let cap = if eager { FEED_CAP } else { CRUISE_CAP } * knob;
            let mut speed = now + if bursting { thrust * DT } else { 0.0 };
            let braking = f.mood != Mood::Cruise && speed > target_speed * 1.25;
            speed *= (-(DRAG + if braking || speed > cap { BRAKE } else { 0.0 }) * DT).exp();
            f.speed = speed.max(0.02) * bl;
            f.energy += (if matches!(f.mood, Mood::Rest { .. }) {
                0.1
            } else if at_hand {
                0.12
            } else if bursting {
                1.0
            } else {
                0.25
            } - f.energy)
                * settle(0.3);

            // Tail beat at Strouhal 0.3 for a 0.2 BL peak-to-peak tail: f = 0.3 U / 0.2. A
            // pleased koi flutters its tail faster than it swims.
            let beat = (1.5 * (f.speed / bl).max(0.1)).clamp(0.3, 2.0) + 1.2 * f.joy;
            f.phase = (f.phase + TAU * beat * DT).rem_euclid(TAU);

            let dir = (f.heading.cos(), f.heading.sin());
            f.vx = dir.0 * f.speed;
            f.vy = dir.1 * f.speed;
            f.x += f.vx * DT;
            f.y += f.vy * DT;

            let link = bl / (JOINTS - 1) as f32;
            f.spine[0] = (f.x + dir.0 * 0.5 * bl, f.y + dir.1 * 0.5 * bl);
            let mut angle = f.heading;
            for i in 1..JOINTS {
                let ((px, py), (jx, jy)) = (f.spine[i - 1], f.spine[i]);
                angle += wrap((py - jy).atan2(px - jx) - angle).clamp(-JOINT_BEND, JOINT_BEND);
                f.spine[i] = (px - angle.cos() * link, py - angle.sin() * link);
            }

            let wake = f.speed / (CRUISE * knob * bl) * (0.4 + 0.6 * f.energy);
            self.splashes.push(Splash { x: f.x - dir.0 * 0.3 * bl, y: f.y - dir.1 * 0.3 * bl, radius: 0.08 * bl, amount: -0.04 * wake });
        }

        // When two koi reach food together, the closer mouth wins; the other re-scores. A
        // petal is only mouthed by the koi that came for it, and the treat only bitten by the
        // koi whose turn it is.
        let mut i = 0;
        while i < self.food.len() {
            let p = &self.food[i];
            let eater = self
                .fish
                .iter()
                .enumerate()
                .filter(|&(k, f)| match (p.kind, f.mood) {
                    (FoodKind::Treat, _) => nibbler == Some(k),
                    (FoodKind::Petals, Mood::Approach { food }) => food == p.id && !p.mouthed,
                    (_, mood) => matches!(mood, Mood::Approach { .. }),
                })
                .map(|(k, f)| (k, (f.x + f.heading.cos() * 0.45 * f.len - p.x).hypot(f.y + f.heading.sin() * 0.45 * f.len - p.y) / f.len))
                .filter(|&(_, d)| d < EAT_RANGE)
                .min_by(|a, b| a.1.total_cmp(&b.1));
            let Some((k, _)) = eater else {
                i += 1;
                continue;
            };
            let spec = p.kind.spec();
            let f = &mut self.fish[k];
            f.mood = Mood::Eat { left: if p.kind == FoodKind::Pellets { 0.4 } else { 0.6 } };
            f.hunger = (f.hunger - spec.filling).max(0.0);
            self.splashes.push(Splash { x: p.x, y: p.y, radius: self.h * 0.02, amount: spec.splash.1 / 3.0 * (1.0 - p.depth()) });
            let p = &mut self.food[i];
            match p.kind {
                FoodKind::Petals => {
                    p.mouthed = true;
                    i += 1;
                }
                FoodKind::Treat => {
                    p.bites = p.bites.saturating_sub(1);
                    self.nibbler = None;
                    self.last_nibbler = k;
                    self.next_bite = TREAT_TURN;
                    if p.bites == 0 {
                        self.food.remove(i);
                    } else {
                        i += 1;
                    }
                }
                _ => {
                    self.food.remove(i);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ten minutes of pond time at a fixed seed, with a few handfuls of food, stays inside
    /// the calm limits: turn rate, speed and spacing.
    #[test]
    fn calm_limits_hold() {
        let (w, h) = (320.0, 190.0);
        let mut school = School::new(320, 190, 5, 7);
        let mut headings: Vec<f32> = school.fish.iter().map(|f| f.vy.atan2(f.vx)).collect();
        let (mut calm_turn, mut turn, mut calm_speed, mut speed) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
        // Koi may brush past or over each other, but never stay on top of one another.
        let n = school.fish.len();
        let (mut close, mut longest_close) = (vec![0u32; n * n], 0u32);
        let (mut eaten, mut dropped) = (0, 0);
        let life = FoodKind::Pellets.spec().floats + FoodKind::Pellets.spec().fades;
        for step in 0..36_000 {
            if step % 7_200 == 3_600 {
                for n in 0..4 {
                    dropped += school.drop_food(FoodKind::Pellets, w * (0.3 + 0.12 * n as f32), h * (0.35 + 0.1 * n as f32));
                }
            }
            let before = school.food.len();
            let sinking = school.food.iter().filter(|p| p.age + DT >= life).count();
            school.step();
            school.splashes.clear();
            eaten += before - sinking - school.food.len();
            let calm = school.food.is_empty() && school.fish.iter().all(|f| matches!(f.mood, Mood::Cruise | Mood::Rest { .. }));
            for (k, f) in school.fish.iter().enumerate() {
                let heading = f.vy.atan2(f.vx);
                let rate = wrap(heading - headings[k]).abs() / DT;
                headings[k] = heading;
                let v = f.vx.hypot(f.vy) / f.len;
                turn = turn.max(rate);
                speed = speed.max(v);
                if calm {
                    calm_turn = calm_turn.max(rate);
                    calm_speed = calm_speed.max(v);
                }
                assert!((0.0..=w).contains(&f.x) && (0.0..=h).contains(&f.y), "koi {k} left the pond at step {step}: {} {}", f.x, f.y);
                for (j, o) in school.fish.iter().enumerate().skip(k + 1) {
                    let pair = k * n + j;
                    close[pair] = if (o.x - f.x).hypot(o.y - f.y) < 0.25 * f.len.max(o.len) { close[pair] + 1 } else { 0 };
                    longest_close = longest_close.max(close[pair]);
                }
            }
        }
        assert!(calm_turn <= CRUISE_TURN * 1.001, "calm turn rate {:.1} deg/s", calm_turn.to_degrees());
        assert!(turn <= FEED_TURN * 1.001, "turn rate {:.1} deg/s", turn.to_degrees());
        assert!(calm_speed <= CRUISE_CAP, "calm speed {calm_speed:.2} BL/s");
        assert!(speed <= FEED_CAP, "speed {speed:.2} BL/s");
        // Two koi cruising side by side can slide over each other for a few seconds, as at
        // different depths: up to 5.6 s across seeds, and the path differs between platforms'
        // float maths. Stuck together would be far longer.
        assert!(longest_close as f32 * DT <= 8.0, "two koi overlapped for {:.1} s", longest_close as f32 * DT);
        assert!(eaten * 5 >= dropped * 3, "only {eaten} of {dropped} pellets eaten");
    }

    /// A koi's speed over its calm cruising speed above which the frame-rate logic counts it
    /// as darting (`fps.dart_speed`).
    const DART: f32 = 1.6;

    /// Pellets land 1 to 3 at a time up to their cap, and the nearest koi rush in and gulp
    /// them all before they sink.
    #[test]
    fn pellets_are_gulped_by_a_rushing_koi() {
        let mut school = School::new(320, 190, 5, 21);
        for _ in 0..300 {
            school.step();
        }
        let (x, y) = (school.fish[0].x, school.fish[0].y);
        let landed = school.drop_food(FoodKind::Pellets, x, y);
        assert!((1..=3).contains(&landed), "{landed} pellets in a handful");
        let (mut fastest, mut sank) = (0.0f32, 0);
        for _ in 0..(25.0 / DT) as usize {
            school.step();
            fastest = fastest.max(school.max_speed_ratio());
            sank += school.food.iter().filter(|p| p.depth() > 0.0).count();
        }
        assert!(school.food.is_empty() && sank == 0, "{} pellets left, {sank} sinking steps", school.food.len());
        assert!(fastest > DART, "the koi only came at {fastest:.2} times cruise speed");

        for _ in 0..20 {
            school.drop_food(FoodKind::Pellets, x, y);
        }
        assert_eq!(school.food.len(), FoodKind::Pellets.spec().cap);
        assert_eq!(school.drop_food(FoodKind::Pellets, x, y), 0);
    }

    /// Each pellet draws one koi, not the whole pond. One more may come when a koi is beaten
    /// to its pellet and turns to another.
    #[test]
    fn a_pellet_draws_one_koi() {
        for seed in 1..=20 {
            let mut school = School::new(320, 190, 7, seed);
            for _ in 0..600 {
                school.step();
            }
            let (_, _, landed) = school.drop_food_random(FoodKind::Pellets);
            let mut came = [false; 7];
            for _ in 0..(15.0 / DT) as usize {
                school.step();
                for (k, f) in school.fish.iter().enumerate() {
                    came[k] |= matches!(f.mood, Mood::Approach { .. });
                }
            }
            let came = came.iter().filter(|&&c| c).count();
            assert!(came <= landed + 1, "seed {seed}: {came} koi came for {landed} pellets");
        }
    }

    /// Pressing on a koi on its way to food pets it: it leaves the food for the hand.
    #[test]
    fn a_koi_after_food_can_be_petted() {
        let mut school = School::new(320, 190, 5, 21);
        for _ in 0..300 {
            school.step();
        }
        let f = &school.fish[0];
        school.drop_food(FoodKind::Pellets, f.x + f.heading.cos() * 3.0 * f.len, f.y + f.heading.sin() * 3.0 * f.len);
        let k = loop {
            school.step();
            if let Some(k) = school.fish.iter().position(|f| matches!(f.mood, Mood::Approach { .. })) {
                break k;
            }
        };
        let (x, y) = school.fish[k].spine[3];
        assert_eq!(school.pet(x, y, 0.25, 6.0), Some(k));
        assert!(matches!(school.fish[k].mood, Mood::Nuzzle { .. }));
    }

    /// Flakes scatter wide and drift, and several koi come for them at cruise speed, so none
    /// darts.
    #[test]
    fn flakes_bring_several_koi_calmly() {
        let mut school = School::new(320, 190, 5, 21);
        for _ in 0..300 {
            school.step();
        }
        let landed = school.drop_food(FoodKind::Flakes, 160.0, 95.0);
        assert!((8..=12).contains(&landed), "{landed} flakes in a handful");
        let widest = school.food.iter().map(|p| (p.x - 160.0).hypot(p.y - 95.0)).fold(0.0, f32::max);
        assert!(widest > 0.04 * 190.0, "flakes landed within {widest:.1} px");
        let (mut fastest, mut eaters, mut drifted) = (0.0f32, [false; 5], 0.0f32);
        for _ in 0..(38.0 / DT) as usize {
            school.step();
            fastest = fastest.max(school.max_speed_ratio());
            for (k, f) in school.fish.iter().enumerate() {
                eaters[k] |= matches!(f.mood, Mood::Eat { .. });
            }
            for p in &school.food {
                drifted = drifted.max((p.x - p.origin.0).hypot(p.y - p.origin.1));
            }
        }
        assert!(fastest < DART, "a koi darted at {fastest:.2} times cruise speed");
        assert!(eaters.iter().filter(|&&e| e).count() >= 2, "only {eaters:?} nibbled");
        assert!(drifted > 2.0, "flakes drifted only {drifted:.2} px");
        assert!(school.food.is_empty(), "flakes outlived their life");
    }

    /// Petals stay on the surface and are never eaten: a koi mouths one at most once, and
    /// they fade after a minute.
    #[test]
    fn petals_are_mouthed_once_and_fade() {
        let mut school = School::new(320, 190, 5, 21);
        for _ in 0..300 {
            school.step();
        }
        let (x, y) = (school.fish[2].x, school.fish[2].y);
        let landed = school.drop_food(FoodKind::Petals, x, y);
        assert!((3..=5).contains(&landed), "{landed} petals in a handful");
        let mut mouthings = 0;
        let mut eating = [false; 5];
        for step in 0..(62.0 / DT) as usize {
            school.step();
            for (k, f) in school.fish.iter().enumerate() {
                let now = matches!(f.mood, Mood::Eat { .. });
                mouthings += usize::from(now && !eating[k]);
                eating[k] = now;
            }
            let age = step as f32 * DT;
            if age < 59.0 {
                assert_eq!(school.food.len(), landed, "a petal went at {age:.1} s");
            }
            assert!(school.food.iter().all(|p| p.depth() == 0.0), "a petal sank");
        }
        assert!((1..=landed).contains(&mouthings), "{mouthings} mouthings of {landed} petals");
        assert!(school.food.is_empty(), "petals outlived a minute");
    }

    /// Seeds sink from the moment they land, spiralling, and a koi that follows one dives a
    /// little and comes back up.
    #[test]
    fn seeds_spiral_down_and_koi_follow() {
        let mut empty = School::new(320, 190, 0, 9);
        let landed = empty.drop_food(FoodKind::Seeds, 160.0, 95.0);
        assert!((3..=6).contains(&landed), "{landed} seeds in a handful");
        let mut last = vec![(0.0f32, 0.0f32, 0.0f32); landed];
        for _ in 0..(10.0 / DT) as usize - 1 {
            empty.step();
            for (p, (depth, angle, turned)) in empty.food.iter().zip(&mut last) {
                let now = (p.y - p.origin.1).atan2(p.x - p.origin.0);
                assert!(p.depth() > *depth, "seed {} stopped sinking", p.id);
                // A seed starts at the centre of its spiral, where it has no angle yet.
                if *depth > 0.0 {
                    *turned += wrap(now - *angle).abs();
                }
                (*depth, *angle) = (p.depth(), now);
            }
        }
        for &(_, _, turned) in &last {
            assert!((turned / 10.0 - TAU / SPIRAL_TURN).abs() < 0.1, "a seed turned {turned:.2} radians in 10 s");
        }
        empty.step();
        empty.step();
        assert!(empty.food.is_empty(), "seeds still in the water after 10 s");

        let mut school = School::new(320, 190, 5, 21);
        for _ in 0..300 {
            school.step();
        }
        let f = &school.fish[1];
        school.drop_food(FoodKind::Seeds, f.x + f.heading.cos() * 1.5 * f.len, f.y + f.heading.sin() * 1.5 * f.len);
        let mut deepest = 0.0f32;
        for _ in 0..(10.0 / DT) as usize {
            school.step();
            deepest = school.fish.iter().map(|f| f.pose().depth).fold(deepest, f32::max);
        }
        assert!((0.15..=DIVE).contains(&deepest), "the deepest koi reached {deepest:.2}");
        for _ in 0..(10.0 / DT) as usize {
            school.step();
        }
        assert!(school.fish.iter().all(|f| f.depth < 0.02), "a koi stayed down");
    }

    /// The whole pond circles the treat and takes turns biting it until it is gone, even with
    /// petals floating nearer some koi. A second treat is refused while one floats.
    #[test]
    fn the_treat_is_circled_and_shared() {
        let mut school = School::new(320, 190, 5, 21);
        for _ in 0..300 {
            school.step();
        }
        let (x, y) = (school.fish[0].x, school.fish[0].y);
        let petals = school.drop_food(FoodKind::Petals, x, y);
        assert_eq!(school.drop_food(FoodKind::Treat, 160.0, 95.0), 1);
        assert_eq!(school.drop_food(FoodKind::Treat, 100.0, 60.0), 0);
        assert_eq!(school.food.len(), petals + 1);
        let (mut near, mut biters, mut fastest) = ([false; 5], [0; 5], 0.0f32);
        let (mut last_bite, mut bites, mut fade) = (0.0, 0, 1.0);
        for step in 0..(61.0 / DT) as usize {
            school.step();
            fastest = fastest.max(school.max_speed_ratio());
            let Some(treat) = school.food.iter().find(|p| p.kind == FoodKind::Treat) else { break };
            for (k, f) in school.fish.iter().enumerate() {
                near[k] |= (treat.x - f.x).hypot(treat.y - f.y) < (TREAT_RING + 1.0) * f.len;
            }
            if treat.fade() < fade - 0.01 {
                let now = step as f32 * DT;
                assert!(bites == 0 || now - last_bite >= TREAT_TURN, "bites {:.2} s apart", now - last_bite);
                (last_bite, bites) = (now, bites + 1);
                biters[school.last_nibbler] += 1;
            }
            fade = treat.fade();
            if step == (5.0 / DT) as usize {
                let coming = school.fish.iter().filter(|f| f.mood == Mood::Approach { food: treat.id }).count();
                assert_eq!(coming, 5, "only {coming} koi on their way after 5 s");
            }
        }
        assert!(near.iter().all(|&n| n), "not every koi came: {near:?}");
        assert!(biters.iter().filter(|&&b| b > 0).count() >= 3, "bites by koi: {biters:?}");
        assert!(fastest < DART, "a koi darted at {fastest:.2} times cruise speed");
        assert!(school.food.iter().all(|p| p.kind != FoodKind::Treat), "the treat outlived a minute, {bites} bites taken");
    }

    /// A press on a koi's spine pets it and a press a BL away does not. Held, the koi nuzzles
    /// the hand and grows pleased; let go, it lingers, calms and swims on. Never faster than
    /// a cruise.
    #[test]
    fn a_pressed_koi_nuzzles_and_settles() {
        let mut school = School::new(320, 190, 1, 21);
        for _ in 0..300 {
            school.step();
        }
        let f = &school.fish[0];
        let (x, y) = f.spine[3];
        let side = (-(f.spine[2].1 - f.spine[4].1), f.spine[2].0 - f.spine[4].0);
        let n = side.0.hypot(side.1);
        assert_eq!(school.pet(x + side.0 / n * f.len, y + side.1 / n * f.len, 0.25, 6.0), None);
        assert_eq!(school.pet(x, y, 0.25, 6.0), Some(0));
        // Pressed mid-body, it turns away and comes round to face the hand.
        let (mut fastest, mut bubbled) = (0.0f32, false);
        for _ in 0..(8.0 / DT) as usize {
            school.step();
            fastest = fastest.max(school.fish[0].speed / school.fish[0].len);
            bubbled |= !school.bubbles.is_empty();
            if matches!(school.fish[0].mood, Mood::Nuzzle { near, .. } if near > 1.0) {
                break;
            }
        }
        let f = &school.fish[0];
        assert!(matches!(f.mood, Mood::Nuzzle { near, .. } if near > 1.0), "{:?} after 8 s", f.mood);
        assert!(f.joy > 0.5 && bubbled, "joy {:.2}, bubbles {bubbled} while nuzzling", f.joy);
        school.let_go();
        assert!(matches!(school.fish[0].mood, Mood::Linger { .. }));
        let mut settled = None;
        for step in 0..(10.0 / DT) as usize {
            school.step();
            fastest = fastest.max(school.fish[0].speed / school.fish[0].len);
            if step == (4.0 / DT) as usize {
                assert!(school.fish[0].joy < 0.05, "joy {:.2} 4 s after letting go", school.fish[0].joy);
            }
            if settled.is_none() && school.fish[0].mood == Mood::Cruise {
                settled = Some(step as f32 * DT);
            }
        }
        assert!(settled.is_some_and(|t| (4.0..=8.1).contains(&t)), "back to cruising after {settled:?} s");
        assert!(fastest <= CRUISE_CAP, "petting brought a koi to {fastest:.2} BL/s");
    }

    /// `p` brings the nearest koi to a hand in the middle of the pond, calmly, and it nuzzles a
    /// while and moves on.
    #[test]
    fn p_brings_the_nearest_koi() {
        let mut school = School::new(320, 190, 5, 21);
        for _ in 0..300 {
            school.step();
        }
        let (x, y) = (160.0, 95.0);
        let nearest = school
            .fish
            .iter()
            .enumerate()
            .map(|(k, f)| (k, f.spine.iter().map(|&(sx, sy)| (sx - x).hypot(sy - y) / f.len).fold(f32::INFINITY, f32::min)))
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(k, _)| k);
        let k = school.pet(x, y, f32::INFINITY, 3.0);
        assert_eq!(k, nearest);
        let k = k.expect("a koi came");
        let (mut touched, mut fastest) = (None, 0.0f32);
        for step in 0..(15.0 / DT) as usize {
            school.step();
            let f = &school.fish[k];
            fastest = fastest.max(school.fish.iter().map(|f| f.speed / f.len).fold(0.0, f32::max));
            if touched.is_none() && (f.x + f.heading.cos() * 0.45 * f.len - x).hypot(f.y + f.heading.sin() * 0.45 * f.len - y) < NUZZLE_REACH * f.len {
                touched = Some(step as f32 * DT);
            }
        }
        assert!(touched.is_some_and(|t| t < 10.0), "the koi reached the hand at {touched:?} s");
        assert!(!matches!(school.fish[k].mood, Mood::Nuzzle { .. }), "still nuzzling after 15 s");
        assert!(fastest <= CRUISE_CAP, "a koi swam {fastest:.2} BL/s");
    }

    /// Random drops, and a click on a corner stone, land every piece in open water.
    #[test]
    fn food_lands_in_open_water() {
        for (w, h) in [(320, 190), (190, 320), (600, 110)] {
            let mut school = School::new(w, h, 0, 5);
            let (w, h) = (w as f32, h as f32);
            for n in 0..500 {
                school.food.clear();
                let kind = FoodKind::ALL[n % 5];
                if n == 0 {
                    school.drop_food(kind, 2.0, 2.0);
                } else {
                    school.drop_food_random(kind);
                }
                for p in &school.food {
                    assert_eq!(open_water(w, h, 0.0, p.x, p.y), (p.x, p.y), "{kind:?} at ({:.0}, {:.0}) in a {w}x{h} pond", p.x, p.y);
                }
            }
        }
    }
}
