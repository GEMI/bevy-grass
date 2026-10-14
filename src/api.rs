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
    /// Blades per square metre at full density.
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
