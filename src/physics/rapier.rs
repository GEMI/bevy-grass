//! The Rapier backend: `bevy_rapier3d` colliders as [`GrassColliderBounds`].
//!
//! The `rapier` cargo feature compiles this module. It never changes how Bevy
//! is compiled: `bevy_rapier3d` is brought in with `default-features = false`
//! and only its `dim3` feature, so no `bevy_gizmos`, `bevy_picking` or other
//! Bevy feature is turned on.
//!
//! It only reports the collider's size and whether it is active; turning that
//! into discs is the engine-neutral fit in the parent module.

use bevy::app::{App, Update};
use bevy::ecs::prelude::*;
use bevy::math::Vec3;
use bevy_rapier3d::prelude::{Collider, ColliderDisabled, RigidBody, RigidBodyDisabled, Sensor};

use super::{GrassColliderBounds, GrassColliderSystems, GrassInteractorFromCollider};

/// Adds the Rapier backend to the collider bridge.
pub(super) fn plugin(app: &mut App) {
    app.add_systems(Update, write_bounds.in_set(GrassColliderSystems::Bounds))
        .add_observer(forget_bounds);
}

/// Writes the bounds of every adapted Rapier collider, and inserts them on
/// an entity that has none yet.
///
/// `Collider::local_aabb` is the shape Rapier simulates: Rapier scales the
/// collider by its `GlobalTransform` (and any `ColliderScale`), so the box
/// already has the scale applied, as [`GrassColliderBounds`] wants.
#[expect(clippy::type_complexity, reason = "Bevy query types are verbose")]
fn write_bounds(
    mut commands: Commands,
    mut colliders: Query<
        (
            Entity,
            &Collider,
            Option<&mut GrassColliderBounds>,
            Has<ColliderDisabled>,
            Has<Sensor>,
        ),
        With<GrassInteractorFromCollider>,
    >,
    bodies: Query<(Has<RigidBody>, Has<RigidBodyDisabled>, Option<&ChildOf>)>,
) {
    for (entity, collider, bounds, collider_disabled, sensor) in &mut colliders {
        let local = collider.local_aabb();
        let measured = GrassColliderBounds {
            center: Vec3::from((local.min + local.max) * 0.5),
            half_extents: Vec3::from((local.max - local.min) * 0.5),
            rounding: rounding(collider),
            active: !(collider_disabled || sensor || body_disabled(entity, &bodies)),
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

/// How rounded a collider looks from above: a ball's or a capsule's radius
/// (scaled, as Rapier simulates it), 0 for anything else.
fn rounding(collider: &Collider) -> f32 {
    if let Some(ball) = collider.raw.as_ball() {
        ball.radius
    } else if let Some(capsule) = collider.raw.as_capsule() {
        capsule.radius
    } else {
        0.0
    }
}

/// Whether the rigid body a collider belongs to is disabled. Rapier attaches
/// a collider to the nearest `RigidBody` at or above it, so a child collider
/// stops with its parent body.
fn body_disabled(
    collider: Entity,
    bodies: &Query<(Has<RigidBody>, Has<RigidBodyDisabled>, Option<&ChildOf>)>,
) -> bool {
    let mut entity = collider;
    while let Ok((body, disabled, parent)) = bodies.get(entity) {
        if disabled {
            return true;
        }
        match parent {
            Some(parent) if !body => entity = parent.parent(),
            _ => return false,
        }
    }
    false
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
