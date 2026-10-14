//! Every summer and autumn preset side by side, to pick a look from.
//!
//! The near row is summer, the far row autumn, each left to right in the order
//! of `GrassStyle::SUMMER` and `GrassStyle::AUTUMN`: the main look, then its
//! variants. A 1.8 m figure stands in front for scale.
//!
//! WASD and the mouse fly the camera (hold the left button to look). S turns
//! grass shadows off and on, at runtime.
//!
//! ```text
//! cargo run --example styles
//! cargo run --example styles -- --fixed-time --screenshot shots/styles.png
//! ```

mod common;

use bevy::camera_controller::free_camera::{FreeCamera, FreeCameraPlugin};
use bevy::light::CascadeShadowConfigBuilder;
use bevy::prelude::*;
use bevy_grass::prelude::*;

/// Side of each preset's patch, and the gap between patches, in metres.
const PATCH: f32 = 9.0;
const GAP: f32 = 1.5;

fn main() {
    let mut app = App::new();
    common::ExampleArgs::parse().install(&mut app, "bevy_grass: styles");
    app.add_plugins((GrassPlugin::default(), FreeCameraPlugin))
        .add_systems(Startup, setup)
        .add_systems(Update, toggle_shadows)
        .run();
}

fn toggle_shadows(keys: Res<ButtonInput<KeyCode>>, mut shadows: ResMut<GrassShadows>) {
    if keys.just_pressed(KeyCode::KeyS) {
        *shadows = if *shadows == GrassShadows::Off {
            GrassShadows::Near
        } else {
            GrassShadows::Off
        };
        info!("grass shadows: {:?}", *shadows);
    }
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut styles: ResMut<Assets<GrassStyle>>,
) {
    let width = 4.0 * PATCH + 3.0 * GAP;
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 13.0, 30.0).looking_at(Vec3::new(0.0, 0.0, -3.0), Vec3::Y),
        FreeCamera::default(),
    ));
    commands.spawn((
        DirectionalLight {
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::YXZ, -0.5, -0.55, 0.0)),
        // One cascade over the whole scene: grass's default shadow mode casts
        // into the first cascade only, so every patch shows its shadows.
        CascadeShadowConfigBuilder {
            num_cascades: 1,
            maximum_distance: 70.0,
            ..default()
        }
        .build(),
    ));
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(200.0, 200.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.20, 0.15, 0.09),
            perceptual_roughness: 0.95,
            ..default()
        })),
    ));
    let _ = common::spawn_person(
        &mut commands,
        &mut meshes,
        &mut materials,
        Vec3::new(0.0, 0.0, PATCH * 0.5 + 2.5),
    );

    for (row, theme) in [GrassStyle::SUMMER, GrassStyle::AUTUMN]
        .into_iter()
        .enumerate()
    {
        let z = -(row as f32) * (PATCH + GAP);
        for (column, style) in theme.into_iter().enumerate() {
            let x = -width * 0.5 + PATCH * 0.5 + column as f32 * (PATCH + GAP);
            commands.spawn((
                GrassField {
                    size: Vec2::splat(PATCH),
                    style: styles.add(style),
                    ..default()
                },
                Transform::from_xyz(x, 0.0, z),
            ));
        }
    }
}
