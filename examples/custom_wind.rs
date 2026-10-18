//! A meadow driven by a wind model of your own instead of the built-in one.
//!
//! `GrassPlugin { wind: false, ..default() }` leaves the built-in wind out.
//! From then on the grass reads `GrassWindFrame`, and this example writes it
//! from a tiny `Weather` resource in `GrassWindSystems::Sample`. The blades
//! and the `FarGrassMaterial` ground both follow the same frame.
//!
//! ```text
//! cargo run --release --example custom_wind
//! cargo run --release --example custom_wind -- --fixed-time --screenshot shots/custom_wind.png
//! ```

mod common;

use bevy::camera_controller::free_camera::{FreeCamera, FreeCameraPlugin};
use bevy::prelude::*;
use bevy_grass::prelude::*;

fn main() {
    let mut app = App::new();
    common::ExampleArgs::parse().install(&mut app, "bevy_grass: custom wind");
    app.add_plugins((
        GrassPlugins.set(GrassPlugin {
            // The built-in gusts, sway and flutter are off. We write the frame.
            wind: false,
            ..default()
        }),
        FreeCameraPlugin,
    ))
    .insert_resource(Weather::default())
    .add_systems(Startup, setup)
    .add_systems(Update, drive_wind.in_set(GrassWindSystems::Sample))
    .run();
}

/// What a weather system would provide: a direction and a strength. This one
/// is constant; [`drive_wind`] adds the movement over time.
#[derive(Resource, Debug, Clone, Copy)]
struct Weather {
    /// Direction the weather system says the wind blows toward, in world XZ.
    direction: Vec2,
    /// Strength the weather system reports, 0 to 1.
    strength: f32,
}

impl Default for Weather {
    fn default() -> Self {
        Self {
            direction: Vec2::new(1.0, 0.2),
            strength: 0.55,
        }
    }
}

/// The whole custom wind model: turn the weather's direction slowly and
/// breathe its strength, then hand the frame to the grass.
fn drive_wind(time: Res<Time>, weather: Res<Weather>, mut wind: ResMut<GrassWindFrame>) {
    let seconds = time.elapsed_secs();
    let turn = seconds * 0.08;
    let (sin, cos) = turn.sin_cos();
    let direction = Vec2::new(
        weather.direction.x * cos - weather.direction.y * sin,
        weather.direction.x * sin + weather.direction.y * cos,
    );
    let breath = 0.75 + 0.25 * (seconds * 0.9).sin();
    *wind = GrassWindFrame {
        direction,
        strength: weather.strength * breath,
        // A custom model can scroll the gust texture itself by advancing
        // `gust_offset` and `previous_gust_offset`. This one leaves the
        // pattern in place, so the motion comes from direction and strength.
        ..GrassWind::BREEZE.into()
    };
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut far_materials: ResMut<Assets<FarGrassMaterial>>,
    mut styles: ResMut<Assets<GrassStyle>>,
) {
    commands.spawn((
        Camera3d::default(),
        FreeCamera::default(),
        Transform::from_xyz(-8.0, 4.0, 14.0).looking_at(Vec3::new(0.0, 0.5, 0.0), Vec3::Y),
    ));
    commands.spawn((
        DirectionalLight {
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(4.0, 8.0, 3.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    let style = GrassStyle::MEADOW;
    let soil = StandardMaterial {
        base_color: Color::srgb(0.2, 0.15, 0.09),
        perceptual_roughness: 0.95,
        ..default()
    };
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(300.0, 300.0))),
        MeshMaterial3d(far_materials.add(FarGrassMaterial {
            base: soil,
            extension: FarGrass::new(&style),
        })),
    ));
    commands.spawn(GrassField {
        size: Vec2::splat(100.0),
        style: styles.add(style),
        ..default()
    });
}
