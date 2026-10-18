use super::{Color, Cow, GrassBlade, GrassSpecies, GrassStyle, GrassVariation, GroundTint};

preset_group! {
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
