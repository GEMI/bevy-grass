use super::{Color, Cow, GrassBlade, GrassSpecies, GrassStyle, GrassVariation};

preset_group! {
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
}
