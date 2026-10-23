use std::f32::consts::FRAC_PI_2;

use bevy::app::App;
use bevy::math::{Quat, Vec2, Vec3, Vec4Swizzles};
use bevy::transform::components::{GlobalTransform, Transform};
use bevy_rapier3d::prelude::{Collider, ColliderDisabled, Sensor};

use crate::physics::tests::{close, ground_stamp, on_ground};
use crate::{GrassInteractor, GrassInteractorFootprint};

use super::*;

/// The collider bridge with the Rapier backend, and no physics plugin: the
/// tests set what Rapier would (a collider's scale, a `GlobalTransform`).
fn app() -> App {
    let mut app = App::new();
    crate::physics::plugin(&mut app);
    app
}

fn footprint(app: &App, entity: Entity) -> GrassInteractorFootprint {
    *app.world().get::<GrassInteractorFootprint>(entity).unwrap()
}

#[test]
fn a_character_capsule_becomes_a_round_footprint() {
    let mut app = app();
    let entity = app
        .world_mut()
        .spawn((
            Collider::capsule_y(0.5, 0.4),
            GrassInteractorFromCollider::default(),
        ))
        .id();
    app.update();

    // Rapier's capsule_y takes the half height between the hemispheres.
    let fitted = footprint(&app, entity);
    assert_eq!(fitted.half_extents, Vec3::new(0.4, 0.9, 0.4));
    assert_eq!(fitted.rounding, 0.4);
    let stamp = ground_stamp(&app, entity);
    assert_eq!(stamp.shape.zw(), Vec2::ZERO, "upright: a disc");
    assert_eq!(
        app.world().get::<GrassInteractor>(entity).unwrap().strength,
        1.0
    );
}

#[test]
fn a_large_box_is_one_square_footprint() {
    let mut app = app();
    let entity = app
        .world_mut()
        .spawn((
            // Rapier calls these half extents: a 6 m by 3 m footprint.
            Collider::cuboid(3.0, 0.5, 1.5),
            GrassInteractorFromCollider::default(),
        ))
        .id();
    app.update();

    let fitted = footprint(&app, entity);
    assert_eq!(fitted.half_extents, Vec3::new(3.0, 0.5, 1.5));
    assert_eq!(fitted.rounding, 0.0);
}

#[test]
fn the_adapter_passes_scale_strength_and_height_through() {
    let mut app = app();
    let entity = app
        .world_mut()
        .spawn((
            Collider::ball(0.3),
            GrassInteractorFromCollider {
                scale: 0.5,
                strength: 0.4,
                max_height: Some(1.5),
            },
        ))
        .id();
    app.update();

    let fitted = footprint(&app, entity);
    assert!((fitted.half_extents.x - 0.15).abs() < 1e-6);
    assert!((fitted.rounding - 0.15).abs() < 1e-6, "a ball is round");
    let interactor = app.world().get::<GrassInteractor>(entity).unwrap();
    assert_eq!(interactor.strength, 0.4);
    assert_eq!(interactor.max_height, Some(1.5));
}

#[test]
fn a_disabled_collider_body_or_sensor_flattens_nothing() {
    let mut app = app();
    let disabled = app
        .world_mut()
        .spawn((
            Collider::ball(0.3),
            GrassInteractorFromCollider::default(),
            ColliderDisabled,
        ))
        .id();
    let body = app
        .world_mut()
        .spawn((
            Collider::ball(0.3),
            GrassInteractorFromCollider::default(),
            RigidBodyDisabled,
        ))
        .id();
    let sensor = app
        .world_mut()
        .spawn((
            Collider::ball(0.3),
            GrassInteractorFromCollider::default(),
            Sensor,
        ))
        .id();
    app.update();

    for entity in [disabled, body, sensor] {
        assert_eq!(
            app.world().get::<GrassInteractor>(entity).unwrap().strength,
            0.0
        );
    }

    app.world_mut()
        .entity_mut(disabled)
        .remove::<ColliderDisabled>();
    app.update();
    assert_eq!(
        app.world()
            .get::<GrassInteractor>(disabled)
            .unwrap()
            .strength,
        1.0
    );
}

#[test]
fn a_child_collider_stops_with_its_disabled_parent_body() {
    let mut app = app();
    let body = app.world_mut().spawn(RigidBody::Dynamic).id();
    let child = app
        .world_mut()
        .spawn((
            Collider::ball(0.3),
            GrassInteractorFromCollider::default(),
            ChildOf(body),
        ))
        .id();
    app.update();
    assert_eq!(
        app.world().get::<GrassInteractor>(child).unwrap().strength,
        1.0
    );

    app.world_mut().entity_mut(body).insert(RigidBodyDisabled);
    app.update();

    assert_eq!(
        app.world().get::<GrassInteractor>(child).unwrap().strength,
        0.0
    );
}

#[test]
fn removing_the_collider_stops_the_interaction_until_it_returns() {
    let mut app = app();
    let entity = app
        .world_mut()
        .spawn((Collider::ball(0.3), GrassInteractorFromCollider::default()))
        .id();
    app.update();
    assert_eq!(
        app.world().get::<GrassInteractor>(entity).unwrap().strength,
        1.0
    );

    app.world_mut().entity_mut(entity).remove::<Collider>();
    app.update();
    assert_eq!(
        app.world().get::<GrassInteractor>(entity).unwrap().strength,
        0.0
    );
    assert!(app.world().get::<GrassColliderBounds>(entity).is_none());

    app.world_mut()
        .entity_mut(entity)
        .insert(Collider::ball(0.5));
    app.update();
    assert_eq!(
        app.world().get::<GrassInteractor>(entity).unwrap().strength,
        1.0
    );
    assert_eq!(footprint(&app, entity).half_extents, Vec3::splat(0.5));
}

#[test]
fn a_scaled_collider_is_covered_once_not_twice() {
    let mut app = app();
    // What Rapier does for a body whose transform has scale 2: the collider
    // is a 12 m by 6 m box in the world.
    let mut collider = Collider::cuboid(3.0, 0.5, 1.5);
    collider.set_scale(Vec3::splat(2.0), 10);
    let transform = Transform::from_scale(Vec3::splat(2.0));
    let entity = app
        .world_mut()
        .spawn((
            collider,
            transform,
            GlobalTransform::from(transform),
            GrassInteractorFromCollider::default(),
        ))
        .id();
    app.update();

    let (_, half, _) = on_ground(&ground_stamp(&app, entity));
    assert!(close(half, Vec2::new(6.0, 3.0)), "{half}");
}

#[test]
fn a_capsule_that_falls_over_flattens_along_its_length() {
    let mut app = app();
    // A 4 m log.
    let standing = Transform::from_xyz(5.0, 0.3, -2.0);
    let entity = app
        .world_mut()
        .spawn((
            Collider::capsule_y(1.7, 0.3),
            standing,
            GlobalTransform::from(standing),
            GrassInteractorFromCollider::default(),
        ))
        .id();
    app.update();
    let (center, half, _) = on_ground(&ground_stamp(&app, entity));
    assert_eq!(center, Vec2::new(5.0, -2.0));
    assert!(close(half, Vec2::splat(0.3)));

    // It falls over along X.
    let lying = standing.with_rotation(Quat::from_rotation_z(FRAC_PI_2));
    *app.world_mut().get_mut::<GlobalTransform>(entity).unwrap() = GlobalTransform::from(lying);
    app.update();

    let stamp = ground_stamp(&app, entity);
    let (_, half, along) = on_ground(&stamp);
    assert!(along.dot(Vec2::X).abs() > 0.9999, "{along}");
    assert!(close(half, Vec2::new(2.0, 0.3)), "{half}");
    assert_eq!(stamp.motion.z, 0.3, "with round ends");
}
