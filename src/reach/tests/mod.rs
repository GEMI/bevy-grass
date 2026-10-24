use bevy::MinimalPlugins;
use bevy::app::{App, PostUpdate};
use bevy::asset::{AssetApp, AssetPlugin, Assets};
use bevy::camera::visibility::InheritedVisibility;
use bevy::camera::{CameraProjection, PerspectiveProjection};
use bevy::ecs::query::With;
use bevy::image::Image;
use bevy::math::Quat;
use bevy::shape::ViewFrustum;
use bevy::transform::components::Transform;

use super::*;
use crate::api::{GrassField, GrassStyle};

/// The field systems with every field visible, and the tables they build.
fn tables_for(fields: &[(Vec2, Transform)]) -> FieldTables {
    fn show(mut fields: Query<&mut InheritedVisibility, With<GrassField>>) {
        for mut visibility in &mut fields {
            *visibility = InheritedVisibility::VISIBLE;
        }
    }
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default()))
        .init_asset::<GrassStyle>()
        .init_asset::<Image>()
        .init_asset::<bevy::mesh::Mesh>()
        .add_systems(
            PostUpdate,
            show.in_set(VisibilitySystems::VisibilityPropagate),
        )
        .add_plugins(crate::fields::plugin);
    for &(size, transform) in fields {
        let style = app
            .world_mut()
            .resource_mut::<Assets<GrassStyle>>()
            .add(GrassStyle::MEADOW);
        app.world_mut().spawn((
            GrassField {
                size,
                style,
                ..Default::default()
            },
            GlobalTransform::from(transform),
        ));
    }
    app.update();
    app.update();
    app.world().resource::<FieldTables>().clone()
}

/// The frustum of a 45° perspective camera at `position` looking at `target`.
fn frustum(position: Vec3, target: Vec3) -> Frustum {
    let projection = PerspectiveProjection::default();
    let world_from_view = Transform::from_translation(position)
        .looking_at(target, Vec3::Y)
        .to_matrix();
    let clip_from_world = projection.get_clip_from_view() * world_from_view.inverse();
    Frustum(ViewFrustum::from_clip_from_world(&clip_from_world))
}

#[test]
fn a_field_box_spans_its_rectangle_and_its_tallest_blade() {
    let tables = tables_for(&[(Vec2::new(64.0, 32.0), Transform::from_xyz(10.0, 2.0, -6.0))]);
    let boxes = field_boxes(&tables);
    assert_eq!(boxes.len(), 1);
    let field = boxes[0];
    assert_eq!(field.corner, Vec3::new(10.0 - 32.0, 2.0, -6.0 - 16.0));
    assert_eq!(field.size, Vec2::new(64.0, 32.0));
    assert_eq!(field.top, 2.0 + GrassStyle::MEADOW.blade.height.y);
    assert_eq!(field.layers, 1, "visible, on layer 0");

    // Inside the rectangle the distance is the height above the blades.
    assert_eq!(field.distance_from(Vec3::new(10.0, 52.6, -6.0)), 50.0);
    // Beside it, the distance across to its edge.
    assert_eq!(
        field.distance_from(Vec3::new(10.0 + 32.0 + 3.0, 2.0, -6.0)),
        3.0
    );
}

#[test]
fn a_turned_field_is_measured_along_its_own_axes() {
    let turned = Transform::from_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2));
    let tables = tables_for(&[(Vec2::new(100.0, 10.0), turned)]);
    let field = field_boxes(&tables)[0];
    // Turned a quarter, the 100 m side runs along world Z.
    assert!(field.distance_from(Vec3::new(0.0, 0.3, 45.0)) < 1e-3);
    assert!((field.distance_from(Vec3::new(45.0, 0.3, 0.0)) - 40.0).abs() < 1e-3);
}

#[test]
fn a_high_camera_sees_a_field_it_cannot_reach() {
    // The RTS of the integration notes: an 800 m map seen from 200 m up.
    let tables = tables_for(&[(Vec2::splat(800.0), Transform::IDENTITY)]);
    let boxes = field_boxes(&tables);
    let position = Vec3::new(0.0, 200.0, 150.0);
    let seen = nearest_seen(&boxes, &frustum(position, Vec3::ZERO), 1, position);
    let distance = seen.expect("the camera looks at the field");
    assert!(
        distance > GrassSettings::HIGH.radius,
        "{distance} m is beyond High's radius"
    );

    // Near the ground, the same camera is within reach.
    let low = Vec3::new(0.0, 5.0, 10.0);
    let seen = nearest_seen(&boxes, &frustum(low, Vec3::ZERO), 1, low).unwrap();
    assert!(seen < GrassSettings::HIGH.radius);
}

#[test]
fn a_field_out_of_view_or_on_other_layers_is_not_seen() {
    let tables = tables_for(&[(Vec2::splat(64.0), Transform::IDENTITY)]);
    let boxes = field_boxes(&tables);
    let position = Vec3::new(0.0, 200.0, 0.0);
    // Looking up at the sky, away from the field.
    let sky = frustum(position, Vec3::new(0.0, 400.0, -10.0));
    assert_eq!(nearest_seen(&boxes, &sky, 1, position), None);
    // Looking at it, but on layer 1 only.
    let down = frustum(position, Vec3::new(0.0, 0.0, -1.0));
    assert_eq!(nearest_seen(&boxes, &down, 1 << 1, position), None);
    assert!(nearest_seen(&boxes, &down, 1, position).is_some());
}
