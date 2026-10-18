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
//!
//! The forest-floor presets (`FOREST_MOSS`, `FOREST_FERNS`, `FOREST_FLOOR`,
//! `FOREST_BRACKEN`) are good bases for a wood: short, wide blades, clumps of
//! ferns and fallen leaves mixed in.

use std::borrow::Cow;
use std::fmt;
use std::path::{Path, PathBuf};

use bevy::app::{App, Plugin};
use bevy::asset::io::Reader;
use bevy::asset::io::embedded::EmbeddedAssetRegistry;
use bevy::asset::{AssetApp, AssetLoader, LoadContext};
use bevy::color::{Color, Srgba};
use bevy::log::warn;
use bevy::math::Vec2;
use bevy::reflect::TypePath;
use serde::{Deserialize, Serialize};

use crate::api::{GrassBlade, GrassSpecies, GrassStyle};
use crate::presets::PRESETS;
use crate::style::MAX_SPECIES;

impl GrassStyle {
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

/// How one setting is written in a file: `File` is what the text holds.
trait FileValue: Sized {
    type File: Copy + Serialize + for<'de> Deserialize<'de>;
    fn to_file(&self) -> Self::File;
    fn from_file(file: Self::File) -> Self;
}

impl FileValue for f32 {
    type File = f32;
    fn to_file(&self) -> f32 {
        *self
    }
    fn from_file(file: f32) -> f32 {
        file
    }
}

impl FileValue for u32 {
    type File = u32;
    fn to_file(&self) -> u32 {
        *self
    }
    fn from_file(file: u32) -> u32 {
        file
    }
}

impl FileValue for Vec2 {
    type File = (f32, f32);
    fn to_file(&self) -> (f32, f32) {
        (self.x, self.y)
    }
    fn from_file((x, y): (f32, f32)) -> Vec2 {
        Vec2::new(x, y)
    }
}

impl FileValue for Color {
    type File = FileColor;
    fn to_file(&self) -> FileColor {
        FileColor(*self)
    }
    fn from_file(file: FileColor) -> Color {
        file.0
    }
}

/// A colour in a file: sRGB as `(0.38, 0.55, 0.16)`, or hex as `"#5c8f24"`
/// (the form colour pickers copy). Written as the tuple, which keeps every
/// bit of a preset's colour.
#[derive(Debug, Clone, Copy)]
struct FileColor(Color);

impl Serialize for FileColor {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let c = self.0.to_srgba();
        (c.red, c.green, c.blue).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for FileColor {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ColorVisitor;

        impl<'de> serde::de::Visitor<'de> for ColorVisitor {
            type Value = FileColor;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(
                    "a colour: (red, green, blue) in sRGB, or a hex string like \"#5c8f24\"",
                )
            }

            fn visit_str<E: serde::de::Error>(self, text: &str) -> Result<FileColor, E> {
                Srgba::hex(text)
                    .map(|color| FileColor(Color::Srgba(color)))
                    .map_err(|error| {
                        E::custom(format!(
                            "{text:?} is not a hex colour ({error}); write \"#rrggbb\""
                        ))
                    })
            }

            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> Result<FileColor, A::Error> {
                let mut channel = |name: &'static str| -> Result<f32, A::Error> {
                    seq.next_element()?
                        .ok_or_else(|| serde::de::Error::custom(format!("a colour needs {name}")))
                };
                let (r, g, b) = (channel("red")?, channel("green")?, channel("blue")?);
                Ok(FileColor(Color::srgb(r, g, b)))
            }
        }

        deserializer.deserialize_any(ColorVisitor)
    }
}

/// Declares the file format once, from the settings it holds, and writes the
/// struct and the code that reads and writes it. A new setting is one line
/// here (plus its field in the style), so the format cannot drift from the
/// type.
///
/// `blade`: settings of a `GrassBlade`, named as the field is. `field`:
/// settings of the whole field, with the path to them in a `GrassStyle`.
/// Names in brackets are older names that still load.
macro_rules! style_file {
    (
        blade {
            $( $bname:ident $([$($balias:literal),+])? : $bty:ty ),* $(,)?
        }
        field {
            $( $fname:ident $([$($falias:literal),+])? : $fty:ty => $($fpath:ident).+ ),* $(,)?
        }
    ) => {
        /// The file format. Mirrors [`GrassStyle`], every field optional.
        #[derive(Debug, Default, Serialize, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct GrassStyleFile {
            #[serde(default, skip_serializing_if = "Option::is_none")]
            base: Option<String>,
            $(
                #[serde(default, skip_serializing_if = "Option::is_none"
                    $($(, alias = $balias)+)?)]
                $bname: Option<<$bty as FileValue>::File>,
            )*
            $(
                #[serde(default, skip_serializing_if = "Option::is_none"
                    $($(, alias = $falias)+)?)]
                $fname: Option<<$fty as FileValue>::File>,
            )*
            /// Read, never used or written: the setting moved to
            /// `GrassSettings` and `GrassCamera`, and files from before that
            /// still load.
            #[serde(default, skip_serializing)]
            min_width_px: Option<f32>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            species: Option<Vec<GrassSpeciesFile>>,
        }

        impl GrassStyleFile {
            /// A file that sets every setting of `blade`, and nothing else.
            fn from_blade(blade: &GrassBlade) -> Self {
                Self {
                    $( $bname: Some(<$bty as FileValue>::to_file(&blade.$bname)), )*
                    ..Self::default()
                }
            }

            /// A file that sets every setting of `style`.
            fn from_style(style: &GrassStyle) -> Self {
                Self {
                    $( $fname: Some(<$fty as FileValue>::to_file(&style.$($fpath).+)), )*
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

            /// The blade settings the file sets, over `blade`.
            fn apply_to_blade(&self, blade: &mut GrassBlade) {
                $(
                    if let Some(value) = self.$bname {
                        blade.$bname = <$bty as FileValue>::from_file(value);
                    }
                )*
            }

            /// The settings of the whole field the file sets, over `style`.
            fn apply_to_field(&self, style: &mut GrassStyle) {
                $(
                    if let Some(value) = self.$fname {
                        style.$($fpath).+ = <$fty as FileValue>::from_file(value);
                    }
                )*
            }

            /// The settings of the whole field that the file sets, by name.
            fn field_settings(&self) -> Vec<&'static str> {
                [
                    $( (stringify!($fname), self.$fname.is_some()), )*
                    ("species", self.species.is_some()),
                ]
                .into_iter()
                .filter_map(|(name, set)| set.then_some(name))
                .collect()
            }
        }
    };
}

style_file! {
    blade {
        height: Vec2,
        width: f32,
        lean: f32,
        stiffness: f32,
        root_color: Color,
        tip_color: Color,
        root_occlusion: f32,
        translucency: f32,
        specular: f32,
        twist: f32,
        droop: f32,
        fold: f32,
        head_length["head"]: f32,
        head_width: f32,
        head_color: Color,
    }
    field {
        density: f32 => density,
        ground_color: Color => ground_tint.color,
        ground_tint: f32 => ground_tint.amount,
        toon_bands: u32 => toon_bands,
        patch_color: Color => variation.patch_color,
        patch_amount: f32 => variation.patch_amount,
        patch_size: f32 => variation.patch_size,
        dry_tips: f32 => variation.dry_tips,
        dead_share["dead_blades"]: f32 => variation.dead_share,
        tufts: f32 => variation.tufts,
        tuft_spacing["tuft_size"]: f32 => variation.tuft_spacing,
        bare_share["bare_patches"]: f32 => variation.bare_share,
        bare_patch_size: f32 => variation.bare_patch_size,
    }
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

#[cfg(test)]
fn srgb(color: Color) -> (f32, f32, f32) {
    let c = color.to_srgba();
    (c.red, c.green, c.blue)
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

    /// The preset `base` names, or the meadow.
    fn base_style(&self) -> Result<GrassStyle, GrassStyleFileError> {
        match &self.base {
            Some(name) => GrassStyle::preset(name)
                .ok_or_else(|| GrassStyleFileError::UnknownBase(name.clone())),
            None => Ok(GrassStyle::MEADOW),
        }
    }

    fn resolve(self) -> Result<GrassStyle, GrassStyleFileError> {
        let mut style = self.base_style()?;
        self.apply_to_blade(&mut style.blade);
        self.apply_to_field(&mut style);
        if let Some(list) = self.species {
            style.species = Cow::Owned(
                list.into_iter()
                    .map(GrassSpeciesFile::resolve)
                    .collect::<Result<_, _>>()?,
            );
        }
        validate_blade(&style.blade)?;
        if style.density < 0.0 || style.density.is_nan() {
            return Err(GrassStyleFileError::Invalid("density must be at least 0"));
        }
        let variation = &style.variation;
        if !(variation.patch_size > 0.0
            && variation.tuft_spacing > 0.0
            && variation.bare_patch_size > 0.0)
        {
            return Err(GrassStyleFileError::Invalid(
                "patch_size, tuft_spacing and bare_patch_size must be above 0",
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
        let mut blade = self.base_style()?.blade;
        self.apply_to_blade(&mut blade);
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
    if !(0.0..0.9).contains(&blade.head_length) {
        return Err(GrassStyleFileError::Invalid(
            "head_length must be at least 0 and below 0.9",
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
/// [`GrassStyleFilePlugin`].
#[derive(Debug, Default, TypePath)]
pub struct GrassStyleLoader;

/// Teaches the app to load `.grass.ron` style files, and ships the presets as
/// files. Part of `GrassPlugins`; it needs [`GrassPlugin`](crate::GrassPlugin).
///
/// Every preset is an asset of the `embedded` source, whatever the game's own
/// asset folder holds, so a crate user can load one by path, to use as it is
/// or to copy:
///
/// ```no_run
/// # use bevy::prelude::*;
/// # use bevy_grass::prelude::*;
/// fn setup(mut commands: Commands, assets: Res<AssetServer>) {
///     commands.spawn(GrassField {
///         size: Vec2::splat(64.0),
///         style: assets.load("embedded://bevy_grass/styles/summer_tall.grass.ron"),
///         ..default()
///     });
/// }
/// ```
///
/// The names are the presets' lower-case names, such as `meadow`,
/// `tall_prairie`, `wildflower_meadow` and `forest_ferns`.
#[derive(Debug, Default, Clone, Copy)]
pub struct GrassStyleFilePlugin;

impl Plugin for GrassStyleFilePlugin {
    fn build(&self, app: &mut App) {
        app.init_asset_loader::<GrassStyleLoader>();
        if let Some(registry) = app.world().get_resource::<EmbeddedAssetRegistry>() {
            // Generated from the preset list, so an embedded preset cannot
            // drift from its constant or from the shipped `.grass.ron` file.
            for (name, style) in PRESETS {
                let file = name.to_ascii_lowercase();
                let path = format!("bevy_grass/styles/{file}.grass.ron");
                let text = style.to_ron();
                registry.insert_asset(PathBuf::new(), Path::new(&path), text.into_bytes());
            }
        }
    }
}

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
    use crate::api::{GrassVariation, GroundTint};

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

    fn preset_path(name: &str) -> String {
        format!(
            "{}/assets/grass_styles/{}.grass.ron",
            env!("CARGO_MANIFEST_DIR"),
            name.to_ascii_lowercase()
        )
    }

    /// The shipped files, which must stay equal to the constants. With
    /// `BLESS_STYLES` set, the file's text is generated from the constant so
    /// a missing file is created rather than failing the test.
    fn shipped(name: &str) -> String {
        if std::env::var_os("BLESS_STYLES").is_some() {
            let style = GrassStyle::preset(name).expect("a preset");
            return format!("{}{}\n", style_doc(name), style.to_ron());
        }
        let file = preset_path(name);
        std::fs::read_to_string(&file).unwrap_or_else(|error| panic!("{file}: {error}"))
    }

    #[test]
    fn every_preset_file_matches_its_constant() {
        // `BLESS_STYLES=1 cargo test` rewrites the files from the constants.
        let bless = std::env::var_os("BLESS_STYLES").is_some();
        for (name, style) in GrassStyle::presets() {
            if bless {
                std::fs::write(preset_path(name), shipped(name)).unwrap();
            }
            let read = GrassStyle::from_ron(&shipped(name)).unwrap();
            assert_eq!(read, style, "{name}");
        }
    }

    #[test]
    fn every_preset_is_shipped_as_an_embedded_file_that_is_the_preset() {
        use bevy::MinimalPlugins;
        use bevy::asset::{AssetPlugin, AssetServer, Assets};

        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<GrassStyle>()
            .add_plugins(GrassStyleFilePlugin);
        let handles: Vec<_> = {
            let server = app.world().resource::<AssetServer>();
            GrassStyle::presets()
                .map(|(name, _)| {
                    let path = format!(
                        "embedded://bevy_grass/styles/{}.grass.ron",
                        name.to_ascii_lowercase()
                    );
                    (name, server.load::<GrassStyle>(path))
                })
                .collect()
        };
        for _ in 0..500 {
            app.update();
            let styles = app.world().resource::<Assets<GrassStyle>>();
            if handles
                .iter()
                .all(|(_, handle)| styles.get(handle).is_some())
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        let styles = app.world().resource::<Assets<GrassStyle>>();
        for (name, style) in GrassStyle::presets() {
            let handle = handles
                .iter()
                .find(|(preset, _)| *preset == name)
                .unwrap_or_else(|| panic!("{name} is not embedded"))
                .1
                .clone();
            let loaded = styles
                .get(&handle)
                .unwrap_or_else(|| panic!("{name} did not load"));
            assert_eq!(*loaded, style, "{name}");
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
    fn colours_can_be_hex_as_artists_copy_them() {
        let style = GrassStyle::from_ron(
            r##"(base: "MEADOW", tip_color: "#5c8f24", root_color: (0.1, 0.2, 0.3),
                 species: [(share: 0.1, style: (head_color: "#ff8000"))])"##,
        )
        .unwrap();
        let (r, g, b) = srgb(style.blade.tip_color);
        let near = |a: f32, b: u8| (a - f32::from(b) / 255.0).abs() < 1e-5;
        assert!(
            near(r, 0x5c) && near(g, 0x8f) && near(b, 0x24),
            "{r} {g} {b}"
        );
        assert_eq!(
            srgb(style.blade.root_color),
            (0.1, 0.2, 0.3),
            "tuples still work"
        );
        assert!(near(srgb(style.species[0].blade.head_color).0, 0xff));

        let Err(GrassStyleFileError::Parse(error)) =
            GrassStyle::from_ron(r##"(tip_color: "#5c8f2")"##)
        else {
            panic!("a bad hex colour must be an error");
        };
        assert!(error.to_string().contains("hex colour"), "{error}");
    }

    #[test]
    fn the_names_before_the_rename_still_load() {
        let old = GrassStyle::from_ron(
            "(base: \"MEADOW\", dead_blades: 0.1, bare_patches: 0.2, tuft_size: 0.9,
              species: [(share: 0.1, style: (head: 0.3))])",
        )
        .unwrap();
        let new = GrassStyle::from_ron(
            "(base: \"MEADOW\", dead_share: 0.1, bare_share: 0.2, tuft_spacing: 0.9,
              species: [(share: 0.1, style: (head_length: 0.3))])",
        )
        .unwrap();
        assert_eq!(old, new);
        assert_eq!(old.variation.dead_share, 0.1);
        assert_eq!(old.species[0].blade.head_length, 0.3);
        // Written with the new names only.
        let text = old.to_ron();
        assert!(text.contains("dead_share") && !text.contains("dead_blades"));
    }

    #[test]
    fn the_format_holds_every_setting_of_the_style() {
        // The file is written from the type's fields, so a setting added to
        // `GrassBlade` or `GrassStyle` and left out of the `style_file!` list
        // shows up here: the style is changed in every setting, written,
        // read, and must come back equal.
        let mut style = GrassStyle::SUMMER_WILD;
        style.blade = GrassBlade {
            height: Vec2::new(0.11, 0.22),
            width: 0.011,
            lean: 0.12,
            stiffness: 0.13,
            root_color: Color::srgb(0.1, 0.2, 0.3),
            tip_color: Color::srgb(0.4, 0.5, 0.6),
            root_occlusion: 0.14,
            translucency: 0.15,
            specular: 0.16,
            twist: 0.17,
            droop: 0.18,
            fold: 0.19,
            head_length: 0.21,
            head_width: 0.023,
            head_color: Color::srgb(0.7, 0.8, 0.9),
        };
        style.density = 12.0;
        style.toon_bands = 3;
        style.ground_tint = GroundTint {
            color: Color::srgb(0.9, 0.8, 0.7),
            amount: 0.31,
        };
        style.variation = GrassVariation {
            patch_color: Color::srgb(0.3, 0.2, 0.1),
            patch_amount: 0.32,
            patch_size: 7.0,
            dry_tips: 0.33,
            dead_share: 0.34,
            tufts: 0.35,
            tuft_spacing: 0.36,
            bare_share: 0.37,
            bare_patch_size: 5.0,
        };
        assert_eq!(GrassStyle::from_ron(&style.to_ron()).unwrap(), style);
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
            GrassStyle::from_ron("(species: [(share: 0.1, style: (head_length: 0.95))])"),
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
