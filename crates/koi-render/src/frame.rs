//! The pond as one RGBA frame: water scaled to the frame, the koi over it, and the food and
//! bubbles over them. The terminal uses it where it cannot layer images, and the web page for
//! its canvas. Also `advance`, the fixed-step loop both of them run the pond with.

use crate::{Poser, Water, bubble_sprite, food_sprite};
use koi_sim::{BUBBLE_LIFE, DT, FoodKind, Pose, School};
use koi_theme::Palette;

/// Steps the school and the water in fixed `DT` steps through `behind` seconds, the time the
/// simulation is behind the frame, and hands the school's splashes to the water. `before`
/// keeps each koi's pose from before the last step, for blending. Returns the seconds
/// stepped, which is `behind` itself after 60 steps: after a long pause the pond carries on
/// from now rather than catching up.
pub fn advance(school: &mut School, water: &mut Water, before: &mut Vec<Pose>, behind: f64) -> f64 {
    let dt = f64::from(DT);
    let mut steps = 0;
    while f64::from(steps + 1) * dt <= behind && steps < 60 {
        *before = school.fish.iter().map(|f| f.pose()).collect();
        school.step();
        for splash in school.splashes.drain(..) {
            water.splash(splash);
        }
        water.step();
        steps += 1;
    }
    for splash in school.splashes.drain(..) {
        water.splash(splash);
    }
    if steps == 60 { behind } else { f64::from(steps) * dt }
}

/// Fade levels of each food kind's sprites, and stages of the bubble's.
pub const FADE_LEVELS: usize = 8;

/// Every food kind's sprites at each fade level, then the bubble's at each stage, in the
/// order `shown_food` indexes them: kind `k` at level `l` (1 to `FADE_LEVELS`) is `k *
/// FADE_LEVELS + l - 1`, and the bubble is kind `FoodKind::ALL.len()`. Each is its side and
/// straight-alpha RGBA pixels. `radius` is a pellet's in sprite pixels, and in a pixel theme
/// each sprite pixel is scaled up to `pixel` screen pixels.
pub fn food_sprites(palette: &Palette, radius: f32, pixel: usize) -> Vec<(usize, Vec<u8>)> {
    let mut pixels = Vec::new();
    let mut sprites = Vec::new();
    for kind in FoodKind::ALL.map(Some).into_iter().chain([None]) {
        for level in 1..=FADE_LEVELS {
            let size = match kind {
                Some(kind) => food_sprite(kind, radius, level as f32 / FADE_LEVELS as f32, palette, &mut pixels),
                None => bubble_sprite(radius, (level as f32 - 0.5) / FADE_LEVELS as f32, palette, &mut pixels),
            };
            if pixel > 0 {
                let mut scaled = Vec::new();
                upscale(&pixels, size, pixel, size * pixel, size * pixel, &mut scaled);
                sprites.push((size * pixel, scaled));
            } else {
                sprites.push((size, pixels.clone()));
            }
        }
    }
    sprites
}

/// The food and risen bubbles in the water: each one's index in `food_sprites`, and where
/// its centre is on the simulation grid. Food that has faded out is left out.
pub fn shown_food(school: &School) -> impl Iterator<Item = (usize, f32, f32)> + '_ {
    let food = school.food.iter().map(|food| (food.kind.index(), (food.fade() * FADE_LEVELS as f32).ceil() as usize, food.x, food.y));
    let bubbles =
        school.bubbles.iter().filter(|b| b.age >= 0.0).map(|b| (FoodKind::ALL.len(), (b.age / BUBBLE_LIFE * FADE_LEVELS as f32) as usize + 1, b.x, b.y));
    food.chain(bubbles).filter(|&(_, level, _, _)| level > 0).map(|(kind, level, x, y)| (kind * FADE_LEVELS + level - 1, x, y))
}

/// Poses each koi in `poses` and hands `place` its sprite: the straight-alpha RGBA pixels, their
/// width and height, and where the sprite's top-left goes in frame pixels. `scale` is frame
/// pixels per simulation unit, across and down. A sprite pixel is a frame pixel, or in a pixel
/// theme `pixel` of them, and the koi snap to the art grid there; the fraction lost to the snap
/// goes into the pose, so koi move evenly.
pub fn place_koi(poses: &[Pose], poser: &mut Poser, scale: (f32, f32), pixel: usize, mut place: impl FnMut(&[u8], usize, usize, i32, i32)) {
    let snap = pixel.max(1) as f32;
    for (k, pose) in poses.iter().enumerate() {
        let (cx, cy) = (pose.x * scale.0, pose.y * scale.1);
        let (left, top, right, bottom) = poser.bounds(k, pose);
        let (x, y) = (((cx + left as f32 * snap) / snap).round() * snap, ((cy + top as f32 * snap) / snap).round() * snap);
        let (w, h) = ((right - left + 1) as usize, (bottom - top + 1) as usize);
        place(poser.pose(k, pose, (x - cx) / snap, (y - cy) / snap, w, h), w, h, x as i32, y as i32);
    }
}

/// The food and risen bubbles as `shown_food` gives them, each with its sprite's top-left in
/// frame pixels: centred on it, and in a pixel theme snapped to the art grid. `food` is
/// `food_sprites`' output and `scale` and `pixel` are as `place_koi` takes them.
pub fn place_food<'a>(school: &'a School, food: &'a [(usize, Vec<u8>)], scale: (f32, f32), pixel: usize) -> impl Iterator<Item = (usize, i32, i32)> + 'a {
    let snap = pixel.max(1) as f32;
    shown_food(school).map(move |(sprite, fx, fy)| {
        let half = food[sprite].0 as f32 / 2.0;
        (sprite, (((fx * scale.0 - half) / snap).round() * snap) as i32, (((fy * scale.1 - half) / snap).round() * snap) as i32)
    })
}

/// Draws the koi in `poses` and then the food over `frame`, which already holds the water and
/// is `frame_w` pixels wide, placed as `place_koi` and `place_food` place them.
#[allow(clippy::too_many_arguments)]
pub fn draw_pond(
    frame: &mut [u8],
    frame_w: usize,
    school: &School,
    poses: &[Pose],
    poser: &mut Poser,
    food: &[(usize, Vec<u8>)],
    scale: (f32, f32),
    pixel: usize,
) {
    place_koi(poses, poser, scale, pixel, |pixels, w, _, x, y| blend(frame, frame_w, pixels, w, x, y, pixel.max(1), 255));
    for (sprite, x, y) in place_food(school, food, scale, pixel) {
        let (side, pixels) = &food[sprite];
        blend(frame, frame_w, pixels, *side, x, y, 1, 255);
    }
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
pub fn stretch(src: &[u8], w: usize, h: usize, out_w: usize, out_h: usize, dst: &mut Vec<u8>) {
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
pub fn upscale(src: &[u8], w: usize, k: usize, out_w: usize, out_h: usize, dst: &mut Vec<u8>) {
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
    use koi_theme::{Catalog, ROOT};

    /// A frame steps whole `DT`s and leaves the rest for the next one; a long pause steps
    /// 60 times and then counts as caught up.
    #[test]
    fn advance_steps_whole_ticks_and_skips_a_long_pause() {
        let theme = Catalog::load(None).0.resolve(ROOT).expect("root theme");
        let mut school = School::new(64, 36, 3, 5);
        let mut water = Water::new(None, 64, 36, [1.0; 2], 0.25, &theme, 5);
        let mut before = Vec::new();
        let dt = f64::from(DT);
        assert_eq!(advance(&mut school, &mut water, &mut before, 2.5 * dt), 2.0 * dt);
        assert_eq!(before.len(), 3);
        assert_eq!(advance(&mut school, &mut water, &mut before, 0.5 * dt), 0.0);
        assert_eq!(advance(&mut school, &mut water, &mut before, 30.0), 30.0);
    }
}
