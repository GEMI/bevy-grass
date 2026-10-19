use bevy::app::App;
use bevy::math::Vec3;
use bevy_rapier3d::prelude::{Collider, ColliderDisabled, Sensor};

use super::*;

fn app() -> App {
    let mut app = App::new();
    app.insert_resource(GrassSettings::default())
        .add_systems(Update, sync_interactors);
    app
}

#[test]
fn a_character_capsule_becomes_one_disc() {
    let mut app = app();
    let entity = app
        .world_mut()
        .spawn((
            Collider::capsule_y(0.5, 0.4),
            GrassInteractorFromCollider::default(),
        ))
        .id();

    app.update();

    let interactor = app.world().get::<GrassInteractor>(entity).unwrap();
    let spread = app.world().get::<GrassInteractorSpread>(entity).unwrap();
    assert_eq!(interactor.radius, 0.4_f32.hypot(0.4));
    assert_eq!(interactor.strength, 1.0);
    assert_eq!(interactor.max_height, None);
    assert_eq!(spread.offsets, vec![Vec3::ZERO]);
}

#[test]
fn a_large_box_gets_a_spread_of_capped_discs() {
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

    let interactor = app.world().get::<GrassInteractor>(entity).unwrap();
    let spread = app.world().get::<GrassInteractorSpread>(entity).unwrap();
    assert_eq!(interactor.radius, 2.0);
    assert!(spread.offsets.len() > 1);
    assert!(spread.offsets.len() <= GrassInteractorSpread::MAX_DISCS);
}

#[test]
fn the_adapter_passes_strength_and_height_through() {
    let mut app = app();
    let entity = app
        .world_mut()
        .spawn((
            Collider::ball(0.3),
            GrassInteractorFromCollider {
                radius_scale: 0.5,
                strength: 0.4,
                max_height: Some(1.5),
            },
        ))
        .id();

    app.update();

    let interactor = app.world().get::<GrassInteractor>(entity).unwrap();
    assert_eq!(interactor.radius, 0.3_f32.hypot(0.3) * 0.5);
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
    let sensor = app
        .world_mut()
        .spawn((
            Collider::ball(0.3),
            GrassInteractorFromCollider::default(),
            Sensor,
        ))
        .id();

    app.update();

    assert_eq!(
        app.world()
            .get::<GrassInteractor>(disabled)
            .unwrap()
            .strength,
        0.0
    );
    assert_eq!(
        app.world().get::<GrassInteractor>(sensor).unwrap().strength,
        0.0
    );

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
fn changing_the_maximum_radius_refits_existing_colliders() {
    let mut app = app();
    let entity = app
        .world_mut()
        .spawn((
            Collider::cuboid(3.0, 0.5, 1.5),
            GrassInteractorFromCollider::default(),
        ))
        .id();
    app.update();
    assert!(
        app.world()
            .get::<GrassInteractorSpread>(entity)
            .unwrap()
            .offsets
            .len()
            > 1
    );

    *app.world_mut().resource_mut::<GrassSettings>() = GrassSettings {
        max_interactor_radius: 4.0,
        ..GrassSettings::default()
    };
    app.update();

    let spread = app.world().get::<GrassInteractorSpread>(entity).unwrap();
    assert_eq!(spread.offsets.len(), 1);
    let radius = app.world().get::<GrassInteractor>(entity).unwrap().radius;
    assert_eq!(radius, 3.0_f32.hypot(1.5));
}

#[test]
fn removing_the_collider_stops_the_interaction() {
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
}
