//! The look of the grass: [`GrassStyle`] and the pieces it is made of.
//!
//! A style is split the way the settings are used. A [`GrassBlade`] is what
//! one plant looks like (size, colour, shape); a [`GrassStyle`] is a blade plus
//! what belongs to the whole field (density, colour variation across it, the
//! ground it grows from, toon shading) and the other plants among it. A
//! [`GrassSpecies`] is a blade and its share, so a field setting cannot be
//! put where it would do nothing.

use std::borrow::Cow;

use bevy::asset::Asset;
use bevy::color::Color;
use bevy::math::Vec2;
use bevy::reflect::Reflect;
use bevy::reflect::std_traits::ReflectDefault;

/// Most species one field mixes, its style's own blades included.
pub(crate) const MAX_SPECIES: usize = 16;

/// What one kind of plant looks like: how tall and wide its blades are, how
/// they are coloured, move and are shaped. A [`GrassStyle`] has one for its own
/// grass, and each [`GrassSpecies`] one for its plant.
///
/// Every setting at 0 in the shape group is the plain blade. The presets are
/// the `blade` of the [`GrassStyle`] presets (`GrassStyle::MEADOW.blade`), and
/// of the plants in [`GrassBlade::SEED_HEADS`] and the others.
#[derive(Debug, Clone, Copy, PartialEq, Reflect)]
#[reflect(Default, Debug, PartialEq)]
pub struct GrassBlade {
    /// Shortest and tallest blade, in metres.
    pub height: Vec2,
    /// Blade width at the root, in metres. Each blade varies ±25% around it.
    pub width: f32,
    /// How far the tip leans at rest, as a fraction of the height.
    pub lean: f32,
    /// Resistance to wind, 0 (soft) to 1 (stiff).
    pub stiffness: f32,
    /// Colour at the root.
    pub root_color: Color,
    /// Colour at the tip.
    pub tip_color: Color,
    /// How much darker the roots are, 0 (none) to 1 (black), standing in for
    /// the light the surrounding blades block.
    pub root_occlusion: f32,
    /// How much sunlight shows through a blade lit from behind, 0 (none) to
    /// 1 (as bright as its sunlit side). Grass seen against the sun glows
    /// instead of going black.
    pub translucency: f32,
    /// Rough specular on the upper half of the blade, 0 (matte) to 1, so
    /// wind waves show as a moving sheen.
    pub specular: f32,

    // The blade's shape. Every setting at 0 is the plain blade.
    /// How far blades twist along their length, in turns at the tip. Each
    /// blade twists its own way, by up to this much.
    pub twist: f32,
    /// How far blades arch over and hang, 0 (upright) to 1 (the tip hangs
    /// near the ground). Each blade droops between none and this much, tall
    /// ones the most.
    pub droop: f32,
    /// How far each blade is folded along its midrib, 0 (open and rounded)
    /// to 1 (a V): a folded blade is narrower face-on, and its two halves
    /// catch the light differently.
    pub fold: f32,
    /// A head at the top of each blade, a seed head or a flower, as a share
    /// of the blade's length; 0 for none. The head turns toward the camera.
    pub head_length: f32,
    /// Width of the head, in metres.
    pub head_width: f32,
    /// Colour of the head.
    pub head_color: Color,
}

/// How far the ground shows through at the foot of the grass.
#[derive(Debug, Clone, Copy, PartialEq, Reflect)]
#[reflect(Default, Debug, PartialEq)]
pub struct GroundTint {
    /// The colour of the ground the field grows on.
    pub color: Color,
    /// How far the lowest third of each blade blends toward `color`, 0 (not
    /// at all) to 1, so the field sits in the terrain instead of on it.
    pub amount: f32,
}

/// How a field varies from place to place, the way real fields do. Every
/// amount at 0 is an even field, and costs nothing.
#[derive(Debug, Clone, Copy, PartialEq, Reflect)]
#[reflect(Default, Debug, PartialEq)]
pub struct GrassVariation {
    /// The colour the blades drift toward in large patches, as the tip
    /// colour; the rest of each blade shifts with it. Real fields are greener
    /// in hollows and yellower on dry rises.
    pub patch_color: Color,
    /// How far blades in a patch take on `patch_color`, 0 (no patches) to 1.
    pub patch_amount: f32,
    /// Size of the colour patches, in metres.
    pub patch_size: f32,
    /// How far the upper part of blades dries toward straw, 0 (not at all)
    /// to 1; some blades dry more than others.
    pub dry_tips: f32,
    /// Share of blades that are dead and straw-coloured, 0 to 1.
    pub dead_share: f32,
    /// How strongly blades gather into tufts, 0 (spread evenly) to 1
    /// (tight tufts with bare ground between them). Tufted blades also lean
    /// out from the tuft's centre. The number of blades does not change.
    pub tufts: f32,
    /// Distance between tufts, in metres.
    pub tuft_spacing: f32,
    /// Share of the ground left bare, in patches, 0 to 1. The grass thins
    /// out toward a patch's edge rather than stopping at a line.
    pub bare_share: f32,
    /// Size of the bare patches, in metres.
    pub bare_patch_size: f32,
}

/// The look of a field of grass. An asset, so fields can share one and it can
/// be edited at runtime.
///
/// The grass itself is the [`blade`](GrassStyle::blade); everything else
/// belongs to the whole field. Start from a preset and change what you want:
///
/// ```
/// # use bevy::prelude::*;
/// # use bevy_grass::prelude::*;
/// let dry_meadow = GrassStyle {
///     blade: GrassBlade {
///         tip_color: Color::srgb(0.62, 0.58, 0.30),
///         ..GrassStyle::MEADOW.blade
///     },
///     density: 45.0,
///     ..GrassStyle::MEADOW
/// };
/// assert_eq!(dry_meadow.blade.height, GrassStyle::MEADOW.blade.height);
/// ```
#[derive(Asset, Debug, Clone, PartialEq, Reflect)]
#[reflect(Default, Debug, PartialEq)]
pub struct GrassStyle {
    /// What the style's own blades look like.
    pub blade: GrassBlade,
    /// Blades per square metre at full density. Blades are spread evenly
    /// (blue noise), so this also sets the distance between neighbouring
    /// blades: about `1 / sqrt(density)` metres. [`GrassStyle::spacing`] and
    /// [`GrassStyle::with_spacing`] work in that distance instead.
    pub density: f32,
    /// How the field varies across its area.
    pub variation: GrassVariation,
    /// The ground the field grows on, and how far it shows through.
    pub ground_tint: GroundTint,
    /// Flat toon shading with this many bands of light; 0 shades physically,
    /// like the rest of the scene. Forward rendering only: a deferred view
    /// shades every style physically.
    pub toon_bands: u32,
    /// Other plants growing among this style's blades: other grasses, seed
    /// heads, flowers, clover. Each takes its share of the blades; the
    /// style's own blades are the rest. Empty for one species.
    ///
    /// Build a list with [`GrassStyle::with_species`], or in a constant with
    /// `Cow::Borrowed(&[...])`. A field mixes at most 16 species, its own
    /// included.
    pub species: Cow<'static, [GrassSpecies]>,
}

/// A plant growing among a style's own blades. See [`GrassStyle::species`].
///
/// A species is a [`GrassBlade`] and a share: the density, the ground and
/// the variation belong to the field's style, so there is no way to set them
/// here and have them ignored.
#[derive(Debug, Clone, PartialEq, Reflect)]
#[reflect(Debug, PartialEq)]
pub struct GrassSpecies {
    /// Share of the field's blades, 0 to 1. When the shares of a style's
    /// species add up to more than 1, they are scaled down to 1 and the
    /// style's own blades disappear.
    pub share: f32,
    /// How much the species grows in patches about a metre across instead
    /// of scattered, 0 (scattered) to 1 (only in patches). Its share stays
    /// the same.
    pub clustering: f32,
    /// What its blades look like.
    pub blade: GrassBlade,
}

impl GrassSpecies {
    /// `share` of the blades look like `blade`, scattered.
    #[must_use]
    pub const fn new(blade: GrassBlade, share: f32) -> Self {
        Self {
            share,
            clustering: 0.0,
            blade,
        }
    }

    /// The same species, growing in patches (`clustering`, 0 to 1).
    #[must_use]
    pub fn clustered(self, clustering: f32) -> Self {
        Self { clustering, ..self }
    }
}

impl GrassStyle {
    /// Average distance between neighbouring blades at full density, in
    /// metres: `1 / sqrt(density)`.
    #[must_use]
    pub fn spacing(&self) -> f32 {
        1.0 / self.density.max(f32::MIN_POSITIVE).sqrt()
    }

    /// The same style with blades about `metres` apart: sets the density to
    /// `1 / metres²`.
    #[must_use]
    pub fn with_spacing(self, metres: f32) -> Self {
        Self {
            density: 1.0 / (metres * metres).max(f32::MIN_POSITIVE),
            ..self
        }
    }

    /// The same style with `blades_per_m2` blades per square metre.
    #[must_use]
    pub fn with_density(self, blades_per_m2: f32) -> Self {
        Self {
            density: blades_per_m2.max(0.0),
            ..self
        }
    }

    /// The same style with `species` growing among its blades.
    #[must_use]
    pub fn with_species(self, species: impl Into<Cow<'static, [GrassSpecies]>>) -> Self {
        Self {
            species: species.into(),
            ..self
        }
    }
}

impl Default for GrassStyle {
    fn default() -> Self {
        Self::MEADOW
    }
}

impl Default for GrassBlade {
    fn default() -> Self {
        GrassStyle::MEADOW.blade
    }
}

impl Default for GroundTint {
    fn default() -> Self {
        GrassStyle::MEADOW.ground_tint
    }
}

impl Default for GrassVariation {
    fn default() -> Self {
        GrassStyle::MEADOW.variation
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spacing_and_density_describe_the_same_field() {
        let style = GrassStyle::SUMMER_TALL.with_spacing(0.2);
        assert!((style.density - 25.0).abs() < 1e-3);
        assert!((style.spacing() - 0.2).abs() < 1e-6);
        let style = style.with_density(100.0);
        assert!((style.spacing() - 0.1).abs() < 1e-6);
    }

    #[test]
    fn every_theme_preset_is_tall_and_valid() {
        for style in GrassStyle::SUMMER.iter().chain(&GrassStyle::AUTUMN) {
            let blade = &style.blade;
            assert!(blade.height.x > 0.0 && blade.height.x < blade.height.y);
            assert!(blade.height.y >= 1.0, "tall grass reaches at least 1 m");
            assert!(style.density > 0.0 && blade.width > 0.0);
            assert!((0.0..=1.0).contains(&blade.translucency));
        }
    }

    #[test]
    fn summer_is_green_and_autumn_is_brown() {
        let rgb = |style: &GrassStyle| style.blade.tip_color.to_srgba();
        for style in &GrassStyle::SUMMER[..3] {
            let tip = rgb(style);
            assert!(tip.green > tip.red && tip.green > tip.blue, "{tip:?}");
        }
        for style in &GrassStyle::AUTUMN {
            let tip = rgb(style);
            assert!(tip.red > tip.green && tip.green > tip.blue, "{tip:?}");
        }
    }

    #[test]
    fn a_style_is_a_blade_and_what_belongs_to_the_field() {
        // Struct update reaches one level into the blade, in a constant too.
        const DRY: GrassStyle = GrassStyle {
            blade: GrassBlade {
                tip_color: Color::srgb(0.7, 0.6, 0.3),
                ..GrassStyle::MEADOW.blade
            },
            density: 10.0,
            ..GrassStyle::MEADOW
        };
        assert_eq!(DRY.blade.height, GrassStyle::MEADOW.blade.height);
        assert_ne!(DRY.blade.tip_color, GrassStyle::MEADOW.blade.tip_color);
        assert_eq!(DRY.variation, GrassStyle::MEADOW.variation);
        assert_eq!(DRY.ground_tint, GrassStyle::MEADOW.ground_tint);
        assert_ne!(DRY, GrassStyle::MEADOW, "styles compare");

        // The defaults are the meadow's.
        assert_eq!(GrassStyle::default(), GrassStyle::MEADOW);
        assert_eq!(GrassBlade::default(), GrassStyle::MEADOW.blade);
        assert_eq!(GrassVariation::default(), GrassStyle::MEADOW.variation);
        assert_eq!(GroundTint::default(), GrassStyle::MEADOW.ground_tint);
    }

    #[test]
    fn the_forest_presets_are_short_wide_and_mixed() {
        // The group holds each forest style once, in order.
        let listed = [
            (GrassStyle::FOREST_MOSS, "FOREST_MOSS"),
            (GrassStyle::FOREST_FERNS, "FOREST_FERNS"),
            (GrassStyle::FOREST_FLOOR, "FOREST_FLOOR"),
            (GrassStyle::FOREST_BRACKEN, "FOREST_BRACKEN"),
        ];
        for ((style, name), group) in listed.iter().zip(&GrassStyle::FOREST) {
            assert_eq!(style, group, "{name}");
            assert!(style.density > 0.0, "{name}");
            assert!(style.blade.height.y >= style.blade.height.x, "{name}");
            assert!(style.blade.width > 0.02, "{name}: forest plants are wide");
            let share: f32 = style.species.iter().map(|species| species.share).sum();
            assert!(share <= 1.0, "{name}: the species leave some blades");
        }

        // Moss is a carpet: very short, wide, dense, and only moss.
        let moss = GrassStyle::FOREST_MOSS;
        assert!(moss.blade.height.y <= 0.1, "moss stays low");
        assert!(
            moss.blade.width > moss.blade.height.y * 0.3,
            "moss is broad"
        );
        assert!(moss.density > GrassStyle::LAWN.density * 0.5);
        assert!(moss.species.is_empty());

        // The fern styles have fern blades among their species, or as their
        // own blades, and the whole-floor mix has every floor plant.
        let is_fern = |blade: &GrassBlade| blade.droop >= 0.6 && blade.fold >= 0.5;
        assert!(is_fern(&GrassStyle::FOREST_FERNS.blade));
        assert!(
            GrassStyle::FOREST_FERNS
                .species
                .iter()
                .any(|species| species.clustering >= 0.7)
        );
        assert!(
            GrassStyle::FOREST_FLOOR
                .species
                .iter()
                .any(|species| species.blade == GrassBlade::FERN)
        );
        assert!(
            GrassStyle::FOREST_FLOOR
                .species
                .iter()
                .any(|species| species.blade == GrassBlade::LEAF_LITTER)
        );
        assert!(is_fern(&GrassStyle::FOREST_BRACKEN.blade));
        let bracken = GrassStyle::FOREST_BRACKEN.blade.tip_color.to_srgba();
        assert!(bracken.red > bracken.green && bracken.green > bracken.blue);
    }

    #[test]
    fn a_plant_is_a_blade_and_a_style_of_its_own() {
        let plants = [
            (GrassStyle::SEED_HEADS, GrassBlade::SEED_HEADS),
            (GrassStyle::WHITE_FLOWERS, GrassBlade::WHITE_FLOWERS),
            (GrassStyle::YELLOW_FLOWERS, GrassBlade::YELLOW_FLOWERS),
            (GrassStyle::CLOVER, GrassBlade::CLOVER),
            (GrassStyle::BROAD_LEAF, GrassBlade::BROAD_LEAF),
        ];
        for ((style, blade), listed) in plants.iter().zip(&GrassStyle::PLANTS) {
            assert_eq!(&style.blade, blade);
            assert_eq!(style, listed);
            assert!(style.species.is_empty(), "a plant is not a mix");
        }
        // The wild styles mix them by blade.
        let wild = &GrassStyle::SUMMER_WILD.species;
        assert_eq!(wild[0].blade, GrassBlade::SEED_HEADS);
        assert_eq!(wild[0].share, 0.10);
        let species = GrassSpecies::new(GrassBlade::CLOVER, 0.2).clustered(0.5);
        assert_eq!((species.share, species.clustering), (0.2, 0.5));
        assert_eq!(species.blade, GrassBlade::CLOVER);
    }
}
