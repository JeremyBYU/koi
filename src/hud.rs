//! The HUD: a row of stones at the bottom of the window for music,
//! food, scene, time of day and help, always shown or (with `hud.show = "auto"`) hidden until
//! a HUD key or `Tab`. `Hud` owns the state
//! machine and its timers, hit tests clicks against cell rectangles, and sends each element
//! as its own Kitty image at z=-2, only when it changes. Text is terminal text over the
//! images, foreground colour only, erased with ECH.

use crate::config::{self, Align, Show};
use crate::layers::Grid;
use koi_render::hud::{Look, Mark, Swatch};
use koi_sim::FoodKind;
use koi_term::{Input, ShmRing};
use koi_theme::{Catalog, Rgb, Summary, Theme, Time};
use std::io;
use std::time::{Duration, Instant};

const FADE_IN: f32 = 0.25;
const FADE_OUT: f32 = 0.6;
const DWELL: f32 = 0.5;
/// How long a tray stays after a choice, so the choice is seen landing.
const TRAY_CLOSE: f32 = 0.4;
/// How long a food key held the food tray open while expanded.
const TRAY_FLASH: f32 = 0.9;
/// Below this many rows there is no expanded row, tray or help card; peeks still show.
const MIN_ROWS: usize = 14;
/// The most items a tray lists, so it fits a ring slot (`largest_image`). `t` still steps
/// through every family.
const TRAY_MOST: usize = 15;
/// The longest chip label, so a long track title or theme name fits a ring slot.
const CHIP_MOST: usize = 44;
const FIRST_ID: u32 = 60;

/// What the caller does for the HUD. The HUD shows the result when the caller reports it
/// back: `track` and `volume` for the music, `set_theme` for a theme switch.
#[derive(Debug, PartialEq)]
pub enum Action {
    NextTrack,
    /// Volume up (`true`) or down by one step.
    Volume(bool),
    Mute,
    /// Step to the next (`true`) or previous family, as `Catalog::next_scene` does.
    Scene(bool),
    /// Step to the next later (`true`) or earlier time, as `Catalog::next_time` does.
    Time(bool),
    /// Switch to this theme id, picked from a tray.
    Theme(String),
}

/// What became of an input event.
#[derive(Debug, PartialEq)]
pub enum Reply {
    /// Not the HUD's: the caller handles it as usual (a click on the water feeds, `q`
    /// quits). A pointer move comes back as `Pass` when it changed nothing on screen and
    /// must not count as activity.
    Pass,
    /// The HUD used it. It counts as input activity.
    Took,
    /// The HUD used it and the caller must act.
    Act(Action),
}

/// The HUD's images, in id order (60 to 67), so a tray draws over a pebble it touches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Element {
    Music,
    Food,
    Scene,
    Time,
    Help,
    Tray,
    Card,
    Chip,
}

impl Element {
    /// The element's place in `ELEMENTS`, and so in every per-element array.
    const fn slot(self) -> usize {
        match self {
            Element::Music => 0,
            Element::Food => 1,
            Element::Scene => 2,
            Element::Time => 3,
            Element::Help => 4,
            Element::Tray => 5,
            Element::Card => 6,
            Element::Chip => 7,
        }
    }
}

const ELEMENTS: [Element; 8] = [Element::Music, Element::Food, Element::Scene, Element::Time, Element::Help, Element::Tray, Element::Card, Element::Chip];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tray {
    Food,
    Scene,
    Time,
}

/// Where the pointer is, down to the part of the pill and the tray item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Spot {
    Note,
    Title,
    Next,
    Food,
    Scene,
    Time,
    Help,
    Item(usize),
    Card,
    Chip,
}

/// Cells, 1-based like the terminal's own coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Rect {
    col: usize,
    row: usize,
    w: usize,
    h: usize,
}

impl Rect {
    fn contains(self, col: usize, row: usize) -> bool {
        col >= self.col && col < self.col + self.w && row >= self.row && row < self.row + self.h
    }
}

/// An alpha that moves toward 1 while `on` and toward 0 otherwise, from `from` at `since`.
#[derive(Clone, Copy)]
struct Fade {
    on: bool,
    since: Instant,
    from: f32,
}

struct Peek {
    element: Element,
    /// Shown in the chip above the element. Empty shows no chip.
    label: String,
    until: Instant,
}

/// A family in the scene tray, with the theme a pick switches to.
struct Family {
    name: String,
    family: String,
    id: String,
    swatch: Swatch,
}

/// One element's image as last sent: where, how it looks, and its fade step (1 to 4).
#[derive(Clone, PartialEq)]
struct View {
    rect: Rect,
    step: usize,
    look: Painting,
}

/// Everything an element's picture depends on.
#[derive(Clone, PartialEq)]
struct Painting {
    /// Counts theme switches.
    theme: u32,
    w: usize,
    h: usize,
    lifted: bool,
    marks: Vec<(Mark, f32, f32)>,
}

/// A run of terminal text: column, row, text, colour.
type Span = (usize, usize, String, Rgb);

pub struct Hud {
    /// `hud.hover` needs motion reports (`koi_term::report_motion`). Replaced when
    /// config.toml reloads.
    pub settings: config::Hud,
    /// The keys the HUD answers to, and the ones it lists on the help card. Replaced when
    /// config.toml reloads.
    pub keys: config::Input,
    cols: usize,
    rows: usize,
    cell_w: usize,
    cell_h: usize,
    look: Look,
    theme: u32,

    food: FoodKind,
    /// Whether the music pill has a place: audio is on and found music.
    music: bool,
    volume: f32,
    muted: bool,
    title: String,
    summary: Option<Summary>,
    swatch: Swatch,
    families: Vec<Family>,
    times: Vec<(Time, String)>,

    expanded: bool,
    help: bool,
    tray: Option<Tray>,
    tray_close: Option<Instant>,
    peek: Option<Peek>,
    last_input: Instant,
    pointer: Option<(usize, usize)>,
    dwell: Option<Instant>,
    hover: Option<Spot>,
    fades: [Fade; 8],

    /// What the terminal shows for each element, and what it holds: a hidden element keeps
    /// its image data, so showing it unchanged is a placement only.
    shown: [Option<View>; 8],
    held: [Option<View>; 8],
    /// Each element's picture at full alpha, for fade steps.
    painted: [Option<(Painting, Vec<u8>)>; 8],
    text: Vec<Span>,
    /// In tmux, the background each span of `text` was written on.
    backs: Vec<Rgb>,
}

impl Hud {
    /// A HUD for `theme` on `grid`, with its row shown if `hud.show` is "always", set up by `cfg.hud` and `cfg.input`, with the music
    /// at `volume` (0 to 1) and `muted`. Food starts as pellets.
    pub fn new(cfg: &config::Config, grid: &Grid, theme: &Theme, catalog: &Catalog, volume: f32, muted: bool, now: Instant) -> Hud {
        let off = Fade { on: false, since: now, from: 0.0 };
        let mut hud = Hud {
            settings: cfg.hud.clone(),
            keys: cfg.input.clone(),
            cols: grid.cols,
            rows: grid.rows,
            cell_w: grid.cell_w,
            cell_h: grid.cell_h,
            look: Look::new(theme, grid.cell_h),
            theme: 0,
            food: FoodKind::Pellets,
            music: cfg.audio.enabled,
            volume,
            muted,
            title: String::new(),
            summary: None,
            swatch: Swatch::new(&theme.palette),
            families: Vec::new(),
            times: Vec::new(),
            expanded: false,
            help: false,
            tray: None,
            tray_close: None,
            peek: None,
            last_input: now,
            pointer: None,
            dwell: None,
            hover: None,
            fades: [off; 8],
            shown: Default::default(),
            held: Default::default(),
            painted: Default::default(),
            text: Vec::new(),
            backs: Vec::new(),
        };
        hud.set_theme(theme, catalog, now);
        hud.tick(now);
        hud
    }

    /// The food drops are of.
    pub fn food(&self) -> FoodKind {
        self.food
    }

    /// Takes the new window size. The caller has deleted every image and cleared the
    /// screen, so everything visible is sent again. A window under `MIN_ROWS` closes the
    /// row, the tray and the card.
    pub fn resize(&mut self, grid: &Grid, theme: &Theme) {
        (self.cols, self.rows, self.cell_w, self.cell_h) = (grid.cols, grid.rows, grid.cell_w, grid.cell_h);
        self.look = Look::new(theme, grid.cell_h);
        self.shown = Default::default();
        self.held = Default::default();
        self.painted = Default::default();
        self.forget_text();
        self.pointer = None;
        self.hover = None;
        if self.rows < MIN_ROWS {
            (self.expanded, self.help, self.tray) = (false, false, None);
        }
    }

    /// The screen lost the HUD's text (the tmux placeholder cells were written over it), so
    /// the next `draw` or `compose` writes it all again.
    pub fn forget_text(&mut self) {
        self.text.clear();
        self.backs.clear();
    }

    /// Repaints for `theme` and reads its family and times from `catalog`. A new family
    /// peeks the scene stone with the theme's name, a new time the time stone; the first
    /// call and a reload of the same theme peek nothing.
    pub fn set_theme(&mut self, theme: &Theme, catalog: &Catalog, now: Instant) {
        let current = &theme.summary;
        match &self.summary {
            Some(before) if before.family != current.family => self.show_peek(Element::Scene, current.name.clone(), now),
            Some(before) if before.time != current.time => self.show_peek(Element::Time, current.time.name().to_string(), now),
            _ => {}
        }
        self.summary = Some(current.clone());
        self.look = Look::new(theme, self.cell_h);
        self.swatch = Swatch::new(&theme.palette);
        self.theme += 1;

        // Walk the families in catalog order the way `t` does, at the current time. A family
        // name no theme has makes `next_scene` start at the first family.
        self.families.clear();
        let mut from = Summary { family: String::new(), ..current.clone() };
        while let Some(id) = catalog.next_scene(&from, true) {
            let Ok(pick) = catalog.resolve(&id) else { break };
            if self.families.iter().any(|f| f.family == pick.summary.family) {
                break;
            }
            from.family = pick.summary.family.clone();
            let swatch = if pick.summary.id == current.id { self.swatch } else { Swatch::new(&pick.palette) };
            self.families.push(Family { name: pick.summary.name, family: pick.summary.family, id, swatch });
        }
        let mut times: Vec<(Time, String)> = catalog.summaries().into_iter().filter(|s| !s.hidden && s.family == current.family).map(|s| (s.time, s.id)).collect();
        times.sort();
        if times.is_empty() {
            times.push((current.time, current.id.clone()));
        }
        self.times = times;
    }

    /// A track started: the pill shows `title`, and peeks. The chip shows the title only
    /// when the pill cannot show all of it.
    pub fn track(&mut self, title: &str, now: Instant) {
        self.title = title.to_string();
        let label = if title.chars().count() > self.title_room() { title.to_string() } else { String::new() };
        self.show_peek(Element::Music, label, now);
    }

    /// The volume or mute changed. Peeks the pill with what changed; a report of what the
    /// HUD already shows peeks nothing.
    pub fn volume(&mut self, level: f32, muted: bool, now: Instant) {
        let label = if muted != self.muted {
            if muted { "muted".to_string() } else { "sound on".to_string() }
        } else if (level - self.volume).abs() > 0.01 {
            format!("volume {:.0}%", level * 100.0)
        } else {
            return;
        };
        (self.volume, self.muted) = (level, muted);
        self.show_peek(Element::Music, label, now);
    }

    /// No music was found: the pill and its peeks go, and the row closes up.
    pub fn no_music(&mut self) {
        self.music = false;
    }

    /// Whether an element is fading, so the frame rate should stay up.
    pub fn fading(&self, now: Instant) -> bool {
        (0..ELEMENTS.len()).any(|k| {
            let a = self.alpha(k, now);
            if self.fades[k].on { a < 1.0 } else { a > 0.0 }
        })
    }

    /// Handles one input event. See `Reply`.
    pub fn input(&mut self, event: &Input, now: Instant) -> Reply {
        self.tick(now);
        let reply = match *event {
            Input::Key(byte) => self.key(char::from(byte), now),
            Input::Escape => {
                self.last_input = now;
                if self.help {
                    self.help = false;
                } else if self.tray.is_some() {
                    self.tray = None;
                } else {
                    self.expanded = false;
                    self.peek = None;
                }
                Reply::Took
            }
            Input::Click { col, row } => self.click(col, row, now),
            Input::Scroll { col, row, up } => {
                let reply = match self.hit(col, row, now) {
                    Some(Spot::Note | Spot::Title | Spot::Next) => Reply::Act(Action::Volume(up)),
                    Some(Spot::Food) => {
                        let n = FoodKind::ALL.len();
                        let next = FoodKind::ALL[(self.food.index() + if up { n - 1 } else { 1 }) % n];
                        self.pick_food(next, now)
                    }
                    _ => return Reply::Pass,
                };
                self.last_input = now;
                reply
            }
            Input::Move { col, row } if self.settings.hover => {
                self.pointer = Some((col, row));
                if row + 4 <= self.rows || self.expanded {
                    self.dwell = None;
                } else if self.dwell.is_none() {
                    self.dwell = Some(now);
                }
                let spot = self.hit(col, row, now);
                if spot == self.hover {
                    return Reply::Pass;
                }
                self.hover = spot;
                if spot.is_some() {
                    self.last_input = now;
                }
                Reply::Took
            }
            Input::Focus(false) => {
                (self.pointer, self.hover, self.dwell) = (None, None, None);
                Reply::Pass
            }
            Input::Move { .. } | Input::Focus(true) | Input::Other => Reply::Pass,
        };
        // Start the fades now, not at the next frame.
        self.tick(now);
        reply
    }

    fn key(&mut self, key: char, now: Instant) -> Reply {
        let keys = &self.keys;
        let room = self.rows >= MIN_ROWS;
        if self.help && key != keys.help {
            self.help = false;
            self.last_input = now;
        }
        let reply = if key == keys.hud {
            if self.expanded {
                (self.expanded, self.tray) = (false, None);
                if self.settings.show == Show::Always {
                    self.settings.show = Show::Auto;
                }
            } else if room {
                (self.expanded, self.peek) = (true, None);
                if self.settings.show == Show::Auto {
                    self.settings.show = Show::Always;
                }
            }
            Reply::Took
        } else if key == keys.help {
            self.help = !self.help && room;
            Reply::Took
        } else if key == keys.next_track {
            Reply::Act(Action::NextTrack)
        } else if key == keys.mute {
            Reply::Act(Action::Mute)
        } else if key == keys.volume_up || key == keys.volume_down {
            Reply::Act(Action::Volume(key == keys.volume_up))
        } else if key == keys.next_theme || key == keys.prev_theme {
            if self.families.len() > 1 {
                Reply::Act(Action::Scene(key == keys.next_theme))
            } else {
                self.show_peek(Element::Scene, "only one scene".to_string(), now);
                Reply::Took
            }
        } else if key == keys.later || key == keys.earlier {
            if self.times.len() > 1 {
                Reply::Act(Action::Time(key == keys.later))
            } else {
                self.show_peek(Element::Time, format!("{} only", self.times[0].0.name()), now);
                Reply::Took
            }
        } else if let Some(k) = keys.food.iter().position(|&f| f == key) {
            if self.expanded {
                self.tray = Some(Tray::Food);
                self.tray_close = Some(now + Duration::from_secs_f32(TRAY_FLASH));
            }
            self.pick_food(FoodKind::ALL[k], now)
        } else {
            return Reply::Pass;
        };
        self.last_input = now;
        reply
    }

    fn click(&mut self, col: usize, row: usize, now: Instant) -> Reply {
        let spot = self.hit(col, row, now);
        if self.help && !matches!(spot, Some(Spot::Card | Spot::Help)) {
            self.help = false;
            self.last_input = now;
            return Reply::Took;
        }
        let Some(spot) = spot else {
            self.tray = None;
            return Reply::Pass;
        };
        self.last_input = now;
        if !self.expanded && spot != Spot::Card {
            if self.rows >= MIN_ROWS {
                (self.expanded, self.peek) = (true, None);
            }
            return Reply::Took;
        }
        match spot {
            Spot::Note => Reply::Act(Action::Mute),
            Spot::Next => Reply::Act(Action::NextTrack),
            Spot::Help => {
                self.help = !self.help;
                Reply::Took
            }
            Spot::Food | Spot::Scene | Spot::Time => {
                let tray = match spot {
                    Spot::Food => Tray::Food,
                    Spot::Scene => Tray::Scene,
                    _ => Tray::Time,
                };
                self.tray = if self.tray == Some(tray) { None } else { Some(tray) };
                self.tray_close = None;
                Reply::Took
            }
            Spot::Item(i) => {
                self.tray_close = Some(now + Duration::from_secs_f32(TRAY_CLOSE));
                match self.tray {
                    Some(Tray::Food) => self.pick_food(FoodKind::ALL[i], now),
                    Some(Tray::Scene) => match &self.families[i] {
                        family if Some(&family.family) != self.summary.as_ref().map(|s| &s.family) => Reply::Act(Action::Theme(family.id.clone())),
                        family => {
                            self.show_peek(Element::Scene, family.name.clone(), now);
                            Reply::Took
                        }
                    },
                    Some(Tray::Time) => match &self.times[i] {
                        (_, id) if Some(id) != self.summary.as_ref().map(|s| &s.id) => Reply::Act(Action::Theme(id.clone())),
                        (time, _) => {
                            self.show_peek(Element::Time, time.name().to_string(), now);
                            Reply::Took
                        }
                    },
                    None => Reply::Took,
                }
            }
            Spot::Title | Spot::Card | Spot::Chip => Reply::Took,
        }
    }

    fn pick_food(&mut self, kind: FoodKind, now: Instant) -> Reply {
        self.food = kind;
        // An open food tray names the food itself. A chip as well would show once the tray
        // closes, over its fading edge.
        let label = if self.tray == Some(Tray::Food) { String::new() } else { kind.name().to_string() };
        self.show_peek(Element::Food, label, now);
        Reply::Took
    }

    fn show_peek(&mut self, element: Element, label: String, now: Instant) {
        if element == Element::Music && !self.music {
            return;
        }
        let room = self.cols.saturating_sub(4).min(CHIP_MOST);
        let label = if label.chars().count() > room { label.chars().take(room.saturating_sub(1)).chain(['…']).collect() } else { label };
        self.peek = Some(Peek { element, label, until: now + Duration::from_secs_f32(self.settings.peek_secs) });
    }

    /// Runs the timers and points each element's fade at whether it should show.
    fn tick(&mut self, now: Instant) {
        if self.dwell.is_some_and(|since| now - since >= Duration::from_secs_f32(DWELL)) && !self.expanded && self.rows >= MIN_ROWS {
            (self.expanded, self.peek, self.dwell, self.last_input) = (true, None, None, now);
        }
        if self.peek.as_ref().is_some_and(|p| now >= p.until) {
            self.peek = None;
        }
        if self.tray_close.is_some_and(|at| now >= at) {
            (self.tray, self.tray_close) = (None, None);
        }
        let over = self.pointer.is_some_and(|(col, row)| row + 4 > self.rows || self.hit(col, row, now).is_some());
        if self.settings.show == Show::Always && self.rows >= MIN_ROWS {
            self.expanded = true;
        } else if self.expanded && !self.help && !over && now - self.last_input >= Duration::from_secs_f32(self.settings.hold_secs) {
            (self.expanded, self.tray, self.tray_close) = (false, None, None);
        }
        let visible = self.visible();
        for (k, element) in ELEMENTS.iter().enumerate() {
            let on = visible.contains(element);
            if on != self.fades[k].on {
                self.fades[k] = Fade { on, since: now, from: self.alpha(k, now) };
            }
        }
    }

    fn visible(&self) -> Vec<Element> {
        let mut shown = Vec::new();
        if self.expanded || self.help {
            if self.music {
                shown.push(Element::Music);
            }
            shown.extend([Element::Food, Element::Scene, Element::Time, Element::Help]);
            if self.tray.is_some() {
                shown.push(Element::Tray);
            }
            if self.help {
                shown.push(Element::Card);
            }
        }
        if let Some(peek) = &self.peek {
            shown.push(peek.element);
            if !peek.label.is_empty() && self.tray.is_none() && !self.help {
                shown.push(Element::Chip);
            }
        }
        shown
    }

    fn alpha(&self, k: usize, now: Instant) -> f32 {
        let fade = self.fades[k];
        if !self.settings.fade {
            return if fade.on { 1.0 } else { 0.0 };
        }
        let t = (now - fade.since).as_secs_f32();
        if fade.on { (fade.from + t / FADE_IN).min(1.0) } else { (fade.from - t / FADE_OUT).max(0.0) }
    }

    /// The fade in 4 steps: 0 is gone, 4 is solid.
    fn step(&self, k: usize, now: Instant) -> usize {
        let a = self.alpha(k, now);
        [0.0, 0.25, 0.5, 0.75].into_iter().filter(|&s| a > s + 0.001).count()
    }

    /// Columns of the music pill: 30, 20 in a narrow window, only the note under 44 columns.
    fn pill_w(&self) -> usize {
        if self.cols >= 64 {
            30
        } else if self.cols >= 44 {
            20
        } else {
            6
        }
    }

    /// Characters of the title the pill shows.
    fn title_room(&self) -> usize {
        match self.pill_w() {
            30 => 14,
            20 => 8,
            _ => 0,
        }
    }

    /// Tray items and the columns each takes.
    fn tray_items(&self, tray: Tray) -> (usize, usize) {
        let (n, widest) = match tray {
            Tray::Food => (FoodKind::ALL.len(), 6),
            Tray::Scene => (self.families.len().min(TRAY_MOST), 8),
            Tray::Time => (self.times.len().min(TRAY_MOST), 6),
        };
        (n, (32 / n.max(1)).clamp(3, widest))
    }

    /// Every element's cells, whether or not it shows. The chip has a place only while a
    /// peek has a label, the tray only while open, the music pill only while there is music.
    fn rects(&self) -> [Option<Rect>; 8] {
        let lead = if self.music { self.pill_w() + 2 } else { 0 };
        let width = lead + 28;
        let top = self.rows.saturating_sub(3).max(1);
        let c0 = if self.settings.align == Align::Left { 3 } else { (self.cols.saturating_sub(width) / 2 + 1).max(1) };
        let stone = |col: usize, w: usize| Some(Rect { col, row: top, w, h: 3 });
        let row = [stone(c0, self.pill_w()).filter(|_| self.music), stone(c0 + lead, 6), stone(c0 + lead + 8, 6), stone(c0 + lead + 16, 6), stone(c0 + lead + 24, 4)];
        let tray = self.tray.map(|tray| {
            let (n, iw) = self.tray_items(tray);
            let w = (n * iw + 2).max(16);
            let under = row[match tray {
                Tray::Food => Element::Food,
                Tray::Scene => Element::Scene,
                Tray::Time => Element::Time,
            }
            .slot()]
            .expect("the row's stones always have a place");
            Rect { col: (under.col + 3).saturating_sub(w / 2).min(self.cols.saturating_sub(w)).max(2), row: top.saturating_sub(5).max(1), w, h: 5 }
        });
        let card = Rect { col: (c0 + width / 2).saturating_sub(17).max(1), row: top.saturating_sub(7).max(1), w: 34, h: 7 };
        let chip = self.peek.as_ref().filter(|p| !p.label.is_empty()).map(|peek| {
            let under = row[peek.element.slot().min(4)].expect("the row's stones always have a place");
            let w = peek.label.chars().count() + 2;
            Rect { col: (under.col + under.w / 2).saturating_sub(w / 2).min(self.cols.saturating_sub(w)).max(2), row: top.saturating_sub(1).max(1), w, h: 1 }
        });
        [row[0], row[1], row[2], row[3], row[4], tray, Some(card), chip]
    }

    /// What is under a cell, among the elements at least half faded in. Front to back.
    fn hit(&self, col: usize, row: usize, now: Instant) -> Option<Spot> {
        let rects = self.rects();
        for element in [Element::Card, Element::Tray, Element::Chip, Element::Music, Element::Food, Element::Scene, Element::Time, Element::Help] {
            let k = element.slot();
            let Some(rect) = rects[k].filter(|r| r.contains(col, row)) else { continue };
            if !self.fades[k].on || self.alpha(k, now) < 0.5 {
                continue;
            }
            return Some(match element {
                Element::Music => {
                    let dc = col - rect.col;
                    if rect.w <= 6 || dc <= 4 {
                        Spot::Note
                    } else if dc >= rect.w - 5 {
                        Spot::Next
                    } else {
                        Spot::Title
                    }
                }
                Element::Food => Spot::Food,
                Element::Scene => Spot::Scene,
                Element::Time => Spot::Time,
                Element::Help => Spot::Help,
                Element::Tray => {
                    let tray = self.tray.expect("the tray has a place only while open");
                    let (n, iw) = self.tray_items(tray);
                    let pad = (rect.w - n * iw) / 2;
                    Spot::Item(((col - rect.col).saturating_sub(pad) / iw).min(n.saturating_sub(1)))
                }
                Element::Card => Spot::Card,
                Element::Chip => Spot::Chip,
            });
        }
        None
    }

    /// The picture of element `k` in `rect`.
    fn painting(&self, k: usize, rect: Rect) -> Painting {
        let (cw, ch) = (self.cell_w as f32, self.cell_h as f32);
        let (w, h) = (rect.w * self.cell_w, rect.h * self.cell_h);
        let k_px = ch / 20.0;
        let hovered = self.hover.filter(|_| self.settings.hover);
        // The centre of the stone Look::paint draws in this rect: it leaves room for the
        // shadow on the right and bottom, so it sits up and left of the rect's centre.
        let top = if h < 30 { 1.0 } else { 2.0 } * k_px;
        let (mid_x, mid_y) = ((w as f32 - 3.0 * k_px) / 2.0, top + (h as f32 - top - k_px) / 2.0);
        let pebble = |mark: Mark, spot: Spot| (hovered == Some(spot), vec![(mark, mid_x, mid_y)]);
        let (lifted, marks) = match ELEMENTS[k] {
            Element::Music => {
                // The note's head hangs below its anchor, so it is anchored 2 px low to look centred.
                let mut marks = vec![(Mark::Note { muted: self.muted }, 3.0 * cw, mid_y + 2.0 * k_px)];
                if rect.w > 6 {
                    marks.push((Mark::Next, (rect.w as f32 - 3.5) * cw, mid_y));
                }
                if rect.w >= 30 {
                    let steps = if self.muted { 0.0 } else { (self.volume * 10.0).round() };
                    marks.extend((0..5).map(|i| (Mark::Dot { fill: (steps - 2.0 * i as f32).clamp(0.0, 2.0) / 2.0 }, (rect.w as f32 - 10.0 + 0.9 * i as f32) * cw, mid_y)));
                }
                (matches!(hovered, Some(Spot::Note | Spot::Title | Spot::Next)), marks)
            }
            Element::Food => pebble(Mark::Food(self.food.index()), Spot::Food),
            Element::Scene => pebble(Mark::Swatch(self.swatch), Spot::Scene),
            Element::Time => pebble(Mark::Time(self.summary.as_ref().map_or(Time::Noon, |s| s.time)), Spot::Time),
            Element::Help => (hovered == Some(Spot::Help), vec![(Mark::Help, mid_x, mid_y)]),
            Element::Tray => {
                let mut marks = Vec::new();
                if let Some(tray) = self.tray {
                    let (n, iw) = self.tray_items(tray);
                    let pad = (rect.w - n * iw) / 2;
                    for i in 0..n {
                        let (mark, chosen) = match tray {
                            Tray::Food => (Mark::Food(i), self.food.index() == i),
                            Tray::Scene => (Mark::Swatch(self.families[i].swatch), self.summary.as_ref().is_some_and(|s| s.family == self.families[i].family)),
                            Tray::Time => (Mark::Time(self.times[i].0), self.summary.as_ref().is_some_and(|s| s.id == self.times[i].1)),
                        };
                        let cx = (pad as f32 + (i * iw) as f32 + iw as f32 / 2.0) * cw - 1.0;
                        let cy = 1.1 * ch - if chosen { 2.0 * k_px } else { 0.0 };
                        if hovered == Some(Spot::Item(i)) {
                            marks.push((Mark::Glow, cx, cy));
                        }
                        marks.push((mark, cx, cy));
                        if chosen {
                            marks.push((Mark::Chosen, cx, 2.0 * ch - 2.0 * k_px));
                        }
                    }
                }
                (false, marks)
            }
            Element::Card | Element::Chip => (false, Vec::new()),
        };
        Painting { theme: self.theme, w, h, lifted, marks }
    }

    /// The terminal text on the HUD, for elements at least three quarters faded in.
    fn spans(&self, rects: &[Option<Rect>; 8], now: Instant) -> Vec<Span> {
        let (ink, dim) = (self.look.colors.ink, self.look.colors.dim);
        let shown = |e: Element| rects[e.slot()].filter(|_| self.fades[e.slot()].on && self.step(e.slot(), now) >= 3);
        let mut spans = Vec::new();
        if let Some(pill) = shown(Element::Music) {
            let room = self.title_room();
            if room > 0 && !self.title.is_empty() {
                let title = if self.title.chars().count() > room { self.title.chars().take(room - 1).chain(['…']).collect() } else { self.title.clone() };
                spans.push((pill.col + 5, pill.row + 1, title, ink));
            }
        }
        if let (Some(chip), Some(peek)) = (shown(Element::Chip), &self.peek) {
            spans.push((chip.col + 1, chip.row, peek.label.clone(), ink));
        }
        if let (Some(tray), Some(which)) = (shown(Element::Tray), self.tray) {
            let (n, iw) = self.tray_items(which);
            let pad = (tray.w - n * iw) / 2;
            let names: Vec<String> = match which {
                Tray::Food => FoodKind::ALL.iter().map(|f| f.name().to_string()).collect(),
                Tray::Scene => self.families.iter().map(|f| f.name.clone()).collect(),
                Tray::Time => self.times.iter().map(|t| t.0.name().to_string()).collect(),
            };
            let chosen = match which {
                Tray::Food => self.food.index(),
                Tray::Scene => self.families.iter().position(|f| self.summary.as_ref().is_some_and(|s| s.family == f.family)).unwrap_or(0),
                Tray::Time => self.times.iter().position(|t| self.summary.as_ref().is_some_and(|s| s.id == t.1)).unwrap_or(0),
            };
            if which == Tray::Food {
                for (i, key) in self.keys.food.iter().enumerate() {
                    spans.push((tray.col + pad + i * iw + iw / 2 - 1, tray.row + 2, key.to_string(), dim));
                }
            }
            let pick = match self.hover {
                Some(Spot::Item(i)) if self.settings.hover => i,
                _ => chosen,
            };
            if let Some(name) = names.get(pick) {
                let name: String = name.chars().take(tray.w - 2).collect();
                spans.push((tray.col + (tray.w - name.chars().count()) / 2, tray.row + 3, name, ink));
            }
        }
        if let Some(card) = shown(Element::Card) {
            let k = &self.keys;
            let lines = [
                ((name(k.hud), "show, hide"), (name(k.help), "this card")),
                ((name(k.feed), "feed"), (format!("{}-{}", k.food[0], k.food[4]), "food")),
                ((name(k.next_track), "next track"), (name(k.mute), "mute")),
                ((format!("{} {}", k.volume_up, k.volume_down), "volume"), (format!("{} {}", k.next_theme, k.prev_theme), "scene")),
                ((format!("{} {}", k.later, k.earlier), "time of day"), (name(k.quit), "quit")),
            ];
            for (i, ((key1, what1), (key2, what2))) in lines.into_iter().enumerate() {
                let row = card.row + 1 + i;
                spans.push((card.col + 2, row, key1, dim));
                spans.push((card.col + 6, row, what1.to_string(), ink));
                spans.push((card.col + 19, row, key2, dim));
                spans.push((card.col + 23, row, what2.to_string(), ink));
            }
        }
        if self.settings.hover && self.expanded {
            let hint = match self.hover {
                Some(Spot::Food) => Some((Element::Food, 0, format!("{}-{}", self.keys.food[0], self.keys.food[4]))),
                Some(Spot::Scene) => Some((Element::Scene, 0, self.keys.next_theme.to_string())),
                Some(Spot::Time) => Some((Element::Time, 0, self.keys.later.to_string())),
                Some(Spot::Note) => Some((Element::Music, 1, self.keys.mute.to_string())),
                Some(Spot::Title) => Some((Element::Music, 11, format!("{} {}", self.keys.volume_down, self.keys.volume_up))),
                Some(Spot::Next) => Some((Element::Music, 26, self.keys.next_track.to_string())),
                _ => None,
            };
            if let Some((element, at, text)) = hint
                && let Some(rect) = shown(element)
            {
                let col = if element == Element::Music { rect.col + at.min(rect.w.saturating_sub(4)) } else { rect.col + (6 - text.chars().count()) / 2 };
                spans.push((col, rect.row + 2, text, dim));
            }
        }
        spans
    }

    /// Appends to `out` whatever changed since the last call: element images through
    /// `ring`, placements, deletes and text. An idle HUD appends nothing. `ring` slots must
    /// hold `largest_image` bytes.
    pub fn draw(&mut self, out: &mut Vec<u8>, ring: &mut ShmRing, now: Instant) -> io::Result<()> {
        self.tick(now);
        let rects = self.rects();
        for (k, id) in (0..ELEMENTS.len()).zip(FIRST_ID..) {
            let step = self.step(k, now);
            let view = rects[k].filter(|_| step > 0).map(|rect| View { rect, step, look: self.painting(k, rect) });
            if view == self.shown[k] {
                continue;
            }
            let Some(view) = view else {
                out.extend_from_slice(format!("\x1b_Ga=d,d=i,i={id},q=2\x1b\\").as_bytes());
                self.shown[k] = None;
                continue;
            };
            let Rect { col, row, w, h } = view.rect;
            out.extend_from_slice(format!("\x1b[{row};{col}H").as_bytes());
            let held = self.held[k].as_ref().is_some_and(|held| held.step == view.step && held.look == view.look);
            if held {
                out.extend_from_slice(format!("\x1b_Ga=p,i={id},p=1,z=-2,C=1,q=2\x1b\\").as_bytes());
            } else {
                if self.painted[k].as_ref().is_none_or(|(look, _)| *look != view.look) {
                    let mut rgba = Vec::new();
                    self.look.paint(view.look.w, view.look.h, id, view.look.lifted, &view.look.marks, &mut rgba);
                    self.painted[k] = Some((view.look.clone(), rgba));
                }
                let (_, full) = self.painted[k].as_ref().expect("painted just above");
                let mut rgba = full.clone();
                if view.step < 4 {
                    for px in rgba.chunks_exact_mut(4) {
                        px[3] = u8::try_from(usize::from(px[3]) * view.step / 4).expect("at most 255");
                    }
                }
                ring.transmit(out, &rgba, &format!("a=T,f=32,s={},v={},i={id},p=1,c={w},r={h},z=-2,C=1", view.look.w, view.look.h))?;
                self.held[k] = Some(view.clone());
            }
            self.shown[k] = Some(view);
        }

        let spans = self.spans(&rects, now);
        if spans != self.text {
            for (col, row, text, _) in &self.text {
                out.extend_from_slice(format!("\x1b[{row};{col}H\x1b[{}X", text.chars().count()).as_bytes());
            }
            for (col, row, text, [r, g, b]) in &spans {
                out.extend_from_slice(format!("\x1b[{row};{col}H\x1b[38;2;{r};{g};{b}m{text}\x1b[0m").as_bytes());
            }
            self.text = spans;
        }
        Ok(())
    }

    /// tmux mode: draws what shows into `frame`, the window's image at the cell size the HUD
    /// was given, and appends to `out` the text if it or what lies under it changed. Text
    /// takes the place of the placeholder cells of image `id`, so each span gets the average
    /// colour of the frame under it as its background, and cells it leaves get their
    /// placeholders back.
    pub fn compose(&mut self, frame: &mut [u8], out: &mut Vec<u8>, id: u8, now: Instant) {
        self.tick(now);
        let rects = self.rects();
        let (cw, ch) = (self.cell_w, self.cell_h);
        let frame_w = self.cols * cw;
        for (k, seed) in (0..ELEMENTS.len()).zip(FIRST_ID..) {
            let step = self.step(k, now);
            let Some(rect) = rects[k].filter(|_| step > 0) else { continue };
            let look = self.painting(k, rect);
            if self.painted[k].as_ref().is_none_or(|(painted, _)| *painted != look) {
                let mut rgba = Vec::new();
                self.look.paint(look.w, look.h, seed, look.lifted, &look.marks, &mut rgba);
                self.painted[k] = Some((look, rgba));
            }
            let (look, rgba) = self.painted[k].as_ref().expect("painted just above");
            let (x, y) = ((rect.col - 1) * cw, (rect.row - 1) * ch);
            let position = |p: usize| i32::try_from(p).expect("a pixel position fits i32");
            crate::layers::blend(frame, frame_w, rgba, look.w, position(x), position(y), 1, u32::try_from(255 * step / 4).expect("at most 255"));
        }

        let spans = self.spans(&rects, now);
        let backs: Vec<Rgb> = spans
            .iter()
            .map(|(col, row, text, _)| {
                let mut sum = [0usize; 4];
                for y in (row - 1) * ch..(*row * ch).min(frame.len() / 4 / frame_w) {
                    for x in (col - 1) * cw..((col - 1 + text.chars().count()) * cw).min(frame_w) {
                        let px = &frame[(y * frame_w + x) * 4..][..3];
                        sum = [sum[0] + usize::from(px[0]), sum[1] + usize::from(px[1]), sum[2] + usize::from(px[2]), sum[3] + 1];
                    }
                }
                [0, 1, 2].map(|c| u8::try_from(sum[c] / sum[3].max(1)).unwrap_or(u8::MAX))
            })
            .collect();
        if spans == self.text && backs == self.backs {
            return;
        }
        for (col, row, text, _) in &self.text {
            koi_term::placeholders(out, id, row - 1, col - 1, text.chars().count());
        }
        for ((col, row, text, [r, g, b]), [br, bg, bb]) in spans.iter().zip(&backs) {
            out.extend_from_slice(format!("\x1b[{row};{col}H\x1b[48;2;{br};{bg};{bb}m\x1b[38;2;{r};{g};{b}m{text}\x1b[0m").as_bytes());
        }
        (self.text, self.backs) = (spans, backs);
    }
}

/// Bytes of the largest HUD image with `cell_w` x `cell_h` pixel cells: the help card, 34x7
/// cells, which also covers a tray up to 47 columns.
pub fn largest_image(cell_w: usize, cell_h: usize) -> usize {
    34 * cell_w * 7 * cell_h * 4
}

/// How a key is written on the help card.
fn name(key: char) -> String {
    match key {
        '\t' => "Tab".to_string(),
        ' ' => "Spc".to_string(),
        key => key.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The default config, with the row hidden until asked for and hover on.
    fn config() -> config::Config {
        use serde::Deserialize;
        let mut cfg = config::Config::deserialize(config::DEFAULT.parse::<toml::Table>().expect("default config parses")).expect("default config loads");
        (cfg.hud.show, cfg.hud.hover) = (Show::Auto, true);
        cfg
    }

    fn grid(cols: usize, rows: usize) -> Grid {
        Grid { cols, rows, cell_w: 10, cell_h: 20, water_w: cols, water_h: rows }
    }

    fn hud(name: &str, cols: usize, rows: usize, t0: Instant) -> Hud {
        let catalog = Catalog::load(None).0;
        let theme = catalog.resolve(name).expect("built-in theme resolves");
        Hud::new(&config(), &grid(cols, rows), &theme, &catalog, 0.6, false, t0)
    }

    fn at(t0: Instant, secs: f32) -> Instant {
        t0 + Duration::from_secs_f32(secs)
    }

    fn key(hud: &mut Hud, key: u8, now: Instant) -> Reply {
        hud.input(&Input::Key(key), now)
    }

    fn shown(hud: &mut Hud, now: Instant) -> Vec<Element> {
        hud.tick(now);
        (0..8).filter(|&k| hud.step(k, now) > 0).map(|k| ELEMENTS[k]).collect()
    }

    /// The row sits bottom centre, 60 columns, one empty row below; narrow windows shrink
    /// the pill, and with no music it goes and the row closes up.
    #[test]
    fn layout() {
        let t0 = Instant::now();
        let r = hud("summer-garden", 160, 45, t0).rects();
        let cells: Vec<(usize, usize, usize)> = r[..5].iter().map(|r| r.map(|r| (r.col, r.row, r.w)).expect("row stone")).collect();
        assert_eq!(cells, [(51, 42, 30), (83, 42, 6), (91, 42, 6), (99, 42, 6), (107, 42, 4)]);
        assert_eq!(r[6].map(|c| (c.row, c.w, c.h)), Some((35, 34, 7)));
        assert_eq!(hud("summer-garden", 50, 20, t0).rects()[0].map(|p| p.w), Some(20));
        assert_eq!(hud("summer-garden", 40, 20, t0).rects()[0].map(|p| p.w), Some(6));

        let mut quiet = hud("summer-garden", 160, 45, t0);
        quiet.no_music();
        quiet.volume(0.2, true, t0);
        key(&mut quiet, b'\t', t0);
        let r = quiet.rects();
        assert_eq!(r[..5].iter().map(|r| r.map(|r| r.col)).collect::<Vec<_>>(), [None, Some(67), Some(75), Some(83), Some(91)]);
        assert_eq!(shown(&mut quiet, at(t0, 1.0)), [Element::Food, Element::Scene, Element::Time, Element::Help]);
    }

    /// A food key peeks the food stone and its chip for 3 s, then a 0.6 s fade out.
    #[test]
    fn peek_times_out() {
        let t0 = Instant::now();
        let mut hud = hud("summer-garden", 160, 45, t0);
        assert!(shown(&mut hud, t0).is_empty());
        assert_eq!(key(&mut hud, b'3', t0), Reply::Took);
        assert_eq!(shown(&mut hud, at(t0, 0.3)), [Element::Food, Element::Chip]);
        assert!(!hud.fading(at(t0, 0.3)));
        assert_eq!(shown(&mut hud, at(t0, 3.2)), [Element::Food, Element::Chip]);
        assert!(hud.fading(at(t0, 3.2)));
        assert!(shown(&mut hud, at(t0, 3.9)).is_empty());
        assert_eq!(hud.food(), FoodKind::Petals);
    }

    /// Resting the pointer shows the row until 4 s after the last HUD input, and not while
    /// the pointer rests on it; `Tab` twice hides it at once.
    #[test]
    fn expanded_holds_while_pointed_at() {
        let t0 = Instant::now();
        let mut hud = hud("summer-garden", 160, 45, t0);
        hud.input(&Input::Move { col: 5, row: 44 }, t0);
        hud.tick(at(t0, 0.5));
        hud.input(&Input::Move { col: 5, row: 30 }, at(t0, 0.5));
        let row = [Element::Music, Element::Food, Element::Scene, Element::Time, Element::Help];
        assert_eq!(shown(&mut hud, at(t0, 1.0)), row);
        assert_eq!(shown(&mut hud, at(t0, 4.4)), row);
        assert_eq!(shown(&mut hud, at(t0, 4.6)), row, "fading out");
        assert!(shown(&mut hud, at(t0, 5.3)).is_empty());

        hud.input(&Input::Move { col: 60, row: 43 }, at(t0, 5.5));
        hud.tick(at(t0, 6.0));
        assert_eq!(hud.input(&Input::Move { col: 60, row: 43 }, at(t0, 6.3)), Reply::Took);
        assert_eq!(hud.input(&Input::Move { col: 61, row: 43 }, at(t0, 6.4)), Reply::Pass, "same spot, nothing changed");
        assert_eq!(shown(&mut hud, at(t0, 12.0)), row);
        hud.input(&Input::Focus(false), at(t0, 12.0));
        assert!(shown(&mut hud, at(t0, 12.7)).is_empty());

        key(&mut hud, b'\t', at(t0, 13.0));
        key(&mut hud, b'\t', at(t0, 13.5));
        assert!(shown(&mut hud, at(t0, 14.2)).is_empty());
    }

    /// Clicks: a hidden HUD lets clicks through, a click on a stone opens its tray, a tray
    /// pick acts and closes 0.4 s later, and a click on the water closes a tray and still
    /// feeds.
    #[test]
    fn clicks_and_trays() {
        let t0 = Instant::now();
        let mut hud = hud("summer-garden", 160, 45, t0);
        let click = |hud: &mut Hud, col, row, secs| hud.input(&Input::Click { col, row }, at(t0, secs));
        assert_eq!(click(&mut hud, 85, 43, 0.0), Reply::Pass);

        key(&mut hud, b'\t', t0);
        assert_eq!(click(&mut hud, 85, 43, 0.5), Reply::Took);
        assert!(shown(&mut hud, at(t0, 0.8)).contains(&Element::Tray));
        let tray = hud.rects()[5].expect("tray is open");
        assert_eq!((tray.row, tray.w), (37, 32));
        assert_eq!(click(&mut hud, tray.col + 1 + 6 + 3, 38, 1.0), Reply::Took);
        assert_eq!(hud.food(), FoodKind::Flakes);
        assert!(shown(&mut hud, at(t0, 1.3)).contains(&Element::Tray));
        shown(&mut hud, at(t0, 1.5));
        assert!(!shown(&mut hud, at(t0, 2.2)).contains(&Element::Tray));

        click(&mut hud, 85, 43, 2.5);
        assert!(shown(&mut hud, at(t0, 2.8)).contains(&Element::Tray));
        assert_eq!(click(&mut hud, 20, 10, 3.0), Reply::Pass);
        assert!(!hud.visible().contains(&Element::Tray));

        assert_eq!(click(&mut hud, 53, 43, 3.1), Reply::Act(Action::Mute));
        assert_eq!(click(&mut hud, 78, 43, 3.2), Reply::Act(Action::NextTrack));
        assert_eq!(click(&mut hud, 65, 43, 3.3), Reply::Took);
        assert_eq!(hud.input(&Input::Scroll { col: 65, row: 43, up: true }, at(t0, 3.4)), Reply::Act(Action::Volume(true)));
        assert_eq!(hud.input(&Input::Scroll { col: 85, row: 43, up: true }, at(t0, 3.5)), Reply::Took);
        assert_eq!(hud.food(), FoodKind::Pellets);
    }

    /// Resting the pointer in the bottom rows expands the row; Esc closes the card, then
    /// the tray, then the row; and a window too short for the row closes it.
    #[test]
    fn dwell_escape_and_shrinking() {
        let t0 = Instant::now();
        let mut hud = hud("summer-garden", 160, 45, t0);
        let row = [Element::Music, Element::Food, Element::Scene, Element::Time, Element::Help];
        hud.input(&Input::Move { col: 5, row: 30 }, t0);
        hud.input(&Input::Move { col: 5, row: 44 }, t0);
        assert!(shown(&mut hud, at(t0, 0.4)).is_empty());
        hud.tick(at(t0, 0.6));
        assert_eq!(shown(&mut hud, at(t0, 0.9)), row);

        let food = hud.rects()[Element::Food.slot()].expect("food stone");
        hud.input(&Input::Click { col: food.col + 1, row: food.row + 1 }, at(t0, 1.0));
        key(&mut hud, b'?', at(t0, 1.1));
        assert!(hud.help && hud.tray.is_some());
        hud.input(&Input::Escape, at(t0, 1.2));
        assert!(!hud.help && hud.tray.is_some());
        hud.input(&Input::Escape, at(t0, 1.3));
        assert!(hud.tray.is_none() && hud.expanded);
        hud.input(&Input::Escape, at(t0, 1.4));
        assert!(!hud.expanded);

        key(&mut hud, b'\t', at(t0, 2.0));
        key(&mut hud, b'?', at(t0, 2.0));
        hud.resize(&grid(80, 10), &Catalog::load(None).0.resolve("summer-garden").expect("built-in theme"));
        assert!(!hud.expanded && !hud.help);
    }

    /// The scene tray lists every family once, and picking another family switches to it
    /// at the current time.
    #[test]
    fn scene_tray_switches_family() {
        let t0 = Instant::now();
        let mut hud = hud("evening-garden", 160, 45, t0);
        let catalog = Catalog::load(None).0;
        let families: Vec<String> = catalog.summaries().into_iter().filter(|s| !s.hidden).map(|s| s.family).fold(Vec::new(), |mut all, f| {
            if !all.contains(&f) {
                all.push(f);
            }
            all
        });
        assert_eq!(hud.families.iter().map(|f| f.family.clone()).collect::<Vec<_>>(), families);
        assert_eq!(hud.families[0].id, "evening-garden", "the garden keeps its evening");
        assert_eq!(hud.times.iter().map(|t| t.0).collect::<Vec<_>>(), [Time::Dawn, Time::Noon, Time::Evening, Time::Night]);

        key(&mut hud, b'\t', t0);
        hud.input(&Input::Click { col: 93, row: 43 }, at(t0, 0.5));
        hud.tick(at(t0, 0.8));
        let tray = hud.rects()[5].expect("tray is open");
        let (n, iw) = hud.tray_items(Tray::Scene);
        let pad = (tray.w - n * iw) / 2;
        let reply = hud.input(&Input::Click { col: tray.col + pad + iw + 1, row: tray.row + 1 }, at(t0, 1.0));
        assert_eq!(reply, Reply::Act(Action::Theme(hud.families[1].id.clone())));

        let next = catalog.resolve(&hud.families[1].id).expect("family resolves");
        hud.set_theme(&next, &catalog, at(t0, 1.1));
        assert_eq!(hud.peek.as_ref().map(|p| (p.element, p.label.clone())), Some((Element::Scene, next.summary.name.clone())));
    }

    /// With `show = "always"` the row fades in at start and stays; `Tab` hides it and turns
    /// the HUD to "auto", and `Tab` again brings it back for good. A short window only peeks.
    #[test]
    fn always_keeps_the_row() {
        let t0 = Instant::now();
        let row = [Element::Music, Element::Food, Element::Scene, Element::Time, Element::Help];
        let catalog = Catalog::load(None).0;
        let theme = catalog.resolve("summer-garden").expect("built-in theme resolves");
        let mut cfg = config();
        cfg.hud.show = Show::Always;
        let mut hud = Hud::new(&cfg, &grid(160, 45), &theme, &catalog, 0.6, false, t0);
        assert_eq!(shown(&mut hud, at(t0, 0.5)), row);
        hud.input(&Input::Escape, at(t0, 1.0));
        assert_eq!(shown(&mut hud, at(t0, 60.0)), row);

        key(&mut hud, b'\t', at(t0, 61.0));
        assert!(hud.settings.show == Show::Auto && shown(&mut hud, at(t0, 62.0)).is_empty());
        key(&mut hud, b'\t', at(t0, 63.0));
        assert!(hud.settings.show == Show::Always);
        assert_eq!(shown(&mut hud, at(t0, 90.0)), row);

        let mut short = Hud::new(&cfg, &grid(80, 10), &theme, &catalog, 0.6, false, t0);
        assert!(shown(&mut short, at(t0, 0.5)).is_empty());
        hud.resize(&grid(80, 10), &theme);
        shown(&mut hud, at(t0, 91.0));
        assert!(shown(&mut hud, at(t0, 92.0)).is_empty());
    }

    /// A family with one time peeks "noon only" and asks for no switch.
    #[test]
    fn one_time_family() {
        let t0 = Instant::now();
        let mut hud = hud("cedar-shade", 160, 45, t0);
        assert_eq!(key(&mut hud, b'l', t0), Reply::Took);
        assert_eq!(hud.peek.as_ref().map(|p| p.label.as_str()), Some("noon only"));
        assert_eq!(key(&mut hud, b't', t0), Reply::Act(Action::Scene(true)));
    }

    /// `?` opens the card over the row; any other key closes it and still does its job.
    #[test]
    fn help_card() {
        let t0 = Instant::now();
        let mut hud = hud("summer-garden", 160, 45, t0);
        assert_eq!(key(&mut hud, b'?', t0), Reply::Took);
        assert!(shown(&mut hud, at(t0, 10.0)).contains(&Element::Card), "the card does not time out");
        assert_eq!(key(&mut hud, b'q', at(t0, 10.0)), Reply::Pass);
        assert!(!hud.help);
        key(&mut hud, b'?', at(t0, 11.0));
        assert_eq!(key(&mut hud, b'n', at(t0, 11.0)), Reply::Act(Action::NextTrack));
        key(&mut hud, b'?', at(t0, 12.0));
        assert_eq!(hud.input(&Input::Escape, at(t0, 12.0)), Reply::Took);
        assert!(!hud.help);
    }

    /// Tiny windows peek but never expand.
    #[test]
    fn short_window_only_peeks() {
        let t0 = Instant::now();
        let mut hud = hud("summer-garden", 80, 10, t0);
        key(&mut hud, b'\t', t0);
        key(&mut hud, b'?', t0);
        assert!(shown(&mut hud, at(t0, 0.5)).is_empty());
        key(&mut hud, b'2', t0);
        assert_eq!(shown(&mut hud, at(t0, 0.5)), [Element::Food, Element::Chip]);
    }

    /// Volume reports peek only when something changed.
    #[test]
    fn volume_peeks_on_change() {
        let t0 = Instant::now();
        let mut hud = hud("summer-garden", 160, 45, t0);
        hud.volume(0.6, false, t0);
        assert!(hud.peek.is_none());
        hud.volume(0.7, false, t0);
        assert_eq!(hud.peek.as_ref().map(|p| p.label.as_str()), Some("volume 70%"));
        hud.volume(0.7, true, t0);
        assert_eq!(hud.peek.as_ref().map(|p| p.label.as_str()), Some("muted"));
    }

    /// An idle HUD sends nothing; a fade step re-sends one image; hiding deletes the
    /// placement and showing it again unchanged is a placement only.
    #[test]
    fn draw_sends_only_changes() {
        let t0 = Instant::now();
        let mut hud = hud("summer-garden", 160, 45, t0);
        let mut ring = ShmRing::new(4, largest_image(10, 20)).expect("shm ring");
        let mut out = Vec::new();
        hud.draw(&mut out, &mut ring, t0).expect("draw");
        assert!(out.is_empty());

        let food = FIRST_ID + u32::try_from(Element::Food.slot()).expect("a small index");
        key(&mut hud, b'1', t0);
        hud.draw(&mut out, &mut ring, at(t0, 0.01)).expect("draw");
        assert!(String::from_utf8_lossy(&out).contains(&format!("i={food},p=1")));
        out.clear();
        hud.draw(&mut out, &mut ring, at(t0, 0.02)).expect("draw");
        assert!(out.is_empty(), "same fade step");
        hud.draw(&mut out, &mut ring, at(t0, 1.0)).expect("draw");
        let text = String::from_utf8_lossy(&out).to_string();
        assert!(text.contains("pellets"), "the chip label shows once faded in: {text}");
        out.clear();
        hud.draw(&mut out, &mut ring, at(t0, 2.0)).expect("draw");
        assert!(out.is_empty(), "an idle HUD sends nothing");

        hud.draw(&mut out, &mut ring, at(t0, 3.1)).expect("draw");
        assert!(String::from_utf8_lossy(&out).contains(&format!("\x1b[{}X", "pellets".len())), "the fade out erases the label's cells first");
        out.clear();
        hud.draw(&mut out, &mut ring, at(t0, 5.0)).expect("draw");
        assert!(String::from_utf8_lossy(&out).contains(&format!("a=d,d=i,i={food}")));
        out.clear();
        key(&mut hud, b'1', at(t0, 6.0));
        hud.draw(&mut out, &mut ring, at(t0, 7.0)).expect("draw");
        let text = String::from_utf8_lossy(&out).to_string();
        assert!(text.contains(&format!("a=p,i={food}")), "{text}");
    }
}
