use super::{Color, Cow, GrassBlade, GrassStyle, GrassVariation, GroundTint, Vec2};

preset_group! {
/// Tall grass in summer and in autumn: the same field in two seasons, to
/// mix, or to blend between at runtime. For a lusher, drier or paler
/// variant, tint one field with `GrassFieldOverrides` instead of adding a
/// style.
SEASONS {
    /// Tall green summer grass.
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
    /// Tall brown autumn grass: the summer field gone dry, so stiffer and
    /// less translucent.
    AUTUMN_TALL => Self {
        blade: GrassBlade {
            lean: 0.25,
            stiffness: 0.35,
            root_color: Color::srgb(0.13, 0.09, 0.04),
            tip_color: Color::srgb(0.52, 0.36, 0.17),
            translucency: 0.4,
            ..Self::SUMMER_TALL.blade
        },
        ..Self::SUMMER_TALL
    };
}
}
