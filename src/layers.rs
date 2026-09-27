use crate::config::Protocol;
use koi_render::{FADE_LEVELS, Poser, stretch, upscale};
use koi_sim::{FoodKind, Pose, School};
use koi_term::{Caps, ShmRing};
use koi_theme::{Palette, Rgb};
use std::io;

/// Main ring slots beyond one write's worth. `recolor` sends all 48 food and bubble images in
/// one write and the next frame sends every koi and up to 8 HUD images, so the ring holds
/// those plus this many, for Ghostty to fall behind by.
const RING_SPARE: usize = 16;
/// The water goes through its own ring, since in a pixel theme it is a full-window image.
/// It is sent at most once a frame, so a few slots give Ghostty as long to read it as the
/// main ring does.
const WATER_SLOTS: usize = 6;
const WATER_ID: u32 = 1;
const FIRST_FOOD_ID: u32 = 2;
const FIRST_KOI_ID: u32 = 100;

/// A half-block cell whose colours moved less than this per channel since it was sent is
/// not sent again.
const BLOCK_TOLERANCE: u8 = 2;

/// Cell column and row, and pixel offset inside that cell.
type Spot = [i32; 4];

/// How the pond reaches the terminal, best first.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Tier {
    /// Kitty graphics through shared memory: every koi, the water and the food their own image.
    Kitty,
    /// Kitty graphics as `Kitty`, sent inline and compressed, for a terminal that cannot read
    /// the pond's shared memory, such as one over SSH.
    KittyDirect,
    /// One frame, sent as sixel images of the cells that changed.
    Sixel,
    /// One frame at two pixels a cell, sent as coloured half-block characters. Any terminal
    /// shows them.
    Blocks,
}

impl Tier {
    /// The name `render.protocol` and `--protocol` use.
    pub fn name(self) -> &'static str {
        match self {
            Tier::Kitty => "kitty",
            Tier::KittyDirect => "kitty-direct",
            Tier::Sixel => "sixel",
            Tier::Blocks => "blocks",
        }
    }

    /// The most frames a second worth sending: each inline image or sixel frame is many
    /// bytes for the terminal to parse, and a painted theme's sixels are the heaviest.
    pub fn fps_cap(self, pixel: bool) -> u32 {
        match self {
            Tier::Kitty | Tier::Blocks => u32::MAX,
            Tier::KittyDirect => 30,
            Tier::Sixel if pixel => 30,
            Tier::Sixel => 20,
        }
    }

    /// Whether the pond is drawn as one frame, rather than as a Kitty image per koi. In tmux
    /// it always is, since tmux cannot place images.
    pub fn framed(self, in_tmux: bool) -> bool {
        in_tmux || matches!(self, Tier::Sixel | Tier::Blocks)
    }
}

/// The tier to draw with: the one `setting` names, else the best the terminal said it has.
/// Inside tmux (`in_tmux`) only Kitty graphics reach the terminal outside, through the
/// placeholder cells, so auto never picks sixel there. Sixel needs 64 colour registers, and a
/// terminal whose largest image is narrower than the window, `screen` in pixels, gets blocks.
/// A forced tier the terminal did not confirm comes with a warning: it is tried anyway.
pub fn choose(setting: Protocol, caps: &Caps, in_tmux: bool, screen: (usize, usize)) -> (Tier, Option<String>) {
    let sixel = caps.sixel.as_ref().is_some_and(|s| s.colors >= 64 && s.max.is_none_or(|(w, _)| w >= screen.0)) && !in_tmux;
    // WezTerm answers the Kitty queries but cannot keep up with the pond's images: it falls
    // behind reading them, garbles koi placed inside a cell, and runs out of memory.
    let kitty = !caps.name.as_deref().is_some_and(|name| name.starts_with("WezTerm"));
    let (tier, confirmed) = match setting {
        Protocol::Auto if kitty && caps.kitty_shm => (Tier::Kitty, true),
        Protocol::Auto if kitty && caps.kitty_direct => (Tier::KittyDirect, true),
        Protocol::Auto if sixel => (Tier::Sixel, true),
        Protocol::Auto => (Tier::Blocks, true),
        Protocol::Kitty => (Tier::Kitty, caps.kitty_shm),
        Protocol::KittyDirect => (Tier::KittyDirect, caps.kitty_direct),
        Protocol::Sixel => (Tier::Sixel, sixel),
        Protocol::Blocks => (Tier::Blocks, true),
    };
    let warning =
        (!confirmed).then(|| format!("the terminal did not say it can show {}, but koi tries it as asked; `--protocol auto` picks one it has", tier.name()));
    (tier, warning)
}

/// What the terminal shows, as Kitty images. The water is one image scaled up from a coarse
/// grid. Each koi is its own image at `fish_px` pixels per cell width, posed afresh every
/// frame and placed to the screen pixel. Food and bubbles are tiny native images. The water,
/// food and overlay are only sent when they changed.
///
/// In a pixel theme (`pixel` above 0) the water and the koi share an art grid of `pixel`
/// screen pixels: each koi is posed with one sprite pixel per art pixel and placed on a
/// multiple of `pixel`. Ghostty scales images with a linear filter, so both are scaled up
/// here by repeating pixels and sent at exactly the screen size, which keeps them crisp.
///
/// Elsewhere everything is drawn into one frame the size of the window instead (`compose`
/// and the HUD's `compose`), which `send` hands to a `Sink`: in tmux, where images cannot
/// be placed at the cursor since tmux does not know about them, a Kitty image the terminal
/// shows through placeholder cells that tmux keeps with the pane; else sixel images or
/// half-block characters. There the grid's cells are the frame's: in tmux the terminal
/// scales them to its own, for sixel they are the terminal's, and for blocks they are 1x2.
pub struct Layers {
    pub grid: Grid,
    fish_px: usize,
    pixel: usize,
    scale_x: f32,
    scale_y: f32,
    /// Food radius in sprite pixels: screen pixels, or art pixels in a pixel theme.
    pellet_radius: f32,
    /// Side of each kind's food sprites in screen pixels, by `FoodKind::index`, and last the
    /// bubbles'.
    food_sizes: [usize; FoodKind::ALL.len() + 1],
    pellets: Vec<(u32, Spot)>,
    last_water: Vec<u8>,
    /// The last water or koi image scaled up to screen pixels, reused.
    scaled: Vec<u8>,
    overlay: Option<String>,
    /// Direct for `Tier::KittyDirect`. For sixel and blocks it sends nothing.
    pub ring: ShmRing,
    /// None with a frame, where the water is part of the frame.
    water_ring: Option<ShmRing>,
    pub renders: usize,
    framed: Option<Frame>,
}

/// The one frame of the pond, and where it goes.
struct Frame {
    frame: Vec<u8>,
    /// The last water image, scaled to the frame.
    water: Vec<u8>,
    /// Side and pixels of each food and bubble sprite, in `recolor`'s id order.
    food: Vec<(usize, Vec<u8>)>,
    sink: Sink,
    /// The cells that show text, the HUD's and the top line's, row by row. The frame is not
    /// drawn over them.
    text: Vec<bool>,
    /// Send it all again: at start, and when the terminal may have lost what it showed.
    redraw: bool,
}

/// Where the frame goes.
enum Sink {
    /// A Kitty image shown through placeholder cells, as tmux needs.
    Placeholders {
        /// The image id, also the colour its placeholder cells are drawn in.
        id: u8,
    },
    /// Sixel images, one for each run of changed cells in a row, in a fixed palette.
    Sixel {
        /// Colour registers to fill, at most 256.
        colors: usize,
        /// A pixel theme's swatches and slot colours, or a median cut of the first frame after
        /// a painted theme's switch. Empty until chosen.
        palette: Vec<Rgb>,
        /// The nearest palette entry to every colour at 5 bits per channel. Empty until built.
        lut: Vec<u8>,
        /// The frame as palette entries, and the one last sent (empty: send every cell).
        entries: Vec<u8>,
        sent: Vec<u8>,
        /// The encoder's bit planes, kept between images.
        planes: Vec<u8>,
        /// The bottom row goes out as coloured spaces, since a sixel image touching the last
        /// line scrolls the screen in xterm. What each of its cells last showed.
        bottom: Vec<Option<[Rgb; 2]>>,
        truecolor: bool,
    },
    /// Half blocks, U+2580 with its top pixel in the foreground and bottom in the background.
    Blocks {
        /// 24-bit colour, else the 256-colour palette.
        truecolor: bool,
        /// What each cell last showed. Empty, or None: send it.
        prev: Vec<Option<[Rgb; 2]>>,
    },
}

/// The window in cells, and the simulation grid in water pixels (`water_px` per cell width).
#[derive(Clone)]
pub struct Grid {
    pub cols: usize,
    pub rows: usize,
    pub cell_w: usize,
    pub cell_h: usize,
    pub water_w: usize,
    pub water_h: usize,
}

impl Layers {
    /// Sets up the rings and sends the food images. `fish_px` is koi image pixels per cell
    /// width, `pixel` the art pixel size in screen pixels (0 when painted), and `water` the
    /// size of the images `Water::render` makes. `tmux` is the image id of tmux mode, which
    /// only the Kitty tiers use. `caps` gives sixel its colour registers and blocks their
    /// colours.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        out: &mut Vec<u8>,
        grid: Grid,
        school: &School,
        poser: &Poser,
        palette: &Palette,
        fish_px: usize,
        pixel: usize,
        water: (usize, usize),
        tier: Tier,
        tmux: Option<u8>,
        caps: &Caps,
    ) -> io::Result<Layers> {
        let scale_x = (grid.cols * grid.cell_w) as f32 / grid.water_w as f32;
        let pellet_radius = grid.water_h as f32 * 0.011 * scale_x / pixel.max(1) as f32;
        let sprites = koi_render::food_sprites(palette, pellet_radius, pixel);
        let food_sizes = std::array::from_fn(|n| sprites[n * FADE_LEVELS].0);

        let screen = (grid.cols * grid.cell_w, grid.rows * grid.cell_h);
        let cells = grid.cols * grid.rows;
        let mut largest = food_sizes.iter().map(|s| s * s * 4).max().unwrap_or(0);
        for k in 0..school.fish.len() {
            // A little extra for the rounding in `bounds`.
            let (cols, rows, w, h) = canvas(&grid, fish_px, pixel, 0, 0, poser.largest(k) + fish_px as i32, poser.largest(k) + fish_px as i32);
            let sent = if pixel > 0 { cols as usize * grid.cell_w * rows as usize * grid.cell_h } else { w * h };
            largest = largest.max(sent * 4);
        }
        largest = largest.max(crate::hud::largest_image(grid.cell_w, grid.cell_h));
        let water_bytes = 4 * if pixel > 0 { screen.0 * screen.1 } else { water.0 * water.1 };
        let sink = tier.framed(tmux.is_some()).then(|| match (tier, tmux) {
            (Tier::Kitty | Tier::KittyDirect, id) => Sink::Placeholders { id: id.expect("a framed Kitty tier is only in tmux") },
            (Tier::Sixel, _) => {
                let colors = caps.sixel.as_ref().map_or(256, |s| s.colors).min(256);
                Sink::Sixel {
                    colors,
                    palette: Vec::new(),
                    lut: Vec::new(),
                    entries: Vec::new(),
                    sent: Vec::new(),
                    planes: Vec::new(),
                    bottom: Vec::new(),
                    truecolor: caps.truecolor,
                }
            }
            (Tier::Blocks, _) => Sink::Blocks { truecolor: caps.truecolor, prev: Vec::new() },
        });
        let ring = |slots: usize, capacity: usize| if tier == Tier::KittyDirect { Ok(ShmRing::direct()) } else { ShmRing::new(slots, capacity) };
        // In tmux the one frame a frame goes through the main ring.
        let (ring, water_ring) = match &sink {
            Some(Sink::Placeholders { .. }) => (ring(WATER_SLOTS, screen.0 * screen.1 * 4)?, None),
            Some(_) => (ShmRing::direct(), None),
            None => (ring((FoodKind::ALL.len() + 1) * FADE_LEVELS + school.fish.len() + 8 + RING_SPARE, largest)?, Some(ring(WATER_SLOTS, water_bytes)?)),
        };
        let mut layers = Layers {
            fish_px,
            pixel,
            scale_x,
            scale_y: (grid.rows * grid.cell_h) as f32 / grid.water_h as f32,
            grid,
            pellet_radius,
            food_sizes,
            pellets: Vec::new(),
            last_water: Vec::new(),
            scaled: Vec::new(),
            overlay: None,
            ring,
            water_ring,
            renders: 0,
            framed: sink.map(|sink| Frame {
                frame: Vec::new(),
                water: vec![0; screen.0 * screen.1 * 4],
                food: Vec::new(),
                sink,
                text: vec![false; cells],
                redraw: true,
            }),
        };
        layers.recolor(out, palette)?;
        Ok(layers)
    }

    /// Sends every food kind's sprites in `palette`'s colours, then the bubble's, replacing
    /// any sent before. Kind `k` at fade level `l` (1 to 8) is image `FIRST_FOOD_ID + k *
    /// FADE_LEVELS + l - 1`, and a bubble at stage `l` is kind `FoodKind::ALL.len()`. The next
    /// `encode` places the food again. With a frame the sprites are kept for `compose`, and
    /// sixel takes the theme's palette.
    pub fn recolor(&mut self, out: &mut Vec<u8>, palette: &Palette) -> io::Result<()> {
        let sprites = koi_render::food_sprites(palette, self.pellet_radius, self.pixel);
        if let Some(f) = &mut self.framed {
            f.food.clear();
            if let Sink::Sixel { palette: colours, colors, lut, .. } = &mut f.sink {
                // A pixel theme only shows its swatches (the water is locked to them) and its
                // slot colours. A painted theme's colours come from its first frame.
                colours.clear();
                if self.pixel > 0 {
                    for colour in palette.swatches.iter().chain(&palette.slots()) {
                        if !colours.contains(colour) && colours.len() < *colors {
                            colours.push(*colour);
                        }
                    }
                }
                lut.clear();
            }
        }
        match &mut self.framed {
            Some(f) => f.food = sprites,
            None => {
                for ((side, sprite), id) in sprites.iter().zip(FIRST_FOOD_ID..) {
                    self.ring.transmit(out, sprite, &format!("a=t,f=32,s={side},v={side},i={id}"))?;
                }
            }
        }
        self.pellets.clear();
        Ok(())
    }

    /// With a frame, sends all of it again on the next `send` (in tmux, the placeholder cells
    /// too), for when the terminal may have lost it.
    pub fn redraw(&mut self) {
        if let Some(f) = &mut self.framed {
            f.redraw = true;
        }
    }

    /// With a frame: draws it, the water (scaled from `water` when given, else the last water)
    /// with the koi in `poses` and the food over it, and returns it for the HUD to draw on. In
    /// tmux, appends the placeholder cells to `out` when they are due. `send` sends the frame.
    /// Without one it does nothing and returns `None`.
    pub fn compose(
        &mut self,
        out: &mut Vec<u8>,
        school: &School,
        poses: &[Pose],
        poser: &mut Poser,
        water: Option<(&[u8], usize, usize)>,
    ) -> Option<&mut [u8]> {
        let t = self.framed.as_mut()?;
        let (screen_w, screen_h) = (self.grid.cols * self.grid.cell_w, self.grid.rows * self.grid.cell_h);
        if t.redraw {
            match &mut t.sink {
                Sink::Placeholders { id } => {
                    for row in 0..self.grid.rows {
                        koi_term::placeholders(out, *id, row, 0, self.grid.cols);
                    }
                }
                Sink::Sixel { sent, bottom, .. } => {
                    sent.clear();
                    bottom.clear();
                }
                Sink::Blocks { prev, .. } => prev.clear(),
            }
            t.redraw = false;
            // The cells took the top line's place, or will be drawn over it.
            self.overlay = None;
        }
        if let Some((rgba, w, h)) = water {
            if self.pixel > 0 {
                upscale(rgba, w, self.pixel, screen_w, screen_h, &mut t.water);
            } else {
                stretch(rgba, w, h, screen_w, screen_h, &mut t.water);
            }
        }
        t.frame.clear();
        t.frame.extend_from_slice(&t.water);
        self.renders += koi_render::draw_pond(&mut t.frame, screen_w, school, poses, poser, &t.food, (self.scale_x, self.scale_y), self.pixel);
        Some(&mut t.frame)
    }

    /// With a frame, sends what `compose` drew, and the top line if it changed. `text` is
    /// the cells showing text, the HUD's and the top line's, as (1-based column, 1-based row,
    /// width): the frame is not drawn over them, and cells that text left are drawn again. In
    /// tmux the frame goes through a passthrough as the image the placeholder cells show.
    /// Without a frame it does nothing.
    pub fn send(&mut self, out: &mut Vec<u8>, overlay: Option<&str>, text: &[(usize, usize, usize)]) -> io::Result<()> {
        let Some(f) = &mut self.framed else { return Ok(()) };
        let (cols, rows, cw, ch) = (self.grid.cols, self.grid.rows, self.grid.cell_w, self.grid.cell_h);
        let mut shown = vec![false; cols * rows];
        for &(col, row, n) in text.iter().filter(|t| (1..=rows).contains(&t.1) && t.0 >= 1) {
            let line = &mut shown[(row - 1) * cols..][..cols];
            line[(col - 1).min(cols)..(col - 1 + n).min(cols)].fill(true);
        }
        let freed: Vec<bool> = shown.iter().zip(&f.text).map(|(&now, &before)| before && !now).collect();
        match &mut f.sink {
            Sink::Placeholders { id } => {
                let mut command = Vec::new();
                self.ring.transmit(&mut command, &f.frame, &format!("a=T,U=1,f=32,s={},v={},i={id},p=1,c={cols},r={rows}", cols * cw, rows * ch))?;
                koi_term::tmux_wrap(out, &command);
                for (cell, _) in freed.iter().enumerate().filter(|c| *c.1) {
                    koi_term::placeholders(out, *id, cell / cols, cell % cols, 1);
                }
            }
            Sink::Sixel { colors, palette, lut, entries, sent, planes, bottom, truecolor } => {
                if palette.is_empty() {
                    *palette = median_cut(&f.frame, *colors);
                    lut.clear();
                }
                if lut.is_empty() {
                    *lut = (0..1 << 15)
                        .map(|bin: usize| nearest(palette, [bin >> 10, bin >> 5 & 31, bin & 31].map(|c| u8::try_from(c << 3 | 4).expect("5 bits"))))
                        .collect();
                    sent.clear();
                }
                entries.clear();
                entries.extend(
                    f.frame.as_chunks::<4>().0.iter().map(|&[r, g, b, _]| lut[usize::from(r >> 3) << 10 | usize::from(g >> 3) << 5 | usize::from(b >> 3)]),
                );
                let w = cols * cw;
                let all = sent.len() != entries.len();
                let mut image = Vec::new();
                for row in 0..rows.saturating_sub(1) {
                    let changed = |c: usize| {
                        let cell = row * cols + c;
                        !shown[cell]
                            && (all
                                || freed[cell]
                                || (0..ch).any(|y| entries[(row * ch + y) * w + c * cw..][..cw] != sent[(row * ch + y) * w + c * cw..][..cw]))
                    };
                    let mut c = 0;
                    while c < cols {
                        if !changed(c) {
                            c += 1;
                            continue;
                        }
                        // One image from here to the last changed cell before the next text.
                        let (mut last, mut next) = (c, c + 1);
                        while next < cols && !shown[row * cols + next] {
                            if changed(next) {
                                last = next;
                            }
                            next += 1;
                        }
                        let width = (last + 1 - c) * cw;
                        image.clear();
                        for y in row * ch..(row + 1) * ch {
                            image.extend_from_slice(&entries[y * w + c * cw..][..width]);
                        }
                        out.extend_from_slice(format!("\x1b[{};{}H", row + 1, c + 1).as_bytes());
                        sixel(out, &image, width, palette, planes);
                        c = next;
                    }
                }
                std::mem::swap(sent, entries);
                if rows > 0 {
                    let cells = (rows - 1) * cols..rows * cols;
                    bottom.resize(cols, None);
                    blocks(out, &f.frame, (cw, ch), rows - 1, bottom, &shown[cells.clone()], &freed[cells], *truecolor, false);
                }
            }
            Sink::Blocks { truecolor, prev } => {
                prev.resize(cols * rows, None);
                for row in 0..rows {
                    let cells = row * cols..(row + 1) * cols;
                    blocks(out, &f.frame, (cw, ch), row, &mut prev[cells.clone()], &shown[cells.clone()], &freed[cells], *truecolor, true);
                }
            }
        }
        f.text = shown;
        if overlay != self.overlay.as_deref() {
            if let Some(text) = overlay {
                out.extend_from_slice(b"\x1b[1;1H");
                out.extend_from_slice(text.as_bytes());
            }
            self.overlay = overlay.map(str::to_string);
        }
        Ok(())
    }

    fn spot(&self, x: i32, y: i32) -> Spot {
        let (x, y, cw, ch) = (x.max(0), y.max(0), self.grid.cell_w as i32, self.grid.cell_h as i32);
        [x / cw, y / ch, x % cw, y % ch]
    }

    /// Appends to `out` the koi in `poses`, plus whatever else changed since the last call:
    /// the water image if `water` is given and differs from the last one sent, pellets and
    /// the overlay line. `force` re-sends everything.
    #[allow(clippy::too_many_arguments)]
    pub fn encode(
        &mut self,
        out: &mut Vec<u8>,
        school: &School,
        poses: &[Pose],
        poser: &mut Poser,
        water: Option<(&[u8], usize, usize)>,
        overlay: Option<&str>,
        force: bool,
    ) -> io::Result<()> {
        let (screen_w, screen_h) = (self.grid.cols * self.grid.cell_w, self.grid.rows * self.grid.cell_h);
        if let Some((rgba, w, h)) = water
            && let Some(water_ring) = &mut self.water_ring
            && (force || rgba != self.last_water.as_slice())
        {
            out.extend_from_slice(b"\x1b[H");
            let keys = |w: usize, h: usize| format!("a=T,f=32,s={w},v={h},i={WATER_ID},p=1,c={},r={},z=-1000,C=1", self.grid.cols, self.grid.rows);
            if self.pixel > 0 {
                upscale(rgba, w, self.pixel, screen_w, screen_h, &mut self.scaled);
                water_ring.transmit(out, &self.scaled, &keys(screen_w, screen_h))?;
            } else {
                water_ring.transmit(out, rgba, &keys(w, h))?;
            }
            self.last_water.clear();
            self.last_water.extend_from_slice(rgba);
        }

        let screen_per_px = if self.pixel > 0 { self.pixel as f32 } else { self.grid.cell_w as f32 / self.fish_px as f32 };
        // Koi snap to a screen pixel, or in a pixel theme to the art grid.
        let snap = self.pixel.max(1) as f32;
        for (k, pose) in poses.iter().enumerate() {
            let (cx, cy) = (pose.x * self.scale_x, pose.y * self.scale_y);
            let (left, top, right, bottom) = poser.bounds(k, pose);
            // Ghostty cannot place an image at a negative position, so a koi hanging off the
            // left or top edge is cropped to the screen.
            let left = left + (-(cx + left as f32 * screen_per_px).round() / screen_per_px).ceil().max(0.0) as i32;
            let top = top + (-(cy + top as f32 * screen_per_px).round() / screen_per_px).ceil().max(0.0) as i32;
            let (x, y) = (((cx + left as f32 * screen_per_px) / snap).round() * snap, ((cy + top as f32 * screen_per_px) / snap).round() * snap);
            let (cols, rows, w, h) = canvas(&self.grid, self.fish_px, self.pixel, left, top, right, bottom);
            // Posing from the exact origin puts the fraction lost to the snap back inside the
            // sprite, so koi move evenly (in a pixel theme, a whole art pixel at a time).
            let pixels = poser.pose(k, pose, (x - cx) / screen_per_px, (y - cy) / screen_per_px, w, h);
            let [col, row, px, py] = self.spot(x as i32, y as i32);
            let id = FIRST_KOI_ID + k as u32;
            out.extend_from_slice(format!("\x1b[{};{}H", row + 1, col + 1).as_bytes());
            let keys = |w: usize, h: usize| format!("a=T,f=32,s={w},v={h},i={id},p=1,X={px},Y={py},c={cols},r={rows},z={},C=1", -100 + k as i32);
            if self.pixel > 0 {
                let (sw, sh) = (cols as usize * self.grid.cell_w, rows as usize * self.grid.cell_h);
                upscale(pixels, w, self.pixel, sw, sh, &mut self.scaled);
                self.ring.transmit(out, &self.scaled, &keys(sw, sh))?;
            } else {
                self.ring.transmit(out, pixels, &keys(w, h))?;
            }
            self.renders += 1;
        }

        let pellets: Vec<(u32, Spot)> = koi_render::shown_food(school)
            .map(|(sprite, fx, fy)| {
                let half = self.food_sizes[sprite / FADE_LEVELS] as f32 / 2.0;
                let snap = self.pixel.max(1) as f32;
                let (x, y) = ((((fx * self.scale_x - half) / snap).round() * snap) as i32, (((fy * self.scale_y - half) / snap).round() * snap) as i32);
                (FIRST_FOOD_ID + sprite as u32, self.spot(x, y))
            })
            .collect();
        if force || pellets != self.pellets {
            for id in FIRST_FOOD_ID..FIRST_FOOD_ID + ((FoodKind::ALL.len() + 1) * FADE_LEVELS) as u32 {
                out.extend_from_slice(format!("\x1b_Ga=d,d=i,i={id},q=2\x1b\\").as_bytes());
            }
            for (n, &(id, spot)) in pellets.iter().enumerate() {
                out.extend_from_slice(
                    format!("\x1b[{};{}H\x1b_Ga=p,i={id},p={},X={},Y={},z=-10,C=1,q=2\x1b\\", spot[1] + 1, spot[0] + 1, n + 1, spot[2], spot[3]).as_bytes(),
                );
            }
            self.pellets = pellets;
        }

        if force || overlay != self.overlay.as_deref() {
            out.extend_from_slice(b"\x1b[1;1H\x1b[2K");
            if let Some(text) = overlay {
                out.extend_from_slice(text.as_bytes());
            }
            self.overlay = overlay.map(str::to_string);
        }
        Ok(())
    }
}

/// Cells a sprite box covers, and the canvas in sprite pixels padded to whole cells, so a
/// c/r placement scales it without stretching. In a pixel theme a sprite pixel is `pixel`
/// screen pixels, and the canvas rounds up to cover the cells.
fn canvas(grid: &Grid, fish_px: usize, pixel: usize, left: i32, top: i32, right: i32, bottom: i32) -> (i32, i32, usize, usize) {
    let screen_per_px = if pixel > 0 { pixel as f32 } else { grid.cell_w as f32 / fish_px as f32 };
    let cols = (((right - left + 1) as f32 * screen_per_px) / grid.cell_w as f32).ceil().max(1.0);
    let rows = (((bottom - top + 1) as f32 * screen_per_px) / grid.cell_h as f32).ceil().max(1.0);
    if pixel > 0 {
        return (cols as i32, rows as i32, (cols as usize * grid.cell_w).div_ceil(pixel), (rows as usize * grid.cell_h).div_ceil(pixel));
    }
    (cols as i32, rows as i32, cols as usize * fish_px, (rows * grid.cell_h as f32 / screen_per_px).round() as usize)
}

/// Up to `n` colours for the RGBA `frame`: a median cut of a sample of its pixels, each
/// box's mean. A frame with fewer colours gives fewer.
fn median_cut(frame: &[u8], n: usize) -> Vec<Rgb> {
    let pixels = frame.len() / 4;
    let sample: Vec<Rgb> = frame.as_chunks::<4>().0.iter().step_by((pixels / 60_000).max(1)).map(|&[r, g, b, _]| [r, g, b]).collect();
    // Each box with its widest channel, and that channel's range times its pixels.
    let measure = |colours: Vec<Rgb>| {
        let range = |c: usize| colours.iter().map(|p| p[c]).max().unwrap_or(0) - colours.iter().map(|p| p[c]).min().unwrap_or(0);
        let channel = (0..3).max_by_key(|&c| range(c)).unwrap_or(0);
        let weight = usize::from(range(channel)) * colours.len();
        (colours, channel, weight)
    };
    let mut boxes = vec![measure(sample)];
    while boxes.len() < n {
        let Some(widest) = (0..boxes.len()).filter(|&i| boxes[i].2 > 0).max_by_key(|&i| boxes[i].2) else { break };
        let (mut colours, channel, _) = boxes.swap_remove(widest);
        colours.sort_unstable_by_key(|p| p[channel]);
        let upper = colours.split_off(colours.len() / 2);
        boxes.push(measure(colours));
        boxes.push(measure(upper));
    }
    boxes
        .iter()
        .filter(|b| !b.0.is_empty())
        .map(|(colours, _, _)| {
            let sum = colours.iter().fold([0usize; 3], |s, p| [s[0] + usize::from(p[0]), s[1] + usize::from(p[1]), s[2] + usize::from(p[2])]);
            sum.map(|c| u8::try_from(c / colours.len()).unwrap_or(u8::MAX))
        })
        .collect()
}

/// The index of `palette`'s colour nearest `colour`.
fn nearest(palette: &[Rgb], colour: Rgb) -> u8 {
    let distance = |p: &Rgb| (0..3).map(|c| (i32::from(p[c]) - i32::from(colour[c])).pow(2)).sum::<i32>();
    let best = (0..palette.len()).min_by_key(|&i| distance(&palette[i])).unwrap_or(0);
    u8::try_from(best).expect("a palette has at most 256 colours")
}

/// Appends `entries`, palette indices `w` pixels wide, as one sixel image at the cursor:
/// square pixels, pixels it does not set left as they are (P2=1), so the rows below the
/// last whole band stay untouched, only the registers it uses defined, and runs of 4 or more
/// run-length encoded. `planes` is scratch space.
fn sixel(out: &mut Vec<u8>, entries: &[u8], w: usize, palette: &[Rgb], planes: &mut Vec<u8>) {
    let h = entries.len() / w;
    out.extend_from_slice(format!("\x1bP0;1;0q\"1;1;{w};{h}").as_bytes());
    let mut used = [false; 256];
    for &e in entries {
        used[usize::from(e)] = true;
    }
    for (i, [r, g, b]) in palette.iter().enumerate().filter(|(i, _)| used[*i]) {
        let percent = |v: &u8| (u32::from(*v) * 100 + 127) / 255;
        out.extend_from_slice(format!("#{i};2;{};{};{}", percent(r), percent(g), percent(b)).as_bytes());
    }
    // One plane of sixel bits per colour, of which a band uses only a few.
    planes.resize(256 * w, 0);
    let (mut in_band, mut seen) = (Vec::new(), [false; 256]);
    for top in (0..h).step_by(6) {
        for bit in 0..6.min(h - top) {
            for (x, &e) in entries[(top + bit) * w..][..w].iter().enumerate() {
                if !seen[usize::from(e)] {
                    seen[usize::from(e)] = true;
                    in_band.push(e);
                }
                planes[usize::from(e) * w + x] |= 1 << bit;
            }
        }
        for (n, &e) in in_band.iter().enumerate() {
            if n > 0 {
                out.push(b'$');
            }
            out.extend_from_slice(format!("#{e}").as_bytes());
            let plane = &mut planes[usize::from(e) * w..][..w];
            let end = plane.iter().rposition(|&b| b != 0).map_or(0, |x| x + 1);
            let mut x = 0;
            while x < end {
                let run = plane[x..end].iter().take_while(|&&b| b == plane[x]).count();
                let char = plane[x] + 63;
                if run >= 4 {
                    out.extend_from_slice(format!("!{run}").as_bytes());
                    out.push(char);
                } else {
                    out.extend(std::iter::repeat_n(char, run));
                }
                x += run;
            }
            plane[..end].fill(0);
            seen[usize::from(e)] = false;
        }
        in_band.clear();
        if top + 6 < h {
            out.push(b'-');
        }
    }
    out.extend_from_slice(b"\x1b\\");
}

/// Appends cell row `row` of the RGBA `frame`, of cells `cell` pixels in size, as half
/// blocks: U+2580 with the mean of each cell's top half in the foreground and of its bottom
/// half in the background, or a space when both are the same. Without `halves` every cell
/// is a space in the mean of all of it, which needs no glyph from the font. A cell that
/// shows text is skipped, and so is one within `BLOCK_TOLERANCE` of what `prev` says it
/// shows, unless text just left it (`freed`). `prev`, `text` and `freed` cover the row's cells.
#[allow(clippy::too_many_arguments)]
fn blocks(
    out: &mut Vec<u8>,
    frame: &[u8],
    cell: (usize, usize),
    row: usize,
    prev: &mut [Option<[Rgb; 2]>],
    text: &[bool],
    freed: &[bool],
    truecolor: bool,
    halves: bool,
) {
    let (cw, ch) = cell;
    let w = prev.len() * cw;
    let mean = |col: usize, ys: std::ops::Range<usize>| {
        let n = ys.len().max(1) * cw;
        let mut sum = [0usize; 3];
        for y in ys {
            for px in frame[(y * w + col * cw) * 4..][..cw * 4].as_chunks::<4>().0 {
                sum = [sum[0] + usize::from(px[0]), sum[1] + usize::from(px[1]), sum[2] + usize::from(px[2])];
            }
        }
        let colour = sum.map(|c| u8::try_from(c / n).unwrap_or(u8::MAX));
        if truecolor { colour } else { cube(colour).1 }
    };
    let sgr = |layer: u8, [r, g, b]: Rgb| if truecolor { format!("\x1b[{layer}8;2;{r};{g};{b}m") } else { format!("\x1b[{layer}8;5;{}m", cube([r, g, b]).0) };
    let (mut at, mut fg, mut bg) = (None, None, None);
    let half = if halves { ch / 2 } else { 0 };
    for col in 0..prev.len() {
        if text[col] {
            continue;
        }
        let bottom = mean(col, row * ch + half..(row + 1) * ch);
        let cell = [if halves { mean(col, row * ch..row * ch + half.max(1)) } else { bottom }, bottom];
        let near = |p: &[Rgb; 2]| (0..2).all(|k| (0..3).all(|c| p[k][c].abs_diff(cell[k][c]) <= BLOCK_TOLERANCE));
        if !freed[col] && prev[col].as_ref().is_some_and(near) {
            continue;
        }
        prev[col] = Some(cell);
        if at != Some(col) {
            out.extend_from_slice(format!("\x1b[{};{}H", row + 1, col + 1).as_bytes());
        }
        if bg != Some(cell[1]) {
            out.extend_from_slice(sgr(4, cell[1]).as_bytes());
            bg = Some(cell[1]);
        }
        if cell[0] == cell[1] {
            out.push(b' ');
        } else {
            if fg != Some(cell[0]) {
                out.extend_from_slice(sgr(3, cell[0]).as_bytes());
                fg = Some(cell[0]);
            }
            out.extend_from_slice("\u{2580}".as_bytes());
        }
        at = Some(col + 1);
    }
    if at.is_some() {
        out.extend_from_slice(b"\x1b[0m");
    }
}

/// The 256-colour palette's nearest entry to `colour`, from its 6x6x6 cube (16 to 231) or,
/// for a colour close to grey, its grey ramp (232 to 255): the index and the colour it shows.
/// A dark blue is nearer a grey than any cube colour, but a pond keeps its hue better blue.
fn cube(colour: Rgb) -> (u8, Rgb) {
    const LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];
    let level = |v: u8| (0..6).min_by_key(|&i| LEVELS[i].abs_diff(v)).unwrap_or(0);
    let [r, g, b] = colour.map(level);
    let in_cube = [LEVELS[r], LEVELS[g], LEVELS[b]];
    let mean = colour.iter().map(|&c| usize::from(c)).sum::<usize>() / 3;
    let step = (mean.saturating_sub(3) / 10).min(23);
    let grey = [u8::try_from(8 + 10 * step).expect("at most 238"); 3];
    let distance = |p: Rgb| (0..3).map(|c| (i32::from(p[c]) - i32::from(colour[c])).pow(2)).sum::<i32>();
    let spread = colour.iter().max().unwrap_or(&0) - colour.iter().min().unwrap_or(&0);
    if spread < 32 && distance(grey) < distance(in_cube) {
        (u8::try_from(232 + step).expect("at most 255"), grey)
    } else {
        (u8::try_from(16 + 36 * r + 6 * g + b).expect("at most 231"), in_cube)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use koi_term::Sixel;
    use serde::Deserialize;
    use serde::de::IntoDeserializer;

    /// A tier's name, which warnings and the stats line show, is the name config.toml and
    /// `--protocol` take for it.
    #[test]
    fn tier_names_are_protocol_names() {
        for tier in [Tier::Kitty, Tier::KittyDirect, Tier::Sixel, Tier::Blocks] {
            let protocol = match tier {
                Tier::Kitty => Protocol::Kitty,
                Tier::KittyDirect => Protocol::KittyDirect,
                Tier::Sixel => Protocol::Sixel,
                Tier::Blocks => Protocol::Blocks,
            };
            let read: Result<Protocol, serde::de::value::Error> = Protocol::deserialize(tier.name().into_deserializer());
            assert_eq!(read.ok(), Some(protocol), "{}", tier.name());
        }
    }

    /// In a pixel theme every koi image is sent at exactly the size of the cells it covers,
    /// so Ghostty does not rescale it, and placed on a multiple of the art pixel, so it shares
    /// the water's grid. The water is sent at the size of the window.
    #[test]
    fn pixel_sprites_are_screen_sized_and_on_the_art_grid() {
        let theme = koi_theme::Catalog::load(None).0.resolve("lantern-dusk").expect("built-in theme");
        let pixel = theme.style.pixel_px as usize;
        let grid = Grid { cols: 64, rows: 18, cell_w: 11, cell_h: 23, water_w: 128, water_h: 75 };
        let (screen_w, screen_h): (usize, usize) = (64 * 11, 18 * 23);
        let (w, h) = (screen_w.div_ceil(pixel), screen_h.div_ceil(pixel));
        let mut school = School::new(grid.water_w, grid.water_h, 5, 4);
        for _ in 0..120 {
            school.step();
        }
        let mut poser = Poser::new(None, &school, &theme, w as f32 / grid.water_w as f32);
        let mut out = Vec::new();
        let mut layers =
            Layers::new(&mut out, grid, &school, &poser, &theme.palette, 11, pixel, (w, h), Tier::KittyDirect, None, &Caps::default()).expect("layers");
        out.clear();
        let poses: Vec<Pose> = school.fish.iter().map(|f| f.pose()).collect();
        let water = vec![128u8; w * h * 4];
        layers.encode(&mut out, &school, &poses, &mut poser, Some((&water, w, h)), None, false).expect("encode");
        let text = String::from_utf8_lossy(&out);
        let key = |command: &str, name: &str| {
            command.split([',', ';']).find_map(|k| k.strip_prefix(&format!("{name}="))).map(|v| v.parse::<usize>().expect("a number"))
        };
        let mut koi = 0;
        for command in text.split("\x1b_G").skip(1) {
            let (Some(id), Some(s), Some(v)) = (key(command, "i"), key(command, "s"), key(command, "v")) else { continue };
            if id == WATER_ID as usize {
                assert_eq!((s, v), (screen_w, screen_h));
            } else if id >= FIRST_KOI_ID as usize {
                let (c, r) = (key(command, "c").expect("columns"), key(command, "r").expect("rows"));
                assert_eq!((s, v), (c * 11, r * 23), "koi {id} is scaled by Ghostty");
                koi += 1;
            }
        }
        assert_eq!(koi, 5);
        // Each koi's cursor move comes just before its image: the cell plus X/Y is the corner.
        for part in text.split("\x1b[").skip(1) {
            let Some((cell, rest)) = part.split_once('H') else { continue };
            let Some((row, col)) = cell.split_once(';') else { continue };
            let Some(command) = rest.strip_prefix("\x1b_G").filter(|c| c.contains(&format!("i={}", FIRST_KOI_ID))) else { continue };
            let (row, col): (usize, usize) = (row.parse().expect("row"), col.parse().expect("col"));
            let (x, y) = ((col - 1) * 11 + key(command, "X").expect("X"), (row - 1) * 23 + key(command, "Y").expect("Y"));
            assert_eq!((x % pixel, y % pixel), (0, 0), "the first koi sits at ({x}, {y}), off the art grid");
        }
    }

    /// Auto takes the best tier the terminal has; a forced tier is used even when the
    /// terminal did not confirm it, with a warning.
    #[test]
    fn choose_the_best_tier() {
        let kitty = Caps { kitty_shm: true, kitty_direct: true, truecolor: true, ..Caps::default() };
        let ssh = Caps { kitty_direct: true, truecolor: true, ..Caps::default() };
        let sixel = |colors, max| Caps { sixel: Some(Sixel { colors, max }), ..Caps::default() };
        let screen = (1600, 900);
        let auto = |caps: &Caps, tmux: bool| choose(Protocol::Auto, caps, tmux, screen);
        assert_eq!(auto(&kitty, false), (Tier::Kitty, None));
        assert_eq!(auto(&kitty, true), (Tier::Kitty, None));
        assert_eq!(auto(&ssh, false), (Tier::KittyDirect, None));
        let wezterm = Caps { name: Some("WezTerm 20260703-142320-59d94d19".to_string()), ..sixel(65536, Some((1600, 990))) };
        assert_eq!(auto(&Caps { kitty_shm: true, kitty_direct: true, ..wezterm }, false), (Tier::Sixel, None));
        assert_eq!(auto(&sixel(256, Some((2000, 2000))), false), (Tier::Sixel, None));
        assert_eq!(auto(&sixel(1024, None), false), (Tier::Sixel, None));
        assert_eq!(auto(&sixel(256, None), true).0, Tier::Blocks, "tmux answers for itself, not the terminal");
        assert_eq!(auto(&sixel(256, Some((1000, 1000))), false).0, Tier::Blocks, "the window is wider than the largest image");
        assert_eq!(auto(&sixel(16, None), false).0, Tier::Blocks, "too few colours");
        assert_eq!(auto(&Caps::default(), false), (Tier::Blocks, None), "silence");
        assert_eq!(choose(Protocol::Kitty, &kitty, false, screen), (Tier::Kitty, None));
        assert_eq!(choose(Protocol::Blocks, &kitty, false, screen), (Tier::Blocks, None));
        for (setting, tier) in [(Protocol::Sixel, Tier::Sixel), (Protocol::KittyDirect, Tier::KittyDirect), (Protocol::Kitty, Tier::Kitty)] {
            let (chosen, warning) = choose(setting, &Caps::default(), false, screen);
            assert_eq!(chosen, tier);
            assert!(warning.is_some_and(|w| w.contains(tier.name())));
        }
    }

    /// The decoded image is the palette colour of every entry within 3, at any height, and
    /// rows below the last whole band are left unset.
    #[test]
    fn sixel_round_trips() {
        let palette: Vec<Rgb> = (0..=255u8).map(|i| [i, i.wrapping_mul(37), 255 - i]).collect();
        for (w, h) in [(1, 1), (7, 13), (40, 6), (33, 20), (64, 1)] {
            // Runs of 5, so run-length encoding is used, and every row different.
            let entries: Vec<u8> = (0..w * h).map(|n: usize| u8::try_from((n % w / 5 * 11 + n / w * 3) % 256).expect("a byte")).collect();
            let mut out = Vec::new();
            sixel(&mut out, &entries, w, &palette, &mut Vec::new());
            let image = icy_sixel::SixelImage::decode(&out).expect("decodes");
            assert_eq!(image.width, w);
            assert!(image.height >= h);
            for (n, px) in image.pixels.as_chunks::<4>().0.iter().enumerate() {
                if n >= w * h {
                    assert_eq!(px[3], 0, "{w}x{h}: pixel {n} below the image is set");
                    continue;
                }
                let want = palette[usize::from(entries[n])];
                assert!((0..3).all(|c| px[c].abs_diff(want[c]) <= 3) && px[3] == 255, "{w}x{h}: pixel {n} is {px:?}, not {want:?}");
            }
        }
        // Three repeats are written out, four are run-length encoded.
        let encode = |w: usize| {
            let mut out = Vec::new();
            sixel(&mut out, &vec![0; w], w, &palette, &mut Vec::new());
            String::from_utf8(out).expect("ASCII")
        };
        assert!(encode(3).ends_with("#0@@@\x1b\\"), "{}", encode(3));
        assert!(encode(4).ends_with("#0!4@\x1b\\"), "{}", encode(4));
    }

    /// Sixel sends a changed cell and whatever lies between it and the row's last changed
    /// cell, never over text, and draws a cell again once text leaves it: the images sent,
    /// pasted where they go, give the new frame. The bottom row is half blocks.
    #[test]
    fn sixel_spans_update_the_last_frame() {
        let theme = koi_theme::Catalog::load(None).0.resolve("summer-garden").expect("built-in theme");
        let grid = Grid { cols: 12, rows: 4, cell_w: 5, cell_h: 7, water_w: 24, water_h: 16 };
        let (w, h) = (60, 28);
        let school = School::new(grid.water_w, grid.water_h, 1, 4);
        let poser = Poser::new(None, &school, &theme, 1.0);
        let caps = Caps { sixel: Some(Sixel { colors: 256, max: None }), truecolor: true, ..Caps::default() };
        let mut layers = Layers::new(&mut Vec::new(), grid, &school, &poser, &theme.palette, 5, 0, (24, 16), Tier::Sixel, None, &caps).expect("layers");
        let colours: [Rgb; 6] = [[10, 40, 80], [200, 30, 20], [240, 240, 230], [30, 30, 30], [90, 160, 60], [250, 200, 40]];
        let paint = |f: &dyn Fn(usize, usize) -> usize| (0..w * h).flat_map(|n| colours[f(n % w, n / w)].into_iter().chain([255])).collect::<Vec<u8>>();
        let mut canvas = vec![0u8; w * h * 4];
        let mut send = |layers: &mut Layers, frame: Vec<u8>, text: &[(usize, usize, usize)]| {
            layers.framed.as_mut().expect("a frame").frame = frame;
            let mut out = Vec::new();
            layers.send(&mut out, None, text).expect("send");
            let out = String::from_utf8(out).expect("UTF-8");
            let mut images = 0;
            for part in out.split("\x1b[").skip(1) {
                let Some((cell, rest)) = part.split_once('H') else { continue };
                if !rest.starts_with("\x1bP") {
                    continue;
                }
                let (row, col) = cell.split_once(';').expect("a cursor move");
                let (row, col): (usize, usize) = (row.parse().expect("row"), col.parse().expect("col"));
                assert!(row < 4, "an image on the bottom row");
                let image = icy_sixel::SixelImage::decode(rest.as_bytes()).expect("decodes");
                for y in 0..7 {
                    for x in 0..image.width {
                        let at = (((row - 1) * 7 + y) * w + (col - 1) * 5 + x) * 4;
                        canvas[at..at + 4].copy_from_slice(&image.pixels[(y * image.width + x) * 4..][..4]);
                    }
                }
                images += 1;
            }
            (images, canvas.clone())
        };
        let same = |canvas: &[u8], frame: &[u8], cells: &dyn Fn(usize, usize) -> bool| {
            (0..w * 21).filter(|n| cells(n % w / 5, n / w / 7)).all(|n| (0..3).all(|c| canvas[n * 4 + c].abs_diff(frame[n * 4 + c]) <= 3))
        };
        let first = paint(&|x, y| (x / 9 + y / 4) % 6);
        let (images, shown) = send(&mut layers, first.clone(), &[]);
        assert_eq!(images, 3, "one image a row, the bottom row aside");
        assert!(same(&shown, &first, &|_, _| true));

        // Two changed cells in row 1 with text between them, one in row 2.
        let second = paint(&|x, y| {
            if (x / 5, y / 7) == (1, 1) || (x / 5, y / 7) == (9, 1) || (x / 5, y / 7) == (4, 2) || (x / 5, y / 7) == (6, 1) { 5 } else { (x / 9 + y / 4) % 6 }
        });
        let (images, shown) = send(&mut layers, second.clone(), &[(6, 2, 3)]);
        assert_eq!(images, 3, "row 1 splits at the text");
        assert!(same(&shown, &second, &|col, row| !(row == 1 && (5..8).contains(&col))));
        assert!(same(&shown, &first, &|col, row| row == 1 && (5..8).contains(&col)), "nothing is drawn over text");

        // The text leaves: its cells are drawn, and only them.
        let (images, shown) = send(&mut layers, second.clone(), &[]);
        assert_eq!(images, 1);
        assert!(same(&shown, &second, &|_, _| true));
        let (images, _) = send(&mut layers, second, &[]);
        assert_eq!(images, 0, "an unchanged frame sends nothing");
    }

    /// Half blocks: the top pixel in the foreground and the bottom in the background, a space
    /// for a cell of one colour, only changed cells after the first frame, none under text,
    /// and the 256-colour palette's cube and grey ramp without 24-bit colour.
    #[test]
    fn blocks_send_only_changed_cells() {
        let frame = |bottom_right: u8| vec![10, 20, 30, 255, 40, 50, 60, 255, 10, 20, 30, 255, 70, 80, bottom_right, 255];
        let mut prev = vec![None; 2];
        let send = |prev: &mut Vec<Option<[Rgb; 2]>>, frame: &[u8], text: [bool; 2], freed: [bool; 2], truecolor: bool| {
            let mut out = Vec::new();
            blocks(&mut out, frame, (1, 2), 0, prev, &text, &freed, truecolor, true);
            String::from_utf8(out).expect("UTF-8")
        };
        assert_eq!(
            send(&mut prev, &frame(90), [false; 2], [false; 2], true),
            "\x1b[1;1H\x1b[48;2;10;20;30m \x1b[48;2;70;80;90m\x1b[38;2;40;50;60m\u{2580}\x1b[0m"
        );
        assert_eq!(send(&mut prev, &frame(90), [false; 2], [false; 2], true), "", "unchanged");
        assert_eq!(send(&mut prev, &frame(92), [false; 2], [false; 2], true), "", "within the tolerance");
        assert_eq!(send(&mut prev, &frame(120), [false; 2], [false; 2], true), "\x1b[1;2H\x1b[48;2;70;80;120m\x1b[38;2;40;50;60m\u{2580}\x1b[0m");
        assert_eq!(send(&mut prev, &frame(200), [false, true], [false; 2], true), "", "text");
        assert_eq!(send(&mut prev, &frame(120), [false; 2], [true, false], true), "\x1b[1;1H\x1b[48;2;10;20;30m \x1b[0m", "text left");
        let mut prev = vec![None; 2];
        assert_eq!(send(&mut prev, &frame(90), [false; 2], [false; 2], false), "\x1b[1;1H\x1b[48;5;233m \x1b[48;5;239m\x1b[38;5;236m\u{2580}\x1b[0m");
        assert_eq!(cube([0, 0, 0]), (16, [0, 0, 0]));
        assert_eq!(cube([255, 255, 255]), (231, [255; 3]));
        assert_eq!(cube([128, 128, 128]), (244, [128; 3]));
        assert_eq!(cube([255, 0, 0]), (196, [255, 0, 0]));
        assert_eq!(cube([100, 150, 200]), (68, [95, 135, 215]));
        assert_eq!(cube([13, 43, 69]), (17, [0, 0, 95]), "blue, though a grey is nearer");
    }
}
