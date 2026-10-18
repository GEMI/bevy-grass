use super::{Color, Cow, GrassBlade, GrassStyle, GrassVariation, GroundTint, Vec2};

preset_group! {
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
}
