//! Turns the simulation into RGBA images: the water surface and one sprite per koi. Each
//! renderer runs on a shared wgpu device when there is one and on the CPU otherwise, and
//! the two paths draw the same picture.

#![warn(missing_docs)]

mod food;
mod gpu;
pub mod hud;
mod koi;
mod water;

pub use food::food_sprite;
pub use gpu::Gpu;
pub use koi::Poser;
pub use water::Water;
