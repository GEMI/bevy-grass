//! The style browser: every preset side by side, loaded from its
//! `.grass.ron` file in `assets/grass_styles`, so editing a file and saving it
//! changes the running example.
//!
//! The rows are the preset groups, near to far: `GrassStyle::GENERAL`,
//! `SUMMER`, `AUTUMN`, `WILD`, `FOREST` and `PLANTS`, each left to right. The
//! names come from `GrassStyle::presets()`, so a preset added to a group shows
//! up here without editing the example. A 1.8 m figure stands in front for
//! scale.
//!
//! WASD and the mouse fly the camera (hold the left button to look). 1–4 pick
//! the wind (calm, breeze, gusty, storm); S turns grass shadows off and on.
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
const PATCH: f32 = 7.0;
const GAP: f32 = 1.5;

/// The file names of the presets in `group`, in the group's order.
fn group_names(group: &[GrassStyle]) -> Vec<&'static str> {
    group
        .iter()
        .map(|style| {
            GrassStyle::presets()
                .find(|(_, preset)| preset == style)
                .map(|(name, _)| name)
                .expect("every style in a group is a preset")
        })
        .collect()
}

/// The rows, near to far: the file names of their presets.
fn rows() -> [Vec<&'static str>; 6] {
    [
        group_names(&GrassStyle::GENERAL),
        group_names(&GrassStyle::SUMMER),
        group_names(&GrassStyle::AUTUMN),
        group_names(&GrassStyle::WILD),
        group_names(&GrassStyle::FOREST),
        group_names(&GrassStyle::PLANTS),
    ]
}

const WINDS: [(&str, GrassWind); 6] = [
    ("calm", GrassWind::CALM),
    ("breeze", GrassWind::BREEZE),
    ("gusty", GrassWind::GUSTY),
    ("storm", GrassWind::STORM),
    // The breeze with its movement reshaped (see `GrassWind::sway` and the
    // other multipliers): steady, with no gusts and no sway ...
    (
        "steady",
        GrassWind {
            sway: 0.0,
            gust_contrast: 0.0,
            ..GrassWind::BREEZE
        },
    ),
    // ... and a lively one, with deep waves and blades that sway fast.
    (
        "lively",
        GrassWind {
            sway: 2.5,
            sway_speed: 2.0,
            gust_contrast: 2.5,
            ..GrassWind::BREEZE
        },
    ),
];

fn main() {
    let mut app = App::new();
    common::ExampleArgs::parse().install(&mut app, "bevy_grass: styles (wind: breeze)");
    app.add_plugins((GrassPlugins, FreeCameraPlugin))
        .add_systems(Startup, setup)
        .add_systems(Update, (toggle_shadows, pick_wind))
        .run();
}

fn toggle_shadows(keys: Res<ButtonInput<KeyCode>>, mut grass: ResMut<GrassSettings>) {
    if keys.just_pressed(KeyCode::KeyS) {
        grass.shadows = if grass.shadows == GrassShadows::Off {
            GrassShadows::Near
        } else {
            GrassShadows::Off
        };
        info!("grass shadows: {:?}", grass.shadows);
    }
}

fn pick_wind(
    keys: Res<ButtonInput<KeyCode>>,
    mut wind: ResMut<GrassWind>,
    mut windows: Query<&mut Window>,
) {
    let picked = [
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
        KeyCode::Digit4,
        KeyCode::Digit5,
        KeyCode::Digit6,
    ]
    .iter()
    .position(|key| keys.just_pressed(*key));
    if let Some(index) = picked {
        let (name, preset) = &WINDS[index];
        *wind = *preset;
        for mut window in &mut windows {
            window.title = format!("bevy_grass: styles (wind: {name})");
        }
    }
}

#[allow(clippy::needless_pass_by_value)]
fn setup(
    mut commands: Commands,
    assets: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 28.0, 40.0).looking_at(Vec3::new(0.0, 0.0, -17.0), Vec3::Y),
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
            maximum_distance: 80.0,
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

    for (row, names) in rows().iter().enumerate() {
        let z = -(row as f32) * (PATCH + GAP);
        let width = names.len() as f32 * (PATCH + GAP) - GAP;
        for (column, name) in names.iter().enumerate() {
            let x = -width * 0.5 + PATCH * 0.5 + column as f32 * (PATCH + GAP);
            commands.spawn((
                GrassField {
                    size: Vec2::splat(PATCH),
                    style: assets.load(format!(
                        "grass_styles/{}.grass.ron",
                        name.to_ascii_lowercase()
                    )),
                    ..default()
                },
                Transform::from_xyz(x, 0.0, z),
            ));
        }
    }
}
