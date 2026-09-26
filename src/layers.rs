use koi_render::Poser;
use koi_sim::{BUBBLE_LIFE, FoodKind, Pose, School};
use koi_term::ShmRing;
use koi_theme::Palette;
use std::io;

const FADE_LEVELS: usize = 8;
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

/// Cell column and row, and pixel offset inside that cell.
type Spot = [i32; 4];

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
/// In tmux (`tmux` set) images cannot be placed at the cursor, since tmux does not know
/// about them. Everything is drawn into one frame the size of the window instead (`compose`
/// and the HUD's `compose`), and the terminal shows it through placeholder cells that tmux
/// keeps with the pane (`send`). There the grid's cells are the frame's, which the terminal
/// scales to its own.
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
    pub ring: ShmRing,
    /// None in tmux, where the water is part of the one frame.
    water_ring: Option<ShmRing>,
    pub renders: usize,
    tmux: Option<Tmux>,
}

/// The frame of tmux mode.
struct Tmux {
    /// The image id, also the colour its placeholder cells are drawn in.
    id: u8,
    frame: Vec<u8>,
    /// The last water image, scaled to the frame.
    water: Vec<u8>,
    /// Side and pixels of each food and bubble sprite, in `recolor`'s id order.
    food: Vec<(usize, Vec<u8>)>,
    /// The placeholder cells must be written: at start, and when tmux may have lost them.
    grid_due: bool,
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
    /// size of the images `Water::render` makes. `tmux` is the image id of tmux mode.
    #[allow(clippy::too_many_arguments)]
    pub fn new(out: &mut Vec<u8>, grid: Grid, school: &School, poser: &Poser, palette: &Palette, fish_px: usize, pixel: usize, water: (usize, usize), tmux: Option<u8>) -> io::Result<Layers> {
        let scale_x = (grid.cols * grid.cell_w) as f32 / grid.water_w as f32;
        let pellet_radius = grid.water_h as f32 * 0.011 * scale_x / pixel.max(1) as f32;
        let mut pixels = Vec::new();
        let food_sizes = std::array::from_fn(|n| {
            let side = match FoodKind::ALL.get(n) {
                Some(&kind) => koi_render::food_sprite(kind, pellet_radius, 1.0, palette, &mut pixels),
                None => koi_render::bubble_sprite(pellet_radius, 1.0, palette, &mut pixels),
            };
            side * pixel.max(1)
        });

        let screen = (grid.cols * grid.cell_w, grid.rows * grid.cell_h);
        let mut largest = food_sizes.iter().map(|s| s * s * 4).max().unwrap_or(0);
        for k in 0..school.fish.len() {
            // A little extra for the rounding in `bounds`.
            let (cols, rows, w, h) = canvas(&grid, fish_px, pixel, 0, 0, poser.largest(k) + fish_px as i32, poser.largest(k) + fish_px as i32);
            let sent = if pixel > 0 { cols as usize * grid.cell_w * rows as usize * grid.cell_h } else { w * h };
            largest = largest.max(sent * 4);
        }
        largest = largest.max(crate::hud::largest_image(grid.cell_w, grid.cell_h));
        let water_bytes = 4 * if pixel > 0 { screen.0 * screen.1 } else { water.0 * water.1 };
        // In tmux the one frame a frame goes through the main ring.
        let (ring, water_ring) = match tmux {
            Some(_) => (ShmRing::new(WATER_SLOTS, screen.0 * screen.1 * 4)?, None),
            None => (ShmRing::new((FoodKind::ALL.len() + 1) * FADE_LEVELS + school.fish.len() + 8 + RING_SPARE, largest)?, Some(ShmRing::new(WATER_SLOTS, water_bytes)?)),
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
            tmux: tmux.map(|id| Tmux { id, frame: Vec::new(), water: vec![0; screen.0 * screen.1 * 4], food: Vec::new(), grid_due: true }),
        };
        layers.recolor(out, palette)?;
        Ok(layers)
    }

    /// Sends every food kind's sprites in `palette`'s colours, then the bubble's, replacing
    /// any sent before. Kind `k` at fade level `l` (1 to 8) is image `FIRST_FOOD_ID + k *
    /// FADE_LEVELS + l - 1`, and a bubble at stage `l` is kind `FoodKind::ALL.len()`. The next
    /// `encode` places the food again. In tmux the sprites are kept for `compose`.
    pub fn recolor(&mut self, out: &mut Vec<u8>, palette: &Palette) -> io::Result<()> {
        let mut pixels = Vec::new();
        let mut id = FIRST_FOOD_ID;
        if let Some(t) = &mut self.tmux {
            t.food.clear();
        }
        for kind in FoodKind::ALL.map(Some).into_iter().chain([None]) {
            for level in 1..=FADE_LEVELS {
                let size = match kind {
                    Some(kind) => koi_render::food_sprite(kind, self.pellet_radius, level as f32 / FADE_LEVELS as f32, palette, &mut pixels),
                    None => koi_render::bubble_sprite(self.pellet_radius, (level as f32 - 0.5) / FADE_LEVELS as f32, palette, &mut pixels),
                };
                let side = size * self.pixel.max(1);
                if self.pixel > 0 {
                    upscale(&pixels, size, self.pixel, side, side, &mut self.scaled);
                }
                let sprite = if self.pixel > 0 { &self.scaled } else { &pixels };
                match &mut self.tmux {
                    Some(t) => t.food.push((side, sprite.clone())),
                    None => self.ring.transmit(out, sprite, &format!("a=t,f=32,s={side},v={side},i={id}"))?,
                }
                id += 1;
            }
        }
        self.pellets.clear();
        Ok(())
    }

    /// In tmux, writes the placeholder cells again on the next `compose`, for when tmux may
    /// have lost them.
    pub fn redraw(&mut self) {
        if let Some(t) = &mut self.tmux {
            t.grid_due = true;
        }
    }

    /// tmux mode: draws the frame, the water (scaled from `water` when given, else the last
    /// water) with the koi in `poses` and the food over it, and returns it for the HUD to draw
    /// on. Appends the placeholder cells to `out` when they are due. `send` sends the frame.
    /// Outside tmux it does nothing and returns `None`.
    pub fn compose(&mut self, out: &mut Vec<u8>, school: &School, poses: &[Pose], poser: &mut Poser, water: Option<(&[u8], usize, usize)>) -> Option<&mut [u8]> {
        let t = self.tmux.as_mut()?;
        let (screen_w, screen_h) = (self.grid.cols * self.grid.cell_w, self.grid.rows * self.grid.cell_h);
        if t.grid_due {
            for row in 0..self.grid.rows {
                koi_term::placeholders(out, t.id, row, 0, self.grid.cols);
            }
            t.grid_due = false;
            // The cells took the top line's place.
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

        // As in `encode`, with no cells to fit: a sprite pixel is a frame pixel, or in a pixel
        // theme `pixel` of them, and the fraction lost to the snap goes into the pose.
        let per_px = self.pixel.max(1);
        let snap = per_px as f32;
        for (k, pose) in poses.iter().enumerate() {
            let (cx, cy) = (pose.x * self.scale_x, pose.y * self.scale_y);
            let (left, top, right, bottom) = poser.bounds(k, pose);
            let (x, y) = (((cx + left as f32 * snap) / snap).round() * snap, ((cy + top as f32 * snap) / snap).round() * snap);
            let (w, h) = ((right - left + 1) as usize, (bottom - top + 1) as usize);
            let pixels = poser.pose(k, pose, (x - cx) / snap, (y - cy) / snap, w, h);
            blend(&mut t.frame, screen_w, pixels, w, x as i32, y as i32, per_px, 255);
            self.renders += 1;
        }
        let food = school.food.iter().map(|food| (food.kind.index(), (food.fade() * FADE_LEVELS as f32).ceil() as usize, food.x, food.y));
        let bubbles = school.bubbles.iter().filter(|b| b.age >= 0.0).map(|b| (FoodKind::ALL.len(), (b.age / BUBBLE_LIFE * FADE_LEVELS as f32) as usize + 1, b.x, b.y));
        for (kind, level, fx, fy) in food.chain(bubbles) {
            if level == 0 {
                continue;
            }
            let (side, sprite) = &t.food[kind * FADE_LEVELS + level - 1];
            let half = *side as f32 / 2.0;
            let (x, y) = ((((fx * self.scale_x - half) / snap).round() * snap) as i32, (((fy * self.scale_y - half) / snap).round() * snap) as i32);
            blend(&mut t.frame, screen_w, sprite, *side, x, y, 1, 255);
        }
        Some(&mut t.frame)
    }

    /// tmux mode: sends the frame through a tmux passthrough as the image the placeholder
    /// cells show, and the top line if it changed. Outside tmux it does nothing.
    pub fn send(&mut self, out: &mut Vec<u8>, overlay: Option<&str>) -> io::Result<()> {
        let Some(t) = &self.tmux else { return Ok(()) };
        let (cols, rows) = (self.grid.cols, self.grid.rows);
        let mut command = Vec::new();
        self.ring.transmit(&mut command, &t.frame, &format!("a=T,U=1,f=32,s={},v={},i={},p=1,c={cols},r={rows}", cols * self.grid.cell_w, rows * self.grid.cell_h, t.id))?;
        koi_term::tmux_wrap(out, &command);
        if overlay != self.overlay.as_deref() {
            koi_term::placeholders(out, t.id, 0, 0, cols);
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
    pub fn encode(&mut self, out: &mut Vec<u8>, school: &School, poses: &[Pose], poser: &mut Poser, water: Option<(&[u8], usize, usize)>, overlay: Option<&str>, force: bool) -> io::Result<()> {
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

        let food = school.food.iter().map(|food| (food.kind.index(), (food.fade() * FADE_LEVELS as f32).ceil() as usize, food.x, food.y));
        let bubbles = school.bubbles.iter().filter(|b| b.age >= 0.0).map(|b| (FoodKind::ALL.len(), (b.age / BUBBLE_LIFE * FADE_LEVELS as f32) as usize + 1, b.x, b.y));
        let pellets: Vec<(u32, Spot)> = food
            .chain(bubbles)
            .filter_map(|(kind, level, fx, fy)| {
                let half = self.food_sizes[kind] as f32 / 2.0;
                let snap = self.pixel.max(1) as f32;
                let (x, y) = ((((fx * self.scale_x - half) / snap).round() * snap) as i32, (((fy * self.scale_y - half) / snap).round() * snap) as i32);
                (level > 0).then(|| (FIRST_FOOD_ID + (kind * FADE_LEVELS + level - 1) as u32, self.spot(x, y)))
            })
            .collect();
        if force || pellets != self.pellets {
            for id in FIRST_FOOD_ID..FIRST_FOOD_ID + ((FoodKind::ALL.len() + 1) * FADE_LEVELS) as u32 {
                out.extend_from_slice(format!("\x1b_Ga=d,d=i,i={id},q=2\x1b\\").as_bytes());
            }
            for (n, &(id, spot)) in pellets.iter().enumerate() {
                out.extend_from_slice(format!("\x1b[{};{}H\x1b_Ga=p,i={id},p={},X={},Y={},z=-10,C=1,q=2\x1b\\", spot[1] + 1, spot[0] + 1, n + 1, spot[2], spot[3]).as_bytes());
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

/// Draws `src`, straight-alpha RGBA `w` pixels wide, over the opaque `dst`, `dst_w` pixels
/// wide, with its top-left at (`x`, `y`), each pixel as a `k` x `k` block and its alpha
/// scaled by `alpha` / 255. What falls outside `dst` is cut off.
#[allow(clippy::too_many_arguments)]
pub fn blend(dst: &mut [u8], dst_w: usize, src: &[u8], w: usize, x: i32, y: i32, k: usize, alpha: u32) {
    let dst_h = dst.len() / 4 / dst_w;
    let at = |origin: i32, i: usize, limit: usize| usize::try_from(i64::from(origin) + i64::try_from(i).ok()?).ok().filter(|&p| p < limit);
    for (sy, row) in src.chunks_exact(w * 4).enumerate() {
        for dy in sy * k..(sy + 1) * k {
            let Some(dy) = at(y, dy, dst_h) else { continue };
            for (sx, px) in row.as_chunks::<4>().0.iter().enumerate() {
                let a = u32::from(px[3]) * alpha / 255;
                if a == 0 {
                    continue;
                }
                for dx in sx * k..(sx + 1) * k {
                    let Some(dx) = at(x, dx, dst_w) else { continue };
                    let d = &mut dst[(dy * dst_w + dx) * 4..][..3];
                    for c in 0..3 {
                        d[c] = u8::try_from((u32::from(px[c]) * a + u32::from(d[c]) * (255 - a) + 127) / 255).unwrap_or(u8::MAX);
                    }
                }
            }
        }
    }
}

/// `src`, RGBA `w` x `h`, scaled to `out_w` x `out_h` with a linear filter between pixel
/// centres, as the terminal scales an image to its cells, into `dst`.
fn stretch(src: &[u8], w: usize, h: usize, out_w: usize, out_h: usize, dst: &mut Vec<u8>) {
    // Where each output pixel's centre falls, in 256ths of a source pixel from the first
    // source pixel's centre: the pixel to its left or above, and the weight of the next one.
    let taps = |n: usize, out: usize| -> Vec<(usize, usize, u32)> {
        (0..out)
            .map(|i| {
                let at = ((2 * i + 1) * n * 256 / (2 * out)).saturating_sub(128);
                let lo = (at / 256).min(n - 1);
                (lo, (lo + 1).min(n - 1), u32::try_from(at % 256).unwrap_or(0))
            })
            .collect()
    };
    let (xs, ys) = (taps(w, out_w), taps(h, out_h));
    dst.clear();
    for &(y0, y1, fy) in &ys {
        let (top, bottom) = (&src[y0 * w * 4..][..w * 4], &src[y1 * w * 4..][..w * 4]);
        for &(x0, x1, fx) in &xs {
            for c in 0..4 {
                let row = |r: &[u8]| u32::from(r[x0 * 4 + c]) * (256 - fx) + u32::from(r[x1 * 4 + c]) * fx;
                dst.push(u8::try_from((row(top) * (256 - fy) + row(bottom) * fy + (1 << 15)) >> 16).unwrap_or(u8::MAX));
            }
        }
    }
}

/// `src`, RGBA `w` pixels wide, scaled up `k` times by repeating pixels and cropped to
/// `out_w` x `out_h`, into `dst`.
fn upscale(src: &[u8], w: usize, k: usize, out_w: usize, out_h: usize, dst: &mut Vec<u8>) {
    dst.clear();
    for y in 0..out_h {
        if y % k == 0 {
            let row = &src[(y / k) * w * 4..][..w * 4];
            dst.extend((0..out_w).flat_map(|x| row[(x / k) * 4..][..4].iter().copied()));
        } else {
            dst.extend_from_within(dst.len() - out_w * 4..);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let mut layers = Layers::new(&mut out, grid, &school, &poser, &theme.palette, 11, pixel, (w, h), None).expect("shm ring");
        out.clear();
        let poses: Vec<Pose> = school.fish.iter().map(|f| f.pose()).collect();
        let water = vec![128u8; w * h * 4];
        layers.encode(&mut out, &school, &poses, &mut poser, Some((&water, w, h)), None, false).expect("encode");
        let text = String::from_utf8_lossy(&out);
        let key = |command: &str, name: &str| command.split([',', ';']).find_map(|k| k.strip_prefix(&format!("{name}="))).map(|v| v.parse::<usize>().expect("a number"));
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
}
