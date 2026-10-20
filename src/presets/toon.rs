use super::{Color, Cow, GrassBlade, GrassSpecies, GrassStyle, GrassVariation, GroundTint, Vec2};

/// A flower for the toon styles: a short stem and a big, flat-coloured head.
const fn toon_flower(height: Vec2, head_color: Color) -> GrassBlade {
    GrassBlade {
        height,
        width: 0.015,
        lean: 0.1,
        stiffness: 0.4,
        root_color: Color::srgb(0.05, 0.30, 0.20),
        tip_color: Color::srgb(0.35, 0.70, 0.30),
        root_occlusion: 0.3,
        translucency: 0.3,
        specular: 0.0,
        twist: 0.0,
        droop: 0.0,
        fold: 0.0,
        head_length: 0.2,
        head_width: 0.07,
        head_color,
    }
}

preset_group! {
/// Toon-shaded fields, where the shapes are stylized as well as the light.
TOON {
    /// A cel-shaded field in the manner of open-world adventure games: wide,
    /// saturated blades gathered into chunky tufts, two bands of light, and
    /// red and white flowers in clumps.
    CEL_FIELD => Self {
        blade: GrassBlade {
            height: Vec2::new(0.35, 0.75),
            width: 0.09,
            lean: 0.35,
            stiffness: 0.3,
            root_color: Color::srgb(0.05, 0.30, 0.30),
            tip_color: Color::srgb(0.62, 0.92, 0.30),
            root_occlusion: 0.35,
            translucency: 0.4,
            specular: 0.0,
            twist: 0.0,
            droop: 0.35,
            fold: 0.0,
            ..Self::MEADOW.blade
        },
        density: 22.0,
        variation: GrassVariation {
            patch_color: Color::srgb(0.75, 0.95, 0.35),
            patch_amount: 0.3,
            patch_size: 10.0,
            tufts: 0.7,
            tuft_spacing: 0.55,
            ..Self::MEADOW.variation
        },
        ground_tint: GroundTint {
            color: Color::srgb(0.30, 0.45, 0.20),
            amount: 0.4,
        },
        toon_bands: 2,
        species: Cow::Borrowed(&[
            GrassSpecies {
                share: 0.05,
                clustering: 0.7,
                blade: toon_flower(Vec2::new(0.5, 0.9), Color::srgb(0.95, 0.25, 0.25)),
            },
            GrassSpecies {
                share: 0.05,
                clustering: 0.7,
                blade: toon_flower(Vec2::new(0.5, 0.9), Color::srgb(0.98, 0.97, 0.92)),
            },
        ]),
    };
    /// Short, soft, floppy mint grass with pastel flowers and four soft
    /// bands of light: cozy and casual games.
    COZY_PASTEL => Self {
        blade: GrassBlade {
            height: Vec2::new(0.15, 0.35),
            width: 0.05,
            lean: 0.35,
            stiffness: 0.35,
            root_color: Color::srgb(0.18, 0.48, 0.42),
            tip_color: Color::srgb(0.66, 0.90, 0.60),
            root_occlusion: 0.3,
            translucency: 0.3,
            specular: 0.0,
            twist: 0.0,
            droop: 0.4,
            fold: 0.0,
            ..Self::MEADOW.blade
        },
        density: 60.0,
        variation: GrassVariation {
            patch_color: Color::srgb(0.95, 0.92, 0.70),
            patch_amount: 0.25,
            patch_size: 6.0,
            tufts: 0.5,
            tuft_spacing: 0.35,
            ..Self::MEADOW.variation
        },
        ground_tint: GroundTint {
            color: Color::srgb(0.55, 0.72, 0.50),
            amount: 0.3,
        },
        toon_bands: 4,
        species: Cow::Borrowed(&[
            GrassSpecies {
                share: 0.05,
                clustering: 0.7,
                blade: toon_flower(Vec2::new(0.25, 0.45), Color::srgb(0.98, 0.62, 0.78)),
            },
            GrassSpecies {
                share: 0.04,
                clustering: 0.7,
                blade: toon_flower(Vec2::new(0.25, 0.45), Color::srgb(0.72, 0.62, 0.98)),
            },
            GrassSpecies {
                share: 0.04,
                clustering: 0.7,
                blade: toon_flower(Vec2::new(0.25, 0.45), Color::srgb(1.00, 0.88, 0.45)),
            },
        ]),
    };
}
}
