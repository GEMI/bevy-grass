use bevy::color::Color;
use bevy::math::Vec2;

use crate::style::{GrassBlade, GrassStyle};

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
