//! Grass trampled by Rapier colliders: a kinematic character and a large
//! vehicle drive through a meadow with a crate scaled to 2.5 m, and
//! every few seconds a log is thrown in, tumbles and comes to rest across the
//! grass. All of them carry `GrassInteractorFromCollider`, so the footprint
//! comes from the collider: one disc for the capsule, a spread of discs for
//! the vehicle, the crate at its scaled size, and a strip along the log once
//! it lies down.
//!
//! ```text
//! cargo run --release --example rapier --features rapier
//! ```
//!
//! Requires the `rapier` cargo feature. The example does not use Rapier's
//! debug renderer, so the feature stays off and Bevy is compiled exactly as
//! `DefaultPlugins` would compile it.

mod common;

use std::f32::consts::FRAC_PI_2;

use bevy::prelude::*;
use bevy_grass::prelude::*;
use bevy_rapier3d::prelude::*;

fn main() {
    let mut app = App::new();
    common::ExampleArgs::parse().install(&mut app, "bevy_grass: rapier");
    app.add_plugins((GrassPlugins, RapierPhysicsPlugin::<NoUserData>::default()))
        .add_systems(Startup, setup)
        .add_systems(Update, (drive, throw))
        .run();
}

/// Moves a kinematic body in a circle, so its trail is easy to see.
#[derive(Component, Debug, Clone, Copy)]
struct Mover {
    center: Vec2,
    radius: f32,
    height: f32,
    speed: f32,
    phase: f32,
}

/// Throws a dynamic body back in every `every` seconds, from where it was
/// spawned.
#[derive(Component, Debug, Clone, Copy)]
struct Thrown {
    from: Transform,
    every: f32,
    next: f32,
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut styles: ResMut<Assets<GrassStyle>>,
) {
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 9.0, 22.0).looking_at(Vec3::new(0.0, 0.0, -1.0), Vec3::Y),
    ));
    commands.spawn((
        DirectionalLight {
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(8.0, 16.0, 6.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // The ground: a plane to look at and a static Rapier box under it.
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

    // A 1.8 m character: the collider is a little wider than the mesh so the
    // grass trail is easy to see.
    commands.spawn((
        Mesh3d(meshes.add(Capsule3d::new(0.25, 1.3))),
        MeshMaterial3d(materials.add(Color::srgb(0.15, 0.32, 0.75))),
        Transform::from_xyz(0.0, 0.9, 0.0),
        RigidBody::KinematicPositionBased,
        Collider::capsule_y(0.5, 0.4),
        GrassInteractorFromCollider::default(),
        Mover {
            center: Vec2::ZERO,
            radius: 4.0,
            height: 0.9,
            speed: 0.9,
            phase: 0.0,
        },
    ));

    // A 6 × 2 × 3 m vehicle. Its collider is larger than one 2 m disc, so the
    // adapter gives it a spread automatically.
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(6.0, 2.0, 3.0))),
        MeshMaterial3d(materials.add(Color::srgb(0.55, 0.24, 0.12))),
        Transform::from_xyz(8.0, 1.0, 0.0),
        RigidBody::KinematicPositionBased,
        Collider::cuboid(3.0, 1.0, 1.5),
        GrassInteractorFromCollider::default(),
        Mover {
            center: Vec2::new(0.0, 1.0),
            radius: 9.0,
            height: 1.0,
            speed: 0.35,
            phase: 2.0,
        },
    ));

    // A 1 m crate scaled to 2.5 m. Rapier scales the collider by the
    // transform, and so does the footprint: its trail is 2.5 m wide.
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(1.0, 1.0, 1.0))),
        MeshMaterial3d(materials.add(Color::srgb(0.62, 0.48, 0.26))),
        Transform::from_xyz(-12.0, 1.25, -9.0).with_scale(Vec3::splat(2.5)),
        RigidBody::KinematicPositionBased,
        Collider::cuboid(0.5, 0.5, 0.5),
        GrassInteractorFromCollider::default(),
        Mover {
            center: Vec2::new(-15.0, -9.0),
            radius: 3.0,
            height: 1.25,
            speed: 0.3,
            phase: 0.0,
        },
    ));

    // A 4 m log, thrown in standing and spinning. While it stands it is one
    // small disc; once it falls over, the grass is flattened along its length.
    let from = Transform::from_xyz(14.0, 3.0, -5.0).with_rotation(Quat::from_rotation_z(0.4));
    commands.spawn((
        Mesh3d(meshes.add(Capsule3d::new(0.3, 3.4))),
        MeshMaterial3d(materials.add(Color::srgb(0.35, 0.24, 0.14))),
        from,
        RigidBody::Dynamic,
        // capsule_y takes the half height between the hemispheres.
        Collider::capsule_y(1.7, 0.3),
        Velocity::default(),
        GrassInteractorFromCollider::default(),
        Thrown {
            from,
            every: 6.0,
            next: 0.0,
        },
    ));
}

fn drive(time: Res<Time>, mut movers: Query<(&Mover, &mut Transform)>) {
    let now = time.elapsed_secs();
    for (mover, mut transform) in &mut movers {
        let angle = now * mover.speed + mover.phase;
        transform.translation.x = mover.center.x + angle.cos() * mover.radius;
        transform.translation.y = mover.height;
        transform.translation.z = mover.center.y + angle.sin() * mover.radius;
        // Face the way it is going, so the vehicle drives lengthwise.
        transform.rotation = Quat::from_rotation_y(-angle - FRAC_PI_2);
    }
}

fn throw(time: Res<Time>, mut thrown: Query<(&mut Thrown, &mut Transform, &mut Velocity)>) {
    let now = time.elapsed_secs();
    for (mut thrown, mut transform, mut velocity) in &mut thrown {
        if now < thrown.next {
            continue;
        }
        thrown.next = now + thrown.every;
        *transform = thrown.from;
        velocity.linear = Vec3::new(-2.5, 0.0, 0.5);
        velocity.angular = Vec3::new(0.0, 1.5, 0.0);
    }
}
