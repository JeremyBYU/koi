//! Turns the simulation into RGBA images: the water surface and one sprite per koi. Each
//! renderer runs on a shared wgpu device when there is one and on the CPU otherwise, and
//! the two paths draw the same picture.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

mod food;
mod frame;
mod gpu;
pub mod hud;
mod koi;
mod water;

pub use food::{bubble_sprite, food_sprite};
pub use frame::{FADE_LEVELS, advance, blend, draw_pond, food_sprites, place_food, place_koi, shown_food, stretch, upscale};
pub use gpu::Gpu;
pub use koi::Poser;
pub use water::Water;

/// Runs `work` on its own thread in `scope`, or right away on wasm, which has no threads.
fn spawn<'scope>(scope: &'scope std::thread::Scope<'scope, '_>, work: impl FnOnce() + Send + 'scope) {
    if cfg!(target_arch = "wasm32") {
        work();
    } else {
        scope.spawn(work);
    }
}
