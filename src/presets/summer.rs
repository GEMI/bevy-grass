use super::{Color, Cow, GrassBlade, GrassStyle, GrassVariation, GroundTint, Vec2};

preset_group! {
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
}
