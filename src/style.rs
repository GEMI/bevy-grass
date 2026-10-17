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
    pub head: f32,
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
    pub dead_blades: f32,
    /// How strongly blades gather into tufts, 0 (spread evenly) to 1
    /// (tight tufts with bare ground between them). Tufted blades also lean
    /// out from the tuft's centre. The number of blades does not change.
    pub tufts: f32,
    /// Distance between tufts, in metres.
    pub tuft_size: f32,
    /// Share of the ground left bare, in patches, 0 to 1. The grass thins
    /// out toward a patch's edge rather than stopping at a line.
    pub bare_patches: f32,
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
    /// Fresh green, soft, mixed heights. The default for temperate worlds.
    pub const MEADOW: Self = Self {
        blade: GrassBlade {
            height: Vec2::new(0.3, 0.6),
            width: 0.02,
            lean: 0.15,
            stiffness: 0.3,
            root_color: Color::srgb(0.06, 0.13, 0.03),
            tip_color: Color::srgb(0.38, 0.55, 0.16),
            root_occlusion: 0.6,
            translucency: 0.6,
            specular: 0.3,
            twist: 0.0,
            droop: 0.0,
            fold: 0.0,
            head: 0.0,
            head_width: 0.03,
            head_color: Color::srgb(0.85, 0.80, 0.60),
        },
        density: 60.0,
        variation: GrassVariation {
            patch_color: Color::srgb(0.55, 0.55, 0.22),
            patch_amount: 0.0,
            patch_size: 12.0,
            dry_tips: 0.0,
            dead_blades: 0.0,
            tufts: 0.0,
            tuft_size: 0.6,
            bare_patches: 0.0,
            bare_patch_size: 3.0,
        },
        ground_tint: GroundTint {
            color: Color::srgb(0.20, 0.15, 0.09),
            amount: 0.25,
        },
        toon_bands: 0,
        species: Cow::Borrowed(&[]),
    };

    /// Deep green to straw tips, loose and flowing. For hiding in grass and
    /// dramatic wind.
    pub const TALL_PRAIRIE: Self = Self {
        blade: GrassBlade {
            height: Vec2::new(0.8, 1.4),
            width: 0.025,
            lean: 0.25,
            stiffness: 0.15,
            root_color: Color::srgb(0.05, 0.10, 0.03),
            tip_color: Color::srgb(0.62, 0.58, 0.30),
            root_occlusion: 0.7,
            translucency: 0.5,
            specular: 0.3,
            twist: 0.0,
            droop: 0.0,
            fold: 0.0,
            head: 0.0,
            head_width: 0.03,
            head_color: Color::srgb(0.85, 0.80, 0.60),
        },
        density: 40.0,
        variation: GrassVariation {
            patch_color: Color::srgb(0.55, 0.55, 0.22),
            patch_amount: 0.0,
            patch_size: 12.0,
            dry_tips: 0.0,
            dead_blades: 0.0,
            tufts: 0.0,
            tuft_size: 0.6,
            bare_patches: 0.0,
            bare_patch_size: 3.0,
        },
        ground_tint: GroundTint {
            color: Color::srgb(0.20, 0.15, 0.09),
            amount: 0.2,
        },
        toon_bands: 0,
        species: Cow::Borrowed(&[]),
    };

    /// Golden, dense, stiff stems with a sheen. Farmland.
    pub const WHEAT: Self = Self {
        blade: GrassBlade {
            height: Vec2::new(0.9, 1.2),
            width: 0.015,
            lean: 0.08,
            stiffness: 0.6,
            root_color: Color::srgb(0.30, 0.27, 0.11),
            tip_color: Color::srgb(0.86, 0.70, 0.32),
            root_occlusion: 0.5,
            translucency: 0.5,
            specular: 0.5,
            ..Self::MEADOW.blade
        },
        density: 90.0,
        ground_tint: GroundTint {
            amount: 0.15,
            ..Self::MEADOW.ground_tint
        },
        ..Self::MEADOW
    };

    /// Yellow-brown, sparse and stiff. Arid and late-summer biomes.
    pub const DRY_SAVANNA: Self = Self {
        blade: GrassBlade {
            height: Vec2::new(0.5, 1.0),
            width: 0.02,
            lean: 0.2,
            stiffness: 0.6,
            root_color: Color::srgb(0.25, 0.20, 0.10),
            tip_color: Color::srgb(0.74, 0.62, 0.36),
            root_occlusion: 0.5,
            translucency: 0.35,
            specular: 0.2,
            ..Self::MEADOW.blade
        },
        density: 25.0,
        ground_tint: GroundTint {
            color: Color::srgb(0.42, 0.33, 0.20),
            amount: 0.3,
        },
        ..Self::MEADOW
    };

    /// Uniform, short and dense. Gardens, towns, sports fields.
    pub const LAWN: Self = Self {
        blade: GrassBlade {
            height: Vec2::new(0.05, 0.12),
            width: 0.008,
            lean: 0.1,
            stiffness: 0.7,
            root_color: Color::srgb(0.04, 0.10, 0.03),
            tip_color: Color::srgb(0.22, 0.42, 0.10),
            root_occlusion: 0.4,
            translucency: 0.4,
            specular: 0.2,
            ..Self::MEADOW.blade
        },
        density: 250.0,
        ground_tint: GroundTint {
            amount: 0.0,
            ..Self::MEADOW.ground_tint
        },
        ..Self::MEADOW
    };

    /// Flat toon bands, wide blades and no specular, for non-photoreal art.
    pub const STYLIZED: Self = Self {
        blade: GrassBlade {
            height: Vec2::new(0.4, 0.8),
            width: 0.06,
            lean: 0.2,
            stiffness: 0.3,
            root_color: Color::srgb(0.10, 0.30, 0.12),
            tip_color: Color::srgb(0.45, 0.75, 0.30),
            root_occlusion: 0.5,
            translucency: 0.3,
            specular: 0.0,
            ..Self::MEADOW.blade
        },
        density: 30.0,
        ground_tint: GroundTint {
            amount: 0.0,
            ..Self::MEADOW.ground_tint
        },
        toon_bands: 3,
        ..Self::MEADOW
    };

    /// The six general-purpose presets, from the architecture plan.
    pub const GENERAL: [Self; 6] = [
        Self::MEADOW,
        Self::TALL_PRAIRIE,
        Self::WHEAT,
        Self::DRY_SAVANNA,
        Self::LAWN,
        Self::STYLIZED,
    ];

    // Tall summer and autumn grass. Each theme has a main look and variants;
    // all of them share the blade shape (4 cm wide, 40 per m², about 16 cm
    // apart), so they can be mixed in one scene and retuned together with
    // `with_density`, `with_spacing` or plain field updates.

    /// Tall green summer grass: the main summer look.
    pub const SUMMER_TALL: Self = Self {
        blade: GrassBlade {
            height: Vec2::new(0.7, 1.3),
            width: 0.04,
            lean: 0.2,
            stiffness: 0.25,
            root_color: Color::srgb(0.05, 0.11, 0.03),
            tip_color: Color::srgb(0.34, 0.56, 0.14),
            root_occlusion: 0.65,
            translucency: 0.6,
            // The themes were approved without sheen or ground tint; a game can
            // add either.
            specular: 0.0,
            twist: 0.0,
            droop: 0.0,
            fold: 0.0,
            head: 0.0,
            head_width: 0.03,
            head_color: Color::srgb(0.85, 0.80, 0.60),
        },
        density: 40.0,
        variation: GrassVariation {
            patch_color: Color::srgb(0.55, 0.55, 0.22),
            patch_amount: 0.0,
            patch_size: 12.0,
            dry_tips: 0.0,
            dead_blades: 0.0,
            tufts: 0.0,
            tuft_size: 0.6,
            bare_patches: 0.0,
            bare_patch_size: 3.0,
        },
        ground_tint: GroundTint {
            color: Color::srgb(0.20, 0.15, 0.09),
            amount: 0.0,
        },
        toon_bands: 0,
        species: Cow::Borrowed(&[]),
    };

    /// Summer variant: deep green, denser and taller, as along a ditch or
    /// a stream.
    pub const SUMMER_LUSH: Self = Self {
        blade: GrassBlade {
            height: Vec2::new(0.9, 1.5),
            lean: 0.15,
            root_color: Color::srgb(0.03, 0.08, 0.02),
            tip_color: Color::srgb(0.22, 0.44, 0.10),
            root_occlusion: 0.7,
            ..Self::SUMMER_TALL.blade
        },
        density: 50.0,
        ..Self::SUMMER_TALL
    };

    /// Summer variant: bright yellow-green and the shortest of the summer
    /// set, a sunny meadow.
    pub const SUMMER_MEADOW: Self = Self {
        blade: GrassBlade {
            height: Vec2::new(0.5, 1.0),
            lean: 0.18,
            stiffness: 0.3,
            root_color: Color::srgb(0.07, 0.13, 0.03),
            tip_color: Color::srgb(0.48, 0.62, 0.18),
            ..Self::SUMMER_TALL.blade
        },
        ..Self::SUMMER_TALL
    };

    /// Summer variant: high summer gone to seed, green stems with pale,
    /// sun-bleached tips.
    pub const SUMMER_SEEDING: Self = Self {
        blade: GrassBlade {
            height: Vec2::new(0.8, 1.4),
            lean: 0.28,
            stiffness: 0.18,
            root_color: Color::srgb(0.06, 0.12, 0.03),
            tip_color: Color::srgb(0.66, 0.64, 0.38),
            ..Self::SUMMER_TALL.blade
        },
        ..Self::SUMMER_TALL
    };

    /// Tall brown autumn grass: the main autumn look. Drier, so stiffer and
    /// less translucent than summer grass.
    pub const AUTUMN_TALL: Self = Self {
        blade: GrassBlade {
            height: Vec2::new(0.7, 1.3),
            width: 0.04,
            lean: 0.25,
            stiffness: 0.35,
            root_color: Color::srgb(0.13, 0.09, 0.04),
            tip_color: Color::srgb(0.52, 0.36, 0.17),
            root_occlusion: 0.65,
            translucency: 0.4,
            specular: 0.0,
            twist: 0.0,
            droop: 0.0,
            fold: 0.0,
            head: 0.0,
            head_width: 0.03,
            head_color: Color::srgb(0.85, 0.80, 0.60),
        },
        density: 40.0,
        variation: GrassVariation {
            patch_color: Color::srgb(0.55, 0.55, 0.22),
            patch_amount: 0.0,
            patch_size: 12.0,
            dry_tips: 0.0,
            dead_blades: 0.0,
            tufts: 0.0,
            tuft_size: 0.6,
            bare_patches: 0.0,
            bare_patch_size: 3.0,
        },
        ground_tint: GroundTint {
            color: Color::srgb(0.20, 0.15, 0.09),
            amount: 0.0,
        },
        toon_bands: 0,
        species: Cow::Borrowed(&[]),
    };

    /// Autumn variant: rust-red, like dying bracken.
    pub const AUTUMN_RUST: Self = Self {
        blade: GrassBlade {
            root_color: Color::srgb(0.16, 0.07, 0.03),
            tip_color: Color::srgb(0.54, 0.29, 0.13),
            ..Self::AUTUMN_TALL.blade
        },
        ..Self::AUTUMN_TALL
    };

    /// Autumn variant: pale dry straw, a field nobody cut.
    pub const AUTUMN_STRAW: Self = Self {
        blade: GrassBlade {
            height: Vec2::new(0.6, 1.1),
            stiffness: 0.45,
            root_color: Color::srgb(0.24, 0.19, 0.10),
            tip_color: Color::srgb(0.72, 0.62, 0.38),
            translucency: 0.5,
            ..Self::AUTUMN_TALL.blade
        },
        ..Self::AUTUMN_TALL
    };

    /// Autumn variant: late autumn, grey-brown and beaten down by rain.
    pub const AUTUMN_FADED: Self = Self {
        blade: GrassBlade {
            height: Vec2::new(0.5, 1.1),
            lean: 0.4,
            stiffness: 0.15,
            root_color: Color::srgb(0.11, 0.10, 0.07),
            tip_color: Color::srgb(0.44, 0.39, 0.30),
            translucency: 0.3,
            ..Self::AUTUMN_TALL.blade
        },
        ..Self::AUTUMN_TALL
    };

    /// The summer theme: the main look first, then its variants.
    pub const SUMMER: [Self; 4] = [
        Self::SUMMER_TALL,
        Self::SUMMER_LUSH,
        Self::SUMMER_MEADOW,
        Self::SUMMER_SEEDING,
    ];

    /// The autumn theme: the main look first, then its variants.
    pub const AUTUMN: [Self; 4] = [
        Self::AUTUMN_TALL,
        Self::AUTUMN_RUST,
        Self::AUTUMN_STRAW,
        Self::AUTUMN_FADED,
    ];

    // Plants to mix into a field as species (`GrassSpecies`). Each is also a
    // style of its own: a field of nothing but clover works.

    /// Thin stems with pale seed heads, like timothy or foxtail.
    pub const SEED_HEADS: Self = Self {
        blade: GrassBlade::SEED_HEADS,
        ..Self::SUMMER_TALL
    };

    /// White flowers on slender stems, like oxeye daisies.
    pub const WHITE_FLOWERS: Self = Self {
        blade: GrassBlade::WHITE_FLOWERS,
        density: 30.0,
        ..Self::SUMMER_TALL
    };

    /// Small yellow flowers, like buttercups.
    pub const YELLOW_FLOWERS: Self = Self {
        blade: GrassBlade::YELLOW_FLOWERS,
        ..Self::WHITE_FLOWERS
    };

    /// Low, broad, dark leaves lying over, like clover.
    pub const CLOVER: Self = Self {
        blade: GrassBlade::CLOVER,
        density: 120.0,
        ..Self::SUMMER_TALL
    };

    /// Wide, folded leaves that arch over, like plantain or dock.
    pub const BROAD_LEAF: Self = Self {
        blade: GrassBlade::BROAD_LEAF,
        density: 30.0,
        ..Self::SUMMER_TALL
    };

    /// The plants to mix in as species.
    pub const PLANTS: [Self; 5] = [
        Self::SEED_HEADS,
        Self::WHITE_FLOWERS,
        Self::YELLOW_FLOWERS,
        Self::CLOVER,
        Self::BROAD_LEAF,
    ];

    // Wild styles: a theme's grass with every kind of variation, and other
    // plants among it.

    /// Summer grass gone wild: tufts and bare spots, blades that twist and
    /// droop, yellower patches, dry tips, seed heads, broad leaves and a few
    /// flowers.
    pub const SUMMER_WILD: Self = Self {
        blade: GrassBlade {
            twist: 0.15,
            droop: 0.25,
            fold: 0.3,
            ..Self::SUMMER_TALL.blade
        },
        variation: GrassVariation {
            patch_color: Color::srgb(0.56, 0.58, 0.22),
            patch_amount: 0.45,
            dry_tips: 0.2,
            dead_blades: 0.04,
            tufts: 0.35,
            bare_patches: 0.06,
            ..Self::SUMMER_TALL.variation
        },
        species: Cow::Borrowed(&[
            GrassSpecies {
                share: 0.10,
                clustering: 0.3,
                blade: GrassBlade::SEED_HEADS,
            },
            GrassSpecies {
                share: 0.06,
                clustering: 0.6,
                blade: GrassBlade::BROAD_LEAF,
            },
            GrassSpecies {
                share: 0.03,
                clustering: 0.8,
                blade: GrassBlade::WHITE_FLOWERS,
            },
            GrassSpecies {
                share: 0.02,
                clustering: 0.8,
                blade: GrassBlade::YELLOW_FLOWERS,
            },
        ]),
        ..Self::SUMMER_TALL
    };

    /// Autumn grass gone wild: drier, more dead blades and beaten-down
    /// tufts, rusty patches, and seed heads and leaves gone brown.
    pub const AUTUMN_WILD: Self = Self {
        blade: GrassBlade {
            twist: 0.15,
            droop: 0.35,
            fold: 0.3,
            ..Self::AUTUMN_TALL.blade
        },
        variation: GrassVariation {
            patch_color: Color::srgb(0.50, 0.30, 0.14),
            patch_amount: 0.4,
            dry_tips: 0.35,
            dead_blades: 0.12,
            tufts: 0.35,
            bare_patches: 0.08,
            ..Self::AUTUMN_TALL.variation
        },
        species: Cow::Borrowed(&[
            GrassSpecies {
                share: 0.12,
                clustering: 0.3,
                blade: GrassBlade {
                    root_color: Color::srgb(0.16, 0.12, 0.06),
                    tip_color: Color::srgb(0.50, 0.40, 0.22),
                    head_color: Color::srgb(0.48, 0.36, 0.20),
                    translucency: 0.4,
                    ..GrassBlade::SEED_HEADS
                },
            },
            GrassSpecies {
                share: 0.06,
                clustering: 0.6,
                blade: GrassBlade {
                    root_color: Color::srgb(0.12, 0.07, 0.03),
                    tip_color: Color::srgb(0.42, 0.24, 0.10),
                    translucency: 0.3,
                    ..GrassBlade::BROAD_LEAF
                },
            },
        ]),
        ..Self::AUTUMN_TALL
    };

    /// A short meadow full of flowers and clover.
    pub const WILDFLOWER_MEADOW: Self = Self {
        blade: GrassBlade {
            twist: 0.1,
            droop: 0.15,
            fold: 0.2,
            ..Self::MEADOW.blade
        },
        variation: GrassVariation {
            patch_color: Color::srgb(0.30, 0.55, 0.14),
            patch_amount: 0.4,
            dry_tips: 0.1,
            tufts: 0.2,
            ..Self::MEADOW.variation
        },
        species: Cow::Borrowed(&[
            GrassSpecies {
                share: 0.15,
                clustering: 0.7,
                blade: GrassBlade::CLOVER,
            },
            GrassSpecies {
                share: 0.06,
                clustering: 0.6,
                blade: GrassBlade::WHITE_FLOWERS,
            },
            GrassSpecies {
                share: 0.06,
                clustering: 0.6,
                blade: GrassBlade::YELLOW_FLOWERS,
            },
        ]),
        ..Self::MEADOW
    };

    /// The wild styles.
    pub const WILD: [Self; 3] = [
        Self::SUMMER_WILD,
        Self::AUTUMN_WILD,
        Self::WILDFLOWER_MEADOW,
    ];

    /// Average distance between neighbouring blades at full density, in
    /// metres: `1 / sqrt(density)`.
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

// The plants that grow among grass. Each is also a style of its own, with the
// same name on `GrassStyle`: a field of nothing but clover works.
impl GrassBlade {
    /// Thin stems with pale seed heads, like timothy or foxtail.
    pub const SEED_HEADS: Self = Self {
        height: Vec2::new(0.8, 1.4),
        width: 0.012,
        lean: 0.1,
        stiffness: 0.45,
        root_color: Color::srgb(0.07, 0.12, 0.04),
        tip_color: Color::srgb(0.40, 0.50, 0.20),
        translucency: 0.5,
        head: 0.2,
        head_width: 0.022,
        head_color: Color::srgb(0.62, 0.56, 0.36),
        ..GrassStyle::SUMMER_TALL.blade
    };

    /// White flowers on slender stems, like oxeye daisies.
    pub const WHITE_FLOWERS: Self = Self {
        height: Vec2::new(0.35, 0.7),
        width: 0.01,
        lean: 0.08,
        stiffness: 0.5,
        root_color: Color::srgb(0.05, 0.11, 0.03),
        tip_color: Color::srgb(0.24, 0.42, 0.12),
        translucency: 0.4,
        head: 0.12,
        head_width: 0.05,
        head_color: Color::srgb(0.92, 0.92, 0.86),
        ..GrassStyle::SUMMER_TALL.blade
    };

    /// Small yellow flowers, like buttercups.
    pub const YELLOW_FLOWERS: Self = Self {
        height: Vec2::new(0.3, 0.6),
        head: 0.1,
        head_width: 0.035,
        head_color: Color::srgb(0.95, 0.78, 0.12),
        ..GrassBlade::WHITE_FLOWERS
    };

    /// Low, broad, dark leaves lying over, like clover.
    pub const CLOVER: Self = Self {
        height: Vec2::new(0.08, 0.2),
        width: 0.035,
        lean: 0.35,
        stiffness: 0.5,
        root_color: Color::srgb(0.04, 0.10, 0.04),
        tip_color: Color::srgb(0.16, 0.34, 0.12),
        root_occlusion: 0.5,
        translucency: 0.4,
        droop: 0.7,
        ..GrassStyle::SUMMER_TALL.blade
    };

    /// Wide, folded leaves that arch over, like plantain or dock.
    pub const BROAD_LEAF: Self = Self {
        height: Vec2::new(0.2, 0.45),
        width: 0.06,
        lean: 0.3,
        stiffness: 0.4,
        root_color: Color::srgb(0.04, 0.09, 0.03),
        tip_color: Color::srgb(0.20, 0.38, 0.10),
        root_occlusion: 0.55,
        translucency: 0.5,
        twist: 0.1,
        droop: 0.6,
        fold: 0.5,
        ..GrassStyle::SUMMER_TALL.blade
    };
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
