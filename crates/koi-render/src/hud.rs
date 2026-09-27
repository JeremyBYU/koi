//! The HUD's stones and icons, painted on the CPU from the active theme: soft painted stones for painterly themes, stepped pixel stones on
//! a fine HUD grid for pixel themes. The binary decides what is shown where; this module only
//! paints one element at a time.

use koi_theme::{Palette, Rgb, Theme, Time, linear, mix, srgb};

/// The colours every HUD element is drawn from, derived from a palette with no new slots.
#[derive(Clone, Debug, PartialEq)]
pub struct Colors {
    /// The pond is dark (OKLCH L of `mid` under 0.45), so the stones are dark too and never
    /// the brightest thing on a night pond.
    pub dark: bool,
    /// The stone's body, under its edges and marks.
    pub fill: Rgb,
    /// Lit top-left edge.
    pub lit: Rgb,
    /// Shaded bottom-right edge.
    pub shade: Rgb,
    /// Stone outline.
    pub outline: Rgb,
    /// Cast shadow, drawn at 30% alpha (40% on a dark pond).
    pub shadow: Rgb,
    /// Text and icon outlines. At least 0.45 OKLCH L away from `fill`.
    pub ink: Rgb,
    /// Secondary text, such as the keys on the help card.
    pub dim: Rgb,
    /// Volume dots and the selection dot.
    pub accent: Rgb,
}

impl Colors {
    /// The HUD colours for `palette`. `pixel` takes the outline from the theme's `outline`
    /// slot, as pixel themes do.
    pub fn new(palette: &Palette, pixel: bool) -> Colors {
        let p = palette;
        let dark = lightness(p.mid) < 0.45;
        let fill = if dark { mix(p.stone_dark, p.deep, 0.35) } else { mix(p.stone_light, p.koi_white, 0.55) };
        let mut ink = if dark { p.ui_text } else { mix(p.koi_sumi, p.deep, 0.15) };
        for _ in 0..10 {
            if (lightness(fill) - lightness(ink)).abs() >= 0.45 {
                break;
            }
            ink = mix(ink, if dark { [255; 3] } else { [0; 3] }, 0.25);
        }
        Colors {
            dark,
            fill,
            lit: mix(fill, p.highlight, if dark { 0.25 } else { 0.5 }),
            shade: if dark { mix(fill, p.deep, 0.4) } else { mix(fill, p.stone_dark, 0.35) },
            outline: if pixel {
                p.outline
            } else if dark {
                mix(p.deep, [0; 3], 0.3)
            } else {
                mix(p.stone_dark, p.deep, 0.4)
            },
            shadow: p.deep,
            ink,
            dim: if dark { p.ui_dim } else { mix(ink, fill, 0.45) },
            accent: p.ui_accent,
        }
    }
}

/// A tiny view of another scene's water, for the scene pebble and tray.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Swatch {
    /// The scene's `shallow`.
    pub shallow: Rgb,
    /// The scene's `mid`.
    pub mid: Rgb,
    /// The scene's `deep`.
    pub deep: Rgb,
    /// The scene's `lily_light`, for a lily pad.
    pub pad: Rgb,
    /// The scene's `koi_red`, for a koi.
    pub koi: Rgb,
}

impl Swatch {
    /// The swatch of a scene with `palette`.
    pub fn new(palette: &Palette) -> Swatch {
        Swatch { shallow: palette.shallow, mid: palette.mid, deep: palette.deep, pad: palette.lily_light, koi: palette.koi_red }
    }
}

/// One icon on a stone. Sizes are for 20 px tall cells and scale with the cell height.
#[derive(Clone, Debug, PartialEq)]
pub enum Mark {
    /// A music note, covered by a leaf when muted.
    Note {
        /// Draw the leaf.
        muted: bool,
    },
    /// The next-track arrow.
    Next,
    /// A volume dot, filled from the left by `fill` (0, 0.5 or 1) in the accent colour.
    Dot {
        /// How much of the dot is filled.
        fill: f32,
    },
    /// A food: 0 pellets, 1 flakes, 2 petals, 3 seeds, 4 a treat.
    Food(usize),
    /// A scene's water.
    Swatch(Swatch),
    /// The sun at its height for a time of day, or the moon at night.
    Time(Time),
    /// A question mark.
    Help,
    /// A pale disc behind a tray item under the pointer.
    Glow,
    /// The accent dot under the chosen tray item.
    Chosen,
}

/// How the HUD looks under one theme at one cell height.
pub struct Look {
    /// The derived colours, also used for the terminal text on the HUD.
    pub colors: Colors,
    palette: Palette,
    /// Device pixels per HUD art pixel for pixel themes, 0 for painterly ones.
    pixel: usize,
    grain: f32,
    /// Device pixels per mockup pixel (the mockup's cells are 20 px tall).
    scale: f32,
}

impl Look {
    /// The look for `theme` with cells `cell_h` device pixels tall. Pixel themes get a 2 px
    /// HUD grid, 3 px on HiDPI cells (30 px or taller), whatever the pond's `pixel_px`.
    pub fn new(theme: &Theme, cell_h: usize) -> Look {
        let pixel = if theme.style.pixel_px == 0 {
            0
        } else if cell_h >= 30 {
            3
        } else {
            2
        };
        Look { colors: Colors::new(&theme.palette, pixel > 0), palette: theme.palette.clone(), pixel, grain: theme.style.grain, scale: cell_h as f32 / 20.0 }
    }

    /// Paints a `w` x `h` device pixel stone into `rgba` (sRGB, straight alpha), with `marks`
    /// centred at their device pixel positions. `seed` gives each element its own fixed
    /// outline wobble. `lifted` raises the stone 1 px and warms its outline, for the element
    /// under the pointer. Pixel themes leave the right and bottom pixels that do not fill a
    /// whole HUD pixel transparent.
    pub fn paint(&self, w: usize, h: usize, seed: u32, lifted: bool, marks: &[(Mark, f32, f32)], rgba: &mut Vec<u8>) {
        let c = &self.colors;
        let k = self.scale;
        let outline = if lifted { mix(c.outline, self.palette.highlight, 0.45) } else { c.outline };
        let fill = if lifted { mix(c.fill, c.lit, 0.35) } else { c.fill };
        // Painterly stones take tens of milliseconds on one core, so they are painted in
        // bands of rows on all of them. Pixel stones are a few thousand art pixels.
        let (px, cw, ch, unit) = if self.pixel == 0 {
            let mut px = vec![[0.0; 4]; w * h];
            let rows = h.div_ceil(std::thread::available_parallelism().map_or(1, |n| n.get())).max(1);
            std::thread::scope(|scope| {
                for (band, px) in px.chunks_mut(w * rows).enumerate() {
                    crate::spawn(scope, move || {
                        let mut canvas = Canvas { w, top: band * rows, unit: 1.0, px };
                        let top = if h < 30 { 1.0 } else { 2.0 } * k;
                        let (a, b) = ((w as f32 - 9.0 * k) / 2.0, (h as f32 - top - k) / 2.0);
                        let (cx, cy) = (3.0 * k + a, top + b - if lifted { 1.0 } else { 0.0 });
                        let shape = |x: f32, y: f32| stone(x, y, cx, cy, a, b, seed);
                        canvas.soft(c.shadow, if c.dark { 0.45 } else { 0.3 }, 2.0 * k, |x, y| stone(x, y, cx + 3.5 * k, cy + 1.5 * k, a, b, seed));
                        canvas.fill(c.shade, 1.0, shape);
                        canvas.soft(c.lit, 1.0, 1.5 * k, |x, y| stone(x, y, cx - 3.0 * k, cy - 3.0 * k, a, b, seed).max(shape(x, y)));
                        canvas.soft(fill, 1.0, 1.5 * k, |x, y| stone(x, y, cx - 0.8 * k, cy - 0.8 * k, a - 0.5 * k, b - 0.5 * k, seed).max(shape(x, y)));
                        if self.grain > 0.0 {
                            for (i, px) in canvas.px.iter_mut().enumerate() {
                                let n = 1.0 + (hash((band * rows * w + i) as u32, seed) - 0.5) * 2.0 * self.grain;
                                (px[0], px[1], px[2]) = (px[0] * n, px[1] * n, px[2] * n);
                            }
                        }
                        for (mark, x, y) in marks {
                            self.mark(&mut canvas, mark, *x, *y - if lifted { 1.0 } else { 0.0 });
                        }
                        canvas.fill(outline, 1.0, |x, y| shape(x, y).abs() - 0.65 * k);
                    });
                }
            });
            (px, w, h, 1)
        } else {
            let unit = self.pixel;
            let (lw, lh) = (w / unit, h / unit);
            let mut px = vec![[0.0; 4]; lw * lh];
            let (x1, y1) = (lw as i32 - 2, lh as i32 - 1);
            let steps = if lw.min(lh) >= 20 { 4 } else { 3 };
            let inside = |x: i32, y: i32| x >= 0 && y >= 0 && x <= x1 && y <= y1 && x.min(x1 - x) + y.min(y1 - y) >= steps - 1;
            for y in 0..lh as i32 {
                for x in 0..lw as i32 {
                    let at = y as usize * lw + x as usize;
                    if inside(x, y) {
                        let edge = !inside(x - 1, y) || !inside(x + 1, y) || !inside(x, y - 1) || !inside(x, y + 1);
                        let lit_edge = !inside(x - 2, y) || !inside(x, y - 2);
                        let shade_edge = !inside(x + 2, y) || !inside(x, y + 2);
                        let color = match (edge, lit_edge, shade_edge) {
                            (true, _, _) => outline,
                            (false, true, false) => c.lit,
                            (false, false, true) => c.shade,
                            _ => fill,
                        };
                        px[at] = premultiplied(color, 1.0);
                    } else if inside(x - 1, y) {
                        px[at] = premultiplied(c.shadow, 140.0 / 255.0);
                    }
                }
            }
            let mut icon_px = vec![[0.0; 4]; lw * lh];
            let mut icons = Canvas { w: lw, top: 0, unit: unit as f32, px: &mut icon_px };
            for (mark, x, y) in marks {
                self.mark(&mut icons, mark, *x, *y);
            }
            let p = &self.palette;
            let snap: Vec<[f32; 3]> = [
                c.ink,
                c.dim,
                c.accent,
                c.lit,
                c.fill,
                c.outline,
                p.food,
                p.lily_flower,
                p.lily_light,
                p.lily_dark,
                p.koi_white,
                p.koi_red,
                p.koi_sumi,
                p.ogon,
                p.asagi_red,
                p.asagi_blue,
                p.highlight,
                p.shallow,
                p.mid,
                p.deep,
            ]
            .into_iter()
            .chain(marks.iter().flat_map(|(mark, _, _)| match mark {
                Mark::Swatch(s) => vec![s.shallow, s.mid, s.deep, s.pad, s.koi],
                _ => Vec::new(),
            }))
            .map(linear)
            .collect();
            for (at, icon) in icon_px.iter().enumerate() {
                let (x, y) = ((at % lw) as i32, (at / lw) as i32);
                if icon[3] < 0.43 || !inside(x, y) {
                    continue;
                }
                let color = [icon[0] / icon[3], icon[1] / icon[3], icon[2] / icon[3]];
                let distance = |s: &[f32; 3]| (color[0] - s[0]).powi(2) * 0.3 + (color[1] - s[1]).powi(2) * 0.59 + (color[2] - s[2]).powi(2) * 0.11;
                let best = snap.iter().min_by(|a, b| distance(a).total_cmp(&distance(b))).expect("the snap list is never empty");
                px[at] = [best[0], best[1], best[2], 1.0];
            }
            (px, lw, lh, unit)
        };

        rgba.clear();
        rgba.resize(w * h * 4, 0);
        for y in 0..(ch * unit).min(h) {
            for x in 0..(cw * unit).min(w) {
                let [r, g, b, a] = px[(y / unit) * cw + x / unit];
                if a <= 0.0 {
                    continue;
                }
                let [r, g, b] = srgb([r / a, g / a, b / a]);
                rgba[(y * w + x) * 4..][..4].copy_from_slice(&[r, g, b, (a.min(1.0) * 255.0).round() as u8]);
            }
        }
    }

    fn mark(&self, canvas: &mut Canvas, mark: &Mark, x: f32, y: f32) {
        let (c, p, k) = (&self.colors, &self.palette, self.scale);
        match mark {
            Mark::Note { muted } => {
                canvas.fill(c.ink, 1.0, |px, py| ellipse(px, py, x - 4.0 * k, y + 6.0 * k, 5.0 * k, 3.8 * k, -0.45));
                canvas.fill(c.ink, 1.0, |px, py| {
                    let stem = segment(px, py, x + 0.4 * k, y + 5.0 * k, x + 0.4 * k, y - 10.0 * k);
                    let flag = segment(px, py, x + 0.4 * k, y - 10.0 * k, x + 3.0 * k, y - 6.5 * k).min(segment(
                        px,
                        py,
                        x + 3.0 * k,
                        y - 6.5 * k,
                        x + 8.0 * k,
                        y - 4.0 * k,
                    ));
                    stem.min(flag) - 1.2 * k
                });
                if *muted {
                    let leaf = |px: f32, py: f32| ellipse(px, py, x + k, y, 11.0 * k, 4.5 * k, -0.7);
                    let vein = mix(p.lily_dark, p.deep, 0.3);
                    canvas.fill(p.lily_light, 1.0, leaf);
                    canvas.fill(vein, 1.0, |px, py| leaf(px, py).abs() - 0.6 * k);
                    let (dx, dy) = (9.0 * k * 0.7f32.cos(), -9.0 * k * 0.7f32.sin());
                    canvas.fill(vein, 1.0, |px, py| segment(px, py, x + k - dx, y - dy, x + k + dx, y + dy) - 0.6 * k);
                }
            }
            Mark::Next => canvas.fill(c.ink, 1.0, |px, py| polygon(px, py, &[(x + 4.5 * k, y), (x - 3.5 * k, y - 5.5 * k), (x - 3.5 * k, y + 5.5 * k)])),
            Mark::Dot { fill } => {
                let r = 3.6 * k;
                if *fill > 0.0 {
                    let edge = x - 5.0 * k + fill * 10.0 * k;
                    canvas.fill(c.accent, 1.0, |px, py| circle(px, py, x, y, r).max(px - edge));
                }
                let ring = if *fill > 0.0 { mix(c.accent, c.ink, 0.35) } else { c.dim };
                canvas.fill(ring, 1.0, |px, py| circle(px, py, x, y, r).abs() - 0.65 * k);
            }
            Mark::Food(kind) => {
                let spots: &[(f32, f32)] = match kind {
                    0 => &[(-5.0, 3.0), (5.0, 4.0), (0.0, -5.0)],
                    1 => &[(-7.0, 2.0), (-2.0, -5.0), (3.0, 4.0), (7.0, -2.0), (0.0, 0.0)],
                    2 => &[(-4.0, 2.0), (4.0, -3.0)],
                    3 => &[(-5.0, -2.0), (1.0, 4.0), (6.0, -3.0)],
                    _ => &[(0.0, 5.0)],
                };
                let edge = 0.55 * k;
                for (i, &(dx, dy)) in spots.iter().enumerate() {
                    let (fx, fy) = (x + dx * k, y + dy * k);
                    match kind {
                        0 => {
                            let s = 1.6 * k;
                            canvas.fill(mix(p.food, p.deep, 0.35), 1.0, |px, py| circle(px, py, fx + 0.6 * s, fy + 0.8 * s, 3.6 * s));
                            canvas.fill(p.food, 1.0, |px, py| circle(px, py, fx, fy, 3.4 * s));
                            canvas.fill(c.ink, 1.0, |px, py| circle(px, py, fx, fy, 3.4 * s).abs() - edge);
                            canvas.fill(mix(p.food, p.highlight, 0.6), 1.0, |px, py| circle(px, py, fx - 1.1 * s, fy - 1.1 * s, 1.2 * s));
                        }
                        1 => {
                            let s = 1.6 * k;
                            let (sin, cos) = (i as f32).sin_cos();
                            let corners: Vec<(f32, f32)> = [(-2.6, -1.0), (0.5, -2.4), (2.6, 0.2), (0.2, 2.2), (-2.0, 1.4)]
                                .iter()
                                .map(|&(u, v)| (fx + (u * cos - v * sin) * s, fy + (u * sin + v * cos) * s))
                                .collect();
                            let color = if i % 2 == 1 { mix(p.koi_white, p.food, 0.25) } else { mix(p.asagi_red, p.koi_white, 0.3) };
                            canvas.fill(color, 1.0, |px, py| polygon(px, py, &corners));
                            canvas.fill(c.ink, 1.0, |px, py| polygon(px, py, &corners).abs() - edge);
                        }
                        2 => {
                            let s = 1.5 * k;
                            let petal = |px: f32, py: f32| ellipse(px, py, fx, fy, 5.5 * s, 2.2 * s, i as f32 * 1.3 - 0.4);
                            canvas.fill(if i % 2 == 1 { p.lily_flower } else { mix(p.lily_flower, p.koi_white, 0.6) }, 1.0, petal);
                            canvas.fill(c.ink, 1.0, |px, py| petal(px, py).abs() - edge);
                        }
                        3 => {
                            let s = 1.6 * k;
                            let seed = |px: f32, py: f32| ellipse(px, py, fx, fy, 2.8 * s, 1.8 * s, 0.4);
                            canvas.fill(mix(p.koi_sumi, p.food, 0.35), 1.0, seed);
                            canvas.fill(c.ink, 1.0, |px, py| seed(px, py).abs() - edge);
                            canvas.fill(p.highlight, 0.5, |px, py| circle(px, py, fx - 0.9 * s, fy - 0.6 * s, 0.7 * s));
                        }
                        _ => {
                            let s = 1.25 * k;
                            let half = |px: f32, py: f32, r: f32| circle(px, py, fx, fy, r * s).max(py - fy);
                            canvas.fill(p.lily_light, 1.0, |px, py| half(px, py, 11.0));
                            canvas.fill(c.ink, 1.0, |px, py| half(px, py, 11.0).abs() - edge);
                            canvas.fill(mix(p.koi_white, p.lily_light, 0.3), 1.0, |px, py| half(px, py, 9.0));
                            canvas.fill(p.koi_red, 1.0, |px, py| half(px, py, 7.8));
                            for (sx, sy) in [(-4.0, -2.5), (0.0, -4.5), (4.0, -2.5)] {
                                canvas.fill(p.koi_sumi, 1.0, |px, py| ellipse(px, py, fx + sx * s, fy + sy * s, 0.9 * s, 1.4 * s, sx * 0.1));
                            }
                        }
                    }
                }
            }
            Mark::Swatch(s) => {
                canvas.fill(s.shallow, 1.0, |px, py| circle(px, py, x, y, 13.0 * k));
                canvas.fill(s.mid, 1.0, |px, py| circle(px, py, x + 1.5 * k, y + k, 9.0 * k));
                canvas.fill(s.deep, 1.0, |px, py| circle(px, py, x + 2.5 * k, y + 2.0 * k, 4.5 * k));
                canvas.fill(s.pad, 1.0, |px, py| circle(px, py, x - 7.0 * k, y - 6.0 * k, 5.0 * k).max(circle(px, py, x, y, 13.0 * k)));
                canvas.fill(s.koi, 1.0, |px, py| ellipse(px, py, x + 5.0 * k, y - 3.0 * k, 4.0 * k, 1.8 * k, 0.5));
                canvas.fill(c.outline, 1.0, |px, py| circle(px, py, x, y, 13.0 * k).abs() - 0.7 * k);
            }
            Mark::Time(Time::Night) => {
                let (mx, bx, by) = (x - k, x + 4.0 * k, y - 4.0 * k);
                canvas.fill(c.ink, 1.0, |px, py| circle(px, py, mx, y, 10.5 * k).max(-circle(px, py, bx, by, 8.0 * k)));
                canvas.fill(mix(p.koi_white, p.ogon, 0.25), 1.0, |px, py| circle(px, py, mx, y, 9.2 * k).max(-circle(px, py, bx, by, 9.3 * k)));
                canvas.fill(p.ogon, 1.0, |px, py| circle(px, py, x + 9.0 * k, y + 6.0 * k, 1.8 * k));
            }
            Mark::Time(time) => {
                let (low, color, rays) = match time {
                    Time::Dawn => (7.0, mix(p.koi_red, p.highlight, 0.4), 3),
                    Time::Morning => (3.0, mix(p.ogon, p.highlight, 0.45), 5),
                    Time::Afternoon => (1.0, p.ogon, 5),
                    Time::Evening => (5.0, mix(p.ogon, p.koi_red, 0.45), 5),
                    Time::Dusk => (8.0, mix(p.koi_red, p.asagi_blue, 0.3), 3),
                    _ => (-2.0, p.ogon, 8),
                };
                let noon = *time == Time::Noon;
                let sy = y + low * k;
                let above = |py: f32| if noon { f32::MIN } else { py - sy };
                for ray in 0..rays {
                    let a = if noon { ray as f32 / 8.0 * std::f32::consts::TAU } else { std::f32::consts::PI * (1.0 + (ray as f32 + 0.5) / rays as f32) };
                    let (sin, cos) = a.sin_cos();
                    canvas.fill(color, 1.0, |px, py| {
                        (segment(px, py, x + cos * 10.0 * k, sy + sin * 10.0 * k, x + cos * 13.5 * k, sy + sin * 13.5 * k) - k).max(above(py))
                    });
                }
                canvas.fill(color, 1.0, |px, py| circle(px, py, x, sy, 7.0 * k).max(above(py)));
                canvas.fill(mix(color, c.ink, 0.4), 1.0, |px, py| (circle(px, py, x, sy, 7.0 * k).abs() - 0.6 * k).max(above(py)));
                if !noon {
                    canvas.fill(c.ink, 1.0, |px, py| segment(px, py, x - 12.0 * k, sy, x + 12.0 * k, sy) - 0.9 * k);
                }
            }
            Mark::Help => {
                let (ax, ay, r) = (x, y - 3.5 * k, 4.2 * k);
                // The arc runs clockwise from just below the left side, over the top, to
                // just below the right side.
                let (start, end) = (std::f32::consts::PI * 0.95, 0.55f32);
                let (sx, sy) = (ax + r * start.cos(), ay + r * start.sin());
                let (ex, ey) = (ax + r * end.cos(), ay + r * end.sin());
                canvas.fill(c.ink, 1.0, |px, py| {
                    let angle = (py - ay).atan2(px - ax);
                    let arc = if angle <= end || angle >= start {
                        ((px - ax).hypot(py - ay) - r).abs()
                    } else {
                        (px - sx).hypot(py - sy).min((px - ex).hypot(py - ey))
                    };
                    arc.min(segment(px, py, ex, ey, x, y + 2.5 * k)) - 1.3 * k
                });
                canvas.fill(c.ink, 1.0, |px, py| circle(px, py, x, y + 6.5 * k, 1.6 * k));
            }
            Mark::Glow => canvas.fill(c.lit, 0.9, |px, py| circle(px, py, x, y, 17.0 * k)),
            Mark::Chosen => {
                canvas.fill(c.accent, 1.0, |px, py| circle(px, py, x, y, 2.6 * k));
                canvas.fill(mix(c.accent, c.ink, 0.35), 1.0, |px, py| circle(px, py, x, y, 2.6 * k).abs() - 0.5 * k);
            }
        }
    }
}

/// Premultiplied linear RGBA pixels, each `unit` device pixels square. Shapes are signed
/// distance functions in device pixels, sampled at pixel centres.
struct Canvas<'a> {
    w: usize,
    /// The row of the whole picture that `px` starts at, when this is one band of it.
    top: usize,
    unit: f32,
    px: &'a mut [[f32; 4]],
}

impl Canvas<'_> {
    /// Paints `color` at `alpha` where `shape` is negative, anti-aliased over one pixel.
    fn fill(&mut self, color: Rgb, alpha: f32, shape: impl Fn(f32, f32) -> f32) {
        let unit = self.unit;
        self.paint(color, alpha, |x, y| (0.5 - shape(x, y) / unit).clamp(0.0, 1.0));
    }

    /// Like `fill`, with an edge that fades over `blur` device pixels each side.
    fn soft(&mut self, color: Rgb, alpha: f32, blur: f32, shape: impl Fn(f32, f32) -> f32) {
        self.paint(color, alpha, |x, y| {
            let t = ((blur - shape(x, y)) / (2.0 * blur)).clamp(0.0, 1.0);
            t * t * (3.0 - 2.0 * t)
        });
    }

    fn paint(&mut self, color: Rgb, alpha: f32, cover: impl Fn(f32, f32) -> f32) {
        let color = linear(color);
        for (at, px) in self.px.iter_mut().enumerate() {
            let (x, y) = (((at % self.w) as f32 + 0.5) * self.unit, ((self.top + at / self.w) as f32 + 0.5) * self.unit);
            let a = cover(x, y) * alpha;
            if a > 0.0 {
                let keep = 1.0 - a;
                *px = [color[0] * a + px[0] * keep, color[1] * a + px[1] * keep, color[2] * a + px[2] * keep, a + px[3] * keep];
            }
        }
    }
}

fn premultiplied(color: Rgb, alpha: f32) -> [f32; 4] {
    let [r, g, b] = linear(color);
    [r * alpha, g * alpha, b * alpha, alpha]
}

fn circle(x: f32, y: f32, cx: f32, cy: f32, r: f32) -> f32 {
    (x - cx).hypot(y - cy) - r
}

/// Distance to the segment from (ax, ay) to (bx, by). Subtract half a width to stroke it.
fn segment(x: f32, y: f32, ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    let (dx, dy) = (bx - ax, by - ay);
    let t = (((x - ax) * dx + (y - ay) * dy) / (dx * dx + dy * dy).max(1e-6)).clamp(0.0, 1.0);
    (x - ax - dx * t).hypot(y - ay - dy * t)
}

/// A convex polygon, corners in either winding. Exact along the edges, which is all
/// anti-aliasing needs.
fn polygon(x: f32, y: f32, corners: &[(f32, f32)]) -> f32 {
    let n = corners.len() as f32;
    let (mx, my) = corners.iter().fold((0.0, 0.0), |(sx, sy), &(cx, cy)| (sx + cx / n, sy + cy / n));
    let mut d = f32::MIN;
    for (i, &(ax, ay)) in corners.iter().enumerate() {
        let (bx, by) = corners[(i + 1) % corners.len()];
        let len = (bx - ax).hypot(by - ay).max(1e-6);
        let (nx, ny) = ((by - ay) / len, (ax - bx) / len);
        let flip = if (mx - ax) * nx + (my - ay) * ny > 0.0 { -1.0 } else { 1.0 };
        d = d.max(((x - ax) * nx + (y - ay) * ny) * flip);
    }
    d
}

/// A rotated ellipse, as a distance near its edge.
fn ellipse(x: f32, y: f32, cx: f32, cy: f32, a: f32, b: f32, angle: f32) -> f32 {
    let (sin, cos) = angle.sin_cos();
    along_gradient(x, y, |x, y| {
        let (dx, dy) = (x - cx, y - cy);
        ((dx * cos + dy * sin) / a).hypot((dy * cos - dx * sin) / b) - 1.0
    })
}

/// A squircle `2a` x `2b` around (cx, cy), its edge pushed in and out by a few percent with
/// a wobble fixed by `seed`, so each element is a slightly different stone.
fn stone(x: f32, y: f32, cx: f32, cy: f32, a: f32, b: f32, seed: u32) -> f32 {
    let (s1, s2) = (hash(1, seed) * 6.0, hash(2, seed) * 6.0);
    along_gradient(x, y, |x, y| {
        let (dx, dy) = (x - cx, y - cy);
        let t = dy.atan2(dx);
        let off = a.min(b) * 0.05 * (0.6 * (3.0 * t + s1).sin() + 0.4 * (5.0 * t + s2).sin());
        ((dx / (a + off)).powi(4) + (dy / (b + off)).powi(4)).sqrt().sqrt() - 1.0
    })
}

/// Turns an implicit shape `f` (negative inside, -1 at its centre) into an approximate
/// distance in pixels by dividing by its gradient, so long thin shapes keep a one-pixel edge
/// all round. Points further than half the shape's radius from the edge only need the sign,
/// which saves most of the work on a large stone.
fn along_gradient(x: f32, y: f32, f: impl Fn(f32, f32) -> f32) -> f32 {
    let v = f(x, y);
    if v.abs() > 0.5 {
        return v.signum() * 1e4;
    }
    let (gx, gy) = (f(x + 0.25, y) - f(x - 0.25, y), f(x, y + 0.25) - f(x, y - 0.25));
    v / (gx.hypot(gy) * 2.0).max(1e-6)
}

fn hash(i: u32, seed: u32) -> f32 {
    let mut h = i.wrapping_mul(0x8da6_b343) ^ seed.wrapping_mul(0xcb1a_b31f);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1_e995);
    h ^= h >> 15;
    (h >> 8) as f32 / (1u32 << 24) as f32
}

/// OKLab lightness, 0 to 1.
fn lightness(c: Rgb) -> f32 {
    let [r, g, b] = linear(c);
    let l = (0.412_221_46 * r + 0.536_332_55 * g + 0.051_445_995 * b).cbrt();
    let m = (0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b).cbrt();
    let s = (0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b).cbrt();
    0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s
}

#[cfg(test)]
mod tests {
    use super::*;
    use koi_theme::Catalog;

    /// Every built-in theme's HUD ink reads on its stones, and dark ponds get dark stones.
    #[test]
    fn ink_contrasts_with_every_theme() {
        let catalog = Catalog::load(None).0;
        for summary in catalog.summaries() {
            let theme = catalog.resolve(&summary.id).expect("built-in theme resolves");
            let c = Colors::new(&theme.palette, theme.style.pixel_px > 0);
            assert!((lightness(c.fill) - lightness(c.ink)).abs() >= 0.45, "{}: ink {:?} on fill {:?}", summary.id, c.ink, c.fill);
            if c.dark {
                assert!(lightness(c.fill) < lightness(theme.palette.highlight), "{}: a dark pond's stone is brighter than its light", summary.id);
            }
        }
    }

    /// Pixel stones have no soft edges: every pixel is fully clear, fully opaque, or the
    /// hard shadow, and the HUD grid's pixels are whole squares.
    #[test]
    fn pixel_stones_are_hard_edged() {
        let catalog = Catalog::load(None).0;
        let theme = catalog.resolve("lantern-dusk").expect("built-in theme resolves");
        assert!(theme.style.pixel_px > 0);
        let look = Look::new(&theme, 21);
        let mut rgba = Vec::new();
        look.paint(60, 63, 7, false, &[(Mark::Food(0), 29.0, 21.0), (Mark::Time(Time::Dusk), 29.0, 21.0)], &mut rgba);
        assert!(rgba.chunks(4).all(|p| matches!(p[3], 0 | 140 | 255)));
        for y in 0..62 {
            for x in 0..60 {
                let at = (y * 60 + x) * 4;
                assert_eq!(rgba[at..at + 4], rgba[((y / 2 * 2) * 60 + x / 2 * 2) * 4..][..4], "({x}, {y})");
            }
        }
        assert!(rgba[(62 * 60) * 4..].iter().all(|&v| v == 0), "the leftover row is transparent");
    }
}
