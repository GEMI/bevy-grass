use super::{Color, Cow, GrassBlade, GrassStyle, GrassVariation, GroundTint, Vec2};

preset_group! {
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
}
