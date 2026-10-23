use std::f32::consts::FRAC_PI_2;

use bevy::app::App;
use bevy::ecs::change_detection::DetectChanges;
use bevy::math::{Quat, Vec2, Vec3, Vec4Swizzles};
use bevy::transform::components::Transform;

use crate::trample::{Stamp, box_stamp};

use super::*;

/// The collider bridge with whatever backends this build has, and no physics
/// engine: the tests write `GrassColliderBounds` and `GlobalTransform` as a
/// backend and Bevy's transform propagation would.
fn app() -> App {
    let mut app = App::new();
    plugin(&mut app);
    app
}

/// An adapted entity with `bounds`, placed by `transform`.
fn spawn(app: &mut App, bounds: GrassColliderBounds, transform: Transform) -> Entity {
    app.world_mut()
        .spawn((
            bounds,
            transform,
            GlobalTransform::from(transform),
            GrassInteractorFromCollider::default(),
        ))
        .id()
}

/// Moves `entity` as transform propagation would.
fn place(app: &mut App, entity: Entity, transform: Transform) {
    let mut entity = app.world_mut().entity_mut(entity);
    *entity.get_mut::<Transform>().unwrap() = transform;
    *entity.get_mut::<GlobalTransform>().unwrap() = GlobalTransform::from(transform);
}

/// The stamp the grass gets for `entity` this frame, at rest: its footprint
/// laid on the ground as extraction lays it.
pub(super) fn ground_stamp(app: &App, entity: Entity) -> Stamp {
    let world = app.world();
    box_stamp(
        world.get::<GlobalTransform>(entity).unwrap(),
        world.get::<GrassInteractorFootprint>(entity).unwrap(),
        world.get::<GrassInteractor>(entity).unwrap().strength,
        Vec2::ZERO,
    )
}

/// A stamp's centre and half extents on the ground (core plus rounding), and
/// its axis.
pub(super) fn on_ground(stamp: &Stamp) -> (Vec2, Vec2, Vec2) {
    (
        stamp.disc.xy(),
        stamp.shape.zw() + stamp.motion.z,
        stamp.shape.xy(),
    )
}

pub(super) fn close(a: Vec2, b: Vec2) -> bool {
    a.distance(b) < 1e-4
}

#[test]
fn bounds_become_a_footprint_and_a_full_strength_interactor() {
    let mut app = app();
    let entity = spawn(
        &mut app,
        GrassColliderBounds {
            rounding: 0.4,
            ..GrassColliderBounds::new(Vec3::new(0.0, 0.1, 0.0), Vec3::new(0.4, 0.9, 0.4))
        },
        Transform::from_xyz(3.0, 0.9, -4.0),
    );
    app.update();

    let footprint = app.world().get::<GrassInteractorFootprint>(entity).unwrap();
    assert_eq!(footprint.center, Vec3::new(0.0, 0.1, 0.0));
    assert_eq!(footprint.half_extents, Vec3::new(0.4, 0.9, 0.4));
    assert_eq!(footprint.rounding, 0.4);
    let interactor = app.world().get::<GrassInteractor>(entity).unwrap();
    assert_eq!(interactor.strength, 1.0);
    assert_eq!(interactor.max_height, None);
    let (center, half, _) = on_ground(&ground_stamp(&app, entity));
    assert_eq!(center, Vec2::new(3.0, -4.0));
    assert!(close(half, Vec2::splat(0.4)));
}

#[test]
fn a_scaled_body_is_covered_at_its_simulated_size_not_twice() {
    let mut app = app();
    // Bounds are what the engine simulates: a 12 × 6 m box, because the
    // body's scale of 2 is already applied to the collider.
    let entity = spawn(
        &mut app,
        GrassColliderBounds::new(Vec3::new(1.0, 0.0, 0.0), Vec3::new(6.0, 1.0, 3.0)),
        Transform::from_scale(Vec3::splat(2.0)),
    );
    app.update();

    let footprint = app.world().get::<GrassInteractorFootprint>(entity).unwrap();
    assert_eq!(footprint.half_extents, Vec3::new(3.0, 0.5, 1.5), "local");
    let (center, half, _) = on_ground(&ground_stamp(&app, entity));
    assert!(close(center, Vec2::new(1.0, 0.0)), "{center}");
    assert!(close(half, Vec2::new(6.0, 3.0)), "{half}");
}

#[test]
fn a_non_uniformly_scaled_body_is_covered_at_its_simulated_size() {
    let mut app = app();
    // A 1 m cube stretched to a 10 m wall, 0.5 m thick.
    let entity = spawn(
        &mut app,
        GrassColliderBounds::new(Vec3::ZERO, Vec3::new(5.0, 1.0, 0.25)),
        Transform::from_scale(Vec3::new(10.0, 2.0, 0.5)),
    );
    app.update();

    let (_, half, along) = on_ground(&ground_stamp(&app, entity));
    assert_eq!(along, Vec2::X);
    assert!(close(half, Vec2::new(5.0, 0.25)), "{half}");
}

#[test]
fn changing_the_scale_refits_the_footprint() {
    let mut app = app();
    let entity = spawn(
        &mut app,
        GrassColliderBounds::new(Vec3::ZERO, Vec3::splat(0.5)),
        Transform::IDENTITY,
    );
    app.update();

    // The engine rescales the collider with the body.
    place(&mut app, entity, Transform::from_scale(Vec3::splat(8.0)));
    app.world_mut()
        .get_mut::<GrassColliderBounds>(entity)
        .unwrap()
        .half_extents = Vec3::splat(4.0);
    app.update();

    let (_, half, _) = on_ground(&ground_stamp(&app, entity));
    assert!(close(half, Vec2::splat(4.0)), "{half}");
}

#[test]
fn the_adapter_scale_strength_and_height_pass_through() {
    let mut app = app();
    let entity = spawn(
        &mut app,
        GrassColliderBounds {
            rounding: 0.3,
            ..GrassColliderBounds::new(Vec3::ZERO, Vec3::splat(0.3))
        },
        Transform::IDENTITY,
    );
    *app.world_mut()
        .get_mut::<GrassInteractorFromCollider>(entity)
        .unwrap() = GrassInteractorFromCollider {
        scale: 2.0,
        strength: 0.4,
        max_height: Some(1.5),
    };
    app.update();

    let footprint = app.world().get::<GrassInteractorFootprint>(entity).unwrap();
    assert_eq!(footprint.half_extents, Vec3::splat(0.6));
    assert_eq!(footprint.rounding, 0.6);
    let interactor = app.world().get::<GrassInteractor>(entity).unwrap();
    assert_eq!(interactor.strength, 0.4);
    assert_eq!(interactor.max_height, Some(1.5));

    // A scale that makes no sense is 1; below 0 is 0.
    for (scale, half) in [(f32::NAN, 0.3), (-1.0, 0.0)] {
        app.world_mut()
            .get_mut::<GrassInteractorFromCollider>(entity)
            .unwrap()
            .scale = scale;
        app.update();
        let footprint = app.world().get::<GrassInteractorFootprint>(entity).unwrap();
        assert_eq!(footprint.half_extents, Vec3::splat(half), "scale {scale}");
    }
}

#[test]
fn moving_or_turning_the_body_rewrites_nothing() {
    let mut app = app();
    let entity = spawn(
        &mut app,
        GrassColliderBounds::new(Vec3::ZERO, Vec3::new(4.0, 1.0, 1.5)),
        Transform::IDENTITY,
    );
    app.update();
    app.update();

    for (step, yaw) in [0.3, FRAC_PI_2, 2.0, -2.5].into_iter().enumerate() {
        let at = Vec3::new(step as f32 * 3.0, 0.0, -2.0);
        place(
            &mut app,
            entity,
            Transform::from_translation(at).with_rotation(Quat::from_rotation_y(yaw)),
        );
        app.update();

        let footprint = app
            .world()
            .entity(entity)
            .get_ref::<GrassInteractorFootprint>()
            .unwrap();
        assert!(
            !footprint.is_changed(),
            "yaw {yaw}: the footprint was rewritten"
        );
        let interactor = app
            .world()
            .entity(entity)
            .get_ref::<GrassInteractor>()
            .unwrap();
        assert!(!interactor.is_changed(), "yaw {yaw}");
        // The stamp follows the body all the same.
        let (center, half, along) = on_ground(&ground_stamp(&app, entity));
        assert!(close(center, Vec2::new(at.x, at.z)));
        assert!(close(half, Vec2::new(4.0, 1.5)), "yaw {yaw}: {half}");
        let forward = Quat::from_rotation_y(yaw) * Vec3::X;
        assert!(along.dot(Vec2::new(forward.x, forward.z)).abs() > 0.9999);
    }
}

#[test]
fn a_box_tipped_onto_its_side_is_covered_where_it_lies() {
    let mut app = app();
    // A 1 × 6 × 1 m pillar: standing, its shadow is 1 m square.
    let entity = spawn(
        &mut app,
        GrassColliderBounds::new(Vec3::ZERO, Vec3::new(0.5, 3.0, 0.5)),
        Transform::IDENTITY,
    );
    app.update();
    assert!(close(
        on_ground(&ground_stamp(&app, entity)).1,
        Vec2::splat(0.5)
    ));

    // It falls over: local Y now lies along world X.
    place(
        &mut app,
        entity,
        Transform::from_xyz(0.0, 0.5, 0.0).with_rotation(Quat::from_rotation_z(FRAC_PI_2)),
    );
    app.update();
    let (_, half, along) = on_ground(&ground_stamp(&app, entity));
    assert!(along.dot(Vec2::X).abs() > 0.9999, "{along}");
    assert!(close(half, Vec2::new(3.0, 0.5)), "{half}");
}

#[test]
fn inactive_bounds_flatten_nothing_until_active_again() {
    let mut app = app();
    let entity = spawn(
        &mut app,
        GrassColliderBounds {
            active: false,
            ..GrassColliderBounds::new(Vec3::ZERO, Vec3::splat(0.3))
        },
        Transform::IDENTITY,
    );
    app.update();
    assert_eq!(
        app.world().get::<GrassInteractor>(entity).unwrap().strength,
        0.0
    );

    app.world_mut()
        .get_mut::<GrassColliderBounds>(entity)
        .unwrap()
        .active = true;
    app.update();
    assert_eq!(
        app.world().get::<GrassInteractor>(entity).unwrap().strength,
        1.0
    );
}

#[test]
fn removing_the_bounds_stops_the_interaction_and_keeps_the_footprint() {
    let mut app = app();
    let entity = spawn(
        &mut app,
        GrassColliderBounds::new(Vec3::ZERO, Vec3::new(4.0, 1.0, 1.5)),
        Transform::IDENTITY,
    );
    app.update();
    let before = *app.world().get::<GrassInteractorFootprint>(entity).unwrap();

    app.world_mut()
        .entity_mut(entity)
        .remove::<GrassColliderBounds>();
    app.update();

    assert_eq!(
        app.world().get::<GrassInteractor>(entity).unwrap().strength,
        0.0
    );
    assert_eq!(
        *app.world().get::<GrassInteractorFootprint>(entity).unwrap(),
        before
    );
    // It had bounds once, so it is not waiting for a backend.
    assert_eq!(
        app.world()
            .get::<GrassInteractorFit>(entity)
            .unwrap()
            .frames_waiting,
        0
    );
}

#[test]
fn an_adapter_with_no_bounds_waits_and_warns_once() {
    let mut app = app();
    let entity = app
        .world_mut()
        .spawn(GrassInteractorFromCollider::default())
        .id();
    for _ in 0..MISSING_BOUNDS_FRAMES + 3 {
        app.update();
    }

    assert_eq!(
        app.world().get::<GrassInteractor>(entity).unwrap().strength,
        0.0
    );
    let fit = app.world().get::<GrassInteractorFit>(entity).unwrap();
    assert!(!fit.bounded);
    // The counter stops at the frame that warned, so it warns only once.
    assert_eq!(fit.frames_waiting, MISSING_BOUNDS_FRAMES);
}

#[test]
fn bounds_written_in_the_bounds_set_are_fitted_the_same_frame() {
    fn my_backend(
        mut commands: Commands,
        adapted: Query<Entity, Added<GrassInteractorFromCollider>>,
    ) {
        for entity in &adapted {
            commands
                .entity(entity)
                .insert(GrassColliderBounds::new(Vec3::ZERO, Vec3::splat(0.25)));
        }
    }
    let mut app = app();
    app.add_systems(Update, my_backend.in_set(GrassColliderSystems::Bounds));
    let entity = app
        .world_mut()
        .spawn(GrassInteractorFromCollider::default())
        .id();
    app.update();

    let footprint = app.world().get::<GrassInteractorFootprint>(entity).unwrap();
    assert_eq!(footprint.half_extents, Vec3::splat(0.25));
    assert_eq!(
        app.world().get::<GrassInteractor>(entity).unwrap().strength,
        1.0
    );
}

#[test]
fn an_axis_scaled_to_nothing_does_not_poison_the_footprint() {
    let mut app = app();
    let entity = spawn(
        &mut app,
        GrassColliderBounds::new(Vec3::ZERO, Vec3::new(3.0, 1.0, 0.0)),
        Transform::from_scale(Vec3::new(1.0, 1.0, 0.0)),
    );
    app.update();

    let stamp = ground_stamp(&app, entity);
    for value in stamp.gpu() {
        assert!(value.is_finite(), "{value}");
    }
}
