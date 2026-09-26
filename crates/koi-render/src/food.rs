//! Food sprites: one small straight-alpha RGBA image per food kind and fade level, painted on
//! the CPU. They are sent once per theme and placed by the binary, so both backends show the
//! same pixels.

use koi_sim::FoodKind;
use koi_theme::{Palette, mix};

/// Paints one sprite of `kind` into `buf` as straight-alpha RGBA rows and returns its side in
/// pixels. The side is odd, so the piece's position is the middle pixel.
///
/// `radius` is a pellet's radius in device pixels, and every kind is sized from it: flakes and
/// seeds are about half a pellet, a petal a little larger, the treat several times. `fade` is
/// `Food::fade`, 0 to 1. Every kind but the treat fades out with it; the treat stays opaque and
/// shrinks instead, one step per bite. The side does not depend on `fade`, so the binary can
/// centre every fade level the same way. Colours come from `palette`, so a theme switch repaints
/// food by calling this again. Sprites never rotate: every petal lies at the same angle.
pub fn food_sprite(kind: FoodKind, radius: f32, fade: f32, palette: &Palette, buf: &mut Vec<u8>) -> usize {
    let p = palette;
    let extent = match kind {
        FoodKind::Pellets => 1.0,
        FoodKind::Flakes => 0.85,
        FoodKind::Petals => 1.4,
        FoodKind::Seeds => 0.75,
        FoodKind::Treat => 2.4,
    };
    let half = (extent * radius).ceil() as usize + 1;
    let size = 2 * half + 1;
    buf.clear();
    for py in 0..size {
        for px in 0..size {
            let u = (px as f32 - half as f32) / radius;
            let v = (py as f32 - half as f32) / radius;
            // Each shape gives its signed distance in radii (negative inside) and its colour there.
            let (d, color) = match kind {
                FoodKind::Pellets => {
                    let shade = ((u + v) * 0.35 + 0.35).clamp(0.0, 1.0);
                    let body = mix(p.food, mix(p.food, p.deep, 0.4), shade);
                    let glint = (1.0 - (u + 0.4).hypot(v + 0.4) / 0.35).clamp(0.0, 1.0);
                    (u.hypot(v) - 1.0, mix(body, p.highlight, 0.6 * glint))
                }
                FoodKind::Flakes => {
                    let a = v.atan2(u);
                    let edge = 0.72 * (1.0 + 0.12 * (3.0 * a + 0.7).sin() + 0.07 * (5.0 * a + 2.1).sin());
                    let color = if u - 0.4 * v > 0.15 { mix(p.asagi_red, p.koi_white, 0.3) } else { mix(p.koi_white, p.food, 0.25) };
                    (u.hypot(v) - edge, color)
                }
                FoodKind::Petals => {
                    let (sin, cos) = (-0.5f32).sin_cos();
                    let (x, y) = (u * cos + v * sin, -u * sin + v * cos);
                    let (a, b) = (1.3, 0.72 * (1.0 - 0.35 * x / 1.3));
                    let base = (0.5 - 0.5 * x / a).clamp(0.0, 1.0);
                    ((x / a).hypot(y / b) * b - b, mix(p.lily_flower, p.koi_white, 0.55 * base))
                }
                FoodKind::Seeds => {
                    let (sin, cos) = 0.4f32.sin_cos();
                    let (x, y) = (u * cos + v * sin, -u * sin + v * cos);
                    let glint = (1.0 - (x + 0.25).hypot(y + 0.15) / 0.18).clamp(0.0, 1.0);
                    ((x / 0.7).hypot(y / 0.45) * 0.45 - 0.45, mix(mix(p.koi_sumi, p.food, 0.35), p.highlight, 0.5 * glint))
                }
                FoodKind::Treat => {
                    // A watermelon slice, flat side up, shrinking as it is eaten.
                    let s = 0.3 + 0.7 * fade;
                    let (u, v, r) = (u / s, v / s, 2.4);
                    let cy = -r / 2.0;
                    let rho = u.hypot(v - cy) / r;
                    let seed = [0.9f32, 1.6, 2.4].iter().any(|&t| (u - t.cos() * 0.55 * r).hypot(v - cy - t.sin() * 0.55 * r) < 0.07 * r);
                    let color = if rho > 0.92 {
                        mix(p.lily_light, p.lily_dark, 0.4)
                    } else if rho > 0.84 {
                        p.lily_light
                    } else if rho > 0.76 {
                        mix(p.koi_white, p.lily_light, 0.3)
                    } else if seed {
                        p.koi_sumi
                    } else {
                        mix(p.koi_red, p.ogon, 0.15 * (1.0 - rho))
                    };
                    (((rho - 1.0) * r).max(cy - v) * s, color)
                }
            };
            let cover = (0.5 - d * radius).clamp(0.0, 1.0);
            let alpha = if kind == FoodKind::Treat { cover } else { cover * fade };
            buf.extend_from_slice(&color);
            buf.push((alpha * 255.0).round() as u8);
        }
    }
    size
}

#[cfg(test)]
mod tests {
    use super::*;

    fn coverage(buf: &[u8]) -> u32 {
        buf.chunks(4).map(|px| u32::from(px[3])).sum()
    }

    /// Every kind paints something opaque at full life. Fading thins every kind but the treat,
    /// which shrinks instead, and the side never changes with the fade.
    #[test]
    fn sprites_fade_or_shrink() {
        let palette = koi_theme::Catalog::load(None).0.resolve(koi_theme::ROOT).expect("root theme").palette;
        let mut buf = Vec::new();
        for kind in FoodKind::ALL {
            let side = food_sprite(kind, 4.0, 1.0, &palette, &mut buf);
            assert_eq!(buf.len(), side * side * 4);
            assert!(buf.chunks(4).any(|px| px[3] == 255), "{kind:?} has no opaque pixel");
            let full = coverage(&buf);
            assert_eq!(food_sprite(kind, 4.0, 0.25, &palette, &mut buf), side);
            let faded = coverage(&buf);
            assert!(faded * 2 < full, "{kind:?} kept {faded} of {full} at a quarter life");
            let opaque = buf.chunks(4).any(|px| px[3] == 255);
            assert_eq!(opaque, kind == FoodKind::Treat, "{kind:?} opacity at a quarter life");
        }
    }
}
