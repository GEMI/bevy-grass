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
//!
//! Other plants mix in through `species`, each a style of its own with its
//! share of the blades:
//!
//! ```text
//! (
//!     base: "SUMMER_TALL",
//!     tufts: 0.3,
//!     species: [
//!         (share: 0.1, style: (base: "SEED_HEADS")),
//!         (share: 0.03, clustering: 0.8, style: (
//!             base: "WHITE_FLOWERS",
//!             head_color: (0.75, 0.55, 0.95),
//!         )),
//!     ],
//! )
//! ```

use std::borrow::Cow;
use std::fmt;

use bevy::asset::io::Reader;
use bevy::asset::{AssetLoader, LoadContext};
use bevy::color::Color;
use bevy::log::warn;
use bevy::math::Vec2;
use bevy::reflect::TypePath;
use serde::{Deserialize, Serialize};

use crate::api::{GrassBlade, GrassSpecies, GrassStyle, GrassVariation, GroundTint};
use crate::fields::MAX_SPECIES;

/// The presets a file can name as its `base`.
static PRESETS: [(&str, GrassStyle); 22] = [
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
    ("SUMMER_WILD", GrassStyle::SUMMER_WILD),
    ("AUTUMN_WILD", GrassStyle::AUTUMN_WILD),
    ("WILDFLOWER_MEADOW", GrassStyle::WILDFLOWER_MEADOW),
    ("SEED_HEADS", GrassStyle::SEED_HEADS),
    ("WHITE_FLOWERS", GrassStyle::WHITE_FLOWERS),
    ("YELLOW_FLOWERS", GrassStyle::YELLOW_FLOWERS),
    ("CLOVER", GrassStyle::CLOVER),
    ("BROAD_LEAF", GrassStyle::BROAD_LEAF),
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
        if file.mentions_min_width() {
            warn!(
                "bevy_grass: `min_width_px` in a .grass.ron file is ignored; a blade's minimum \
                 width depends on the camera. Set GrassSettings::min_width_px, or \
                 GrassCamera::min_width_px on one camera, and delete the line."
            );
        }
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    height: Option<(f32, f32)>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    width: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    density: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    lean: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    stiffness: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    root_color: Option<Srgb>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tip_color: Option<Srgb>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    root_occlusion: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    translucency: Option<f32>,
    /// Read, never used or written: the setting moved to `GrassSettings` and
    /// `GrassCamera`, and files from before that still load.
    #[serde(default, skip_serializing)]
    min_width_px: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    specular: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ground_color: Option<Srgb>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ground_tint: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    toon_bands: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    twist: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    droop: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    fold: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    head: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    head_width: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    head_color: Option<Srgb>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    patch_color: Option<Srgb>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    patch_amount: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    patch_size: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    dry_tips: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    dead_blades: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tufts: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tuft_size: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    bare_patches: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    bare_patch_size: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    species: Option<Vec<GrassSpeciesFile>>,
}

/// One entry of `species`.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct GrassSpeciesFile {
    share: f32,
    #[serde(default)]
    clustering: Option<f32>,
    style: GrassStyleFile,
}

fn srgb(color: Color) -> Srgb {
    let c = color.to_srgba();
    (c.red, c.green, c.blue)
}

fn color((r, g, b): Srgb) -> Color {
    Color::srgb(r, g, b)
}

impl GrassStyleFile {
    /// Whether the file, or a species in it, still sets the old
    /// `min_width_px`.
    fn mentions_min_width(&self) -> bool {
        self.min_width_px.is_some()
            || self
                .species
                .iter()
                .flatten()
                .any(|species| species.style.mentions_min_width())
    }

    /// A file that sets every setting of `blade`, and nothing else.
    fn from_blade(blade: &GrassBlade) -> Self {
        Self {
            height: Some((blade.height.x, blade.height.y)),
            width: Some(blade.width),
            lean: Some(blade.lean),
            stiffness: Some(blade.stiffness),
            root_color: Some(srgb(blade.root_color)),
            tip_color: Some(srgb(blade.tip_color)),
            root_occlusion: Some(blade.root_occlusion),
            translucency: Some(blade.translucency),
            specular: Some(blade.specular),
            twist: Some(blade.twist),
            droop: Some(blade.droop),
            fold: Some(blade.fold),
            head: Some(blade.head),
            head_width: Some(blade.head_width),
            head_color: Some(srgb(blade.head_color)),
            ..Self::default()
        }
    }

    fn from_style(style: &GrassStyle) -> Self {
        let variation = &style.variation;
        Self {
            density: Some(style.density),
            ground_color: Some(srgb(style.ground_tint.color)),
            ground_tint: Some(style.ground_tint.amount),
            toon_bands: Some(style.toon_bands),
            patch_color: Some(srgb(variation.patch_color)),
            patch_amount: Some(variation.patch_amount),
            patch_size: Some(variation.patch_size),
            dry_tips: Some(variation.dry_tips),
            dead_blades: Some(variation.dead_blades),
            tufts: Some(variation.tufts),
            tuft_size: Some(variation.tuft_size),
            bare_patches: Some(variation.bare_patches),
            bare_patch_size: Some(variation.bare_patch_size),
            species: Some(
                style
                    .species
                    .iter()
                    .map(|species| GrassSpeciesFile {
                        share: species.share,
                        clustering: Some(species.clustering),
                        style: Self::from_blade(&species.blade),
                    })
                    .collect(),
            ),
            ..Self::from_blade(&style.blade)
        }
    }

    /// The preset `base` names, or the meadow.
    fn base_style(&self) -> Result<GrassStyle, GrassStyleFileError> {
        match &self.base {
            Some(name) => GrassStyle::preset(name)
                .ok_or_else(|| GrassStyleFileError::UnknownBase(name.clone())),
            None => Ok(GrassStyle::MEADOW),
        }
    }

    /// The blade this file describes: its settings over `base`'s.
    fn blade(&self, base: &GrassBlade) -> GrassBlade {
        GrassBlade {
            height: self
                .height
                .map_or(base.height, |(lo, hi)| Vec2::new(lo, hi)),
            width: self.width.unwrap_or(base.width),
            lean: self.lean.unwrap_or(base.lean),
            stiffness: self.stiffness.unwrap_or(base.stiffness),
            root_color: self.root_color.map_or(base.root_color, color),
            tip_color: self.tip_color.map_or(base.tip_color, color),
            root_occlusion: self.root_occlusion.unwrap_or(base.root_occlusion),
            translucency: self.translucency.unwrap_or(base.translucency),
            specular: self.specular.unwrap_or(base.specular),
            twist: self.twist.unwrap_or(base.twist),
            droop: self.droop.unwrap_or(base.droop),
            fold: self.fold.unwrap_or(base.fold),
            head: self.head.unwrap_or(base.head),
            head_width: self.head_width.unwrap_or(base.head_width),
            head_color: self.head_color.map_or(base.head_color, color),
        }
    }

    /// The settings of the whole field that the file sets, by name.
    fn field_settings(&self) -> Vec<&'static str> {
        [
            ("density", self.density.is_some()),
            ("ground_color", self.ground_color.is_some()),
            ("ground_tint", self.ground_tint.is_some()),
            ("toon_bands", self.toon_bands.is_some()),
            ("patch_color", self.patch_color.is_some()),
            ("patch_amount", self.patch_amount.is_some()),
            ("patch_size", self.patch_size.is_some()),
            ("dry_tips", self.dry_tips.is_some()),
            ("dead_blades", self.dead_blades.is_some()),
            ("tufts", self.tufts.is_some()),
            ("tuft_size", self.tuft_size.is_some()),
            ("bare_patches", self.bare_patches.is_some()),
            ("bare_patch_size", self.bare_patch_size.is_some()),
            ("species", self.species.is_some()),
        ]
        .into_iter()
        .filter_map(|(name, set)| set.then_some(name))
        .collect()
    }

    fn resolve(self) -> Result<GrassStyle, GrassStyleFileError> {
        let base = self.base_style()?;
        let (bv, bg) = (&base.variation, &base.ground_tint);
        let style = GrassStyle {
            blade: self.blade(&base.blade),
            density: self.density.unwrap_or(base.density),
            variation: GrassVariation {
                patch_color: self.patch_color.map_or(bv.patch_color, color),
                patch_amount: self.patch_amount.unwrap_or(bv.patch_amount),
                patch_size: self.patch_size.unwrap_or(bv.patch_size),
                dry_tips: self.dry_tips.unwrap_or(bv.dry_tips),
                dead_blades: self.dead_blades.unwrap_or(bv.dead_blades),
                tufts: self.tufts.unwrap_or(bv.tufts),
                tuft_size: self.tuft_size.unwrap_or(bv.tuft_size),
                bare_patches: self.bare_patches.unwrap_or(bv.bare_patches),
                bare_patch_size: self.bare_patch_size.unwrap_or(bv.bare_patch_size),
            },
            ground_tint: GroundTint {
                color: self.ground_color.map_or(bg.color, color),
                amount: self.ground_tint.unwrap_or(bg.amount),
            },
            toon_bands: self.toon_bands.unwrap_or(base.toon_bands),
            species: match self.species {
                Some(list) => Cow::Owned(
                    list.into_iter()
                        .map(GrassSpeciesFile::resolve)
                        .collect::<Result<_, _>>()?,
                ),
                None => base.species,
            },
        };
        validate_blade(&style.blade)?;
        if style.density < 0.0 || style.density.is_nan() {
            return Err(GrassStyleFileError::Invalid("density must be at least 0"));
        }
        let variation = &style.variation;
        if !(variation.patch_size > 0.0
            && variation.tuft_size > 0.0
            && variation.bare_patch_size > 0.0)
        {
            return Err(GrassStyleFileError::Invalid(
                "patch_size, tuft_size and bare_patch_size must be above 0",
            ));
        }
        if style.species.len() >= MAX_SPECIES {
            return Err(GrassStyleFileError::Invalid(
                "a style mixes at most 15 species besides its own blades",
            ));
        }
        Ok(style)
    }

    /// The file as a species' blade, which may name `base` and the blade's
    /// settings and nothing else.
    fn resolve_blade(self) -> Result<GrassBlade, GrassStyleFileError> {
        let field_settings = self.field_settings();
        if !field_settings.is_empty() {
            return Err(GrassStyleFileError::SpeciesFieldSettings(field_settings));
        }
        let blade = self.blade(&self.base_style()?.blade);
        validate_blade(&blade)?;
        Ok(blade)
    }
}

/// The range checks a blade needs, in a style or a species.
fn validate_blade(blade: &GrassBlade) -> Result<(), GrassStyleFileError> {
    if !(blade.height.x >= 0.0 && blade.height.x <= blade.height.y && blade.height.y > 0.0) {
        return Err(GrassStyleFileError::Invalid(
            "height must be (shortest, tallest) with 0 <= shortest <= tallest and tallest > 0",
        ));
    }
    if blade.width.is_nan() || blade.width <= 0.0 {
        return Err(GrassStyleFileError::Invalid("width must be above 0"));
    }
    if !(0.0..0.9).contains(&blade.head) {
        return Err(GrassStyleFileError::Invalid(
            "head must be at least 0 and below 0.9",
        ));
    }
    Ok(())
}

impl GrassSpeciesFile {
    fn resolve(self) -> Result<GrassSpecies, GrassStyleFileError> {
        if !(0.0..=1.0).contains(&self.share) {
            return Err(GrassStyleFileError::Invalid(
                "a species' share must be from 0 to 1",
            ));
        }
        Ok(GrassSpecies {
            share: self.share,
            clustering: self.clustering.unwrap_or(0.0),
            blade: self.style.resolve_blade()?,
        })
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
    /// A species' `style` sets things that belong to the whole field, which
    /// a species cannot have: the names of the settings.
    SpeciesFieldSettings(Vec<&'static str>),
}

impl fmt::Display for GrassStyleFileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "could not read the grass style: {error}"),
            Self::Parse(error) => write!(f, "not a grass style: {error}"),
            Self::UnknownBase(name) => write!(
                f,
                "no preset called {name:?}; the presets are {}",
                PRESETS
                    .iter()
                    .map(|(name, _)| *name)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Self::Invalid(why) => write!(f, "invalid grass style: {why}"),
            Self::SpeciesFieldSettings(names) => write!(
                f,
                "a species' style sets {}, which belong to the field's style, not to a species \
                 (a species is a blade: height, width, colours, shape); move them to the \
                 style that lists the species",
                names.join(", ")
            ),
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

    #[test]
    fn min_width_px_is_no_longer_a_style_setting_but_old_files_still_load() {
        // A file from before the setting moved to `GrassSettings`, with the
        // line in the style and in a species.
        let old = r#"(
            base: "SUMMER_TALL",
            min_width_px: 2.0,
            species: [(share: 0.1, style: (base: "CLOVER", min_width_px: 3.0))],
        )"#;
        let style = GrassStyle::from_ron(old).expect("old files must keep loading");
        let without = GrassStyle::from_ron(
            r#"(base: "SUMMER_TALL", species: [(share: 0.1, style: (base: "CLOVER"))])"#,
        )
        .unwrap();
        assert_eq!(style.to_ron(), without.to_ron(), "the old value is ignored");
        assert!(
            !style.to_ron().contains("min_width_px"),
            "a style no longer writes the setting"
        );
        assert!(!GrassStyleFile::default().mentions_min_width());
        let file: GrassStyleFile = options().from_str(old).unwrap();
        assert!(file.mentions_min_width(), "so the loader can warn about it");
    }

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
            assert_eq!(read, style, "{name}");
        }
    }

    #[test]
    fn no_shipped_file_still_sets_the_old_min_width() {
        // Copying a preset must not start a game with a warning.
        for (name, _) in GrassStyle::presets() {
            assert!(!shipped(name).contains("min_width_px"), "{name}");
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
        assert_eq!(style.blade.height, GrassStyle::AUTUMN_FADED.blade.height);
        assert_eq!(srgb(style.blade.tip_color), (0.70, 0.72, 0.74));
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

    #[test]
    fn a_species_is_a_blade_and_a_file_cannot_say_otherwise() {
        // The settings of the whole field do nothing in a species. In code the
        // type has nowhere to put them; in a file they are an error that
        // names them, not something silently ignored.
        let Err(error) = GrassStyle::from_ron(
            "(species: [(share: 0.1, style: (base: \"CLOVER\", tufts: 0.8, density: 5.0))])",
        ) else {
            panic!("field settings in a species must be an error");
        };
        let GrassStyleFileError::SpeciesFieldSettings(names) = &error else {
            panic!("wrong error: {error}");
        };
        assert_eq!(names, &["density", "tufts"]);
        let message = error.to_string();
        assert!(message.contains("density, tufts"), "{message}");

        // Nested species are the same mistake.
        assert!(matches!(
            GrassStyle::from_ron(
                "(species: [(share: 0.1, style: (species: [(share: 0.1, style: ())]))])"
            ),
            Err(GrassStyleFileError::SpeciesFieldSettings(names)) if names == ["species"]
        ));

        // Blade settings are fine, and are range-checked like a style's.
        assert!(
            GrassStyle::from_ron("(species: [(share: 0.1, style: (head_color: (1.0, 0.0, 0.0)))])")
                .is_ok()
        );
        assert!(matches!(
            GrassStyle::from_ron("(species: [(share: 0.1, style: (height: (2.0, 1.0)))])"),
            Err(GrassStyleFileError::Invalid(_))
        ));
        assert!(matches!(
            GrassStyle::from_ron("(species: [(share: 0.1, style: (head: 0.95))])"),
            Err(GrassStyleFileError::Invalid(_))
        ));
    }

    #[test]
    fn every_preset_survives_a_round_trip_through_a_file() {
        for (name, style) in GrassStyle::presets() {
            let text = style.to_ron();
            let read = GrassStyle::from_ron(&text).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(read, style, "{name}");
            // A species lists its blade and nothing of the field's.
            let species = species_text(&text);
            for field_setting in ["density", "tufts", "ground_tint", "toon_bands"] {
                assert!(!species.contains(field_setting), "{name}: {field_setting}");
            }
        }
    }

    /// The part of a written style after `species:`, where the species are.
    fn species_text(text: &str) -> &str {
        text.split("species:").nth(1).unwrap_or("")
    }

    #[test]
    fn species_name_only_what_differs_too() {
        let style = GrassStyle::from_ron(
            r#"(
                base: "SUMMER_TALL",
                species: [
                    (share: 0.1, style: (base: "SEED_HEADS")),
                    (share: 0.03, clustering: 0.8, style: (
                        base: "WHITE_FLOWERS",
                        head_color: (0.75, 0.55, 0.95),
                    )),
                ],
            )"#,
        )
        .unwrap();
        assert_eq!(style.species.len(), 2);
        assert_eq!(style.species[0].clustering, 0.0);
        assert_eq!(style.species[0].blade, GrassBlade::SEED_HEADS);
        assert_eq!(style.species[1].clustering, 0.8);
        assert_eq!(srgb(style.species[1].blade.head_color), (0.75, 0.55, 0.95));
        assert_eq!(
            style.species[1].blade.height,
            GrassBlade::WHITE_FLOWERS.height
        );
    }
}
