//! The public types: everything a game touches.

use bevy::asset::{Assets, Handle};
use bevy::camera::visibility::Visibility;
use bevy::color::Color;
use bevy::ecs::change_detection::{DetectChanges, Ref};
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::reflect::ReflectComponent;
use bevy::ecs::system::{Local, Query, Res};
use bevy::image::Image;
use bevy::log::warn;
use bevy::math::{Vec2, Vec3};
use bevy::platform::collections::HashSet;
use bevy::reflect::Reflect;
use bevy::reflect::std_traits::ReflectDefault;
use bevy::render::RenderApp;
use bevy::render::extract_component::ExtractComponent;
use bevy::transform::components::Transform;

use crate::{GrassSettings, GrassShadows};

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
///
/// [`GrassField::new`] adds a style for you. A field left with the empty
/// default style handle draws nothing and, after a few frames, logs one
/// warning saying how to give it a style.
#[derive(Component, Debug, Clone, Default, PartialEq, Reflect)]
#[reflect(Component, Default, Debug, PartialEq)]
#[require(Transform, Visibility)]
pub struct GrassField {
    /// Width (X) and depth (Z) of the field, in metres, before the
    /// transform's scale.
    pub size: Vec2,
    /// How the blades look. Add a preset with `styles.add(GrassStyle::MEADOW)`,
    /// or use [`GrassField::new`] to add it in the same call.
    pub style: Handle<GrassStyle>,
    /// Where the blade roots sit.
    pub ground: GrassGround,
    /// Optional density mask covering the whole field: red is density
    /// (0 bare, 1 full), green is a height multiplier. `None` means full
    /// density everywhere. The first row of the image is the field's −Z edge.
    pub density_map: Option<Handle<Image>>,
}

impl GrassField {
    /// A field of `size` metres whose blades look like `style`, which is
    /// added to `styles`. Every other setting is the default.
    ///
    /// This is the shortest way to spawn a field from a preset:
    ///
    /// ```
    /// # use bevy::prelude::*;
    /// # use bevy_grass::prelude::*;
    /// # fn setup(mut commands: Commands, mut styles: ResMut<Assets<GrassStyle>>) {
    /// commands.spawn(GrassField::new(
    ///     Vec2::splat(64.0),
    ///     GrassStyle::MEADOW,
    ///     &mut styles,
    /// ));
    /// # }
    /// ```
    ///
    /// To make several fields share one style, add it once and use its handle
    /// in each field instead:
    ///
    /// ```
    /// # use bevy::prelude::*;
    /// # use bevy_grass::prelude::*;
    /// # fn setup(mut commands: Commands, mut styles: ResMut<Assets<GrassStyle>>) {
    /// let style = styles.add(GrassStyle::MEADOW);
    /// commands.spawn(GrassField {
    ///     size: Vec2::splat(64.0),
    ///     style: style.clone(),
    ///     ..default()
    /// });
    /// commands.spawn(GrassField {
    ///     size: Vec2::splat(32.0),
    ///     style,
    ///     ..default()
    /// });
    /// # }
    /// ```
    #[must_use]
    pub fn new(size: Vec2, style: GrassStyle, styles: &mut Assets<GrassStyle>) -> Self {
        Self {
            size,
            style: styles.add(style),
            ..Self::default()
        }
    }

    /// The same as [`GrassField::new`], named for callers who start from a
    /// preset.
    #[must_use]
    pub fn from_preset(size: Vec2, preset: GrassStyle, styles: &mut Assets<GrassStyle>) -> Self {
        Self::new(size, preset, styles)
    }
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
/// moving interactors leave trails. (The 1.5 s is
/// [`GrassSettings::trample_recovery`](crate::GrassSettings::trample_recovery),
/// and the limits below are settings too.)
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
/// The numbers are the defaults of [`GrassSettings`](crate::GrassSettings).
///
/// - **Radius: 2 m**
///   ([`max_interactor_radius`](crate::GrassSettings::max_interactor_radius)).
///   A bigger `radius` is cut to it, and the plugin warns once about the
///   entity. (Each interactor is stamped in a fixed footprint, which is why a
///   hundred cost what one does.) For one large object, use a
///   [`GrassInteractorFootprint`]; for a few separate contacts, a
///   [`GrassInteractorSpread`]; or raise the setting: the cost of an
///   interactor grows with the square of its radius.
/// - **Count: [`GrassSettings::max_interactors`](crate::GrassSettings::max_interactors).**
///   A camera stamps that many a frame, the ones nearest to it first.
/// - **Distance: a 64 m window**
///   ([`trample_cell_size`](crate::GrassSettings::trample_cell_size) × 256
///   cells). Each camera tracks a 64 × 64 m square of ground, centred on the
///   camera (on the ground it looks at, for an orthographic one).
///   Interactors outside it do nothing, and trails outside it are forgotten.
/// - **Height.** Interaction is horizontal: with `max_height` at `None`, an
///   interactor flattens the grass below it at any height.
#[derive(Component, Debug, Clone, Copy, PartialEq, Reflect)]
#[reflect(Component, Default, Debug, PartialEq)]
#[require(Transform)]
pub struct GrassInteractor {
    /// Radius of the flattened disc, in metres. Pushes beyond
    /// [`GrassSettings::max_interactor_radius`](crate::GrassSettings::max_interactor_radius)
    /// (2 m by default) are clipped.
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

/// Extra discs for one [`GrassInteractor`], in its local space. Use it for
/// something larger than one disc: a vehicle, a wall, a large creature.
///
/// One [`GrassInteractor`] is one disc, and the grass pushes at most
/// [`GrassSettings::max_interactor_radius`](crate::GrassSettings::max_interactor_radius)
/// (2 m by default) from it. A spread says "stamp this interactor here, and
/// here too", which covers a large object without adding entities or
/// changing the interaction model. An empty `offsets` means one disc at the
/// entity's origin, the same as having no spread at all.
///
/// ```
/// # use bevy::prelude::*;
/// # use bevy_grass::prelude::*;
/// # fn setup(mut commands: Commands) {
/// // A wall-like box: its footprint covered with discs of at most 2 m.
/// let (radius, spread) = GrassInteractorSpread::covering_aabb(
///     Vec3::ZERO,
///     Vec3::new(4.0, 0.5, 0.5),
///     2.0,
/// );
/// commands.spawn((
///     GrassInteractor { radius, ..default() },
///     spread,
/// ));
/// # }
/// ```
///
/// The offsets are local, so a moving, rotating or scaled body carries them
/// along. Each disc pushes grass away from its own centre, which suits a few
/// feet or wheels; for one large solid object a [`GrassInteractorFootprint`]
/// flattens it as one shape instead.
#[derive(Component, Debug, Clone, Default, PartialEq, Reflect)]
#[reflect(Component, Default, Debug, PartialEq)]
#[require(GrassInteractor)]
pub struct GrassInteractorSpread {
    /// Disc centres in the interactor's local space. An empty list means one
    /// disc at the entity's origin.
    pub offsets: Vec<Vec3>,
}

impl GrassInteractorSpread {
    /// Most discs [`GrassInteractorSpread::covering_aabb`] puts in one
    /// spread. A larger footprint is sampled coarsely rather than making one
    /// object take every interactor slot in a camera.
    pub const MAX_DISCS: usize = 32;

    /// No extra discs: one disc at the interactor's origin.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The discs covering a local AABB, and the radius they use.
    ///
    /// `center` and `half_extents` are in the interactor's local space,
    /// `radius` is the largest disc the grass may stamp (usually
    /// [`GrassSettings::max_interactor_radius`](crate::GrassSettings::max_interactor_radius)).
    /// One disc covers a small AABB. A larger one gets a grid of discs of at
    /// most `radius`, and at most [`GrassInteractorSpread::MAX_DISCS`] discs;
    /// a footprint too large for that is sampled coarsely.
    #[must_use]
    pub fn covering_aabb(center: Vec3, half_extents: Vec3, radius: f32) -> (f32, Self) {
        let max_radius = if radius.is_finite() {
            radius.max(0.0)
        } else {
            1.0
        };
        let center = Vec3::new(
            finite_coordinate(center.x),
            finite_coordinate(center.y),
            finite_coordinate(center.z),
        );
        let half = Vec3::new(
            finite_extent(half_extents.x),
            finite_extent(half_extents.y),
            finite_extent(half_extents.z),
        );
        let diagonal = half.x.hypot(half.z);
        if diagonal <= max_radius {
            return (
                diagonal,
                Self {
                    offsets: vec![center],
                },
            );
        }
        if max_radius <= 0.0 {
            return (0.0, Self::new());
        }

        let width = 2.0 * half.x;
        let depth = 2.0 * half.z;
        // A cell of side r√2 is covered by a disc of radius r, corners and all.
        let cell = max_radius * std::f32::consts::SQRT_2;
        let mut columns = cell_count(width, cell);
        let mut rows = cell_count(depth, cell);
        if columns.saturating_mul(rows) > Self::MAX_DISCS {
            // Keep the footprint's proportions while fitting the cap.
            let aspect = if depth > 0.0 {
                width / depth
            } else {
                f32::INFINITY
            };
            columns = (Self::MAX_DISCS as f32 * aspect)
                .sqrt()
                .round()
                .clamp(1.0, Self::MAX_DISCS as f32) as usize;
            rows = (Self::MAX_DISCS / columns).max(1);
        }

        let step_x = width / columns as f32;
        let step_z = depth / rows as f32;
        let mut offsets = Vec::with_capacity(columns * rows);
        for column in 0..columns {
            let x = center.x - half.x + (column as f32 + 0.5) * step_x;
            for row in 0..rows {
                let z = center.z - half.z + (row as f32 + 0.5) * step_z;
                offsets.push(Vec3::new(x, center.y, z));
            }
        }
        (max_radius, Self { offsets })
    }
}

/// The box one [`GrassInteractor`] flattens instead of a disc: a vehicle, a
/// crate, a fallen log, a large creature.
///
/// The grass under the box lies flat. Where the box moves, the grass under it
/// lies down the way it moves, and only the grass along its sides is pushed
/// out sideways, the way a roller or a sledge lays it; a box at rest pushes
/// the grass out from its nearest edge. Past the edges the push fades out
/// over a short margin.
///
/// The box is in the entity's local space and follows its `GlobalTransform`:
/// the grass is flattened under the box's shadow on the ground, so a crate
/// tipped onto its side or a log that falls over flattens where it lies.
/// [`GrassInteractorFromCollider`](crate::GrassInteractorFromCollider) fills
/// this in from a physics collider.
///
/// ```
/// # use bevy::prelude::*;
/// # use bevy_grass::prelude::*;
/// # fn setup(mut commands: Commands) {
/// // A 6 × 3 m cart.
/// commands.spawn((
///     Transform::from_xyz(0.0, 1.0, 0.0),
///     GrassInteractorFootprint::new(Vec3::ZERO, Vec3::new(3.0, 1.0, 1.5)),
/// ));
/// # }
/// ```
///
/// With a footprint, the interactor's `radius` and any
/// [`GrassInteractorSpread`] are not used; its `strength` and `max_height`
/// are. A box is clipped to
/// [`GrassSettings::MAX_INTERACTOR_RADIUS`](crate::GrassSettings::MAX_INTERACTOR_RADIUS)
/// (16 m) from its centre along each side.
#[derive(Component, Debug, Clone, Copy, PartialEq, Reflect)]
#[reflect(Component, Default, Debug, PartialEq)]
#[require(GrassInteractor)]
pub struct GrassInteractorFootprint {
    /// The centre of the box, in the entity's local space.
    pub center: Vec3,
    /// Half the size of the box along each local axis.
    pub half_extents: Vec3,
    /// How rounded the box looks from above, in metres: 0 for a box, the
    /// radius for a ball or a capsule. An upright capsule with a rounding of
    /// its radius is a disc; lying down, it is a capsule-shaped strip.
    pub rounding: f32,
}

impl GrassInteractorFootprint {
    /// A box of `half_extents` around `center`, with square corners.
    #[must_use]
    pub fn new(center: Vec3, half_extents: Vec3) -> Self {
        Self {
            center,
            half_extents,
            rounding: 0.0,
        }
    }
}

impl Default for GrassInteractorFootprint {
    /// A 1 m cube on the entity's origin.
    fn default() -> Self {
        Self::new(Vec3::ZERO, Vec3::splat(0.5))
    }
}

/// The local disc centres of an interactor: its spread, or one at the origin.
pub(crate) fn interactor_offsets(spread: Option<&GrassInteractorSpread>) -> &[Vec3] {
    spread
        .filter(|spread| !spread.offsets.is_empty())
        .map_or(&[Vec3::ZERO], |spread| spread.offsets.as_slice())
}

/// A coordinate that can go into an offset: finite, or 0.
fn finite_coordinate(value: f32) -> f32 {
    if value.is_finite() { value } else { 0.0 }
}

/// A half extent as a distance: absolute, finite and non-negative. The upper
/// bound keeps `2 * half` finite, so the grid arithmetic below cannot overflow
/// into an infinite offset.
fn finite_extent(value: f32) -> f32 {
    if value.is_finite() {
        value.abs().min(f32::MAX / 4.0)
    } else {
        0.0
    }
}

/// One axis of a grid that covers `length`: at least one cell, and never more
/// than [`GrassInteractorSpread::MAX_DISCS`], which keeps a degenerate or
/// enormous AABB from making an enormous list.
fn cell_count(length: f32, cell: f32) -> usize {
    if length <= 0.0 || !cell.is_finite() {
        return 1;
    }
    (length / cell.max(1e-4))
        .ceil()
        .clamp(1.0, GrassInteractorSpread::MAX_DISCS as f32) as usize
}

/// Warns, once per entity, about an interactor bigger than the grass can
/// stamp.
pub(crate) fn warn_oversized_interactors(
    interactors: Query<(Entity, Ref<GrassInteractor>)>,
    settings: Res<GrassSettings>,
    mut warned: Local<HashSet<Entity>>,
) {
    let largest = settings.sanitized().max_interactor_radius;
    let settings_changed = settings.is_changed();
    for (entity, interactor) in &interactors {
        // New and changed interactors are checked; so is every interactor
        // when the limit may have moved.
        if !(settings_changed || interactor.is_changed()) {
            continue;
        }
        let too_big = interactor.radius > largest;
        if too_big && warned.insert(entity) {
            warn!(
                "bevy_grass: the GrassInteractor on {entity} has radius {} m, but interactors \
                 push at most {largest} m (GrassSettings::max_interactor_radius); the rest is \
                 ignored. Use a GrassInteractorFootprint for one large object, or several \
                 discs no bigger than that (see GrassInteractorSpread).",
                interactor.radius,
            );
        } else if !too_big {
            warned.remove(&entity);
        }
    }
}

#[cfg(test)]
mod tests;
