//! [`GrassStyle`] as a `.grass.ron` file: an asset loader, so styles can be
//! tuned in a text editor and, with Bevy's `file_watcher` feature, change in
//! a running game the moment the file is saved.
//!
//! Every field is optional. A missing field comes from `base`, the name of a
//! preset constant (`MEADOW` when not given), so a variant needs only what
//! differs:
//!
//! ```text
//! // assets/grass/frosted.grass.ron
//! (
//!     base: "AUTUMN_FADED",
//!     tip_color: (0.70, 0.72, 0.74),
//! )
//! ```
//!
//! Colours are sRGB, as in `Color::srgb`. The crate's `assets/grass_styles`
//! folder holds every preset as a file to copy from.

use std::fmt;

use bevy::asset::io::Reader;
use bevy::asset::{AssetLoader, LoadContext};
use bevy::color::Color;
use bevy::math::Vec2;
use bevy::reflect::TypePath;
use serde::{Deserialize, Serialize};

use crate::api::GrassStyle;

/// The presets a file can name as its `base`.
const PRESETS: [(&str, GrassStyle); 14] = [
    ("MEADOW", GrassStyle::MEADOW),
    ("TALL_PRAIRIE", GrassStyle::TALL_PRAIRIE),
    ("WHEAT", GrassStyle::WHEAT),
    ("DRY_SAVANNA", GrassStyle::DRY_SAVANNA),
    ("LAWN", GrassStyle::LAWN),
    ("STYLIZED", GrassStyle::STYLIZED),
    ("SUMMER_TALL", GrassStyle::SUMMER_TALL),
    ("SUMMER_LUSH", GrassStyle::SUMMER_LUSH),
    ("SUMMER_MEADOW", GrassStyle::SUMMER_MEADOW),
    ("SUMMER_SEEDING", GrassStyle::SUMMER_SEEDING),
    ("AUTUMN_TALL", GrassStyle::AUTUMN_TALL),
    ("AUTUMN_RUST", GrassStyle::AUTUMN_RUST),
    ("AUTUMN_STRAW", GrassStyle::AUTUMN_STRAW),
    ("AUTUMN_FADED", GrassStyle::AUTUMN_FADED),
];

impl GrassStyle {
    /// The preset constant called `name`, such as `"SUMMER_TALL"`.
    pub fn preset(name: &str) -> Option<Self> {
        PRESETS
            .iter()
            .find(|(preset, _)| *preset == name)
            .map(|(_, style)| style.clone())
    }

    /// Every preset constant, with its name.
    pub fn presets() -> impl Iterator<Item = (&'static str, Self)> {
        PRESETS.iter().map(|(name, style)| (*name, style.clone()))
    }

    /// Reads a style from the text of a `.grass.ron` file.
    pub fn from_ron(text: &str) -> Result<Self, GrassStyleFileError> {
        let file: GrassStyleFile = options()
            .from_str(text)
            .map_err(GrassStyleFileError::Parse)?;
        file.resolve()
    }

    /// Writes the style as the text of a `.grass.ron` file, every field
    /// given.
    pub fn to_ron(&self) -> String {
        let file = GrassStyleFile::from_style(self);
        let config = ron::ser::PrettyConfig::new()
            .struct_names(false)
            .new_line("\n".to_owned());
        options()
            .to_string_pretty(&file, config)
            .expect("a style always serializes")
    }
}

/// Fields are written plainly, `width: 0.04`, not `width: Some(0.04)`.
fn options() -> ron::Options {
    ron::Options::default().with_default_extension(ron::extensions::Extensions::IMPLICIT_SOME)
}

/// An sRGB colour, as written in a file.
type Srgb = (f32, f32, f32);

/// The file format. Mirrors [`GrassStyle`], every field optional.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct GrassStyleFile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    base: Option<String>,
    #[serde(default)]
    height: Option<(f32, f32)>,
    #[serde(default)]
    width: Option<f32>,
    #[serde(default)]
    density: Option<f32>,
    #[serde(default)]
    lean: Option<f32>,
    #[serde(default)]
    stiffness: Option<f32>,
    #[serde(default)]
    root_color: Option<Srgb>,
    #[serde(default)]
    tip_color: Option<Srgb>,
    #[serde(default)]
    root_occlusion: Option<f32>,
    #[serde(default)]
    translucency: Option<f32>,
    #[serde(default)]
    min_width_px: Option<f32>,
    #[serde(default)]
    specular: Option<f32>,
    #[serde(default)]
    ground_color: Option<Srgb>,
    #[serde(default)]
    ground_tint: Option<f32>,
    #[serde(default)]
    toon_bands: Option<u32>,
}

fn srgb(color: Color) -> Srgb {
    let c = color.to_srgba();
    (c.red, c.green, c.blue)
}

fn color((r, g, b): Srgb) -> Color {
    Color::srgb(r, g, b)
}

impl GrassStyleFile {
    fn from_style(style: &GrassStyle) -> Self {
        Self {
            base: None,
            height: Some((style.height.x, style.height.y)),
            width: Some(style.width),
            density: Some(style.density),
            lean: Some(style.lean),
            stiffness: Some(style.stiffness),
            root_color: Some(srgb(style.root_color)),
            tip_color: Some(srgb(style.tip_color)),
            root_occlusion: Some(style.root_occlusion),
            translucency: Some(style.translucency),
            min_width_px: Some(style.min_width_px),
            specular: Some(style.specular),
            ground_color: Some(srgb(style.ground_color)),
            ground_tint: Some(style.ground_tint),
            toon_bands: Some(style.toon_bands),
        }
    }

    fn resolve(self) -> Result<GrassStyle, GrassStyleFileError> {
        let base = match &self.base {
            Some(name) => GrassStyle::preset(name)
                .ok_or_else(|| GrassStyleFileError::UnknownBase(name.clone()))?,
            None => GrassStyle::MEADOW,
        };
        let style = GrassStyle {
            height: self
                .height
                .map_or(base.height, |(lo, hi)| Vec2::new(lo, hi)),
            width: self.width.unwrap_or(base.width),
            density: self.density.unwrap_or(base.density),
            lean: self.lean.unwrap_or(base.lean),
            stiffness: self.stiffness.unwrap_or(base.stiffness),
            root_color: self.root_color.map_or(base.root_color, color),
            tip_color: self.tip_color.map_or(base.tip_color, color),
            root_occlusion: self.root_occlusion.unwrap_or(base.root_occlusion),
            translucency: self.translucency.unwrap_or(base.translucency),
            min_width_px: self.min_width_px.unwrap_or(base.min_width_px),
            specular: self.specular.unwrap_or(base.specular),
            ground_color: self.ground_color.map_or(base.ground_color, color),
            ground_tint: self.ground_tint.unwrap_or(base.ground_tint),
            toon_bands: self.toon_bands.unwrap_or(base.toon_bands),
        };
        if !(style.height.x >= 0.0 && style.height.x <= style.height.y && style.height.y > 0.0) {
            return Err(GrassStyleFileError::Invalid(
                "height must be (shortest, tallest) with 0 <= shortest <= tallest and tallest > 0",
            ));
        }
        if !(style.width > 0.0 && style.density >= 0.0) {
            return Err(GrassStyleFileError::Invalid(
                "width must be above 0 and density at least 0",
            ));
        }
        Ok(style)
    }
}

/// Why a `.grass.ron` file could not be read.
#[derive(Debug)]
#[non_exhaustive]
pub enum GrassStyleFileError {
    /// The file could not be read.
    Io(std::io::Error),
    /// The text is not a valid style.
    Parse(ron::error::SpannedError),
    /// `base` names no preset.
    UnknownBase(String),
    /// A value is out of range.
    Invalid(&'static str),
}

impl fmt::Display for GrassStyleFileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "could not read the grass style: {error}"),
            Self::Parse(error) => write!(f, "not a grass style: {error}"),
            Self::UnknownBase(name) => write!(
                f,
                "no preset called {name:?}; the presets are {}",
                PRESETS.map(|(name, _)| name).join(", ")
            ),
            Self::Invalid(why) => write!(f, "invalid grass style: {why}"),
        }
    }
}

impl std::error::Error for GrassStyleFileError {}

/// Loads `.grass.ron` files as [`GrassStyle`] assets. Registered by
/// `GrassPlugin`.
#[derive(Debug, Default, TypePath)]
pub struct GrassStyleLoader;

impl AssetLoader for GrassStyleLoader {
    type Asset = GrassStyle;
    type Settings = ();
    type Error = GrassStyleFileError;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &(),
        _load_context: &mut LoadContext<'_>,
    ) -> Result<GrassStyle, GrassStyleFileError> {
        let mut bytes = Vec::new();
        reader
            .read_to_end(&mut bytes)
            .await
            .map_err(GrassStyleFileError::Io)?;
        let text = std::str::from_utf8(&bytes).map_err(|error| {
            GrassStyleFileError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, error))
        })?;
        GrassStyle::from_ron(text)
    }

    fn extensions(&self) -> &[&str] {
        &["grass.ron"]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shipped files, which must stay equal to the constants.
    fn shipped(name: &str) -> String {
        let file = format!(
            "{}/assets/grass_styles/{}.grass.ron",
            env!("CARGO_MANIFEST_DIR"),
            name.to_lowercase()
        );
        std::fs::read_to_string(&file).unwrap_or_else(|error| panic!("{file}: {error}"))
    }

    #[test]
    fn every_preset_file_matches_its_constant() {
        // `BLESS_STYLES=1 cargo test` rewrites the files from the constants.
        let bless = std::env::var_os("BLESS_STYLES").is_some();
        for (name, style) in GrassStyle::presets() {
            if bless {
                let file = format!(
                    "{}/assets/grass_styles/{}.grass.ron",
                    env!("CARGO_MANIFEST_DIR"),
                    name.to_lowercase()
                );
                let doc = style_doc(name);
                std::fs::write(&file, format!("{doc}{}\n", style.to_ron())).unwrap();
            }
            let read = GrassStyle::from_ron(&shipped(name)).unwrap();
            assert_eq!(format!("{read:?}"), format!("{style:?}"), "{name}");
        }
    }

    /// The comment at the top of a preset's file.
    fn style_doc(name: &str) -> String {
        format!(
            "// GrassStyle::{name}, as a file. Copy it, edit it, and load it with\n\
             // `asset_server.load(\"path/to/copy.grass.ron\")`. Every field is\n\
             // optional: a missing one comes from `base`, a preset name.\n"
        )
    }

    #[test]
    fn a_variant_names_only_what_differs() {
        let style = GrassStyle::from_ron(
            r#"(
                base: "AUTUMN_FADED",
                tip_color: (0.70, 0.72, 0.74),
                density: 20.0,
            )"#,
        )
        .unwrap();
        assert_eq!(style.density, 20.0);
        assert_eq!(style.height, GrassStyle::AUTUMN_FADED.height);
        assert_eq!(srgb(style.tip_color), (0.70, 0.72, 0.74));
    }

    #[test]
    fn mistakes_are_reported() {
        assert!(matches!(
            GrassStyle::from_ron("(base: \"PAMPAS\")"),
            Err(GrassStyleFileError::UnknownBase(_))
        ));
        assert!(matches!(
            GrassStyle::from_ron("(hieght: (0.1, 0.2))"),
            Err(GrassStyleFileError::Parse(_))
        ));
        assert!(matches!(
            GrassStyle::from_ron("(height: (0.5, 0.2))"),
            Err(GrassStyleFileError::Invalid(_))
        ));
    }
}
