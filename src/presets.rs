//! The preset styles and the one list they come from.
//!
//! To add a preset, add one entry to the block for its group below:
//!
//! ```text
//! MY_STYLE => Self { /* the style */ };
//! ```
//!
//! Then run `BLESS_STYLES=1 cargo test` to write
//! `assets/grass_styles/my_style.grass.ron` from the constant. The file name is
//! the lower-case constant name. Everything else (`PRESETS`, the group arrays
//! and the embedded preset assets) is generated from this list, so no other
//! file needs a style list.

use std::borrow::Cow;

use bevy::color::Color;
use bevy::math::Vec2;

use crate::style::{GrassBlade, GrassSpecies, GrassStyle, GrassVariation, GroundTint};

/// Declares the presets once: their constants, group arrays and `PRESETS`.
///
/// Each entry is `NAME => style;` inside the block of its group. A preset
/// appears exactly once in the list; its group is the block it is in.
macro_rules! preset_styles {
    (
        $(
            $(#[$group_attr:meta])*
            $group:ident {
                $(
                    $(#[$attr:meta])*
                    $name:ident => $value:expr;
                )*
            }
        )*
    ) => {
        impl GrassStyle {
            $(
                $(
                    $(#[$attr])*
                    pub const $name: Self = $value;
                )*
            )*
        }

        impl GrassStyle {
            $(
                $(#[$group_attr])*
                pub const $group: [Self; preset_styles!(@count $($name)*)] = [
                    $( Self::$name, )*
                ];
            )*
        }

        /// Every preset constant and its name, in the order of the groups and
        /// their entries above.
        pub(crate) static PRESETS: &[(&str, GrassStyle)] = &[
            $(
                $(
                    (stringify!($name), GrassStyle::$name),
                )*
            )*
        ];
    };
    (@count) => { 0 };
    (@count $head:ident $($tail:ident)*) => {
        1 + preset_styles!(@count $($tail)*)
    };
}

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
        head_length: 0.2,
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
        head_length: 0.12,
        head_width: 0.05,
        head_color: Color::srgb(0.92, 0.92, 0.86),
        ..GrassStyle::SUMMER_TALL.blade
    };

    /// Small yellow flowers, like buttercups.
    pub const YELLOW_FLOWERS: Self = Self {
        height: Vec2::new(0.3, 0.6),
        head_length: 0.1,
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

    /// Very short, wide and folded: a moss or lichen carpet.
    pub const MOSS: Self = Self {
        height: Vec2::new(0.02, 0.07),
        width: 0.025,
        lean: 0.25,
        stiffness: 0.45,
        root_color: Color::srgb(0.03, 0.07, 0.03),
        tip_color: Color::srgb(0.14, 0.30, 0.13),
        root_occlusion: 0.55,
        translucency: 0.35,
        specular: 0.15,
        droop: 0.45,
        fold: 0.35,
        ..GrassStyle::MEADOW.blade
    };

    /// Arching, strongly folded and twisted: a fern frond, as near as a
    /// single strip of blade gets.
    pub const FERN: Self = Self {
        height: Vec2::new(0.4, 0.9),
        width: 0.05,
        lean: 0.35,
        stiffness: 0.25,
        root_color: Color::srgb(0.05, 0.12, 0.04),
        tip_color: Color::srgb(0.18, 0.40, 0.14),
        root_occlusion: 0.6,
        translucency: 0.45,
        specular: 0.2,
        twist: 0.2,
        droop: 0.7,
        fold: 0.5,
        ..GrassStyle::MEADOW.blade
    };

    /// The fern gone rust: dying bracken.
    pub const BRACKEN: Self = Self {
        height: Vec2::new(0.35, 0.8),
        width: 0.055,
        stiffness: 0.35,
        root_color: Color::srgb(0.18, 0.09, 0.04),
        tip_color: Color::srgb(0.55, 0.30, 0.12),
        translucency: 0.35,
        droop: 0.6,
        ..Self::FERN
    };

    /// Wide, flat, drooping: fallen leaves lying over the ground.
    pub const LEAF_LITTER: Self = Self {
        height: Vec2::new(0.05, 0.15),
        width: 0.05,
        lean: 0.5,
        stiffness: 0.3,
        root_color: Color::srgb(0.12, 0.08, 0.04),
        tip_color: Color::srgb(0.40, 0.28, 0.14),
        root_occlusion: 0.5,
        translucency: 0.25,
        specular: 0.1,
        twist: 0.15,
        droop: 0.7,
        fold: 0.3,
        ..GrassStyle::MEADOW.blade
    };
}

preset_styles! {
    /// The six general-purpose presets.
    GENERAL {
        /// Fresh green, soft, mixed heights. The default for temperate worlds.
        MEADOW => Self {
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
                head_length: 0.0,
                head_width: 0.03,
                head_color: Color::srgb(0.85, 0.80, 0.60),
            },
            density: 60.0,
            variation: GrassVariation {
                patch_color: Color::srgb(0.55, 0.55, 0.22),
                patch_amount: 0.0,
                patch_size: 12.0,
                dry_tips: 0.0,
                dead_share: 0.0,
                tufts: 0.0,
                tuft_spacing: 0.6,
                bare_share: 0.0,
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
        TALL_PRAIRIE => Self {
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
                head_length: 0.0,
                head_width: 0.03,
                head_color: Color::srgb(0.85, 0.80, 0.60),
            },
            density: 40.0,
            variation: GrassVariation {
                patch_color: Color::srgb(0.55, 0.55, 0.22),
                patch_amount: 0.0,
                patch_size: 12.0,
                dry_tips: 0.0,
                dead_share: 0.0,
                tufts: 0.0,
                tuft_spacing: 0.6,
                bare_share: 0.0,
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
        WHEAT => Self {
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
        DRY_SAVANNA => Self {
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
        LAWN => Self {
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
        STYLIZED => Self {
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
    }
    /// The summer theme: the main look first, then its variants.
    SUMMER {
        /// Tall green summer grass: the main summer look.
        SUMMER_TALL => Self {
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
                head_length: 0.0,
                head_width: 0.03,
                head_color: Color::srgb(0.85, 0.80, 0.60),
            },
            density: 40.0,
            variation: GrassVariation {
                patch_color: Color::srgb(0.55, 0.55, 0.22),
                patch_amount: 0.0,
                patch_size: 12.0,
                dry_tips: 0.0,
                dead_share: 0.0,
                tufts: 0.0,
                tuft_spacing: 0.6,
                bare_share: 0.0,
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
        SUMMER_LUSH => Self {
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
        SUMMER_MEADOW => Self {
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
        SUMMER_SEEDING => Self {
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
    }
    /// The autumn theme: the main look first, then its variants.
    AUTUMN {
        /// Tall brown autumn grass: the main autumn look. Drier, so stiffer and
        /// less translucent than summer grass.
        AUTUMN_TALL => Self {
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
                head_length: 0.0,
                head_width: 0.03,
                head_color: Color::srgb(0.85, 0.80, 0.60),
            },
            density: 40.0,
            variation: GrassVariation {
                patch_color: Color::srgb(0.55, 0.55, 0.22),
                patch_amount: 0.0,
                patch_size: 12.0,
                dry_tips: 0.0,
                dead_share: 0.0,
                tufts: 0.0,
                tuft_spacing: 0.6,
                bare_share: 0.0,
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
        AUTUMN_RUST => Self {
            blade: GrassBlade {
                root_color: Color::srgb(0.16, 0.07, 0.03),
                tip_color: Color::srgb(0.54, 0.29, 0.13),
                ..Self::AUTUMN_TALL.blade
            },
            ..Self::AUTUMN_TALL
        };
        /// Autumn variant: pale dry straw, a field nobody cut.
        AUTUMN_STRAW => Self {
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
        AUTUMN_FADED => Self {
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
    }
    /// The wild styles.
    WILD {
        /// Summer grass gone wild: tufts and bare spots, blades that twist and
        /// droop, yellower patches, dry tips, seed heads, broad leaves and a few
        /// flowers.
        SUMMER_WILD => Self {
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
                dead_share: 0.04,
                tufts: 0.35,
                bare_share: 0.06,
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
        AUTUMN_WILD => Self {
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
                dead_share: 0.12,
                tufts: 0.35,
                bare_share: 0.08,
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
        WILDFLOWER_MEADOW => Self {
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
    }
    /// The plants to mix in as species.
    PLANTS {
        /// Thin stems with pale seed heads, like timothy or foxtail.
        SEED_HEADS => Self {
            blade: GrassBlade::SEED_HEADS,
            ..Self::SUMMER_TALL
        };
        /// White flowers on slender stems, like oxeye daisies.
        WHITE_FLOWERS => Self {
            blade: GrassBlade::WHITE_FLOWERS,
            density: 30.0,
            ..Self::SUMMER_TALL
        };
        /// Small yellow flowers, like buttercups.
        YELLOW_FLOWERS => Self {
            blade: GrassBlade::YELLOW_FLOWERS,
            ..Self::WHITE_FLOWERS
        };
        /// Low, broad, dark leaves lying over, like clover.
        CLOVER => Self {
            blade: GrassBlade::CLOVER,
            density: 120.0,
            ..Self::SUMMER_TALL
        };
        /// Wide, folded leaves that arch over, like plantain or dock.
        BROAD_LEAF => Self {
            blade: GrassBlade::BROAD_LEAF,
            density: 30.0,
            ..Self::SUMMER_TALL
        };
    }
    /// The four forest-floor styles: a moss carpet, fern clumps, the whole floor mixed, and autumn bracken.
    FOREST {
        /// A dense carpet of very short, wide moss, in dark patches.
        FOREST_MOSS => Self {
            blade: GrassBlade::MOSS,
            density: 220.0,
            variation: GrassVariation {
                patch_color: Color::srgb(0.20, 0.34, 0.15),
                patch_amount: 0.35,
                patch_size: 3.0,
                dry_tips: 0.05,
                dead_share: 0.02,
                tufts: 0.25,
                tuft_spacing: 0.35,
                bare_share: 0.05,
                bare_patch_size: 1.2,
            },
            ground_tint: GroundTint {
                color: Color::srgb(0.12, 0.09, 0.05),
                amount: 0.35,
            },
            ..Self::MEADOW
        };
        /// Arching fern fronds in tight clumps, with broad leaves and clover
        /// between them. The nearest a single strip of blade gets to a fern.
        FOREST_FERNS => Self {
            blade: GrassBlade::FERN,
            density: 40.0,
            variation: GrassVariation {
                patch_color: Color::srgb(0.20, 0.36, 0.13),
                patch_amount: 0.35,
                patch_size: 4.0,
                dry_tips: 0.12,
                dead_share: 0.02,
                tufts: 0.55,
                tuft_spacing: 0.75,
                bare_share: 0.12,
                bare_patch_size: 1.8,
            },
            ground_tint: GroundTint {
                color: Color::srgb(0.12, 0.09, 0.05),
                amount: 0.3,
            },
            species: Cow::Borrowed(&[
                GrassSpecies {
                    share: 0.15,
                    clustering: 0.75,
                    blade: GrassBlade::BROAD_LEAF,
                },
                GrassSpecies {
                    share: 0.06,
                    clustering: 0.8,
                    blade: GrassBlade::CLOVER,
                },
            ]),
            ..Self::MEADOW
        };
        /// The whole floor as one field: moss, clumps of ferns, fallen leaves,
        /// clover and broad leaves among them.
        FOREST_FLOOR => Self {
            blade: GrassBlade::MOSS,
            density: 140.0,
            variation: GrassVariation {
                patch_color: Color::srgb(0.22, 0.34, 0.14),
                patch_amount: 0.45,
                tufts: 0.35,
                tuft_spacing: 0.45,
                bare_share: 0.08,
                bare_patch_size: 1.5,
                ..Self::FOREST_MOSS.variation
            },
            ground_tint: GroundTint {
                amount: 0.4,
                ..Self::FOREST_MOSS.ground_tint
            },
            species: Cow::Borrowed(&[
                GrassSpecies {
                    share: 0.18,
                    clustering: 0.55,
                    blade: GrassBlade::FERN,
                },
                GrassSpecies {
                    share: 0.10,
                    clustering: 0.35,
                    blade: GrassBlade::LEAF_LITTER,
                },
                GrassSpecies {
                    share: 0.06,
                    clustering: 0.7,
                    blade: GrassBlade::CLOVER,
                },
                GrassSpecies {
                    share: 0.05,
                    clustering: 0.6,
                    blade: GrassBlade::BROAD_LEAF,
                },
            ]),
            ..Self::FOREST_MOSS
        };
        /// Bracken in its autumn colour: rusty fern fronds over fallen leaves.
        FOREST_BRACKEN => Self {
            blade: GrassBlade::BRACKEN,
            density: 34.0,
            variation: GrassVariation {
                patch_color: Color::srgb(0.45, 0.28, 0.12),
                patch_amount: 0.4,
                patch_size: 5.0,
                dry_tips: 0.25,
                dead_share: 0.08,
                tufts: 0.45,
                tuft_spacing: 0.7,
                bare_share: 0.1,
                bare_patch_size: 2.0,
            },
            ground_tint: GroundTint {
                color: Color::srgb(0.15, 0.10, 0.06),
                amount: 0.35,
            },
            species: Cow::Borrowed(&[
                GrassSpecies {
                    share: 0.12,
                    clustering: 0.5,
                    blade: GrassBlade::LEAF_LITTER,
                },
                GrassSpecies {
                    share: 0.08,
                    clustering: 0.6,
                    blade: GrassBlade {
                        root_color: Color::srgb(0.14, 0.08, 0.03),
                        tip_color: Color::srgb(0.46, 0.27, 0.12),
                        translucency: 0.3,
                        ..GrassBlade::BROAD_LEAF
                    },
                },
            ]),
            ..Self::MEADOW
        };
    }
}
