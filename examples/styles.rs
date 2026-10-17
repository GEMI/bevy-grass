//! The style browser: every preset side by side, loaded from its
//! `.grass.ron` file in `assets/grass_styles`, so editing a file and saving it
//! changes the running example.
//!
//! The near row is the six general presets (`GrassStyle::GENERAL`: meadow,
//! tall prairie, wheat, dry savanna, lawn, stylized), then summer and autumn
//! (`GrassStyle::SUMMER`, `GrassStyle::AUTUMN`: the main look, then its
//! variants), then the wild styles (`GrassStyle::WILD`: summer wild, autumn
//! wild, wildflower meadow) and last the plants a style can mix in
//! (`GrassStyle::PLANTS`: seed heads, white and yellow flowers, clover, broad
//! leaf), each left to right. A 1.8 m figure stands in front for scale.
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

/// The rows, near to far: the file names of their presets.
const ROWS: [&[&str]; 5] = [
    &[
        "meadow",
        "tall_prairie",
        "wheat",
        "dry_savanna",
        "lawn",
        "stylized",
    ],
    &[
        "summer_tall",
        "summer_lush",
        "summer_meadow",
        "summer_seeding",
    ],
    &["autumn_tall", "autumn_rust", "autumn_straw", "autumn_faded"],
    &["summer_wild", "autumn_wild", "wildflower_meadow"],
    &[
        "seed_heads",
        "white_flowers",
        "yellow_flowers",
        "clover",
        "broad_leaf",
    ],
];

const WINDS: [(&str, GrassWind); 4] = [
    ("calm", GrassWind::CALM),
    ("breeze", GrassWind::BREEZE),
    ("gusty", GrassWind::GUSTY),
    ("storm", GrassWind::STORM),
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
        Transform::from_xyz(0.0, 22.0, 32.0).looking_at(Vec3::new(0.0, 0.0, -12.0), Vec3::Y),
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

    for (row, names) in ROWS.iter().enumerate() {
        let z = -(row as f32) * (PATCH + GAP);
        let width = names.len() as f32 * (PATCH + GAP) - GAP;
        for (column, name) in names.iter().enumerate() {
            let x = -width * 0.5 + PATCH * 0.5 + column as f32 * (PATCH + GAP);
            commands.spawn((
                GrassField {
                    size: Vec2::splat(PATCH),
                    style: assets.load(format!("grass_styles/{name}.grass.ron")),
                    ..default()
                },
                Transform::from_xyz(x, 0.0, z),
            ));
        }
    }
}
