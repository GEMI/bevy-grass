//! Colliders as grass interactors, without depending on a physics engine.
//!
//! [`GrassInteractorFromCollider`] asks for an interactor that follows a
//! collider, and a [`GrassColliderBounds`] says how big the collider is. The
//! `rapier` and `avian` cargo features each add a backend that reads its
//! engine's `Collider` and writes the bounds; this module turns the bounds
//! into a [`GrassInteractor`] with a [`GrassInteractorFootprint`]. Everything
//! that is policy (scale, what a disabled collider does) lives here, once, so
//! a backend is only a translation, and another engine (or a game's own
//! collider type) needs no change to the core.
//!
//! Nothing here reads a physics transform. The engines already write
//! `Transform`/`GlobalTransform`, the grass reads `GlobalTransform`, and the
//! backends only report a size. That keeps the bridge independent of the
//! physics schedule, and it works with interpolation and custom schedules.

use bevy::app::{App, Update};
use bevy::ecs::prelude::*;
use bevy::log::warn;
use bevy::math::Vec3;
use bevy::reflect::Reflect;
use bevy::reflect::std_traits::ReflectDefault;
use bevy::transform::components::GlobalTransform;

use crate::{GrassInteractor, GrassInteractorFootprint};

#[cfg(feature = "avian")]
mod avian;
#[cfg(feature = "rapier")]
mod rapier;

/// Derive a [`GrassInteractor`] from this entity's physics collider.
///
/// Add it next to a collider; the plugin fills in a
/// [`GrassInteractorFootprint`] from the collider's size and shape, so the
/// grass is flattened under the whole body as one shape. A capsule character
/// is a disc; a vehicle, crate, wall or large creature a box whose sides push
/// the grass out while the grass under it lies the way it moves; a capsule or
/// log that falls over a strip along its length. The footprint follows the
/// body's scale. A disabled collider, a disabled rigid body or a sensor
/// flattens nothing.
///
/// The size comes from a [`GrassColliderBounds`], which a physics backend
/// writes: turn on the `rapier` cargo feature for `bevy_rapier3d` colliders
/// or `avian` for `avian3d` colliders. With Rapier:
///
/// ```
/// # use bevy::prelude::*;
/// # use bevy_grass::prelude::*;
/// # #[cfg(feature = "rapier")]
/// # fn setup(mut commands: Commands) {
/// # use bevy_rapier3d::prelude::*;
/// commands.spawn((
///     RigidBody::Dynamic,
///     Collider::capsule_y(0.5, 0.4),
///     GrassInteractorFromCollider::default(),
/// ));
/// # }
/// ```
///
/// Put it on the entity that has the `Collider`; for a body made of several
/// child colliders, put one on each child that should touch the grass. With
/// no physics engine, or another one, insert a [`GrassColliderBounds`]
/// yourself. If no bounds arrive within a few frames the plugin warns once,
/// which is what a missing cargo feature or a `bevy_rapier3d`/`avian3d`
/// version other than the one this crate is built against looks like.
///
/// Changing the collider's shape or the body's scale updates the grass on the
/// next frame. Moving or turning the body needs no update: the footprint is
/// local, and the grass lays it on the ground from `GlobalTransform` every
/// frame.
///
/// `GrassInteractor` is a required component: adding this one adds it too.
/// While the adapter is present it owns the interactor's strength, height and
/// footprint; change this component's fields rather than editing the derived
/// ones. Removing this component leaves the last derived interactor in place,
/// so a game can switch from automatic to hand-written interaction without
/// losing its tuning. Removing only the collider stops the interaction
/// (strength becomes 0) but leaves the last footprint until a collider
/// returns; removing the entity removes both.
#[derive(Component, Debug, Clone, Copy, PartialEq, Reflect)]
#[reflect(Component, Default, Debug, PartialEq)]
#[require(GrassInteractor, GrassInteractorFootprint, GrassInteractorFit)]
pub struct GrassInteractorFromCollider {
    /// Scales the footprint around its centre. 1.0 uses the collider as it
    /// is; 1.2 makes a character's trail a little wider. Below 0 is 0, and a
    /// non-finite value is treated as 1.
    pub scale: f32,
    /// How flat the grass lies under the body, 0 (untouched) to 1 (flat).
    pub strength: f32,
    /// How high above the ground the entity's position may be and still
    /// flatten grass, in metres. `None` means any height; a flying creature
    /// wants `Some(height)`.
    pub max_height: Option<f32>,
}

impl Default for GrassInteractorFromCollider {
    fn default() -> Self {
        Self {
            scale: 1.0,
            strength: 1.0,
            max_height: None,
        }
    }
}

/// The collider a [`GrassInteractorFromCollider`] flattens grass under: its
/// bounding box along the entity's own axes, and how rounded it is.
///
/// The `rapier` and `avian` backends write it from their engine's
/// `Collider`, in [`GrassColliderSystems::Bounds`]. Write it yourself for a
/// collider type of your own, or with no physics engine at all:
///
/// ```
/// # use bevy::prelude::*;
/// # use bevy_grass::prelude::*;
/// # fn setup(mut commands: Commands) {
/// // A 6 × 2 × 3 m cart.
/// commands.spawn((
///     Transform::from_xyz(0.0, 1.0, 0.0),
///     GrassColliderBounds::new(Vec3::ZERO, Vec3::new(3.0, 1.0, 1.5)),
///     GrassInteractorFromCollider::default(),
/// ));
/// # }
/// ```
///
/// The sizes are what the physics engine simulates: in metres, along the
/// entity's local axes, with the entity's scale already applied (both
/// engines scale a collider by its transform). The plugin divides the scale
/// back out for the footprint, which `GlobalTransform` scales again. Removing
/// the component stops the interaction, as removing the collider does.
#[derive(Component, Debug, Clone, Copy, PartialEq, Reflect)]
#[reflect(Component, Default, Debug, PartialEq)]
pub struct GrassColliderBounds {
    /// The centre of the box, from the entity's origin, in metres along its
    /// local axes.
    pub center: Vec3,
    /// Half the size of the box along each local axis, in metres.
    pub half_extents: Vec3,
    /// How rounded the collider looks from above, in metres: 0 for a box, the
    /// radius for a ball or a capsule. See
    /// [`GrassInteractorFootprint::rounding`].
    pub rounding: f32,
    /// Whether the collider pushes grass. A backend sets it to `false` for a
    /// disabled collider, a disabled rigid body and a sensor.
    pub active: bool,
}

impl GrassColliderBounds {
    /// An active box of `half_extents` around `center`, with square corners.
    #[must_use]
    pub fn new(center: Vec3, half_extents: Vec3) -> Self {
        Self {
            center,
            half_extents,
            rounding: 0.0,
            active: true,
        }
    }
}

impl Default for GrassColliderBounds {
    fn default() -> Self {
        Self::new(Vec3::ZERO, Vec3::ZERO)
    }
}

/// The collider bridge's systems, in `Update`, in this order.
///
/// A system that writes [`GrassColliderBounds`] for a collider type of your
/// own belongs `.in_set(GrassColliderSystems::Bounds)`, so its bounds are
/// fitted the same frame:
///
/// ```no_run
/// # use bevy::prelude::*;
/// # use bevy_grass::prelude::*;
/// # fn my_collider_bounds() {}
/// # let mut app = App::new();
/// app.add_systems(Update, my_collider_bounds.in_set(GrassColliderSystems::Bounds));
/// ```
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum GrassColliderSystems {
    /// The physics backends write [`GrassColliderBounds`] from their
    /// engine's colliders.
    Bounds,
    /// Every [`GrassInteractorFromCollider`] is fitted to its bounds.
    Fit,
}

/// How many frames an adapter may wait for its bounds before it is worth a
/// warning: long enough for a body spawned in `Startup` to get its collider.
const MISSING_BOUNDS_FRAMES: u8 = 3;

/// Whether bounds ever arrived for an adapter, and how long it has waited.
/// Private: it is an implementation detail of [`GrassInteractorFromCollider`].
#[derive(Component, Debug, Clone, Copy, Default, PartialEq)]
struct GrassInteractorFit {
    bounded: bool,
    frames_waiting: u8,
}

/// Adds the collider bridge, and the backends the cargo features turn on.
pub(super) fn plugin(app: &mut App) {
    app.configure_sets(
        Update,
        (GrassColliderSystems::Bounds, GrassColliderSystems::Fit).chain(),
    )
    .add_systems(Update, fit_interactors.in_set(GrassColliderSystems::Fit));
    #[cfg(feature = "rapier")]
    rapier::plugin(app);
    #[cfg(feature = "avian")]
    avian::plugin(app);
}

/// Fits every adapted entity to its bounds. It runs in `Update`, before the
/// `PostUpdate` extraction, with the scale the last frame propagated; a
/// collider or scale changed during physics is seen one frame later, which
/// is fine for a footprint. Only a change of shape or scale writes anything:
/// a moving or turning body is laid on the ground at extraction.
#[expect(clippy::type_complexity, reason = "Bevy query types are verbose")]
fn fit_interactors(
    mut query: Query<(
        Entity,
        &GrassInteractorFromCollider,
        Option<&GrassColliderBounds>,
        &GlobalTransform,
        &mut GrassInteractor,
        &mut GrassInteractorFootprint,
        &mut GrassInteractorFit,
    )>,
) {
    for (entity, source, bounds, transform, mut interactor, mut footprint, mut fit) in &mut query {
        let Some(bounds) = bounds else {
            if interactor.strength != 0.0 {
                interactor.strength = 0.0;
            }
            if !fit.bounded && fit.frames_waiting < MISSING_BOUNDS_FRAMES {
                fit.frames_waiting += 1;
                if fit.frames_waiting == MISSING_BOUNDS_FRAMES {
                    warn_missing_bounds(entity);
                }
            }
            continue;
        };
        if !fit.bounded {
            fit.bounded = true;
        }

        let scale = if source.scale.is_finite() {
            source.scale.max(0.0)
        } else {
            1.0
        };
        // The bounds are in metres, scale applied; the footprint is in the
        // local space `GlobalTransform` maps to the world.
        let local = transform.scale().map(snapped);
        let fitted = GrassInteractorFootprint {
            center: unscaled(bounds.center, local),
            half_extents: unscaled(bounds.half_extents * scale, local),
            rounding: bounds.rounding * scale,
        };
        if *footprint != fitted {
            *footprint = fitted;
        }
        let strength = if bounds.active { source.strength } else { 0.0 };
        if interactor.strength != strength {
            interactor.strength = strength;
        }
        if interactor.max_height != source.max_height {
            interactor.max_height = source.max_height;
        }
    }
}

/// `scale` without the last few bits of noise. A turning body's
/// `GlobalTransform` decomposes to a scale of 0.99999994 one frame and 1 the
/// next; unsnapped, the footprint would be rewritten every frame. About 3e-5
/// of the value is kept, far below what grass can show. (Rapier snaps a
/// collider's scale for the same reason.)
fn snapped(scale: f32) -> f32 {
    if scale.is_finite() {
        f32::from_bits(scale.to_bits().wrapping_add(0x80) & !0xFF)
    } else {
        scale
    }
}

/// `value` (in metres) in the local space of a transform with `scale`. An
/// axis scaled to nothing has nothing on it.
fn unscaled(value: Vec3, scale: Vec3) -> Vec3 {
    let axis = |value: f32, scale: f32| {
        if scale == 0.0 || !scale.is_finite() {
            0.0
        } else {
            value / scale
        }
    };
    Vec3::new(
        axis(value.x, scale.x),
        axis(value.y, scale.y),
        axis(value.z, scale.z),
    )
}

/// Says why an adapter got no bounds, for the backends this build has.
fn warn_missing_bounds(entity: Entity) {
    let backends: &[&str] = &[
        #[cfg(feature = "rapier")]
        "bevy_rapier3d 0.37",
        #[cfg(feature = "avian")]
        "avian3d 0.8",
    ];
    if backends.is_empty() {
        warn!(
            "bevy_grass: {entity} has a GrassInteractorFromCollider but no collider size, so it \
             flattens nothing. No physics backend is compiled in: turn on bevy_grass's `rapier` \
             or `avian` cargo feature, or insert a GrassColliderBounds yourself."
        );
    } else {
        warn!(
            "bevy_grass: {entity} has a GrassInteractorFromCollider but no collider size, so it \
             flattens nothing. Put it on the entity that has the Collider, and check that the \
             game uses {}: with another version Cargo builds two copies, and bevy_grass cannot \
             see the game's colliders.",
            backends.join(" or "),
        );
    }
}

#[cfg(test)]
mod tests;
