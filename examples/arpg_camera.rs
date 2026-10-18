//! Grass under DEMO ARPG GAME's camera: orthographic, 30° below the horizon, a fixed
//! 45° yaw, a 14.64 m tall frame (ADR-043), one directional light with one
//! shadow cascade (ADR-060) and linear fog. A 1.8 m hero walks a loop and six
//! smaller monsters wander around it; all of them trample the grass.
//!
//! At this camera one pixel covers 2–3 cm of ground, so the blades are wider
//! and fewer than a perspective game would use, with a 2-pixel minimum width.
//!
//! ```text
//! cargo run --release --example arpg_camera
//! cargo run --release --example arpg_camera -- --fixed-time --screenshot shots/arpg.png
//! ```

mod common;

use bevy::camera::ScalingMode;
use bevy::light::CascadeShadowConfigBuilder;
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::prelude::*;
use bevy_grass::prelude::*;

const PITCH_DEGREES: f32 = 30.0;
const YAW_DEGREES: f32 = 45.0;
const VIEW_HEIGHT_METRES: f32 = 14.64;
const CAMERA_DISTANCE: f32 = 40.0;
/// The art bible's sun elevation.
const SUN_ELEVATION_DEGREES: f32 = 22.5;

fn main() {
    let mut app = App::new();
    common::ExampleArgs::parse().install(&mut app, "bevy_grass: ARPG camera");
    app.add_plugins(GrassPlugins)
        .insert_resource(GrassWind::BREEZE)
        .add_systems(Startup, setup)
        .add_systems(Update, walk)
        .run();
}

/// Walks an entity around a circle, at ground level plus `lift`.
#[derive(Component)]
struct Walker {
    center: Vec2,
    radius: f32,
    /// Metres per second along the circle; negative walks clockwise.
    speed: f32,
    phase: f32,
    lift: f32,
}

fn walk(time: Res<Time>, mut walkers: Query<(&Walker, &mut Transform)>) {
    for (walker, mut transform) in &mut walkers {
        let angle = walker.phase + time.elapsed_secs() * walker.speed / walker.radius;
        let at = walker.center + Vec2::from_angle(angle) * walker.radius;
        transform.translation = Vec3::new(at.x, walker.lift, at.y);
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "one setup function keeps the example's whole scene in one place"
)]
fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut styles: ResMut<Assets<GrassStyle>>,
) {
    let target = Vec3::ZERO;
    let rotation = Quat::from_euler(
        EulerRot::YXZ,
        YAW_DEGREES.to_radians(),
        -PITCH_DEGREES.to_radians(),
        0.0,
    );
    let mut transform = Transform::from_rotation(rotation);
    transform.translation = target - transform.forward() * CAMERA_DISTANCE;
    commands.spawn((
        Camera3d::default(),
        // The camera is far from the ground, so blades are thin on screen: at
        // least 2 pixels wide. A pixel width belongs to the camera, not the style.
        GrassCamera {
            min_width_px: Some(2.0),
            ..default()
        },
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::FixedVertical {
                viewport_height: VIEW_HEIGHT_METRES,
            },
            ..OrthographicProjection::default_3d()
        }),
        transform,
        DistanceFog {
            color: Color::srgb(0.55, 0.59, 0.63),
            falloff: FogFalloff::Linear {
                start: CAMERA_DISTANCE,
                end: CAMERA_DISTANCE + 40.0,
            },
            ..default()
        },
    ));

    commands.spawn((
        DirectionalLight {
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(
            EulerRot::YXZ,
            (YAW_DEGREES + 70.0).to_radians(),
            -SUN_ELEVATION_DEGREES.to_radians(),
            0.0,
        )),
        CascadeShadowConfigBuilder {
            num_cascades: 1,
            minimum_distance: CAMERA_DISTANCE - VIEW_HEIGHT_METRES,
            maximum_distance: CAMERA_DISTANCE + VIEW_HEIGHT_METRES,
            ..default()
        }
        .build(),
    ));

    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(200.0, 200.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.16, 0.13, 0.08),
            perceptual_roughness: 0.95,
            ..default()
        })),
    ));
    let hero = common::spawn_person(&mut commands, &mut meshes, &mut materials, target);
    commands.entity(hero).insert((
        Walker {
            center: Vec2::ZERO,
            radius: 3.0,
            speed: 1.4,
            phase: 0.0,
            lift: 0.9,
        },
        GrassInteractor {
            radius: 0.5,
            strength: 1.0,
            ..default()
        },
    ));
    let monster = meshes.add(Capsule3d::new(0.3, 0.4));
    let hide = materials.add(Color::srgb(0.22, 0.2, 0.18));
    for i in 0..6 {
        let i = i as f32;
        commands.spawn((
            Mesh3d(monster.clone()),
            MeshMaterial3d(hide.clone()),
            Transform::default(),
            Walker {
                center: Vec2::new(-1.0, 1.5),
                radius: 4.5 + i * 0.6,
                speed: if i % 2.0 == 0.0 { 2.2 } else { -1.8 },
                phase: i * 1.1,
                lift: 0.5,
            },
            GrassInteractor {
                radius: 0.6,
                strength: 1.0,
                ..default()
            },
        ));
    }

    commands.spawn(GrassField {
        size: Vec2::splat(64.0),
        style: styles.add(GrassStyle {
            blade: GrassBlade {
                height: Vec2::new(0.4, 0.9),
                width: 0.05,
                ..GrassStyle::TALL_PRAIRIE.blade
            },
            // 36, not 45: the look approved at gate G1 drew 36 per m² while a
            // rounding bug capped 45 (fixed in the perf pass).
            density: 36.0,
            ..GrassStyle::TALL_PRAIRIE
        }),
        ..default()
    });
}
