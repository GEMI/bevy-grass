//! The smallest Rapier interaction example: a field, a ground collider, a
//! capsule character and a large box, all trampling grass.
//!
//! `GrassInteractorFromCollider` derives the grass footprint from the Rapier
//! collider. The physics engine already writes `Transform`/`GlobalTransform`,
//! so there is no transform to copy: the adapter only decides how big the
//! footprint is (and gives a large collider several discs). `avian_minimal`
//! is the same scene with Avian; only the physics lines differ.
//!
//! ```text
//! cargo run --release --example rapier_minimal --features rapier
//! ```

use bevy::prelude::*;
use bevy_grass::prelude::*;
use bevy_rapier3d::prelude::*;

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins,
            GrassPlugins,
            RapierPhysicsPlugin::<NoUserData>::default(),
        ))
        .add_systems(Startup, setup)
        .add_systems(Update, orbit)
        .run();
}

/// A kinematic body that circles a point, so it leaves a visible trail.
#[derive(Component)]
struct Orbit {
    center: Vec2,
    radius: f32,
    height: f32,
    speed: f32,
    phase: f32,
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut styles: ResMut<Assets<GrassStyle>>,
) {
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 6.0, 16.0).looking_at(Vec3::new(0.0, 0.0, 0.0), Vec3::Y),
    ));
    commands.spawn((
        DirectionalLight {
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(8.0, 16.0, 6.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // Visual ground, and the static Rapier collider under it.
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(80.0, 80.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.18, 0.14, 0.09),
            perceptual_roughness: 0.95,
            ..default()
        })),
        Transform::from_xyz(0.0, -0.01, 0.0),
    ));
    commands.spawn((
        RigidBody::Fixed,
        // Rapier cuboid constructors take half extents.
        Collider::cuboid(40.0, 0.5, 40.0),
        Transform::from_xyz(0.0, -0.5, 0.0),
    ));

    commands.spawn(GrassField::new(
        Vec2::splat(64.0),
        GrassStyle::MEADOW,
        &mut styles,
    ));

    // A capsule character. The adapter reads the collider and adds the
    // required `GrassInteractor` and `GrassInteractorSpread` for you.
    commands.spawn((
        Mesh3d(meshes.add(Capsule3d::new(0.25, 1.3))),
        MeshMaterial3d(materials.add(Color::srgb(0.15, 0.32, 0.75))),
        Transform::from_xyz(0.0, 0.9, 0.0),
        RigidBody::KinematicPositionBased,
        Collider::capsule_y(0.5, 0.4),
        GrassInteractorFromCollider::default(),
        Orbit {
            center: Vec2::ZERO,
            radius: 4.0,
            height: 0.9,
            speed: 0.9,
            phase: 0.0,
        },
    ));

    // A large box: the adapter flattens it as one box footprint, however
    // wide.
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(6.0, 2.0, 3.0))),
        MeshMaterial3d(materials.add(Color::srgb(0.55, 0.24, 0.12))),
        Transform::from_xyz(8.0, 1.0, 0.0),
        RigidBody::KinematicPositionBased,
        Collider::cuboid(3.0, 1.0, 1.5),
        GrassInteractorFromCollider::default(),
        Orbit {
            center: Vec2::ZERO,
            radius: 9.0,
            height: 1.0,
            speed: 0.35,
            phase: 2.0,
        },
    ));
}

fn orbit(time: Res<Time>, mut bodies: Query<(&Orbit, &mut Transform)>) {
    let now = time.elapsed_secs();
    for (orbit, mut transform) in &mut bodies {
        let angle = now * orbit.speed + orbit.phase;
        transform.translation.x = orbit.center.x + angle.cos() * orbit.radius;
        transform.translation.y = orbit.height;
        transform.translation.z = orbit.center.y + angle.sin() * orbit.radius;
        transform.rotation = Quat::from_rotation_y(-angle);
    }
}
