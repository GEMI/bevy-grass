//! The public types: everything a game touches.

use bevy::asset::{Asset, Handle};
use bevy::color::Color;
use bevy::ecs::component::Component;
use bevy::ecs::resource::Resource;
use bevy::image::Image;
use bevy::math::Vec2;
use bevy::reflect::Reflect;
use bevy::render::RenderApp;
use bevy::render::extract_component::ExtractComponent;
use bevy::transform::components::Transform;

/// One area of grass.
///
/// The field is a rectangle of `size` metres centred on its [`Transform`]'s
/// translation, lying in the XZ plane; the translation's Y is the ground level
/// for [`GrassGround::Flat`]. Rotation and scale are ignored.
#[derive(Component, Debug, Clone, Default, Reflect)]
#[require(Transform)]
pub struct GrassField {
    /// Width (X) and depth (Z) of the field, in metres.
    pub size: Vec2,
    /// How the blades look. Add a preset with `styles.add(GrassStyle::MEADOW)`.
    pub style: Handle<GrassStyle>,
    /// Where the blade roots sit.
    pub ground: GrassGround,
    /// Optional density mask covering the whole field: red is density
    /// (0 bare, 1 full), green is a height multiplier. `None` means full
    /// density everywhere. The first row of the image is the field's −Z edge.
    pub density: Option<Handle<Image>>,
}

/// Where blade roots sit.
#[derive(Debug, Clone, Default, Reflect)]
#[non_exhaustive]
pub enum GrassGround {
    /// A flat plane at the field's height.
    #[default]
    Flat,
    /// A heightmap covering the whole field, laid out like the density mask:
    /// the red channel, 0 to 1, times `height_scale` metres above the field's
    /// height. Sampled bilinearly. Blades grow straight up on slopes.
    Heightmap {
        /// The heightmap. The first row is the field's −Z edge.
        image: Handle<Image>,
        /// Metres between red 0 and red 1.
        height_scale: f32,
    },
}

/// The look of the blades. An asset, so fields can share one and it can be
/// edited at runtime.
#[derive(Asset, Debug, Clone, Reflect)]
pub struct GrassStyle {
    /// Shortest and tallest blade, in metres.
    pub height: Vec2,
    /// Blade width at the root, in metres. Each blade varies ±25% around it.
    pub width: f32,
    /// Blades per square metre at full density. Blades are spread evenly
    /// (blue noise), so this also sets the distance between neighbouring
    /// blades: about `1 / sqrt(density)` metres. [`GrassStyle::spacing`] and
    /// [`GrassStyle::with_spacing`] work in that distance instead.
    pub density: f32,
    /// How far the tip leans at rest, as a fraction of the height.
    pub lean: f32,
    /// Resistance to wind, 0 (soft) to 1 (stiff).
    pub stiffness: f32,
    /// Colour at the root.
    pub root_color: Color,
    /// Colour at the tip.
    pub tip_color: Color,
    /// How much darker the roots are, 0 (none) to 1 (black), standing in for
    /// the light the surrounding blades block.
    pub root_occlusion: f32,
    /// How much sunlight shows through a blade lit from behind, 0 (none) to
    /// 1 (as bright as its sunlit side). Grass seen against the sun glows
    /// instead of going black.
    pub translucency: f32,
    /// Minimum on-screen width of a blade, in pixels. Blades narrower than a
    /// pixel shimmer; raise this for cameras far from the ground.
    pub min_width_px: f32,
}

impl GrassStyle {
    /// Fresh green, soft, mixed heights. The default for temperate worlds.
    pub const MEADOW: Self = Self {
        height: Vec2::new(0.3, 0.6),
        width: 0.02,
        density: 60.0,
        lean: 0.15,
        stiffness: 0.3,
        root_color: Color::srgb(0.06, 0.13, 0.03),
        tip_color: Color::srgb(0.38, 0.55, 0.16),
        root_occlusion: 0.6,
        translucency: 0.6,
        min_width_px: 1.0,
    };

    /// Deep green to straw tips, loose and flowing. For hiding in grass and
    /// dramatic wind.
    pub const TALL_PRAIRIE: Self = Self {
        height: Vec2::new(0.8, 1.4),
        width: 0.025,
        density: 40.0,
        lean: 0.25,
        stiffness: 0.15,
        root_color: Color::srgb(0.05, 0.10, 0.03),
        tip_color: Color::srgb(0.62, 0.58, 0.30),
        root_occlusion: 0.7,
        translucency: 0.5,
        min_width_px: 1.0,
    };

    // Tall summer and autumn grass. Each theme has a main look and variants;
    // all of them share the blade shape (4 cm wide, 40 per m², about 16 cm
    // apart), so they can be mixed in one scene and retuned together with
    // `with_density`, `with_spacing` or plain field updates.

    /// Tall green summer grass: the main summer look.
    pub const SUMMER_TALL: Self = Self {
        height: Vec2::new(0.7, 1.3),
        width: 0.04,
        density: 40.0,
        lean: 0.2,
        stiffness: 0.25,
        root_color: Color::srgb(0.05, 0.11, 0.03),
        tip_color: Color::srgb(0.34, 0.56, 0.14),
        root_occlusion: 0.65,
        translucency: 0.6,
        min_width_px: 1.0,
    };

    /// Summer variant: deep green, denser and taller, as along a ditch or
    /// a stream.
    pub const SUMMER_LUSH: Self = Self {
        height: Vec2::new(0.9, 1.5),
        density: 50.0,
        lean: 0.15,
        root_color: Color::srgb(0.03, 0.08, 0.02),
        tip_color: Color::srgb(0.22, 0.44, 0.10),
        root_occlusion: 0.7,
        ..Self::SUMMER_TALL
    };

    /// Summer variant: bright yellow-green and the shortest of the summer
    /// set, a sunny meadow.
    pub const SUMMER_MEADOW: Self = Self {
        height: Vec2::new(0.5, 1.0),
        lean: 0.18,
        stiffness: 0.3,
        root_color: Color::srgb(0.07, 0.13, 0.03),
        tip_color: Color::srgb(0.48, 0.62, 0.18),
        ..Self::SUMMER_TALL
    };

    /// Summer variant: high summer gone to seed, green stems with pale,
    /// sun-bleached tips.
    pub const SUMMER_SEEDING: Self = Self {
        height: Vec2::new(0.8, 1.4),
        lean: 0.28,
        stiffness: 0.18,
        root_color: Color::srgb(0.06, 0.12, 0.03),
        tip_color: Color::srgb(0.66, 0.64, 0.38),
        ..Self::SUMMER_TALL
    };

    /// Tall brown autumn grass: the main autumn look. Drier, so stiffer and
    /// less translucent than summer grass.
    pub const AUTUMN_TALL: Self = Self {
        height: Vec2::new(0.7, 1.3),
        width: 0.04,
        density: 40.0,
        lean: 0.25,
        stiffness: 0.35,
        root_color: Color::srgb(0.13, 0.09, 0.04),
        tip_color: Color::srgb(0.52, 0.36, 0.17),
        root_occlusion: 0.65,
        translucency: 0.4,
        min_width_px: 1.0,
    };

    /// Autumn variant: rust-red, like dying bracken.
    pub const AUTUMN_RUST: Self = Self {
        root_color: Color::srgb(0.16, 0.07, 0.03),
        tip_color: Color::srgb(0.54, 0.29, 0.13),
        ..Self::AUTUMN_TALL
    };

    /// Autumn variant: pale dry straw, a field nobody cut.
    pub const AUTUMN_STRAW: Self = Self {
        height: Vec2::new(0.6, 1.1),
        stiffness: 0.45,
        root_color: Color::srgb(0.24, 0.19, 0.10),
        tip_color: Color::srgb(0.72, 0.62, 0.38),
        translucency: 0.5,
        ..Self::AUTUMN_TALL
    };

    /// Autumn variant: late autumn, grey-brown and beaten down by rain.
    pub const AUTUMN_FADED: Self = Self {
        height: Vec2::new(0.5, 1.1),
        lean: 0.4,
        stiffness: 0.15,
        root_color: Color::srgb(0.11, 0.10, 0.07),
        tip_color: Color::srgb(0.44, 0.39, 0.30),
        translucency: 0.3,
        ..Self::AUTUMN_TALL
    };

    /// The summer theme: the main look first, then its variants.
    pub const SUMMER: [Self; 4] = [
        Self::SUMMER_TALL,
        Self::SUMMER_LUSH,
        Self::SUMMER_MEADOW,
        Self::SUMMER_SEEDING,
    ];

    /// The autumn theme: the main look first, then its variants.
    pub const AUTUMN: [Self; 4] = [
        Self::AUTUMN_TALL,
        Self::AUTUMN_RUST,
        Self::AUTUMN_STRAW,
        Self::AUTUMN_FADED,
    ];

    /// Average distance between neighbouring blades at full density, in
    /// metres: `1 / sqrt(density)`.
    pub fn spacing(&self) -> f32 {
        1.0 / self.density.max(f32::MIN_POSITIVE).sqrt()
    }

    /// The same style with blades about `metres` apart: sets the density to
    /// `1 / metres²`.
    #[must_use]
    pub fn with_spacing(self, metres: f32) -> Self {
        Self {
            density: 1.0 / (metres * metres).max(f32::MIN_POSITIVE),
            ..self
        }
    }

    /// The same style with `blades_per_m2` blades per square metre.
    #[must_use]
    pub fn with_density(self, blades_per_m2: f32) -> Self {
        Self {
            density: blades_per_m2.max(0.0),
            ..self
        }
    }
}

impl Default for GrassStyle {
    fn default() -> Self {
        Self::MEADOW
    }
}

/// The wind, shared by every field.
#[derive(Resource, Debug, Clone, Reflect)]
pub struct Wind {
    /// Direction the wind blows toward, in the XZ plane (X, Z). Normalized on use.
    pub direction: Vec2,
    /// Overall strength, 0 (still) to 1 (storm).
    pub strength: f32,
    /// Size of a gust, in metres.
    pub gust_scale: f32,
    /// How fast gusts travel across the field, in metres per second.
    pub gust_speed: f32,
}

impl Wind {
    /// Barely moving, a gentle sway.
    pub const CALM: Self = Self {
        direction: Vec2::new(1.0, 0.3),
        strength: 0.1,
        gust_scale: 40.0,
        gust_speed: 2.0,
    };
    /// Visible rolling waves. The default.
    pub const BREEZE: Self = Self {
        direction: Vec2::new(1.0, 0.3),
        strength: 0.35,
        gust_scale: 25.0,
        gust_speed: 4.0,
    };
    /// Fast, uneven gusts.
    pub const GUSTY: Self = Self {
        direction: Vec2::new(1.0, 0.3),
        strength: 0.6,
        gust_scale: 15.0,
        gust_speed: 7.0,
    };
    /// Heavy bending and fast flutter.
    pub const STORM: Self = Self {
        direction: Vec2::new(1.0, 0.3),
        strength: 1.0,
        gust_scale: 8.0,
        gust_speed: 12.0,
    };
}

impl Default for Wind {
    fn default() -> Self {
        Self::BREEZE
    }
}

/// Marks a camera that should not draw grass (minimaps, UI cameras).
#[derive(Component, ExtractComponent, Debug, Clone, Copy, Default, Reflect)]
#[extract_app(RenderApp)]
pub struct NoGrass;

/// Anything that flattens grass: the player, monsters, vehicles, items on
/// the ground. Grass within `radius` of the entity's position lies down away
/// from it and springs back over about 1.5 s of game time after it leaves, so
/// moving interactors leave trails.
///
/// Every interactor costs the same whatever its size, and a camera stamps at
/// most `GrassPlugin::max_interactors` of them a frame, nearest first.
/// Interaction is horizontal only: an interactor flattens the grass below it
/// at any height.
#[derive(Component, Debug, Clone, Copy, PartialEq, Reflect)]
#[require(Transform)]
pub struct GrassInteractor {
    /// Radius of the flattened disc, in metres. Pushes beyond 2 m are clipped.
    pub radius: f32,
    /// How flat the grass lies at the centre, 0 (untouched) to 1 (flat).
    pub strength: f32,
}

impl Default for GrassInteractor {
    fn default() -> Self {
        Self {
            radius: 0.6,
            strength: 1.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::GrassStyle;

    #[test]
    fn spacing_and_density_describe_the_same_field() {
        let style = GrassStyle::SUMMER_TALL.with_spacing(0.2);
        assert!((style.density - 25.0).abs() < 1e-3);
        assert!((style.spacing() - 0.2).abs() < 1e-6);
        let style = style.with_density(100.0);
        assert!((style.spacing() - 0.1).abs() < 1e-6);
    }

    #[test]
    fn every_theme_preset_is_tall_and_valid() {
        for style in GrassStyle::SUMMER.iter().chain(&GrassStyle::AUTUMN) {
            assert!(style.height.x > 0.0 && style.height.x < style.height.y);
            assert!(style.height.y >= 1.0, "tall grass reaches at least 1 m");
            assert!(style.density > 0.0 && style.width > 0.0);
            assert!((0.0..=1.0).contains(&style.translucency));
        }
    }

    #[test]
    fn summer_is_green_and_autumn_is_brown() {
        let rgb = |style: &GrassStyle| style.tip_color.to_srgba();
        for style in &GrassStyle::SUMMER[..3] {
            let tip = rgb(style);
            assert!(tip.green > tip.red && tip.green > tip.blue, "{tip:?}");
        }
        for style in &GrassStyle::AUTUMN {
            let tip = rgb(style);
            assert!(tip.red > tip.green && tip.green > tip.blue, "{tip:?}");
        }
    }
}
