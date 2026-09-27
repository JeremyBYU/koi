//! The pond in a web page. The simulation and the CPU renderers, compiled to wasm, make the
//! pond's layers each tick, as the terminal's Kitty mode does: the water image, which the page
//! scales to its canvas, and the koi and food sprites with where each goes. They also paint the
//! HUD's stones, and play `koi-synth`'s ambient sound, chimes and petting sound for the page
//! to stream. The page (`site/`) owns the canvas, the pointer, the keys, the controls and the
//! music.

#![forbid(unsafe_code)]

use koi_render::hud::{Look, Mark, Swatch};
use koi_render::{Poser, Water};
use koi_sim::{DT, FoodKind, Pose, School};
use koi_synth::Ambient;
use koi_theme::{Catalog, ROOT, Rgb, Theme, Time, mix};
use wasm_bindgen::prelude::*;

/// CSS pixels per water pixel: the terminal draws two to a cell about nine pixels wide.
const CSS_PER_WATER: f32 = 4.5;
/// The terminal's defaults for `pond.koi`, `render.water_px` and `render.water_fps`.
const KOI: usize = 5;
const WATER_PX: f32 = 2.0;
const WATER_FPS: f64 = 30.0;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn error(message: &str);
}

/// Sends a panic's message and place to the browser's console. Without this the page only
/// sees an "unreachable" trap.
#[wasm_bindgen(start)]
fn start() {
    std::panic::set_hook(Box::new(|info| error(&info.to_string())));
}

/// What a press in the pond did.
#[wasm_bindgen]
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Touch {
    /// Nothing: the food kind is at its cap.
    Nothing,
    /// Dropped food.
    Fed,
    /// Put a hand in the water for the koi pressed on; `drag` leads it and `release` lets go.
    Petted,
}

/// A HUD pebble's icon.
#[wasm_bindgen]
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Icon {
    /// A food, by `FoodKind::index`.
    Food,
    /// The scene's water.
    Scene,
    /// The time of day.
    Time,
    /// A question mark.
    Help,
}

/// The sizes a canvas and a theme's pixel size give the pond.
struct Grid {
    /// The simulation grid, in water pixels.
    water: (usize, usize),
    /// The canvas, in its own pixels.
    frame: (usize, usize),
    /// Frame pixels per art pixel in a pixel theme, else 0.
    pixel: usize,
    /// The images `Water::render` makes, and water image pixels per simulation unit.
    image: (usize, usize),
    per_sim: [f32; 2],
}

impl Grid {
    /// The grid for a canvas `css` CSS pixels in size, drawn at `density` frame pixels per
    /// CSS pixel.
    fn new(css: (f32, f32), density: f32, theme: &Theme) -> Grid {
        let water = (((css.0 / CSS_PER_WATER).round() as usize).max(8), ((css.1 / CSS_PER_WATER).round() as usize).max(8));
        let frame = (((css.0 * density).round() as usize).max(1), ((css.1 * density).round() as usize).max(1));
        // `pixel_px` is device pixels per art pixel, as in the terminal, so a dense screen
        // shows finer art pixels.
        let pixel = theme.style.pixel_px as usize;
        let image = if pixel > 0 { (frame.0.div_ceil(pixel), frame.1.div_ceil(pixel)) } else { water };
        Grid { water, frame, pixel, image, per_sim: [image.0 as f32 / water.0 as f32, image.1 as f32 / water.1 as f32] }
    }

    /// The water's `ratio`, as `Water::new` takes it.
    fn ratio(&self) -> f32 {
        WATER_PX / 8.0 * self.per_sim[0]
    }

    /// Frame pixels per simulation unit, across and down.
    fn scale(&self) -> (f32, f32) {
        (self.frame.0 as f32 / self.water.0 as f32, self.frame.1 as f32 / self.water.1 as f32)
    }

    /// The koi sprites for `school` in `theme`, and the food sprites with their pellet radius.
    fn sprites(&self, school: &School, theme: &Theme) -> (Poser, Vec<(usize, Vec<u8>)>, f32) {
        let scale = self.scale();
        let poser = Poser::new(None, school, theme, if self.pixel > 0 { self.per_sim[0] } else { scale.0 });
        let radius = self.water.1 as f32 * 0.011 * scale.0 / self.pixel.max(1) as f32;
        (poser, koi_render::food_sprites(&theme.palette, radius, self.pixel), radius)
    }
}

/// The pond, in layers for the page's canvas.
#[wasm_bindgen]
pub struct Pond {
    catalog: Catalog,
    theme: Theme,
    seed: u64,
    /// The canvas in CSS pixels, and frame pixels per CSS pixel.
    css: (f32, f32),
    density: f32,
    grid: Grid,
    school: School,
    water: Water,
    poser: Poser,
    /// The koi before the last simulation step, to blend from.
    before: Vec<Pose>,
    food: Vec<(usize, Vec<u8>)>,
    pellet_radius: f32,
    /// The last water image, `grid.image` in size.
    water_image: Vec<u8>,
    /// This tick's koi sprites, one after another, and for each its start in `koi_sheet`,
    /// width, height and top-left in canvas pixels.
    koi_sheet: Vec<u8>,
    koi_places: Vec<i32>,
    /// The ambient sound, and whether the page is playing it: chimes only sound while it is.
    ambient: Ambient,
    sound: bool,
    /// The time, in the page's milliseconds, the simulation has reached, and of the last water image.
    sim_ms: Option<f64>,
    water_ms: Option<f64>,
}

#[wasm_bindgen]
impl Pond {
    /// A pond for a canvas `css_w` x `css_h` CSS pixels in size, drawn at `density` frame
    /// pixels per CSS pixel, in the theme `theme` (else the root theme) and laid out by `seed`.
    #[wasm_bindgen(constructor)]
    pub fn new(css_w: f32, css_h: f32, density: f32, theme: &str, seed: u32) -> Pond {
        let catalog = Catalog::load(None).0;
        let theme = catalog.resolve(theme).or_else(|_| catalog.resolve(ROOT)).expect("the built-in root theme resolves");
        let (css, seed) = ((css_w, css_h), u64::from(seed));
        let grid = Grid::new(css, density, &theme);
        let school = School::new(grid.water.0, grid.water.1, KOI, seed);
        let water = Water::new(None, grid.image.0, grid.image.1, grid.per_sim, grid.ratio(), &theme, seed);
        let (poser, food, pellet_radius) = grid.sprites(&school, &theme);
        let before = school.fish.iter().map(|f| f.pose()).collect();
        let mut ambient = Ambient::new(seed);
        ambient.night = theme.summary.time == Time::Night;
        Pond {
            catalog,
            theme,
            seed,
            css,
            density,
            grid,
            school,
            water,
            poser,
            before,
            food,
            pellet_radius,
            water_image: Vec::new(),
            koi_sheet: Vec::new(),
            koi_places: Vec::new(),
            ambient,
            sound: false,
            sim_ms: None,
            water_ms: None,
        }
    }

    /// Fits the pond to a canvas of a new size: the same seed lays out the same pond on the new grid.
    pub fn resize(&mut self, css_w: f32, css_h: f32, density: f32) {
        (self.css, self.density) = ((css_w, css_h), density);
        self.grid = Grid::new(self.css, density, &self.theme);
        self.school = School::new(self.grid.water.0, self.grid.water.1, KOI, self.seed);
        self.water = Water::new(None, self.grid.image.0, self.grid.image.1, self.grid.per_sim, self.grid.ratio(), &self.theme, self.seed);
        (self.poser, self.food, self.pellet_radius) = self.grid.sprites(&self.school, &self.theme);
        self.before = self.school.fish.iter().map(|f| f.pose()).collect();
        self.water_ms = None;
    }

    /// Runs the pond up to `now_ms`, the page's clock in milliseconds, and poses the koi,
    /// blended between their last two steps. The water is drawn again `WATER_FPS` times a
    /// second; returns whether this tick drew it.
    pub fn tick(&mut self, now_ms: f64) -> bool {
        let dt = f64::from(DT) * 1000.0;
        let from = self.sim_ms.unwrap_or(now_ms);
        let sim = from + 1000.0 * koi_render::advance(&mut self.school, &mut self.water, &mut self.before, (now_ms - from) / 1000.0);
        self.sim_ms = Some(sim);

        let blend = ((now_ms - sim) / dt).min(1.0) as f32;
        let poses: Vec<Pose> = self.before.iter().zip(&self.school.fish).map(|(before, f)| before.lerp(&f.pose(), blend)).collect();
        let (sheet, places) = (&mut self.koi_sheet, &mut self.koi_places);
        sheet.clear();
        places.clear();
        koi_render::place_koi(&poses, &mut self.poser, self.grid.scale(), self.grid.pixel, |pixels, w, h, x, y| {
            let fits = |n: usize| i32::try_from(n).expect("a sprite's size and place fit i32");
            places.extend([fits(sheet.len()), fits(w), fits(h), x, y]);
            sheet.extend_from_slice(pixels);
        });
        // Frames come a display refresh apart, so half of a 60 Hz one of slack keeps the water
        // at its rate rather than every third frame.
        let fresh = self.water_ms.is_none_or(|t| now_ms - t >= 1000.0 / WATER_FPS - 8.0);
        if fresh {
            self.water_ms = Some(now_ms);
            self.water_image.clear();
            self.water_image.extend_from_slice(self.water.render(&self.school.shadows()));
        }
        fresh
    }

    /// Where the water image's RGBA pixels start in the wasm memory, until the next `tick`,
    /// `resize` or theme switch. The page scales it to fill the canvas, smoothly, or in a pixel
    /// scene by `pixel` without smoothing.
    pub fn water_ptr(&self) -> *const u8 {
        self.water_image.as_ptr()
    }

    /// The water image's width in pixels.
    pub fn water_width(&self) -> usize {
        self.grid.image.0
    }

    /// The water image's height in pixels.
    pub fn water_height(&self) -> usize {
        self.grid.image.1
    }

    /// Canvas pixels per art pixel in a pixel scene, which the page draws the water and koi
    /// sprites at without smoothing; 0 in a painted one.
    pub fn pixel(&self) -> usize {
        self.grid.pixel
    }

    /// Where this tick's koi sprites start in the wasm memory, one after another.
    pub fn koi_ptr(&self) -> *const u8 {
        self.koi_sheet.as_ptr()
    }

    /// Five numbers for each koi sprite: its first byte from `koi_ptr`, width, height, and
    /// top-left in canvas pixels. In a pixel scene each sprite pixel covers `pixel` canvas pixels.
    pub fn koi_places(&self) -> Vec<i32> {
        self.koi_places.clone()
    }

    /// Three numbers for each food and bubble in the water: its sprite, by `food_sprite`, and its
    /// top-left in canvas pixels.
    pub fn food_places(&self) -> Vec<i32> {
        koi_render::place_food(&self.school, &self.food, self.grid.scale(), self.grid.pixel)
            .flat_map(|(sprite, x, y)| [i32::try_from(sprite).expect("sprite numbers fit i32"), x, y])
            .collect()
    }

    /// How many food and bubble sprites there are. They change with the theme and the canvas.
    pub fn food_sprites(&self) -> usize {
        self.food.len()
    }

    /// The side of food sprite `i`, in canvas pixels.
    pub fn food_side(&self, i: usize) -> usize {
        self.food[i].0
    }

    /// Food sprite `i`'s RGBA pixels.
    pub fn food_sprite(&self, i: usize) -> Vec<u8> {
        self.food[i].1.clone()
    }

    /// The canvas's width in pixels.
    pub fn frame_width(&self) -> usize {
        self.grid.frame.0
    }

    /// The canvas's height in pixels.
    pub fn frame_height(&self) -> usize {
        self.grid.frame.1
    }

    /// A press at (`x`, `y`) CSS pixels: pets the koi it lands within `reach` body lengths of,
    /// else drops a handful of food kind `food` (a `FoodKind::index`) there.
    pub fn press(&mut self, x: f32, y: f32, food: usize, reach: f32) -> Touch {
        let pan = x / self.css.0 * 2.0 - 1.0;
        let (x, y) = self.to_sim(x, y);
        if self.school.pet(x, y, reach, 6.0).is_some() {
            if self.sound {
                self.ambient.pet(pan);
            }
            return Touch::Petted;
        }
        let kind = FoodKind::ALL[food.min(FoodKind::ALL.len() - 1)];
        if self.school.drop_food(kind, x, y) == 0 {
            return Touch::Nothing;
        }
        if self.sound {
            self.ambient.chime(kind, pan);
        }
        Touch::Fed
    }

    /// Moves the hand a koi is nuzzling to (`x`, `y`) CSS pixels.
    pub fn drag(&mut self, x: f32, y: f32) {
        let (x, y) = self.to_sim(x, y);
        self.school.move_hand(x, y);
    }

    /// Takes the hand out of the water.
    pub fn release(&mut self) {
        self.school.let_go();
    }

    /// Puts a hand in the middle of the pond for the nearest koi, for 3 s of nuzzling. False
    /// with no koi.
    pub fn pet_nearest(&mut self) -> bool {
        let (w, h) = self.grid.water;
        let petted = self.school.pet(w as f32 / 2.0, h as f32 / 2.0, f32::INFINITY, 3.0).is_some();
        if petted && self.sound {
            self.ambient.pet(0.0);
        }
        petted
    }

    /// Drops a handful of food kind `food` somewhere in open water. False when none landed,
    /// the kind being at its cap.
    pub fn feed_anywhere(&mut self, food: usize) -> bool {
        let kind = FoodKind::ALL[food.min(FoodKind::ALL.len() - 1)];
        let (x, _, landed) = self.school.drop_food_random(kind);
        if landed > 0 && self.sound {
            self.ambient.chime(kind, x / self.grid.water.0 as f32 * 2.0 - 1.0);
        }
        landed > 0
    }

    /// Starts or stops the chimes and petting sound, as the page starts or stops playing
    /// `sound`.
    pub fn set_sound(&mut self, on: bool) {
        self.sound = on;
    }

    /// Sets the level of the ambient bed, the drone and the rumble, from 0 to 1; see
    /// `koi_synth::Ambient::bed`.
    pub fn set_bed(&mut self, level: f32) {
        self.ambient.bed = level.clamp(0.0, 1.0);
    }

    /// The next `frames` of the ambient sound at `koi_synth::RATE` frames a second, left and
    /// right interleaved, for the page's AudioWorklet.
    pub fn sound(&mut self, frames: usize) -> Vec<f32> {
        (0..frames).flat_map(|_| <[f32; 2]>::from(self.ambient.render_frame())).collect()
    }

    /// The ambient sound's frames a second.
    pub fn sound_rate() -> u32 {
        koi_synth::RATE
    }

    /// Switches to the next scene (the previous one when `forward` is false), at the same time
    /// of day when it has it. False when there is no other.
    pub fn next_scene(&mut self, forward: bool) -> bool {
        let id = self.catalog.next_scene(&self.theme.summary, forward);
        self.switch(id)
    }

    /// Switches to the next time of day in the scene (the earlier one when `later` is false).
    /// False when the scene has only one.
    pub fn next_time(&mut self, later: bool) -> bool {
        let id = self.catalog.next_time(&self.theme.summary, later);
        self.switch(id)
    }

    /// Switches to the theme `id`. False when there is none by that name.
    pub fn set_theme(&mut self, id: &str) -> bool {
        self.switch(Some(id.to_string()))
    }

    /// The theme's id, as `set_theme` takes it.
    pub fn theme_id(&self) -> String {
        self.theme.summary.id.clone()
    }

    /// The theme's name.
    pub fn theme_name(&self) -> String {
        self.theme.summary.name.clone()
    }

    /// The theme's time of day, such as "noon".
    pub fn time_name(&self) -> String {
        self.theme.summary.time.name().to_string()
    }

    /// Whether the scene has another time of day for `next_time` to switch to. Most scenes
    /// have only one.
    pub fn has_times(&self) -> bool {
        self.catalog.next_time(&self.theme.summary, true).is_some()
    }

    /// The page's colours in the theme, as CSS custom properties for the root element's
    /// `style`: the garden around the pond. Light scenes give a pale stone page and dark ones
    /// a page of deep water, with the HUD's own ink, and the stones' colours for the controls.
    pub fn page_style(&self) -> String {
        let p = &self.theme.palette;
        let c = Look::new(&self.theme, 20).colors;
        let ground = if c.dark { mix(p.deep, [0; 3], 0.55) } else { mix(p.stone_light, p.koi_white, 0.72) };
        let well = if c.dark { mix(ground, p.mid, 0.25) } else { mix(ground, p.stone_dark, 0.1) };
        let css = |[r, g, b]: Rgb| format!("#{r:02x}{g:02x}{b:02x}");
        [
            ("ground", ground),
            ("ink", c.ink),
            ("dim", mix(c.ink, ground, 0.4)),
            ("accent", p.ui_accent),
            ("well", well),
            ("rim", if c.dark { mix(p.stone_dark, p.deep, 0.5) } else { mix(p.stone_dark, ground, 0.35) }),
            ("stone", c.fill),
            ("stone-ink", c.ink),
            ("stone-dim", c.dim),
            ("stone-line", c.outline),
        ]
        .map(|(name, color)| format!("--{name}:{};", css(color)))
        .concat()
    }

    /// A pebble with `icon` on it, as the terminal draws it three cells tall, in RGBA: `cell_h`
    /// device pixels to a cell, 6 half-as-wide cells across, so `3 * cell_h` pixels square.
    /// `index` picks the food. `lifted` raises it, for the pebble under the pointer, and `seed`
    /// gives each its own outline.
    pub fn paint_pebble(&self, icon: Icon, index: usize, cell_h: usize, lifted: bool, seed: u32) -> Vec<u8> {
        let (w, h) = (3 * cell_h, 3 * cell_h);
        let k = cell_h as f32 / 20.0;
        let mark = match icon {
            Icon::Food => Mark::Food(index.min(FoodKind::ALL.len() - 1)),
            Icon::Scene => Mark::Swatch(Swatch::new(&self.theme.palette)),
            Icon::Time => Mark::Time(self.theme.summary.time),
            Icon::Help => Mark::Help,
        };
        // Where the terminal centres a pebble's mark: the stone leaves room for its shadow on
        // the right and bottom.
        let top = if h < 30 { 1.0 } else { 2.0 } * k;
        let (x, y) = ((w as f32 - 3.0 * k) / 2.0, top + (h as f32 - top - k) / 2.0);
        let mut rgba = Vec::new();
        Look::new(&self.theme, cell_h).paint(w, h, seed, lifted, &[(mark, x, y)], &mut rgba);
        rgba
    }

    /// The music stone, `cols` half-cells wide and three cells tall as `paint_pebble` sizes
    /// them: the note (under a leaf when `muted`), the next arrow at the right, and from 30
    /// columns the five volume dots before it, for `volume` from 0 to 1. The page writes the
    /// title between the note and the dots.
    pub fn paint_music(&self, cols: usize, cell_h: usize, volume: f32, muted: bool, lifted: bool, seed: u32) -> Vec<u8> {
        let cw = cell_h as f32 / 2.0;
        let (w, h) = (cols * cell_h / 2, 3 * cell_h);
        let k = cell_h as f32 / 20.0;
        let top = if h < 30 { 1.0 } else { 2.0 } * k;
        let y = top + (h as f32 - top - k) / 2.0;
        // The note's head hangs below its anchor, so it is anchored 2 px low to look centred.
        let mut marks = vec![(Mark::Note { muted }, 3.0 * cw, y + 2.0 * k), (Mark::Next, (cols as f32 - 3.5) * cw, y)];
        if cols >= 30 {
            let steps = if muted { 0.0 } else { (volume * 10.0).round() };
            marks.extend((0..5).map(|i| (Mark::Dot { fill: (steps - 2.0 * i as f32).clamp(0.0, 2.0) / 2.0 }, (cols as f32 - 10.0 + 0.9 * i as f32) * cw, y)));
        }
        let mut rgba = Vec::new();
        Look::new(&self.theme, cell_h).paint(w, h, seed, lifted, &marks, &mut rgba);
        rgba
    }
}

impl Pond {
    fn to_sim(&self, x: f32, y: f32) -> (f32, f32) {
        (x / self.css.0 * self.grid.water.0 as f32, y / self.css.1 * self.grid.water.1 as f32)
    }

    /// Same pond, new light: the koi, the waves and the layout carry on. A new pixel size needs
    /// a new grid for the water and the koi sprites.
    fn switch(&mut self, id: Option<String>) -> bool {
        let Some(Ok(next)) = id.map(|id| self.catalog.resolve(&id)) else { return false };
        if next.style.pixel_px == self.theme.style.pixel_px {
            self.water.set_theme(&next);
            self.poser.recolor(&self.school, &next);
            self.food = koi_render::food_sprites(&next.palette, self.pellet_radius, self.grid.pixel);
        } else {
            self.grid = Grid::new(self.css, self.density, &next);
            self.water.regrid(self.grid.image.0, self.grid.image.1, self.grid.per_sim, self.grid.ratio(), &next);
            (self.poser, self.food, self.pellet_radius) = self.grid.sprites(&self.school, &next);
        }
        self.ambient.night = next.summary.time == Time::Night;
        self.theme = next;
        self.water_ms = None;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every built-in theme draws a whole water image and every koi, a theme switch draws the
    /// water again, and a press on open water feeds.
    #[test]
    fn draws_every_theme() {
        let mut pond = Pond::new(320.0, 200.0, 1.0, ROOT, 7);
        for n in 0..40 {
            let fresh = pond.tick(f64::from(n) * 20.0);
            assert!(fresh || n % 3 != 1, "the tick after a switch draws the water");
            assert_eq!(pond.water_image.len(), pond.water_width() * pond.water_height() * 4);
            assert_eq!(pond.koi_places.len(), 5 * KOI);
            if n % 3 == 0 {
                pond.next_scene(true);
            }
        }
        assert_eq!(pond.press(4.0, 4.0, 0, 0.25), Touch::Fed);
    }

    /// Pixel scenes at a desktop page's size and a phone's, where the art pixel is not a whole
    /// number of CSS pixels, and in a box laid out at zero width, whose image is one pixel wide.
    #[test]
    fn pixel_scenes_at_page_sizes() {
        for (w, h, d) in [(1180.0, 740.0, 1.0), (358.0, 624.0, 2.0), (1180.0, 740.0, 1.25), (0.0, 340.0, 1.0)] {
            let mut pond = Pond::new(w, h, d, "pocket-moss", 7);
            for n in 0..5 {
                pond.tick(f64::from(n) * 16.7);
            }
        }
    }
}
