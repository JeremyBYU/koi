//! Themes: one TOML file per look, resolved through `extends` into a palette, light, style
//! and scene. `themes/summer-garden.toml` is the root and lists every key with its default.
//! docs/THEMES.md describes every key.
//!
//! The built-in themes are compiled in. Debug builds read them from the repository instead,
//! so edits show without a rebuild. A user file with a built-in's id replaces it.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

use serde::Deserialize;
use std::path::{Path, PathBuf};

/// An sRGB colour, one byte per channel.
pub type Rgb = [u8; 3];

/// The theme every chain ends at. A theme without `extends` extends it, and it is the one to
/// fall back to when a theme fails to load.
pub const ROOT: &str = "summer-garden";

/// The built-in themes, in the order `t` steps through their families.
const BUILT_IN: [(&str, &str); 25] = [
    ("summer-garden", include_str!("../../../themes/summer-garden.toml")),
    ("morning-mist", include_str!("../../../themes/morning-mist.toml")),
    ("evening-garden", include_str!("../../../themes/evening-garden.toml")),
    ("moonlit-pond", include_str!("../../../themes/moonlit-pond.toml")),
    ("cedar-shade", include_str!("../../../themes/cedar-shade.toml")),
    ("cedar-shade-dusk", include_str!("../../../themes/cedar-shade-dusk.toml")),
    ("cedar-shade-night", include_str!("../../../themes/cedar-shade-night.toml")),
    ("maple-afternoon", include_str!("../../../themes/maple-afternoon.toml")),
    ("maple-afternoon-dusk", include_str!("../../../themes/maple-afternoon-dusk.toml")),
    ("maple-afternoon-night", include_str!("../../../themes/maple-afternoon-night.toml")),
    ("petal-spring", include_str!("../../../themes/petal-spring.toml")),
    ("petal-spring-dusk", include_str!("../../../themes/petal-spring-dusk.toml")),
    ("petal-spring-night", include_str!("../../../themes/petal-spring-night.toml")),
    ("rainy-afternoon", include_str!("../../../themes/rainy-afternoon.toml")),
    ("rainy-afternoon-dusk", include_str!("../../../themes/rainy-afternoon-dusk.toml")),
    ("rainy-afternoon-night", include_str!("../../../themes/rainy-afternoon-night.toml")),
    ("ink-and-vermilion", include_str!("../../../themes/ink-and-vermilion.toml")),
    ("ink-and-vermilion-dusk", include_str!("../../../themes/ink-and-vermilion-dusk.toml")),
    ("ink-and-vermilion-night", include_str!("../../../themes/ink-and-vermilion-night.toml")),
    ("hillside-summer", include_str!("../../../themes/hillside-summer.toml")),
    ("lantern-dusk", include_str!("../../../themes/lantern-dusk.toml")),
    ("pocket-moss", include_str!("../../../themes/pocket-moss.toml")),
    ("pocket-moss-dusk", include_str!("../../../themes/pocket-moss-dusk.toml")),
    ("pocket-moss-night", include_str!("../../../themes/pocket-moss-night.toml")),
    ("pixel", include_str!("../../../themes/pixel.toml")),
];

/// Keys that belong to the file they are written in and are never inherited.
const FILE_KEYS: [&str; 7] = ["name", "description", "extends", "family", "time", "credit", "hidden"];

/// Palette slots with a rule for when they are unset. A parent's value was picked for the
/// parent's colours, so a child derives the slot again unless it sets it itself.
const DERIVED: [&str; 8] = ["outline", "cloud", "asagi_blue", "asagi_red", "food", "ui_text", "ui_dim", "ui_accent"];

/// The colour slots every drawing reads from. Every theme resolves all of them: the first
/// fourteen are required, the rest are derived from those when a theme leaves them out.
#[derive(Clone, Debug, PartialEq)]
pub struct Palette {
    /// Deepest water.
    pub deep: Rgb,
    /// Water at middle depth.
    pub mid: Rgb,
    /// Water over the shallow shelf. Also tints the floor seen through the water.
    pub shallow: Rgb,
    /// Sunlight: lit sides, caustics, ripple lines, glints.
    pub highlight: Rgb,
    /// Shade: cast shadows and cool sides. Its hue, at full brightness, tints unlit floor.
    pub shadow: Rgb,
    /// Lit rim stones, sand, pebbles.
    pub stone_light: Rgb,
    /// Shaded rim stones, pebbles.
    pub stone_dark: Rgb,
    /// Shaded pads, moss, bank, foliage.
    pub lily_dark: Rgb,
    /// Lit pads, moss, bank, foliage.
    pub lily_light: Rgb,
    /// Flower and blossom petals, maple leaves.
    pub lily_flower: Rgb,
    /// White koi skin. Also white petals and water lilies.
    pub koi_white: Rgb,
    /// The red (hi) of Kohaku, Sanke and Showa.
    pub koi_red: Rgb,
    /// The black (sumi) of Sanke and Showa, and the eyes.
    pub koi_sumi: Rgb,
    /// Metallic gold of the Ogon, and flower centres.
    pub ogon: Rgb,
    /// Koi outlines and pixel outlines. For `outline = "rim"`, the light edge.
    pub outline: Rgb,
    /// Cloud reflections on the water.
    pub cloud: Rgb,
    /// Blue-grey back of the Asagi.
    pub asagi_blue: Rgb,
    /// Orange flanks of the Asagi.
    pub asagi_red: Rgb,
    /// Food pellets, the ochre in the sand, plank wood.
    pub food: Rgb,
    /// Status line and toast text.
    pub ui_text: Rgb,
    /// Secondary status text.
    pub ui_dim: Rgb,
    /// Errors and emphasis on the status line.
    pub ui_accent: Rgb,
    /// The colours `style.palette_lock` snaps to. Empty means every slot colour above.
    pub swatches: Vec<Rgb>,
}

impl Palette {
    /// Every slot colour above, in order.
    pub fn slots(&self) -> [Rgb; 22] {
        [
            self.deep,
            self.mid,
            self.shallow,
            self.highlight,
            self.shadow,
            self.stone_light,
            self.stone_dark,
            self.lily_dark,
            self.lily_light,
            self.lily_flower,
            self.koi_white,
            self.koi_red,
            self.koi_sumi,
            self.ogon,
            self.outline,
            self.cloud,
            self.asagi_blue,
            self.asagi_red,
            self.food,
            self.ui_text,
            self.ui_dim,
            self.ui_accent,
        ]
    }
}

/// `[light]`: where the light comes from and how strong it is.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
pub struct Light {
    /// Towards the sun in image space (y down), as a unit vector. Resolution normalises the
    /// file's value; a zero vector is an error.
    pub sun: [f32; 2],
    /// How far lit sides mix toward `highlight`.
    pub warm: f32,
    /// How far shaded sides mix toward `shadow`.
    pub cool: f32,
    /// Multiplies the whole water image at the end of shading.
    pub ambient: f32,
    /// Multiplies every cast shadow offset. Above 1 is a low sun.
    pub shadow_len: f32,
    /// 0 is direct sun, 1 overcast: cast shadows and caustics fade out.
    pub diffuse: f32,
}

/// Koi outline modes, applied in the pose pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Outline {
    /// No outline.
    None,
    /// An anti-aliased edge between water, body and `outline`.
    Soft,
    /// A solid `outline` edge.
    Dark,
    /// Pixel-art selective outline: body-tinted on the sun side, `outline` in shade.
    Selout,
    /// A light `outline` rim on the sun side, `shadow` on the far side.
    Rim,
}

/// Ordered dithering on water band edges.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Dither {
    /// No dithering.
    None,
    /// 2x2 Bayer matrix.
    Bayer2,
    /// 4x4 Bayer matrix.
    Bayer4,
}

/// Which images snap to `palette.swatches`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PaletteLock {
    /// Nothing snaps.
    Off,
    /// The water image snaps; the koi keep their colours.
    Scene,
    /// The water and the koi snap.
    All,
}

/// How the brightest wave crests are drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HighlightStyle {
    /// A glint with a soft halo (`bloom`).
    Soft,
    /// Short one-art-pixel horizontal strokes, for pixel themes.
    Dashes,
}

/// `[style]`: how shapes and light are drawn.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
pub struct Style {
    /// Device pixels per art pixel, 4 to 8. 0 means painted, at `render.water_px`.
    pub pixel_px: u32,
    /// Posterize steps for floor light, pads and stones. At least 1.
    pub tone_steps: u16,
    /// Flat water depth bands. At least 1.
    pub depth_bands: u16,
    /// Width of the blend between bands and tones, as a fraction of one band. 0 is a hard edge.
    pub band_softness: f32,
    /// Cel steps on the koi. At least 1.
    pub koi_tones: u16,
    /// Koi outline mode.
    pub outline: Outline,
    /// Static paper grain as a fraction of brightness. 0 also keeps band edges straight.
    pub grain: f32,
    /// Dithering on water band edges.
    pub dither: Dither,
    /// Dither amplitude as a fraction of one band.
    pub dither_strength: f32,
    /// Which images snap to the swatches.
    pub palette_lock: PaletteLock,
    /// Caustic net strength. 1 is the original look.
    pub caustics: f32,
    /// Caustic cell size multiplier. Larger is coarser and calmer. Above 0.
    pub caustic_scale: f32,
    /// Width of the caustic lines. Above 0.
    pub caustic_softness: f32,
    /// Glint strength.
    pub glint: f32,
    /// Surface tilt toward the sun where glints start, from 0 to below 1.
    pub glint_threshold: f32,
    /// How the brightest crests are drawn.
    pub highlight_style: HighlightStyle,
    /// Halo strength around bright crests.
    pub bloom: f32,
    /// How strongly clouds reflect in the water.
    pub cloud_reflections: f32,
    /// Strength of the dappled shadow under foliage.
    pub leaf_shadows: f32,
    /// How far the water image mixes toward a vertical gradient. 0 is off.
    pub wash: f32,
    /// The wash colour at the top edge.
    #[serde(deserialize_with = "hex")]
    pub wash_top: Rgb,
    /// The wash colour at the bottom edge.
    #[serde(deserialize_with = "hex")]
    pub wash_bottom: Rgb,
    /// Clouds, dapple, caustics and petals advance in steps of 1 / `anim_hz` seconds. Above 0.
    pub anim_hz: f32,
}

/// The pond floor under the water.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Floor {
    /// Sand with ochre and sage patches.
    Sand,
    /// Denser, greyer pebbles.
    Pebbles,
    /// A green floor.
    Moss,
    /// Streaked grey slate.
    Slate,
}

/// The pond's edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Rim {
    /// Rim stones of varied size along the shore.
    Stones,
    /// A boardwalk ring.
    Planks,
}

/// The ground beyond the rim.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Bank {
    /// Moss green.
    Moss,
    /// Grass.
    Grass,
    /// Gravel.
    Gravel,
}

/// The flower on lily pads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Flower {
    /// Cupped lotus.
    Lotus,
    /// Flatter, star-shaped water lily.
    WaterLily,
}

/// A corner of the pond.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Corner {
    /// Top left.
    TopLeft,
    /// Top right.
    TopRight,
    /// Bottom left.
    BottomLeft,
    /// Bottom right.
    BottomRight,
}

/// The overhanging foliage.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FoliageKind {
    /// Broad leaves.
    Broadleaf,
    /// Maple leaves.
    Maple,
    /// Cherry, with blossom dots.
    Cherry,
    /// Pine needles.
    Pine,
    /// Bamboo.
    Bamboo,
}

/// Something drifting on the surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PetalKind {
    /// A `lily_flower` petal.
    Blossom,
    /// A `koi_white` petal.
    White,
    /// A small green leaf.
    Leaf,
    /// A maple leaf.
    Maple,
}

/// Weather over the pond.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Weather {
    /// None.
    Clear,
    /// Raindrop rings.
    Rain,
    /// Mist toward the edges.
    Mist,
    /// A few warm pulsing points.
    Fireflies,
}

/// `[scene]`: what is in the pond. Mostly painted once per theme; petals and weather are
/// drawn every frame. The layout seed stays the same across themes.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Scene {
    /// The floor.
    pub floor: Floor,
    /// Pebble density multiplier. 0 is none.
    pub pebbles: f32,
    /// The pond's edge.
    pub rim: Rim,
    /// Moss on rim stones.
    pub moss: f32,
    /// The ground beyond the rim.
    pub bank: Bank,
    /// Lily pad size multiplier.
    pub pad_size: f32,
    /// Share of pads with a flower. Above 0 means at least one flower.
    pub flowers: f32,
    /// The flower kind.
    pub flower: Flower,
    /// Corners with foliage; the first is the large one.
    pub foliage: Vec<Corner>,
    /// Multiplies foliage leaf counts and reach.
    pub foliage_density: f32,
    /// The foliage kind.
    pub foliage_kind: FoliageKind,
    /// Drifting petals and leaves, at most 24.
    pub petals: u32,
    /// Kinds of drifting things. Repeats weight the pick.
    pub petal_kinds: Vec<PetalKind>,
    /// Weather.
    pub weather: Weather,
    /// Weather strength.
    pub weather_amount: f32,
}

/// Times of day, in order. A family's themes are the same place at different times.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Time {
    /// Dawn.
    Dawn,
    /// Morning.
    Morning,
    /// Noon, the default.
    Noon,
    /// Afternoon.
    Afternoon,
    /// Evening.
    Evening,
    /// Dusk.
    Dusk,
    /// Night.
    Night,
}

impl Time {
    const ALL: [Time; 7] = [Time::Dawn, Time::Morning, Time::Noon, Time::Afternoon, Time::Evening, Time::Dusk, Time::Night];

    /// The name used in theme files and the toast.
    pub fn name(self) -> &'static str {
        match self {
            Time::Dawn => "dawn",
            Time::Morning => "morning",
            Time::Noon => "noon",
            Time::Afternoon => "afternoon",
            Time::Evening => "evening",
            Time::Dusk => "dusk",
            Time::Night => "night",
        }
    }
}

/// A theme's own keys, which are never inherited, and its id.
#[derive(Clone, Debug, PartialEq)]
pub struct Summary {
    /// The file name without `.toml`.
    pub id: String,
    /// Shown in the toast. Defaults to the id.
    pub name: String,
    /// One line for `--list-themes`.
    pub description: String,
    /// Themes in one family are the same place at different times. Defaults to the id.
    pub family: String,
    /// Orders the family. Defaults to noon.
    pub time: Time,
    /// Palette author and URL, when the palette is borrowed.
    pub credit: String,
    /// A base for other themes, left out of the `t` and `l` cycles.
    pub hidden: bool,
}

/// A fully resolved theme.
#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    /// The theme's own keys.
    pub summary: Summary,
    /// Every colour slot, derived slots filled.
    pub palette: Palette,
    /// `[light]`.
    pub light: Light,
    /// `[style]`.
    pub style: Style,
    /// `[scene]`.
    pub scene: Scene,
    /// Problems that did not stop the theme loading, such as unknown keys.
    pub warnings: Vec<String>,
    /// The files on disk the theme was resolved from, leaf first, for hot reload. Themes
    /// compiled into the binary have none.
    pub files: Vec<PathBuf>,
}

/// Every theme the game can switch to: the built-ins, with the user's files over them.
pub struct Catalog {
    entries: Vec<Entry>,
    /// The built-in root's text. It lists every key, so it is the schema and the base every
    /// resolution starts from, even when a user file replaces `summer-garden`.
    root: String,
}

struct Entry {
    id: String,
    text: String,
    /// Where the text came from, for messages.
    label: String,
    path: Option<PathBuf>,
}

impl Catalog {
    /// The built-in themes, then every `*.toml` in `user_dir`. A user file replaces the
    /// built-in with the same id. Unreadable user files come back as warnings.
    pub fn load(user_dir: Option<&Path>) -> (Catalog, Vec<String>) {
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../themes");
        let mut catalog = Catalog { entries: Vec::new(), root: String::new() };
        for (id, text) in BUILT_IN {
            let file = repo.join(format!("{id}.toml"));
            let (text, path) = match std::fs::read_to_string(&file) {
                Ok(fresh) if cfg!(debug_assertions) => (fresh, Some(file)),
                _ => (text.to_string(), None),
            };
            if id == ROOT {
                catalog.root.clone_from(&text);
            }
            catalog.insert(Entry { id: id.to_string(), text, label: format!("themes/{id}.toml"), path });
        }

        let mut warnings = Vec::new();
        let mut files: Vec<PathBuf> = match user_dir.map(std::fs::read_dir) {
            Some(Ok(dir)) => dir.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|x| x == "toml")).collect(),
            _ => Vec::new(),
        };
        files.sort();
        for path in files {
            let Some(id) = path.file_stem().and_then(|s| s.to_str()) else { continue };
            let label = match std::env::var_os("HOME").and_then(|home| path.strip_prefix(home).ok()) {
                Some(rest) => format!("~/{}", rest.display()),
                None => path.display().to_string(),
            };
            match std::fs::read_to_string(&path) {
                Ok(text) => catalog.insert(Entry { id: id.to_string(), text, label, path: Some(path.clone()) }),
                Err(e) => warnings.push(format!("{label}: {e}")),
            }
        }
        (catalog, warnings)
    }

    fn insert(&mut self, entry: Entry) {
        match self.entries.iter_mut().find(|e| e.id == entry.id) {
            Some(slot) => *slot = entry,
            None => self.entries.push(entry),
        }
    }

    fn find(&self, id: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.id == id)
    }

    /// Resolves the theme `name`: an id, or a display name ("Evening Garden"). Files merge key by key from the root down the `extends` chain; lists
    /// replace whole. The error names the file and key, for the toast.
    pub fn resolve(&self, name: &str) -> Result<Theme, String> {
        let id = name.trim().to_lowercase().replace(' ', "-");
        let mut warnings = Vec::new();
        let leaf = self.find(&id).ok_or_else(|| format!("no theme \"{id}\" (koi --list-themes lists them)"))?;

        let mut chain: Vec<(&Entry, toml::Table)> = Vec::new();
        let mut next = Some(leaf);
        while let Some(entry) = next {
            let table: toml::Table = entry.text.parse().map_err(|e| format!("{}: {e}", entry.label))?;
            let parent = match table.get("extends") {
                None if entry.id == ROOT => None,
                None => Some(ROOT.to_string()),
                Some(toml::Value::String(parent)) => Some(parent.clone()),
                Some(other) => return Err(format!("{}: `extends` expects a string, got {}", entry.label, other.type_str())),
            };
            chain.push((entry, table));
            next = match parent {
                None => None,
                Some(parent) if chain.iter().any(|(e, _)| e.id == parent) => {
                    let ids: Vec<&str> = chain.iter().map(|(e, _)| e.id.as_str()).collect();
                    return Err(format!("{}: {} extends {parent}, a cycle", entry.label, ids.join(" extends ")));
                }
                Some(parent) => Some(self.find(&parent).ok_or_else(|| format!("{}: extends \"{parent}\", which does not exist", entry.label))?),
            };
        }
        let summary = summary(&leaf.id, &leaf.label, &chain[0].1)?;

        let mut schema: toml::Table = self.root.parse().map_err(|e| format!("themes/{ROOT}.toml: {e}"))?;
        if let Some(toml::Value::Table(palette)) = schema.get_mut("palette") {
            for key in ["outline", "cloud"] {
                palette.insert(key.to_string(), toml::Value::String(String::new()));
            }
        }
        let mut merged = schema.clone();
        let files = chain.iter().filter_map(|(e, _)| e.path.clone()).collect();
        for (entry, mut table) in chain.into_iter().rev() {
            for key in FILE_KEYS {
                table.remove(key);
            }
            if let Some(toml::Value::Table(palette)) = merged.get_mut("palette") {
                for key in DERIVED {
                    palette.remove(key);
                }
            }
            merge(&mut merged, table, &schema, "", &entry.label, &mut warnings)?;
        }

        let mut section = |key: &str| merged.remove(key).unwrap_or(toml::Value::Table(toml::Table::new()));
        let palette = match section("palette") {
            toml::Value::Table(table) => palette(&table)?,
            other => return Err(format!("{}: `palette` expects a table, got {}", leaf.label, other.type_str())),
        };
        let mut light: Light = section("light").try_into().map_err(|e| format!("{}: [light] {}", leaf.label, e.to_string().trim().replace('\n', " ")))?;
        let style: Style = section("style").try_into().map_err(|e| format!("{}: [style] {}", leaf.label, e.to_string().trim().replace('\n', " ")))?;
        let scene: Scene = section("scene").try_into().map_err(|e| format!("{}: [scene] {}", leaf.label, e.to_string().trim().replace('\n', " ")))?;

        let len = light.sun[0].hypot(light.sun[1]);
        if !len.is_finite() || len <= 1e-3 {
            return Err(format!("{}: light.sun must point somewhere, got {:?}", leaf.label, light.sun));
        }
        light.sun = light.sun.map(|v| v / len);
        if style.tone_steps == 0 || style.depth_bands == 0 || style.koi_tones == 0 {
            return Err(format!("{}: style.tone_steps, depth_bands and koi_tones must be at least 1", leaf.label));
        }
        if style.pixel_px != 0 && !(4..=8).contains(&style.pixel_px) {
            return Err(format!("{}: style.pixel_px = {} must be 0 or 4 to 8", leaf.label, style.pixel_px));
        }
        if !(style.anim_hz > 0.0 && style.caustic_scale > 0.0 && style.caustic_softness > 0.0) {
            return Err(format!("{}: style.anim_hz, caustic_scale and caustic_softness must be above 0", leaf.label));
        }
        if !(0.0..1.0).contains(&style.glint_threshold) {
            return Err(format!("{}: style.glint_threshold = {} must be from 0 to below 1", leaf.label, style.glint_threshold));
        }
        let ranges = [
            ("light.warm", light.warm, 0.0, 1.0),
            ("light.cool", light.cool, 0.0, 1.0),
            ("light.ambient", light.ambient, 0.0, 2.0),
            ("light.shadow_len", light.shadow_len, 0.0, 4.0),
            ("light.diffuse", light.diffuse, 0.0, 1.0),
            ("style.band_softness", style.band_softness, 0.0, 1.0),
            ("style.grain", style.grain, 0.0, 1.0),
            ("style.dither_strength", style.dither_strength, 0.0, 1.0),
            ("style.caustics", style.caustics, 0.0, 2.0),
            ("style.caustic_scale", style.caustic_scale, 0.0, 4.0),
            ("style.caustic_softness", style.caustic_softness, 0.0, 1.0),
            ("style.glint", style.glint, 0.0, 2.0),
            ("style.bloom", style.bloom, 0.0, 1.0),
            ("style.cloud_reflections", style.cloud_reflections, 0.0, 1.0),
            ("style.leaf_shadows", style.leaf_shadows, 0.0, 1.0),
            ("style.wash", style.wash, 0.0, 1.0),
            ("style.anim_hz", style.anim_hz, 0.0, 60.0),
            ("scene.pebbles", scene.pebbles, 0.0, 2.0),
            ("scene.moss", scene.moss, 0.0, 4.0),
            ("scene.pad_size", scene.pad_size, 0.0, 2.0),
            ("scene.flowers", scene.flowers, 0.0, 1.0),
            ("scene.foliage_density", scene.foliage_density, 0.0, 4.0),
            ("scene.weather_amount", scene.weather_amount, 0.0, 1.0),
        ];
        if let Some((key, value, lo, hi)) = ranges.into_iter().find(|&(_, value, lo, hi)| !(lo..=hi).contains(&value)) {
            return Err(format!("{}: {key} = {value} must be from {lo} to {hi}", leaf.label));
        }
        if scene.petals > 24 {
            return Err(format!("{}: scene.petals = {} is more than 24", leaf.label, scene.petals));
        }
        if scene.petals > 0 && scene.petal_kinds.is_empty() {
            return Err(format!("{}: scene.petals = {} needs at least one of scene.petal_kinds", leaf.label, scene.petals));
        }
        Ok(Theme { summary, palette, light, style, scene, warnings, files })
    }

    /// Every theme's own keys, in catalog order: built-ins first, then user themes by file
    /// name. Hidden themes are included. Files that do not parse are left out; resolving
    /// them reports why.
    pub fn summaries(&self) -> Vec<Summary> {
        self.entries.iter().filter_map(|e| summary(&e.id, &e.label, &e.text.parse().ok()?).ok()).collect()
    }

    /// The theme `t` (or `T`, with `forward` false) switches to from `current`: the next
    /// family in catalog order, at the current time if it has it, else noon, else its
    /// earliest time. Themes that fail to resolve are skipped, and so is a family where none
    /// resolve. None when there is no other family.
    pub fn next_scene(&self, current: &Summary, forward: bool) -> Option<String> {
        let visible: Vec<Summary> = self.summaries().into_iter().filter(|s| !s.hidden).collect();
        let mut families: Vec<&str> = Vec::new();
        for s in &visible {
            if !families.contains(&s.family.as_str()) {
                families.push(&s.family);
            }
        }
        let n = families.len();
        let at = families.iter().position(|f| *f == current.family);
        for step in 1..=n {
            let to = match at {
                Some(_) if step == n => return None,
                Some(at) if forward => (at + step) % n,
                Some(at) => (at + n - step) % n,
                None => step - 1,
            };
            let members: Vec<&Summary> = visible.iter().filter(|s| s.family == families[to] && self.resolve(&s.id).is_ok()).collect();
            let pick = members
                .iter()
                .find(|s| s.time == current.time)
                .or_else(|| members.iter().find(|s| s.time == Time::Noon))
                .or_else(|| members.iter().min_by_key(|s| s.time));
            if let Some(pick) = pick {
                return Some(pick.id.clone());
            }
        }
        None
    }

    /// The theme `l` (or `L`, with `later` false) switches to from `current`: the next time
    /// its family has, wrapping round, skipping themes that fail to resolve. None when the
    /// family has one theme.
    pub fn next_time(&self, current: &Summary, later: bool) -> Option<String> {
        let mut members: Vec<Summary> =
            self.summaries().into_iter().filter(|s| !s.hidden && s.family == current.family && (s.id == current.id || self.resolve(&s.id).is_ok())).collect();
        members.sort_by_key(|s| s.time);
        let n = members.len();
        let at = members.iter().position(|s| s.id == current.id)?;
        if n < 2 {
            return None;
        }
        Some(members[if later { (at + 1) % n } else { (at + n - 1) % n }].id.clone())
    }
}

fn summary(id: &str, label: &str, table: &toml::Table) -> Result<Summary, String> {
    let text = |key: &str| match table.get(key) {
        None => Ok(None),
        Some(toml::Value::String(s)) => Ok(Some(s.clone())),
        Some(other) => Err(format!("{label}: `{key}` expects a string, got {}", other.type_str())),
    };
    let time = match text("time")? {
        None => Time::Noon,
        Some(name) => Time::ALL
            .into_iter()
            .find(|t| t.name() == name)
            .ok_or_else(|| format!("{label}: time = \"{name}\" is not one of dawn, morning, noon, afternoon, evening, dusk, night"))?,
    };
    let hidden = match table.get("hidden") {
        None => false,
        Some(toml::Value::Boolean(hidden)) => *hidden,
        Some(other) => return Err(format!("{label}: `hidden` expects a boolean, got {}", other.type_str())),
    };
    Ok(Summary {
        id: id.to_string(),
        name: text("name")?.unwrap_or_else(|| id.to_string()).chars().filter(|c| !c.is_control()).collect(),
        description: text("description")?.unwrap_or_default().chars().filter(|c| !c.is_control()).collect(),
        family: text("family")?.unwrap_or_else(|| id.to_string()),
        time,
        credit: text("credit")?.unwrap_or_default(),
        hidden,
    })
}

/// Merges `from` over `into`: tables key by key, everything else (lists too) replaced whole.
/// A key missing from `schema` is a warning; a value of the wrong type, or a colour that is
/// not `#RRGGBB`, is an error naming `label`.
fn merge(into: &mut toml::Table, from: toml::Table, schema: &toml::Table, path: &str, label: &str, warnings: &mut Vec<String>) -> Result<(), String> {
    for (key, value) in from {
        let full = if path.is_empty() { key.clone() } else { format!("{path}.{key}") };
        let Some(expected) = schema.get(&key) else {
            warnings.push(format!("{label}: unknown key `{full}` ignored"));
            continue;
        };
        match (expected, value) {
            (toml::Value::Table(expected), toml::Value::Table(value)) => {
                let mut inner = match into.remove(&key) {
                    Some(toml::Value::Table(inner)) => inner,
                    _ => toml::Table::new(),
                };
                merge(&mut inner, value, expected, &full, label, warnings)?;
                into.insert(key, toml::Value::Table(inner));
            }
            (expected, value) if expected.type_str() == value.type_str() || (expected.is_float() && value.is_integer()) => {
                let colors: Vec<&toml::Value> = match &value {
                    toml::Value::Array(items) if full == "palette.swatches" => items.iter().collect(),
                    toml::Value::String(_) if path == "palette" || full == "style.wash_top" || full == "style.wash_bottom" => vec![&value],
                    _ => Vec::new(),
                };
                if let Some(bad) = colors.iter().find(|c| c.as_str().and_then(parse_hex).is_none()) {
                    return Err(format!("{label}: {full} = {bad} is not a #RRGGBB color"));
                }
                into.insert(key, value);
            }
            (expected, value) => return Err(format!("{label}: `{full}` expects {}, got {}", expected.type_str(), value.type_str())),
        }
    }
    Ok(())
}

fn parse_hex(text: &str) -> Option<Rgb> {
    let digits = text.strip_prefix('#')?;
    if digits.len() != 6 || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let [_, r, g, b] = u32::from_str_radix(digits, 16).ok()?.to_be_bytes();
    Some([r, g, b])
}

fn hex<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Rgb, D::Error> {
    let text = String::deserialize(d)?;
    parse_hex(&text).ok_or_else(|| serde::de::Error::custom(format!("\"{text}\" is not a #RRGGBB color")))
}

/// `c` in linear light, 0 to 1 per channel, with the exact sRGB curve.
pub fn linear(c: Rgb) -> [f32; 3] {
    c.map(|v| {
        let s = f32::from(v) / 255.0;
        if s <= 0.04045 { s / 12.92 } else { ((s + 0.055) / 1.055).powf(2.4) }
    })
}

/// Linear light back to sRGB bytes, clamped to 0..1 first. The inverse of `linear`.
pub fn srgb(c: [f32; 3]) -> Rgb {
    c.map(|v| {
        let v = v.clamp(0.0, 1.0);
        let s = if v <= 0.003_130_8 { v * 12.92 } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 };
        (s * 255.0).round() as u8
    })
}

/// `a` mixed toward `b` by `t`, in linear light.
pub fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let (a, b) = (linear(a), linear(b));
    srgb(std::array::from_fn(|k| a[k] + (b[k] - a[k]) * t))
}

/// The palette from a merged `[palette]` table, with the derived slots filled from the
/// final values of the slots they come from.
fn palette(table: &toml::Table) -> Result<Palette, String> {
    let slot = |key: &str| match table.get(key) {
        None => Ok(None),
        Some(value) => value.as_str().and_then(parse_hex).map(Some).ok_or_else(|| format!("palette.{key} = {value} is not a #RRGGBB color")),
    };
    let required = |key: &str| slot(key)?.ok_or_else(|| format!("palette.{key} is missing"));
    let (koi_white, koi_red, koi_sumi, ogon) = (required("koi_white")?, required("koi_red")?, required("koi_sumi")?, required("ogon")?);
    let (shallow, highlight, shadow) = (required("shallow")?, required("highlight")?, required("shadow")?);
    let (stone_light, stone_dark, lily_dark, lily_light) = (required("stone_light")?, required("stone_dark")?, required("lily_dark")?, required("lily_light")?);
    let swatches = match table.get("swatches") {
        Some(toml::Value::Array(items)) => items
            .iter()
            .map(|c| c.as_str().and_then(parse_hex).ok_or_else(|| format!("palette.swatches: {c} is not a #RRGGBB color")))
            .collect::<Result<_, _>>()?,
        _ => Vec::new(),
    };
    Ok(Palette {
        deep: required("deep")?,
        mid: required("mid")?,
        shallow,
        highlight,
        shadow,
        stone_light,
        stone_dark,
        lily_dark,
        lily_light,
        lily_flower: required("lily_flower")?,
        koi_white,
        koi_red,
        koi_sumi,
        ogon,
        outline: slot("outline")?.unwrap_or_else(|| mix(koi_sumi, shadow, 0.35)),
        cloud: slot("cloud")?.unwrap_or_else(|| mix(koi_white, highlight, 0.4)),
        asagi_blue: slot("asagi_blue")?.unwrap_or_else(|| mix(koi_white, shadow, 0.5)),
        asagi_red: slot("asagi_red")?.unwrap_or_else(|| mix(koi_red, ogon, 0.3)),
        food: slot("food")?.unwrap_or_else(|| {
            let (a, b) = (linear(stone_light), linear(ogon));
            srgb(std::array::from_fn(|k| (a[k] + b[k]) * 0.5 * 0.75))
        }),
        ui_text: slot("ui_text")?.unwrap_or(koi_white),
        ui_dim: slot("ui_dim")?.unwrap_or_else(|| mix(shallow, highlight, 0.5)),
        ui_accent: slot("ui_accent")?.unwrap_or(ogon),
        swatches,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with(files: &[(&str, &str)]) -> Catalog {
        let (mut catalog, _) = Catalog::load(None);
        for (id, text) in files {
            catalog.insert(Entry { id: id.to_string(), text: text.to_string(), label: format!("{id}.toml"), path: None });
        }
        catalog
    }

    #[test]
    fn every_built_in_resolves_cleanly() {
        let catalog = with(&[]);
        for (id, _) in BUILT_IN {
            let theme = catalog.resolve(id).unwrap_or_else(|e| panic!("{id}: {e}"));
            assert!(theme.warnings.is_empty(), "{id}: {:?}", theme.warnings);
        }
        let hillside = catalog.resolve("hillside-summer").expect("resolves");
        assert_eq!((hillside.style.pixel_px, hillside.style.anim_hz, hillside.style.caustics), (4, 4.0, 0.3), "pixel.toml sits between the root and hillside");
        assert_eq!(hillside.scene.foliage, vec![Corner::TopLeft], "lists replace whole");
    }

    /// File keys and derived slots belong to the file that sets them; everything else merges.
    #[test]
    fn inheritance_rules() {
        let catalog = with(&[
            ("dim", "name = \"Dim\"\nfamily = \"dim\"\ntime = \"evening\"\n[palette]\nshadow = \"#000000\"\nasagi_blue = \"#123456\"\n[light]\nwarm = 0.5"),
            ("dimmer", "extends = \"dim\"\n[palette]\nkoi_white = \"#FFFFFF\"\n[style]\ncaustics = 1"),
        ]);
        let dim = catalog.resolve("dim").expect("dim");
        let dimmer = catalog.resolve("dimmer").expect("dimmer");
        let root = catalog.resolve(ROOT).expect("root");
        assert_eq!(dim.palette.asagi_blue, [0x12, 0x34, 0x56]);
        assert_eq!(dimmer.palette.asagi_blue, mix([255; 3], [0; 3], 0.5), "derived slots are derived again in a child");
        assert_eq!(dimmer.palette.ui_text, [255; 3], "ui_text follows the child's own koi_white");
        assert_eq!(dimmer.palette.shadow, [0; 3]);
        assert_eq!(dimmer.light.warm, 0.5);
        assert_eq!(dimmer.style.caustics, 1.0, "an integer is fine for a float key");
        assert_eq!(dimmer.palette.deep, root.palette.deep);
        assert_eq!(
            (dimmer.summary.name.as_str(), dimmer.summary.family.as_str(), dimmer.summary.time),
            ("dimmer", "dimmer", Time::Noon),
            "name, family and time are not inherited"
        );
        assert_eq!(root.palette.asagi_blue, [0x6E, 0x93, 0xB5], "the root keeps the derived slots it sets");
    }

    #[test]
    fn broken_themes_are_errors_and_unknown_keys_warnings() {
        let catalog = with(&[
            ("bad-color", "[palette]\ndeep = \"#12345\""),
            ("bad-type", "[light]\nwarm = \"lots\""),
            ("bad-enum", "[style]\noutline = \"thick\""),
            ("loop-a", "extends = \"loop-b\""),
            ("loop-b", "extends = \"loop-a\""),
            ("orphan", "extends = \"nowhere\""),
            ("bare-petals", "[scene]\npetals = 3\npetal_kinds = []"),
            ("endless-leaves", "[scene]\nfoliage_density = inf"),
            ("jungle", "[scene]\nfoliage_density = 1e20"),
            ("odd", "[palette]\nsparkle = \"#FFFFFF\"\n[style]\ncaustics = 0.2"),
        ]);
        let err = |id: &str| catalog.resolve(id).expect_err(id);
        assert_eq!(err("bad-color"), "bad-color.toml: palette.deep = \"#12345\" is not a #RRGGBB color");
        assert!(err("bad-type").contains("light.warm` expects float, got string"));
        assert!(err("bad-enum").contains("thick"), "{}", err("bad-enum"));
        assert!(err("loop-a").contains("a cycle"));
        assert!(err("orphan").contains("\"nowhere\", which does not exist"));
        assert!(err("bare-petals").contains("needs at least one of scene.petal_kinds"));
        assert_eq!(err("endless-leaves"), "endless-leaves.toml: scene.foliage_density = inf must be from 0 to 4");
        assert!(err("jungle").contains("scene.foliage_density = 100000000000000000000 must be from 0 to 4"), "{}", err("jungle"));
        let odd = catalog.resolve("odd").expect("unknown keys only warn");
        assert_eq!(odd.warnings, vec!["odd.toml: unknown key `palette.sparkle` ignored"]);
        assert_eq!(odd.style.caustics, 0.2);
    }

    #[test]
    fn names_and_user_overrides() {
        let catalog = with(&[("evening-garden", "name = \"My Evening\"\n[palette]\nkoi_red = \"#EE6343\"")]);
        assert_eq!(catalog.resolve("Moonlit Pond").expect("display name").summary.id, "moonlit-pond");
        let mine = catalog.resolve("evening-garden").expect("user theme");
        assert_eq!((mine.summary.name.as_str(), mine.palette.koi_red), ("My Evening", [0xEE, 0x63, 0x43]));
        assert_eq!(mine.summary.family, "evening-garden", "the user file replaces the built-in whole");

        // A user root is still merged over the built-in root, which every theme extends.
        let built_in = with(&[]).resolve(ROOT).expect("built-in root");
        let catalog = with(&[(ROOT, "[palette]\nkoi_red = \"#EE6343\"\n[style]\ncaustics = 0.9")]);
        let root = catalog.resolve(ROOT).expect("user root");
        assert_eq!((root.palette.koi_red, root.palette.deep), ([0xEE, 0x63, 0x43], built_in.palette.deep));
        assert_ne!(root.palette.asagi_blue, built_in.palette.asagi_blue, "derived again, not the built-in's own value");
        assert_eq!(catalog.resolve("evening-garden").expect("child of the user root").style.caustics, 0.9);
    }

    #[test]
    fn stepping_scenes_and_times() {
        let catalog = with(&[]);
        let at = |id: &str| catalog.resolve(id).expect(id).summary;
        assert_eq!(catalog.next_time(&at("summer-garden"), true).as_deref(), Some("evening-garden"));
        assert_eq!(catalog.next_time(&at("moonlit-pond"), true).as_deref(), Some("morning-mist"), "later wraps from night to dawn");
        assert_eq!(catalog.next_time(&at("morning-mist"), false).as_deref(), Some("moonlit-pond"));
        assert_eq!(catalog.next_time(&at("cedar-shade"), true).as_deref(), Some("cedar-shade-dusk"));
        assert_eq!(catalog.next_time(&at("cedar-shade-night"), true).as_deref(), Some("cedar-shade"), "later wraps from night to noon");
        let lone = with(&[("lone-pond", "name = \"Lone Pond\"")]);
        assert_eq!(lone.next_time(&lone.resolve("lone-pond").expect("lone-pond").summary, true), None, "a family of one");
        assert_eq!(catalog.next_scene(&at("evening-garden"), true).as_deref(), Some("cedar-shade"), "no evening there, so noon");
        assert_eq!(catalog.next_scene(&at("cedar-shade"), false).as_deref(), Some("summer-garden"), "back to the garden at noon");
        assert_eq!(catalog.next_scene(&at("maple-afternoon"), true).as_deref(), Some("petal-spring"));
        assert_eq!(catalog.next_scene(&at("rainy-afternoon"), true).as_deref(), Some("ink-and-vermilion"));
        assert_eq!(catalog.next_scene(&at("ink-and-vermilion"), true).as_deref(), Some("hillside-summer"));
        assert_eq!(
            catalog.next_scene(&at("lantern-dusk"), true).as_deref(),
            Some("pocket-moss-dusk"),
            "pixel-garden to the next family at dusk, skipping hidden pixel"
        );
        assert_eq!(catalog.next_scene(&at("pocket-moss"), true).as_deref(), Some("summer-garden"), "wraps round");
    }

    /// A user file that breaks a built-in family does not stop `t`: it skips the family and
    /// reaches every other one.
    #[test]
    fn stepping_skips_a_broken_family() {
        let catalog = with(&[("cedar-shade", "[scene]\nfoliage_density = inf")]);
        let mut current = catalog.resolve(ROOT).expect("root").summary;
        let mut seen = Vec::new();
        while let Some(id) = catalog.next_scene(&current, true) {
            current = catalog.resolve(&id).expect("next_scene picks themes that resolve").summary;
            if seen.contains(&current.family) {
                break;
            }
            seen.push(current.family.clone());
        }
        assert_eq!(seen, ["maple-afternoon", "petal-spring", "rainy-afternoon", "ink-and-vermilion", "pixel-garden", "pocket-moss", "garden"]);
        assert_eq!(
            catalog.next_scene(&catalog.resolve("maple-afternoon").expect("maple-afternoon").summary, false).as_deref(),
            Some("summer-garden"),
            "backward skips it too"
        );
    }
}
