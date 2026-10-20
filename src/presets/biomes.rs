use super::{Color, Cow, GrassBlade, GrassSpecies, GrassStyle, GrassVariation, GroundTint, Vec2};

preset_group! {
/// Places with a look of their own: bog, marsh, dunes, farmland, steppe,
/// outback, jungle, frost, a burnt field and lavender. Each one pushes a
/// setting the general presets leave alone.
BIOMES {
    /// A Baltic raised bog: low sedge in tussocks over red and ochre
    /// sphagnum, white tufts of cotton grass, and dark pools where the ground
    /// shows through.
    COTTON_BOG => Self {
        // Sphagnum: a carpet of red moss, ochre-green in patches.
        blade: GrassBlade {
            height: Vec2::new(0.03, 0.08),
            width: 0.03,
            root_color: Color::srgb(0.32, 0.07, 0.04),
            tip_color: Color::srgb(0.78, 0.26, 0.12),
            ..GrassBlade::MOSS
        },
        density: 200.0,
        variation: GrassVariation {
            patch_color: Color::srgb(0.52, 0.46, 0.16),
            patch_amount: 0.55,
            patch_size: 3.0,
            dry_tips: 0.1,
            dead_share: 0.03,
            tufts: 0.45,
            tuft_spacing: 0.5,
            bare_share: 0.2,
            bare_patch_size: 2.0,
        },
        ground_tint: GroundTint {
            color: Color::srgb(0.10, 0.08, 0.05),
            amount: 0.3,
        },
        species: Cow::Borrowed(&[
            // Sedge in tussocks.
            GrassSpecies {
                share: 0.2,
                clustering: 0.6,
                blade: GrassBlade {
                    height: Vec2::new(0.15, 0.4),
                    width: 0.01,
                    lean: 0.2,
                    stiffness: 0.4,
                    root_color: Color::srgb(0.10, 0.10, 0.04),
                    tip_color: Color::srgb(0.46, 0.44, 0.18),
                    root_occlusion: 0.6,
                    translucency: 0.5,
                    specular: 0.2,
                    twist: 0.1,
                    droop: 0.3,
                    fold: 0.4,
                    head_length: 0.0,
                    head_width: 0.03,
                    head_color: Color::srgb(0.85, 0.80, 0.60),
                },
            },
            GrassSpecies {
                share: 0.045,
                clustering: 0.5,
                blade: GrassBlade::COTTON_GRASS,
            },
        ]),
        ..Self::MEADOW
    };
    /// Reeds taller than a person in dense clumps, with cattails among them
    /// and open water between the clumps.
    REED_MARSH => Self {
        blade: GrassBlade {
            height: Vec2::new(1.2, 2.0),
            width: 0.025,
            lean: 0.15,
            stiffness: 0.6,
            root_color: Color::srgb(0.08, 0.12, 0.05),
            tip_color: Color::srgb(0.42, 0.50, 0.24),
            root_occlusion: 0.7,
            translucency: 0.55,
            specular: 0.35,
            twist: 0.1,
            droop: 0.3,
            fold: 0.3,
            ..Self::MEADOW.blade
        },
        density: 45.0,
        variation: GrassVariation {
            patch_color: Color::srgb(0.55, 0.50, 0.28),
            patch_amount: 0.35,
            patch_size: 8.0,
            dry_tips: 0.25,
            dead_share: 0.08,
            tufts: 0.7,
            tuft_spacing: 0.8,
            bare_share: 0.15,
            bare_patch_size: 3.0,
        },
        ground_tint: GroundTint {
            color: Color::srgb(0.12, 0.11, 0.07),
            amount: 0.3,
        },
        species: Cow::Borrowed(&[GrassSpecies {
            share: 0.22,
            clustering: 0.7,
            blade: GrassBlade::CATTAIL,
        }]),
        ..Self::MEADOW
    };
    /// Coastal dune grass: blue-grey, rolled, waxy blades in separate clumps
    /// on open sand, like marram on the Curonian Spit.
    DUNE_MARRAM => Self {
        blade: GrassBlade {
            height: Vec2::new(0.5, 0.95),
            width: 0.012,
            lean: 0.45,
            stiffness: 0.55,
            root_color: Color::srgb(0.30, 0.30, 0.22),
            tip_color: Color::srgb(0.58, 0.64, 0.50),
            root_occlusion: 0.4,
            translucency: 0.35,
            specular: 0.6,
            twist: 0.15,
            droop: 0.55,
            fold: 0.8,
            ..Self::MEADOW.blade
        },
        density: 45.0,
        variation: GrassVariation {
            patch_color: Color::srgb(0.70, 0.66, 0.48),
            patch_amount: 0.25,
            patch_size: 6.0,
            dry_tips: 0.35,
            dead_share: 0.1,
            tufts: 1.0,
            tuft_spacing: 1.1,
            bare_share: 0.4,
            bare_patch_size: 4.0,
        },
        ground_tint: GroundTint {
            color: Color::srgb(0.78, 0.70, 0.52),
            amount: 0.55,
        },
        ..Self::MEADOW
    };
    /// Ripening rye, green-gold with seed heads, with blue cornflowers and
    /// red poppies growing through it.
    RYE_AND_CORNFLOWERS => Self {
        blade: GrassBlade {
            height: Vec2::new(1.1, 1.6),
            width: 0.012,
            lean: 0.1,
            stiffness: 0.55,
            root_color: Color::srgb(0.18, 0.22, 0.08),
            tip_color: Color::srgb(0.70, 0.68, 0.36),
            root_occlusion: 0.6,
            translucency: 0.5,
            specular: 0.45,
            droop: 0.15,
            head_length: 0.15,
            head_width: 0.018,
            head_color: Color::srgb(0.72, 0.64, 0.40),
            ..Self::MEADOW.blade
        },
        density: 80.0,
        variation: GrassVariation {
            patch_color: Color::srgb(0.55, 0.62, 0.30),
            patch_amount: 0.3,
            patch_size: 10.0,
            dry_tips: 0.2,
            ..Self::MEADOW.variation
        },
        ground_tint: GroundTint {
            color: Color::srgb(0.30, 0.24, 0.15),
            amount: 0.2,
        },
        species: Cow::Borrowed(&[
            GrassSpecies {
                share: 0.06,
                clustering: 0.45,
                blade: GrassBlade::CORNFLOWER,
            },
            GrassSpecies {
                share: 0.035,
                clustering: 0.55,
                blade: GrassBlade::POPPY,
            },
        ]),
        ..Self::MEADOW
    };
    /// Pampas grass: every tuft a fountain of long arching leaves, with tall
    /// cream plumes standing out of it.
    PAMPAS => Self {
        blade: GrassBlade {
            height: Vec2::new(1.6, 2.6),
            width: 0.018,
            lean: 0.6,
            stiffness: 0.3,
            root_color: Color::srgb(0.10, 0.14, 0.06),
            tip_color: Color::srgb(0.48, 0.54, 0.34),
            root_occlusion: 0.75,
            translucency: 0.5,
            specular: 0.4,
            twist: 0.1,
            droop: 0.9,
            fold: 0.4,
            ..Self::MEADOW.blade
        },
        density: 70.0,
        variation: GrassVariation {
            patch_color: Color::srgb(0.55, 0.56, 0.36),
            patch_amount: 0.2,
            patch_size: 8.0,
            dry_tips: 0.3,
            dead_share: 0.06,
            tufts: 1.0,
            tuft_spacing: 1.8,
            bare_share: 0.15,
            bare_patch_size: 3.0,
        },
        ground_tint: GroundTint {
            color: Color::srgb(0.22, 0.17, 0.10),
            amount: 0.3,
        },
        species: Cow::Borrowed(&[GrassSpecies {
            share: 0.06,
            clustering: 0.0,
            blade: GrassBlade {
                height: Vec2::new(2.2, 3.0),
                width: 0.01,
                lean: 0.12,
                stiffness: 0.45,
                root_color: Color::srgb(0.30, 0.30, 0.18),
                tip_color: Color::srgb(0.70, 0.66, 0.50),
                root_occlusion: 0.6,
                translucency: 0.7,
                specular: 0.3,
                twist: 0.0,
                droop: 0.2,
                fold: 0.0,
                head_length: 0.3,
                head_width: 0.045,
                head_color: Color::srgb(0.94, 0.88, 0.74),
            },
        }]),
        ..Self::MEADOW
    };
    /// Spinifex: stiff, needle-thin blades splayed out of round hummocks on
    /// bare red earth. Outback, desert and wasteland.
    SPINIFEX => Self {
        blade: GrassBlade {
            height: Vec2::new(0.4, 0.7),
            width: 0.01,
            lean: 0.85,
            stiffness: 0.9,
            root_color: Color::srgb(0.32, 0.30, 0.18),
            tip_color: Color::srgb(0.76, 0.70, 0.42),
            root_occlusion: 0.45,
            translucency: 0.3,
            specular: 0.35,
            twist: 0.0,
            droop: 0.0,
            fold: 0.9,
            ..Self::MEADOW.blade
        },
        density: 130.0,
        variation: GrassVariation {
            patch_color: Color::srgb(0.62, 0.62, 0.40),
            patch_amount: 0.3,
            patch_size: 6.0,
            dry_tips: 0.4,
            dead_share: 0.15,
            tufts: 1.0,
            tuft_spacing: 0.9,
            bare_share: 0.3,
            bare_patch_size: 3.0,
        },
        ground_tint: GroundTint {
            color: Color::srgb(0.55, 0.28, 0.14),
            amount: 0.4,
        },
        ..Self::MEADOW
    };
    /// Elephant grass: a wall of broad, arching blades twice as tall as a
    /// person, with sunlight glowing through it. Something to hide in.
    ELEPHANT_GRASS => Self {
        blade: GrassBlade {
            height: Vec2::new(2.4, 3.6),
            width: 0.045,
            lean: 0.25,
            stiffness: 0.3,
            root_color: Color::srgb(0.04, 0.09, 0.02),
            tip_color: Color::srgb(0.30, 0.52, 0.12),
            root_occlusion: 0.8,
            translucency: 0.85,
            specular: 0.35,
            twist: 0.2,
            droop: 0.75,
            fold: 0.45,
            ..Self::MEADOW.blade
        },
        density: 40.0,
        variation: GrassVariation {
            patch_color: Color::srgb(0.45, 0.55, 0.16),
            patch_amount: 0.3,
            patch_size: 8.0,
            dry_tips: 0.15,
            dead_share: 0.05,
            tufts: 0.6,
            tuft_spacing: 0.9,
            bare_share: 0.0,
            bare_patch_size: 3.0,
        },
        ground_tint: GroundTint {
            color: Color::srgb(0.12, 0.09, 0.05),
            amount: 0.3,
        },
        species: Cow::Borrowed(&[GrassSpecies {
            share: 0.08,
            clustering: 0.6,
            blade: GrassBlade {
                height: Vec2::new(0.4, 0.9),
                width: 0.09,
                ..GrassBlade::BROAD_LEAF
            },
        }]),
        ..Self::MEADOW
    };
    /// A meadow on a frosty morning: stiff, glinting, near-white blades
    /// growing out of snow, with straw stalks and frosted seed heads, and
    /// green where the frost has melted.
    FROSTED_MEADOW => Self {
        blade: GrassBlade {
            height: Vec2::new(0.12, 0.32),
            width: 0.022,
            lean: 0.2,
            stiffness: 0.95,
            root_color: Color::srgb(0.40, 0.48, 0.44),
            tip_color: Color::srgb(0.94, 0.97, 1.00),
            root_occlusion: 0.5,
            translucency: 0.15,
            specular: 0.9,
            twist: 0.05,
            droop: 0.2,
            fold: 0.3,
            ..Self::MEADOW.blade
        },
        density: 130.0,
        variation: GrassVariation {
            patch_color: Color::srgb(0.35, 0.50, 0.38),
            patch_amount: 0.35,
            patch_size: 6.0,
            dry_tips: 0.0,
            dead_share: 0.1,
            tufts: 0.3,
            tuft_spacing: 0.6,
            bare_share: 0.12,
            bare_patch_size: 3.0,
        },
        ground_tint: GroundTint {
            color: Color::srgb(0.90, 0.93, 0.96),
            amount: 0.6,
        },
        species: Cow::Borrowed(&[GrassSpecies {
            share: 0.06,
            clustering: 0.4,
            blade: GrassBlade {
                height: Vec2::new(0.35, 0.7),
                stiffness: 0.95,
                root_color: Color::srgb(0.30, 0.28, 0.20),
                tip_color: Color::srgb(0.85, 0.88, 0.90),
                translucency: 0.15,
                specular: 0.9,
                head_color: Color::srgb(0.90, 0.92, 0.94),
                ..GrassBlade::SEED_HEADS
            },
        }]),
        ..Self::MEADOW
    };
    /// After a grass fire: charcoal stubble in grey ash, scorched stalks, and
    /// bright new shoots coming through in patches.
    BURNT_REGROWTH => Self {
        blade: GrassBlade {
            height: Vec2::new(0.08, 0.3),
            width: 0.015,
            lean: 0.25,
            stiffness: 0.8,
            root_color: Color::srgb(0.02, 0.02, 0.02),
            tip_color: Color::srgb(0.10, 0.09, 0.08),
            root_occlusion: 0.3,
            translucency: 0.0,
            specular: 0.05,
            twist: 0.2,
            droop: 0.3,
            fold: 0.2,
            ..Self::MEADOW.blade
        },
        density: 90.0,
        variation: GrassVariation {
            patch_color: Color::srgb(0.40, 0.39, 0.37),
            patch_amount: 0.35,
            patch_size: 5.0,
            dry_tips: 0.0,
            dead_share: 0.15,
            tufts: 0.2,
            tuft_spacing: 0.6,
            bare_share: 0.3,
            bare_patch_size: 2.0,
        },
        ground_tint: GroundTint {
            color: Color::srgb(0.16, 0.15, 0.14),
            amount: 0.5,
        },
        species: Cow::Borrowed(&[
            // New shoots.
            GrassSpecies {
                share: 0.22,
                clustering: 0.85,
                blade: GrassBlade {
                    height: Vec2::new(0.08, 0.28),
                    width: 0.012,
                    lean: 0.12,
                    stiffness: 0.5,
                    root_color: Color::srgb(0.10, 0.25, 0.05),
                    tip_color: Color::srgb(0.42, 0.78, 0.18),
                    root_occlusion: 0.3,
                    translucency: 0.8,
                    specular: 0.2,
                    twist: 0.0,
                    droop: 0.0,
                    fold: 0.0,
                    head_length: 0.0,
                    head_width: 0.03,
                    head_color: Color::srgb(0.85, 0.80, 0.60),
                },
            },
            // Scorched stalks still standing.
            GrassSpecies {
                share: 0.06,
                clustering: 0.3,
                blade: GrassBlade {
                    height: Vec2::new(0.5, 0.9),
                    width: 0.008,
                    lean: 0.3,
                    stiffness: 0.9,
                    root_color: Color::srgb(0.03, 0.03, 0.03),
                    tip_color: Color::srgb(0.20, 0.14, 0.08),
                    root_occlusion: 0.3,
                    translucency: 0.0,
                    specular: 0.05,
                    twist: 0.0,
                    droop: 0.0,
                    fold: 0.0,
                    head_length: 0.0,
                    head_width: 0.03,
                    head_color: Color::srgb(0.85, 0.80, 0.60),
                },
            },
        ]),
        ..Self::MEADOW
    };
    /// Lavender: grey-green bushes of stiff stems with long purple heads.
    /// A density mask with stripes plants it in rows.
    LAVENDER => Self {
        blade: GrassBlade {
            height: Vec2::new(0.35, 0.6),
            width: 0.008,
            lean: 0.25,
            stiffness: 0.75,
            root_color: Color::srgb(0.22, 0.28, 0.20),
            tip_color: Color::srgb(0.48, 0.55, 0.45),
            root_occlusion: 0.6,
            translucency: 0.3,
            specular: 0.2,
            head_length: 0.4,
            head_width: 0.022,
            head_color: Color::srgb(0.52, 0.38, 0.78),
            ..Self::MEADOW.blade
        },
        density: 160.0,
        variation: GrassVariation {
            patch_color: Color::srgb(0.46, 0.52, 0.42),
            patch_amount: 0.0,
            patch_size: 12.0,
            dry_tips: 0.0,
            dead_share: 0.02,
            tufts: 1.0,
            tuft_spacing: 0.45,
            bare_share: 0.0,
            bare_patch_size: 3.0,
        },
        ground_tint: GroundTint {
            color: Color::srgb(0.40, 0.32, 0.22),
            amount: 0.2,
        },
        ..Self::MEADOW
    };
}
}
