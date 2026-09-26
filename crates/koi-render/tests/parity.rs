//! The GPU and CPU backends shade every frame with separate code (water.wgsl and koi.wgsl
//! against water.rs and koi.rs), so a style feature added to one and not the other shows up
//! here as a frame that differs.

use koi_render::{Gpu, Poser, Water};
use koi_sim::{Pose, School, Splash};
use koi_theme::{Catalog, ROOT};

/// One fixed frame: a pond built in the root theme, stepped through three splashes with a
/// switch to `id` halfway, then the water image and one posed koi. A pixel theme moves the
/// water to a finer art grid on the switch, carrying the waves over. The first koi is half
/// dived, so its shadow and body show the dive.
fn frame(gpu: Option<&Gpu>, catalog: &Catalog, id: &str) -> (Vec<u8>, Vec<u8>) {
    let (w, h, seed) = (192, 108, 11);
    let theme = catalog.resolve(id).expect("built-in theme");
    let school = School::new(w, h, 5, seed);
    let mut water = Water::new(gpu, w, h, [1.0; 2], 0.25, &catalog.resolve(ROOT).expect("root theme"), seed);
    for tick in 0..90u8 {
        if tick == 45 && theme.style.pixel_px > 0 {
            water.regrid(240, 135, [1.25; 2], 0.25 * 1.25, &theme);
        } else if tick == 45 {
            water.set_theme(&theme);
        }
        if tick % 30 == 0 {
            water.splash(Splash { x: 60.0 + f32::from(tick), y: 50.0, radius: 6.0, amount: 1.5 });
        }
        water.step();
    }
    let mut shadows = school.shadows();
    shadows[0].depth = 0.5;
    let pond = water.render(&shadows).to_vec();
    let mut poser = Poser::new(gpu, &school, &theme, 2.0);
    let pose = Pose { depth: 0.5, ..school.fish[0].pose() };
    let (left, top, right, bottom) = poser.bounds(0, &pose);
    let (sw, sh) = (usize::try_from(right - left + 1).expect("width"), usize::try_from(bottom - top + 1).expect("height"));
    let koi = poser.pose(0, &pose, left as f32 + 0.3, top as f32 + 0.6, sw, sh).to_vec();
    (pond, koi)
}

#[test]
fn backends_draw_the_same_frame() {
    let (catalog, _) = Catalog::load(None);
    // Only a chosen Vulkan driver (lavapipe in CI) is used, and then it must be there.
    let gpu = std::env::var_os("VK_ICD_FILENAMES").map(|_| Gpu::new().expect("VK_ICD_FILENAMES is set, so an adapter must be found"));
    for id in ["summer-garden", "evening-garden", "moonlit-pond", "morning-mist", "rainy-afternoon", "hillside-summer", "lantern-dusk", "pocket-moss"] {
        let (cpu_pond, cpu_koi) = frame(None, &catalog, id);
        let Some(gpu) = &gpu else {
            eprintln!("VK_ICD_FILENAMES is not set; skipping the GPU half");
            continue;
        };
        let (gpu_pond, gpu_koi) = frame(Some(gpu), &catalog, id);
        for (what, cpu, gpu) in [("water", &cpu_pond, &gpu_pond), ("koi", &cpu_koi, &gpu_koi)] {
            assert_eq!(cpu.len(), gpu.len());
            let worst = cpu.iter().zip(gpu.iter()).enumerate().max_by_key(|(_, (a, b))| a.abs_diff(**b)).map(|(i, (a, b))| (i / 4, a.abs_diff(*b)));
            let (pixel, diff) = worst.expect("a non-empty image");
            assert!(diff <= 2, "{id} {what}: pixel {pixel} differs by {diff}/255 between the CPU and the GPU");
        }
    }
}
