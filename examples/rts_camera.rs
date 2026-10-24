//! Grass under a strategy camera: a perspective camera that zooms from 15 m
//! to 200 m over an 800 m map, with tanks trampling the grass.
//!
//! A perspective camera draws grass within `GrassSettings::radius` of
//! itself, so from 150 m up it would draw none. `GrassCamera` measures the
//! radius from the ground below the camera instead (`radius_from_ground`), so
//! the same band of grass follows the zoom, and keeps blades 2 pixels wide.
//! Past the grass, `FarGrassMaterial` carries its colour and gusts. The tanks
//! flatten the grass in the shape of their hulls, and the trample window
//! follows the ground the camera shows, not the point below it.
//!
//! Mouse wheel or Q and E zoom, W A S D pan, F switches `radius_from_ground`
//! off and on to compare.
//!
//! ```text
//! cargo run --release --example rts_camera
//! cargo run --release --example rts_camera -- --height 120 --fixed-time --screenshot shots/rts.png
//! cargo run --release --example rts_camera -- --height 120 --frames 600 --perf
//! ```
//!
//! `--height METRES` starts at that height (60 by default), and
//! `--from-camera` starts with the radius measured from the camera, as
//! without `radius_from_ground`.

mod common;

use bevy::input::mouse::AccumulatedMouseScroll;
use bevy::prelude::*;
use bevy_grass::prelude::*;

/// Side of the map, and of the field over it.
const MAP: f32 = 800.0;
const LOWEST: f32 = 15.0;
const HIGHEST: f32 = 200.0;
/// The camera's pitch below the horizon: steeper close up, as strategy
/// cameras do, so units are seen from the side when zoomed in.
const PITCH_HIGH_DEGREES: f32 = 55.0;
const PITCH_LOW_DEGREES: f32 = 35.0;

fn main() {
    let args = common::ExampleArgs::parse();
    let height = args
        .value("--height")
        .and_then(|value| value.parse().ok())
        .unwrap_or(60.0f32)
        .clamp(LOWEST, HIGHEST);
    let from_ground = !args.has("--from-camera");
    let mut app = App::new();
    args.install(&mut app, "bevy_grass: RTS camera");
    app.add_plugins(GrassPlugins.set(GrassPlugin {
        settings: GrassSettings {
            // A strategy camera shows more ground than the default 64 m
            // window; 1 m cells make it 256 m, and the trails are coarser
            // than a person could see from up here anyway.
            trample_cell_size: 1.0,
            ..GrassSettings::HIGH
        },
        ..default()
    }))
    .insert_resource(Rig {
        focus: Vec2::ZERO,
        height,
        from_ground,
    })
    .add_systems(Startup, setup)
    .add_systems(Update, (control, place_camera, drive).chain())
    .run();
}

/// Where the camera looks and how high it is.
#[derive(Resource)]
struct Rig {
    focus: Vec2,
    height: f32,
    /// Whether the camera starts with `radius_from_ground`.
    from_ground: bool,
}

/// A tank driving a circle.
#[derive(Component)]
struct Tank {
    center: Vec2,
    radius: f32,
    /// Metres per second; negative drives clockwise.
    speed: f32,
    phase: f32,
}

fn setup(
    mut commands: Commands,
    rig: Res<Rig>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut ground: ResMut<Assets<FarGrassMaterial>>,
    mut styles: ResMut<Assets<GrassStyle>>,
) {
    commands.spawn((
        Camera3d::default(),
        GrassCamera {
            radius_from_ground: rig.from_ground,
            min_width_px: Some(2.0),
            ..default()
        },
    ));
    commands.spawn((
        DirectionalLight {
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(4.0, 8.0, 3.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // The battlefield: shortened meadow over the whole map, and the
    // ground fading to its colour past the last blade.
    let style = GrassStyle::MEADOW;
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(MAP * 2.0, MAP * 2.0))),
        MeshMaterial3d(ground.add(FarGrassMaterial {
            // Seen from above, the ground shows between the blades:
            // a soil close to the roots' colour keeps the field green.
            base: StandardMaterial {
                base_color: Color::srgb(0.12, 0.14, 0.06),
                perceptual_roughness: 0.95,
                ..default()
            },
            extension: FarGrass::new(&style),
        })),
    ));
    commands.spawn((
        GrassField::new(Vec2::splat(MAP), style, &mut styles),
        GrassFieldOverrides {
            height_scale: 0.6,
            ..default()
        },
    ));

    // Tanks, each flattening a box the size of its hull.
    let hull = Vec3::new(2.4, 1.2, 4.0);
    let mesh = meshes.add(Cuboid::from_size(hull));
    let paint = materials.add(Color::srgb(0.30, 0.34, 0.22));
    for i in 0..12 {
        let i = i as f32;
        commands.spawn((
            Mesh3d(mesh.clone()),
            MeshMaterial3d(paint.clone()),
            Tank {
                center: Vec2::new((i % 4.0 - 1.5) * 30.0, (i / 4.0).floor() * 30.0 - 30.0),
                radius: 8.0 + i % 3.0 * 4.0,
                speed: if i % 2.0 == 0.0 { 6.0 } else { -5.0 },
                phase: i,
            },
            GrassInteractorFootprint::new(Vec3::ZERO, hull * 0.5),
        ));
    }
}

/// Zoom, pan, and the comparison switch.
fn control(
    keys: Res<ButtonInput<KeyCode>>,
    scroll: Res<AccumulatedMouseScroll>,
    time: Res<Time>,
    mut rig: ResMut<Rig>,
    mut cameras: Query<&mut GrassCamera>,
) {
    let dt = time.delta_secs();
    let mut zoom = -scroll.delta.y * 0.1;
    if keys.pressed(KeyCode::KeyQ) {
        zoom += dt;
    }
    if keys.pressed(KeyCode::KeyE) {
        zoom -= dt;
    }
    rig.height = (rig.height * (1.0 + zoom)).clamp(LOWEST, HIGHEST);
    let mut pan = Vec2::ZERO;
    for (key, direction) in [
        (KeyCode::KeyW, Vec2::NEG_Y),
        (KeyCode::KeyS, Vec2::Y),
        (KeyCode::KeyA, Vec2::NEG_X),
        (KeyCode::KeyD, Vec2::X),
    ] {
        if keys.pressed(key) {
            pan += direction;
        }
    }
    let speed = rig.height;
    rig.focus =
        (rig.focus + pan * speed * dt).clamp(Vec2::splat(-MAP * 0.5), Vec2::splat(MAP * 0.5));
    if keys.just_pressed(KeyCode::KeyF) {
        for mut camera in &mut cameras {
            camera.radius_from_ground = !camera.radius_from_ground;
            info!("radius_from_ground: {}", camera.radius_from_ground);
        }
    }
}

/// The camera above and behind its focus, pitched by its height.
fn place_camera(rig: Res<Rig>, mut cameras: Query<&mut Transform, With<Camera3d>>) {
    let t = ((rig.height - LOWEST) / (HIGHEST - LOWEST)).clamp(0.0, 1.0);
    let pitch = PITCH_LOW_DEGREES.lerp(PITCH_HIGH_DEGREES, t).to_radians();
    let focus = Vec3::new(rig.focus.x, 0.0, rig.focus.y);
    let eye = focus + Vec3::new(0.0, rig.height, rig.height / pitch.tan());
    for mut transform in &mut cameras {
        *transform = Transform::from_translation(eye).looking_at(focus, Vec3::Y);
    }
}

fn drive(time: Res<Time>, mut tanks: Query<(&Tank, &mut Transform)>) {
    for (tank, mut transform) in &mut tanks {
        let angle = tank.phase + time.elapsed_secs() * tank.speed / tank.radius;
        let at = tank.center + Vec2::from_angle(angle) * tank.radius;
        // Facing along the circle, the way it drives.
        let heading = Vec2::from_angle(angle).perp() * tank.speed.signum();
        transform.translation = Vec3::new(at.x, 0.6, at.y);
        transform.rotation = Quat::from_rotation_y(heading.x.atan2(heading.y));
    }
}
