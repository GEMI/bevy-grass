//! The public types: everything a game touches.

use bevy::asset::Handle;
use bevy::camera::visibility::Visibility;
use bevy::color::Color;
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::query::Changed;
use bevy::ecs::reflect::{ReflectComponent, ReflectResource};
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Local, Query};
use bevy::image::Image;
use bevy::log::warn;
use bevy::math::{Dir2, Vec2};
use bevy::platform::collections::HashSet;
use bevy::reflect::Reflect;
use bevy::reflect::std_traits::ReflectDefault;
use bevy::render::RenderApp;
use bevy::render::extract_component::ExtractComponent;
use bevy::transform::components::Transform;

use crate::GrassShadows;

pub use crate::style::{GrassBlade, GrassSpecies, GrassStyle, GrassVariation, GroundTint};

/// One area of grass.
///
/// The field is a rectangle of `size` metres centred on its [`Transform`]'s
/// translation, lying in the XZ plane; the translation's Y is the ground level
/// for [`GrassField::ground`] = [`GrassGround::Flat`].
///
/// A field follows the conventions of Bevy's other renderables:
/// - **Scale:** the rectangle is `size` times the transform's X and Z scale,
///   like a scaled `Plane3d` mesh. Y scale does nothing: heights come from
///   the style and the heightmap.
/// - **Rotation:** not supported yet, so a rotated field is drawn unrotated
///   and logs one warning.
/// - **[`Visibility`]:** a field hidden by its own `Visibility::Hidden`, or by
///   a hidden parent (a streamed-out level, say), is not drawn. Showing it
///   again is cheap: no table is rebuilt.
/// - **[`RenderLayers`](bevy::camera::visibility::RenderLayers):** a camera draws the field only if the two share a
///   layer. A field or camera without the component is on layer 0, as for
///   meshes. Layers 0 to 31 are supported.
#[derive(Component, Debug, Clone, Default, PartialEq, Reflect)]
#[reflect(Component, Default, Debug, PartialEq)]
#[require(Transform, Visibility)]
pub struct GrassField {
    /// Width (X) and depth (Z) of the field, in metres, before the
    /// transform's scale.
    pub size: Vec2,
    /// How the blades look. Add a preset with `styles.add(GrassStyle::MEADOW)`.
    pub style: Handle<GrassStyle>,
    /// Where the blade roots sit.
    pub ground: GrassGround,
    /// Optional density mask covering the whole field: red is density
    /// (0 bare, 1 full), green is a height multiplier. `None` means full
    /// density everywhere. The first row of the image is the field's −Z edge.
    pub density_map: Option<Handle<Image>>,
}

/// Changes how one field looks without a style asset of its own: "this
/// meadow, a bit drier" for one area of a level, while the other fields keep
/// sharing the style.
///
/// ```no_run
/// # use bevy::prelude::*;
/// # use bevy_grass::prelude::*;
/// # fn setup(mut commands: Commands, mut styles: ResMut<Assets<GrassStyle>>) {
/// let meadow = styles.add(GrassStyle::MEADOW);
/// commands.spawn(GrassField { size: Vec2::splat(64.0), style: meadow.clone(), ..default() });
/// // The same style, yellower, shorter and sparser, near the road.
/// commands.spawn((
///     GrassField { size: Vec2::splat(32.0), style: meadow, ..default() },
///     GrassFieldOverrides {
///         tint: Some(Color::srgb(1.3, 1.1, 0.7)),
///         height_scale: 0.6,
///         density_scale: 0.5,
///     },
/// ));
/// # }
/// ```
///
/// The overrides are applied when the field's entries are built, so they cost
/// nothing per frame, and changing one rewrites that field's entries only.
#[derive(Component, Debug, Clone, Copy, PartialEq, Reflect)]
#[reflect(Component, Default, Debug, PartialEq)]
pub struct GrassFieldOverrides {
    /// Multiplies the root and tip colours of every blade of the field, in
    /// linear light: white or `None` leaves them as they are. Heads, the
    /// ground tint and the style's colour patches are not tinted.
    pub tint: Option<Color>,
    /// Multiplies the style's [`density`](GrassStyle::density): 0.5 is half
    /// as many blades. Below 0 is 0.
    pub density_scale: f32,
    /// Multiplies the height of every blade, and the height multiplier of
    /// the density map still applies on top. Below 0 is 0.
    pub height_scale: f32,
}

impl Default for GrassFieldOverrides {
    fn default() -> Self {
        Self {
            tint: None,
            density_scale: 1.0,
            height_scale: 1.0,
        }
    }
}

/// Where blade roots sit.
#[derive(Debug, Clone, Default, PartialEq, Reflect)]
#[reflect(Default, Debug, PartialEq)]
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

/// The wind, shared by every field.
///
/// The presets are constants, so a wind is changed like a style, by struct
/// update: `GrassWind { strength: 0.8, ..GrassWind::GUSTY }`.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Reflect)]
#[reflect(Resource, Default, Debug, PartialEq)]
pub struct GrassWind {
    /// Direction the wind blows toward, in the XZ plane (X, Z). Any length:
    /// it is normalized on use, and (0, 0) blows along +X. To set it from a
    /// `Dir2`, use [`GrassWind::toward`].
    pub direction: Vec2,
    /// Overall strength, 0 (still) to 1 (storm).
    pub strength: f32,
    /// Size of a gust, in metres.
    pub gust_scale: f32,
    /// How fast gusts travel across the field, in metres per second.
    pub gust_speed: f32,
}

impl GrassWind {
    /// The same wind, blowing toward `direction` (X and Z of the world).
    #[must_use]
    pub fn toward(self, direction: Dir2) -> Self {
        Self {
            direction: direction.as_vec2(),
            ..self
        }
    }

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

impl Default for GrassWind {
    fn default() -> Self {
        Self::BREEZE
    }
}

/// What one camera draws differently from the [`GrassSettings`](crate::GrassSettings) resource.
/// Each setting left at `None` follows the resource, including when it
/// changes at runtime.
///
/// Per-view rendering settings live on the camera in Bevy (`Msaa`,
/// `Tonemapping`, `Bloom`); so do these. A minimap at Low next to a main
/// camera at High is one component:
///
/// ```no_run
/// # use bevy::prelude::*;
/// # use bevy_grass::prelude::*;
/// # fn setup(mut commands: Commands) {
/// commands.spawn((
///     Camera3d::default(),
///     GrassCamera {
///         radius: Some(30.0),
///         density_scale: Some(0.4),
///         shadows: Some(GrassShadows::Off),
///         ..default()
///     },
/// ));
/// # }
/// ```
///
/// The buffers a camera draws with are sized by
/// [`GrassSettings::instance_capacity`](crate::GrassSettings::instance_capacity), which is the same for every camera.
/// To leave a camera out altogether, use [`NoGrass`] or `RenderLayers`.
#[derive(Component, ExtractComponent, Debug, Clone, Copy, PartialEq, Default, Reflect)]
#[extract_app(RenderApp)]
#[reflect(Component, Default, Debug, PartialEq)]
pub struct GrassCamera {
    /// Overrides [`GrassSettings::radius`](crate::GrassSettings::radius).
    pub radius: Option<f32>,
    /// Overrides [`GrassSettings::density_scale`](crate::GrassSettings::density_scale).
    pub density_scale: Option<f32>,
    /// Overrides [`GrassSettings::min_width_px`](crate::GrassSettings::min_width_px).
    pub min_width_px: Option<f32>,
    /// Overrides [`GrassSettings::shadows`](crate::GrassSettings::shadows): which blades this camera's
    /// view casts shadows for.
    pub shadows: Option<GrassShadows>,
}

/// Marks a camera that should not draw grass (minimaps, UI cameras). A
/// shorthand: a camera whose `RenderLayers` share none with a field's does
/// not draw that field either.
#[derive(Component, ExtractComponent, Debug, Clone, Copy, Default, Reflect)]
#[extract_app(RenderApp)]
#[reflect(Component, Default, Debug)]
pub struct NoGrass;

/// Marks the directional light grass follows: the one whose shadows grass
/// casts, and the one a deferred camera treats as the sun.
///
/// A scene with a sun and a moon has two directional lights, and grass
/// follows one. Without this marker it is the first one that casts shadows
/// (the one spawned first), and the log says so when there are several. Mark the light that
/// should decide:
///
/// ```no_run
/// # use bevy::prelude::*;
/// # use bevy::light::CascadeShadowConfigBuilder;
/// # use bevy_grass::prelude::*;
/// # fn setup(mut commands: Commands) {
/// commands.spawn((
///     DirectionalLight { shadow_maps_enabled: true, ..default() },
///     // Grass blades are a centimetre or two wide: keep the first cascade
///     // shallow, or a shadow texel is wider than a blade and blade shadows
///     // blink as the grass sways.
///     CascadeShadowConfigBuilder {
///         num_cascades: 3,
///         first_cascade_far_bound: 12.0,
///         maximum_distance: 60.0,
///         ..default()
///     }
///     .build(),
///     GrassSun,
/// ));
/// # }
/// ```
///
/// Marking a light that does not cast shadows means grass casts none. If
/// several lights are marked, the one spawned first is used.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Default, Reflect)]
#[reflect(Component, Default, Debug, PartialEq)]
pub struct GrassSun;

/// Anything that flattens grass: the player, monsters, vehicles, items on
/// the ground. Grass within `radius` of the entity's position lies down away
/// from it and springs back over about 1.5 s of game time after it leaves, so
/// moving interactors leave trails.
///
/// ```no_run
/// # use bevy::prelude::*;
/// # use bevy_grass::prelude::*;
/// # fn setup(mut commands: Commands) {
/// commands.spawn((
///     Transform::from_xyz(0.0, 0.0, 0.0),
///     // A walking person, who steps on the grass; a bird that flies higher
///     // than a metre above the ground does not.
///     GrassInteractor { radius: 0.5, max_height: Some(1.0), ..default() },
/// ));
/// # }
/// ```
///
/// # Limits
///
/// - **Radius: 2 m.** A bigger `radius` is cut to 2 m, and the plugin warns
///   once about the entity. (Each interactor is stamped in a fixed footprint,
///   which is why a hundred cost what one does.) For something bigger, use
///   several interactors around it.
/// - **Count: [`GrassSettings::max_interactors`](crate::GrassSettings::max_interactors).**
///   A camera stamps that many a frame, the ones nearest to it first.
/// - **Distance: a 64 m window.** Each camera tracks a 64 × 64 m square of
///   ground, centred on the camera (on the ground it looks at, for an
///   orthographic one). Interactors outside it do nothing, and trails
///   outside it are forgotten.
/// - **Height.** Interaction is horizontal: with `max_height` at `None`, an
///   interactor flattens the grass below it at any height.
#[derive(Component, Debug, Clone, Copy, PartialEq, Reflect)]
#[reflect(Component, Default, Debug, PartialEq)]
#[require(Transform)]
pub struct GrassInteractor {
    /// Radius of the flattened disc, in metres. Pushes beyond 2 m are clipped.
    pub radius: f32,
    /// How flat the grass lies at the centre, 0 (untouched) to 1 (flat).
    pub strength: f32,
    /// How high above the ground the entity's position may be and still
    /// flatten grass, in metres: the highest blade, for a creature that
    /// steps on the grass and should not flatten it while it flies or is
    /// carried. The push fades out over the upper half of the range. `None`
    /// is any height.
    ///
    /// The ground is the highest ground in the 16 m chunk of a field below
    /// the entity, so on a steep heightmap a flier is judged against the top
    /// of the slope. Outside every field there is no ground, and the
    /// interactor always pushes.
    pub max_height: Option<f32>,
}

impl Default for GrassInteractor {
    fn default() -> Self {
        Self {
            radius: 0.6,
            strength: 1.0,
            max_height: None,
        }
    }
}

/// Warns, once per entity, about an interactor bigger than the grass can
/// stamp.
pub(crate) fn warn_oversized_interactors(
    interactors: Query<(Entity, &GrassInteractor), Changed<GrassInteractor>>,
    mut warned: Local<HashSet<Entity>>,
) {
    for (entity, interactor) in &interactors {
        let too_big = interactor.radius > crate::render::trample::MAX_PUSH_RADIUS;
        if too_big && warned.insert(entity) {
            warn!(
                "bevy_grass: the GrassInteractor on {entity} has radius {} m, but interactors                  push at most {} m; the rest is ignored. Use several interactors for something                  bigger.",
                interactor.radius,
                crate::render::trample::MAX_PUSH_RADIUS,
            );
        } else if !too_big {
            warned.remove(&entity);
        }
    }
}
