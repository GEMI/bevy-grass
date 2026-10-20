//! The Avian backend: `avian3d` colliders as [`GrassColliderBounds`].
//!
//! The `avian` cargo feature compiles this module. It never changes how Bevy
//! is compiled: `avian3d` is brought in with `default-features = false` and
//! only its `3d` and `parry-f32` features, so no `bevy_gizmos`,
//! `bevy_picking` or other Bevy feature is turned on.
//!
//! It only reports the collider's size and whether it is active; turning that
//! into discs is the engine-neutral fit in the parent module.

use avian3d::prelude::{Collider, ColliderDisabled, ColliderOf, RigidBodyDisabled, Sensor};
use bevy::app::{App, Update};
use bevy::ecs::prelude::*;

use super::{GrassColliderBounds, GrassColliderSystems, GrassInteractorFromCollider};

/// Adds the Avian backend to the collider bridge.
pub(super) fn plugin(app: &mut App) {
    app.add_systems(Update, write_bounds.in_set(GrassColliderSystems::Bounds))
        .add_observer(forget_bounds);
}

/// Writes the bounds of every adapted Avian collider, and inserts them on an
/// entity that has none yet.
///
/// `Collider::shape_scaled` is the shape Avian simulates: Avian scales the
/// collider by its transform, so its local box already has the scale applied,
/// as [`GrassColliderBounds`] wants. (Avian's `ColliderAabb` is a world box
/// that grows as the body turns; the local one keeps the body's own axes.)
#[expect(clippy::type_complexity, reason = "Bevy query types are verbose")]
fn write_bounds(
    mut commands: Commands,
    mut colliders: Query<
        (
            Entity,
            &Collider,
            Option<&mut GrassColliderBounds>,
            Option<&ColliderOf>,
            Has<ColliderDisabled>,
            Has<RigidBodyDisabled>,
            Has<Sensor>,
        ),
        With<GrassInteractorFromCollider>,
    >,
    disabled_bodies: Query<(), With<RigidBodyDisabled>>,
) {
    for (entity, collider, bounds, body, collider_disabled, body_disabled, sensor) in &mut colliders
    {
        let shape = collider.shape_scaled();
        let local = shape.compute_local_aabb();
        let (min, max) = (local.mins, local.maxs);
        // Avian attaches a collider to its body through `ColliderOf`, so a
        // child collider stops with its parent body.
        let body_disabled =
            body_disabled || body.is_some_and(|of| disabled_bodies.contains(of.body));
        let measured = GrassColliderBounds {
            center: (min + max) * 0.5,
            half_extents: (max - min) * 0.5,
            // How rounded it looks from above: a ball's or a capsule's
            // radius, 0 for anything else.
            rounding: shape
                .as_ball()
                .map(|ball| ball.radius)
                .or_else(|| shape.as_capsule().map(|capsule| capsule.radius))
                .unwrap_or(0.0),
            active: !(collider_disabled || body_disabled || sensor),
        };
        match bounds {
            Some(mut bounds) => {
                if *bounds != measured {
                    *bounds = measured;
                }
            }
            None => {
                commands.entity(entity).insert(measured);
            }
        }
    }
}

/// Removing a collider removes its bounds, which stops the interaction.
fn forget_bounds(
    removed: On<Remove<Collider>>,
    adapted: Query<(), With<GrassColliderBounds>>,
    mut commands: Commands,
) {
    if adapted.contains(removed.entity) {
        commands
            .entity(removed.entity)
            .try_remove::<GrassColliderBounds>();
    }
}

#[cfg(test)]
mod tests;
