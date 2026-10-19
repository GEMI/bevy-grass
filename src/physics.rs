//! Optional Rapier integration: derive a [`GrassInteractor`] from a
//! `bevy_rapier3d` collider.
//!
//! The `rapier` cargo feature compiles this module. It never changes how Bevy
//! is compiled: `bevy_rapier3d` is brought in with `default-features = false`
//! and only its `dim3` feature, so no `bevy_gizmos`, `bevy_picking` or other
//! Bevy feature is turned on.
//!
//! The bridge reads the collider's local AABB, not a physics transform.
//! Rapier already writes `Transform`/`GlobalTransform`, and the grass reads
//! `GlobalTransform`; this module only decides the footprint. That keeps it
//! independent of the physics schedule and works with interpolation and
//! custom schedules.

use bevy::app::{App, Update};
use bevy::ecs::prelude::*;
use bevy::math::Vec3;
use bevy::reflect::Reflect;
use bevy::reflect::std_traits::ReflectDefault;
use bevy_rapier3d::prelude::{Collider, ColliderDisabled, RigidBodyDisabled, Sensor};

use crate::{GrassInteractor, GrassInteractorSpread, GrassSettings};

/// Derive a [`GrassInteractor`] from this entity's Rapier [`Collider`].
///
/// Add it next to a collider; the plugin fills in the interactor's radius
/// (and, for a large collider, a [`GrassInteractorSpread`]) from the
/// collider's local AABB. A capsule character gets one disc; a vehicle, wall
/// or large creature gets several discs, each no bigger than
/// [`GrassSettings::max_interactor_radius`](crate::GrassSettings::max_interactor_radius).
/// A disabled collider, a disabled rigid body on the same entity, or a
/// sensor flattens nothing.
///
/// ```no_run
/// # use bevy::prelude::*;
/// # use bevy_grass::prelude::*;
/// # use bevy_rapier3d::prelude::*;
/// # fn setup(mut commands: Commands) {
/// commands.spawn((
///     RigidBody::Dynamic,
///     Collider::capsule_y(0.5, 0.4),
///     GrassInteractorFromCollider::default(),
/// ));
/// # }
/// ```
///
/// The component is only available with the `rapier` cargo feature. The
/// footprint comes from the collider, so changing the collider's shape or
/// scale updates the grass on the next frame; moving the body needs no
/// update because the offsets are local and the grass reads
/// `GlobalTransform`.
///
/// `GrassInteractor` is a required component: adding this one adds it too.
/// While the adapter is present it owns the interactor's radius, strength,
/// height and spread; change `radius_scale` or this component's fields rather
/// than editing the derived ones. Removing this component leaves the last
/// derived interactor in place, so a game can switch from automatic to
/// hand-written interaction without losing its tuning. Removing only the
/// collider stops the interaction (strength becomes 0) but leaves the last
/// footprint until a collider returns; removing the entity removes both.
#[derive(Component, Debug, Clone, Copy, PartialEq, Reflect)]
#[reflect(Component, Default, Debug, PartialEq)]
#[require(GrassInteractor, GrassInteractorSpread, GrassInteractorFit)]
pub struct GrassInteractorFromCollider {
    /// Multiplies the radius derived from the collider. 1.0 uses it as it
    /// is; 1.2 makes a character's trail a little wider. Below 0 is 0, and a
    /// non-finite value is treated as 1.
    pub radius_scale: f32,
    /// How flat the grass lies at the centre, 0 (untouched) to 1 (flat).
    pub strength: f32,
    /// How high above the ground the entity's position may be and still
    /// flatten grass, in metres. `None` means any height; a flying creature
    /// wants `Some(height)`.
    pub max_height: Option<f32>,
}

impl Default for GrassInteractorFromCollider {
    fn default() -> Self {
        Self {
            radius_scale: 1.0,
            strength: 1.0,
            max_height: None,
        }
    }
}

/// The last collider footprint the adapter fitted, so a moving body does not
/// rebuild its spread every frame. Private: it is an implementation detail of
/// [`GrassInteractorFromCollider`].
#[derive(Component, Debug, Clone, Copy, Default, PartialEq)]
struct GrassInteractorFit {
    center: Vec3,
    half: Vec3,
    max_radius: f32,
    radius_scale: f32,
}

/// Adds the Rapier bridge to the grass plugin.
pub(super) fn plugin(app: &mut App) {
    app.add_systems(Update, sync_interactors);
}

/// Fits every adapted collider. Runs in `Update`, before the `PostUpdate`
/// extraction; a collider shape changed during physics is seen one frame
/// later, which is fine for a footprint.
#[expect(clippy::type_complexity, reason = "Bevy query types are verbose")]
fn sync_interactors(
    settings: Res<GrassSettings>,
    mut query: Query<(
        Option<&Collider>,
        &GrassInteractorFromCollider,
        &mut GrassInteractor,
        &mut GrassInteractorSpread,
        &mut GrassInteractorFit,
        Has<ColliderDisabled>,
        Has<RigidBodyDisabled>,
        Has<Sensor>,
    )>,
) {
    let max_radius = settings.sanitized().max_interactor_radius;
    for (
        collider,
        source,
        mut interactor,
        mut spread,
        mut fit,
        collider_disabled,
        body_disabled,
        sensor,
    ) in &mut query
    {
        let Some(collider) = collider else {
            if interactor.strength != 0.0 {
                interactor.strength = 0.0;
            }
            continue;
        };
        let local = collider.local_aabb();
        let center = Vec3::from((local.min + local.max) * 0.5);
        let half = Vec3::from((local.max - local.min) * 0.5);
        let radius_scale = if source.radius_scale.is_finite() {
            source.radius_scale.max(0.0)
        } else {
            1.0
        };
        let footprint = GrassInteractorFit {
            center,
            half,
            max_radius,
            radius_scale,
        };

        // A moving body marks its collider changed every frame, so compare the
        // footprint itself: only a shape or scale change allocates a spread.
        if *fit != footprint {
            let (radius, cover) = GrassInteractorSpread::covering_aabb(center, half, max_radius);
            let radius = (radius * radius_scale).min(max_radius);
            if interactor.radius != radius {
                interactor.radius = radius;
            }
            if spread.offsets != cover.offsets {
                spread.offsets = cover.offsets;
            }
            *fit = footprint;
        }

        let strength = if collider_disabled || body_disabled || sensor {
            0.0
        } else {
            source.strength
        };
        if interactor.strength != strength {
            interactor.strength = strength;
        }
        if interactor.max_height != source.max_height {
            interactor.max_height = source.max_height;
        }
    }
}

#[cfg(test)]
mod tests;
