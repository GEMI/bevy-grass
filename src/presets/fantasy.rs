use super::{Color, Cow, GrassBlade, GrassSpecies, GrassStyle, GrassVariation, GroundTint, Vec2};

preset_group! {
/// Plants that do not grow on Earth: crystal, alien ribbons, kelp and a
/// corrupted field.
FANTASY {
    /// Clusters of glassy crystal shards that barely move in the wind:
    /// folded into sharp V-shaped facets, fully translucent and glossy.
    CRYSTAL_SPIRES => Self {
        blade: GrassBlade {
            height: Vec2::new(0.3, 1.0),
            width: 0.12,
            lean: 0.2,
            stiffness: 1.0,
            root_color: Color::srgb(0.05, 0.15, 0.40),
            tip_color: Color::srgb(0.55, 0.90, 1.00),
            root_occlusion: 0.5,
            translucency: 1.0,
            specular: 1.0,
            twist: 0.0,
            droop: 0.0,
            fold: 1.0,
            ..Self::MEADOW.blade
        },
        density: 30.0,
        variation: GrassVariation {
            patch_color: Color::srgb(0.85, 0.55, 1.00),
            patch_amount: 0.5,
            patch_size: 6.0,
            dry_tips: 0.0,
            dead_share: 0.0,
            tufts: 0.9,
            tuft_spacing: 0.8,
            bare_share: 0.3,
            bare_patch_size: 2.0,
        },
        ground_tint: GroundTint {
            color: Color::srgb(0.10, 0.10, 0.14),
            amount: 0.4,
        },
        species: Cow::Borrowed(&[GrassSpecies {
            // A few great shards.
            share: 0.06,
            clustering: 0.4,
            blade: GrassBlade {
                height: Vec2::new(1.1, 1.7),
                width: 0.2,
                lean: 0.1,
                stiffness: 1.0,
                root_color: Color::srgb(0.05, 0.15, 0.40),
                tip_color: Color::srgb(0.55, 0.90, 1.00),
                root_occlusion: 0.5,
                translucency: 1.0,
                specular: 1.0,
                twist: 0.0,
                droop: 0.0,
                fold: 1.0,
                head_length: 0.0,
                head_width: 0.03,
                head_color: Color::srgb(0.85, 0.80, 0.60),
            },
        }]),
        ..Self::MEADOW
    };
    /// Wide violet-to-cyan ribbons, each twisted two full turns, with
    /// magenta seed pods among them. Flat two-band toon shading.
    CORKSCREW_ALIEN => Self {
        blade: GrassBlade {
            height: Vec2::new(0.6, 1.3),
            width: 0.12,
            lean: 0.15,
            stiffness: 0.2,
            root_color: Color::srgb(0.18, 0.03, 0.30),
            tip_color: Color::srgb(0.10, 0.95, 0.85),
            root_occlusion: 0.5,
            translucency: 0.8,
            specular: 0.3,
            twist: 2.0,
            droop: 0.1,
            fold: 0.0,
            ..Self::MEADOW.blade
        },
        density: 14.0,
        variation: GrassVariation {
            patch_color: Color::srgb(0.55, 0.20, 0.95),
            patch_amount: 0.4,
            patch_size: 8.0,
            ..Self::MEADOW.variation
        },
        ground_tint: GroundTint {
            color: Color::srgb(0.10, 0.05, 0.14),
            amount: 0.4,
        },
        toon_bands: 2,
        species: Cow::Borrowed(&[GrassSpecies {
            share: 0.08,
            clustering: 0.7,
            blade: GrassBlade {
                height: Vec2::new(0.5, 1.2),
                width: 0.02,
                lean: 0.1,
                stiffness: 0.3,
                root_color: Color::srgb(0.18, 0.03, 0.30),
                tip_color: Color::srgb(0.30, 0.80, 0.80),
                root_occlusion: 0.5,
                translucency: 0.6,
                specular: 0.3,
                twist: 1.0,
                droop: 0.0,
                fold: 0.0,
                head_length: 0.25,
                head_width: 0.08,
                head_color: Color::srgb(1.00, 0.35, 0.75),
            },
        }]),
    };
    /// A kelp bed: long, soft, translucent ribbons rising from holdfasts on
    /// the sand, some with a float, and seagrass between them. Give the
    /// level a slow wind with lively sway, and it moves like a current.
    KELP_BED => Self {
        blade: GrassBlade {
            height: Vec2::new(1.4, 3.0),
            width: 0.14,
            lean: 0.1,
            stiffness: 0.0,
            root_color: Color::srgb(0.10, 0.08, 0.02),
            tip_color: Color::srgb(0.45, 0.40, 0.10),
            root_occlusion: 0.6,
            translucency: 1.0,
            specular: 0.5,
            twist: 0.35,
            droop: 0.25,
            fold: 0.0,
            ..Self::MEADOW.blade
        },
        density: 14.0,
        variation: GrassVariation {
            patch_color: Color::srgb(0.40, 0.30, 0.10),
            patch_amount: 0.3,
            patch_size: 6.0,
            dry_tips: 0.0,
            dead_share: 0.0,
            tufts: 0.6,
            tuft_spacing: 1.2,
            bare_share: 0.25,
            bare_patch_size: 3.0,
        },
        ground_tint: GroundTint {
            color: Color::srgb(0.42, 0.40, 0.30),
            amount: 0.4,
        },
        species: Cow::Borrowed(&[
            // Seagrass.
            GrassSpecies {
                share: 0.35,
                clustering: 0.6,
                blade: GrassBlade {
                    height: Vec2::new(0.2, 0.5),
                    width: 0.015,
                    lean: 0.2,
                    stiffness: 0.1,
                    root_color: Color::srgb(0.04, 0.12, 0.06),
                    tip_color: Color::srgb(0.18, 0.42, 0.22),
                    root_occlusion: 0.5,
                    translucency: 0.8,
                    specular: 0.3,
                    twist: 0.2,
                    droop: 0.2,
                    fold: 0.0,
                    head_length: 0.0,
                    head_width: 0.03,
                    head_color: Color::srgb(0.85, 0.80, 0.60),
                },
            },
            // Kelp with a gas float at the top.
            GrassSpecies {
                share: 0.15,
                clustering: 0.3,
                blade: GrassBlade {
                    height: Vec2::new(1.4, 3.0),
                    width: 0.14,
                    lean: 0.1,
                    stiffness: 0.0,
                    root_color: Color::srgb(0.10, 0.08, 0.02),
                    tip_color: Color::srgb(0.45, 0.40, 0.10),
                    root_occlusion: 0.6,
                    translucency: 1.0,
                    specular: 0.5,
                    twist: 0.35,
                    droop: 0.25,
                    fold: 0.0,
                    head_length: 0.06,
                    head_width: 0.07,
                    head_color: Color::srgb(0.50, 0.42, 0.12),
                },
            },
        ]),
        ..Self::MEADOW
    };
    /// Corrupted land: bruised violet blades that twist and hang, sickly
    /// green patches, dead stalks, and pale bone-white seed heads.
    BLIGHTED => Self {
        blade: GrassBlade {
            height: Vec2::new(0.4, 1.0),
            width: 0.03,
            lean: 0.3,
            stiffness: 0.2,
            root_color: Color::srgb(0.08, 0.02, 0.10),
            tip_color: Color::srgb(0.50, 0.24, 0.60),
            root_occlusion: 0.8,
            translucency: 0.3,
            specular: 0.4,
            twist: 0.5,
            droop: 0.8,
            fold: 0.4,
            ..Self::MEADOW.blade
        },
        density: 45.0,
        variation: GrassVariation {
            patch_color: Color::srgb(0.45, 0.62, 0.12),
            patch_amount: 0.5,
            patch_size: 6.0,
            dry_tips: 0.25,
            dead_share: 0.25,
            tufts: 0.4,
            tuft_spacing: 0.6,
            bare_share: 0.15,
            bare_patch_size: 2.5,
        },
        ground_tint: GroundTint {
            color: Color::srgb(0.08, 0.06, 0.08),
            amount: 0.5,
        },
        species: Cow::Borrowed(&[GrassSpecies {
            share: 0.06,
            clustering: 0.5,
            blade: GrassBlade {
                height: Vec2::new(0.6, 1.2),
                width: 0.01,
                lean: 0.15,
                stiffness: 0.8,
                root_color: Color::srgb(0.25, 0.22, 0.20),
                tip_color: Color::srgb(0.80, 0.78, 0.70),
                root_occlusion: 0.6,
                translucency: 0.2,
                specular: 0.2,
                twist: 0.0,
                droop: 0.0,
                fold: 0.0,
                head_length: 0.1,
                head_width: 0.025,
                head_color: Color::srgb(0.92, 0.90, 0.82),
            },
        }]),
        ..Self::MEADOW
    };
}
}
