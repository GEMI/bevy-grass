//! Grass on a mesh: rolling hills built as a `Mesh`, turned 30° about Y, with
//! the grass field on the terrain's own entity.
//!
//! `GrassGround::mesh` bakes the top of the mesh into a heightmap once, on
//! the CPU, so the blades stand on the hills and turn with the entity. Past
//! the mesh's edge nothing grows.
//!
//! ```text
//! cargo run --release --example mesh_terrain
//! cargo run --release --example mesh_terrain -- --fixed-time --screenshot shots/mesh_terrain.png
//! ```

mod common;

use bevy::camera_controller::free_camera::{FreeCamera, FreeCameraPlugin};
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;
use bevy_grass::prelude::*;

/// Side of the terrain and of the field on it, in metres.
const SIZE: f32 = 120.0;

fn main() {
    let mut app = App::new();
    common::ExampleArgs::parse().install(&mut app, "bevy_grass: mesh terrain");
    app.add_plugins((GrassPlugins, FreeCameraPlugin))
        .add_systems(Startup, setup)
        .run();
}

/// Height of the hills at `x`, `z`.
fn height(x: f32, z: f32) -> f32 {
    3.0 * (x * 0.07).sin() * (z * 0.05).cos() + 1.5 * (x * 0.13 + z * 0.11).sin()
}

/// A square of `SIZE` with hills, as any terrain mesh a game loads or builds.
fn hills() -> Mesh {
    let mut mesh = Mesh::from(Plane3d::default().mesh().size(SIZE, SIZE).subdivisions(127));
    if let Some(VertexAttributeValues::Float32x3(positions)) =
        mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION)
    {
        for position in positions.iter_mut() {
            position[1] = height(position[0], position[2]);
        }
    }
    mesh.compute_smooth_normals();
    mesh
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<FarGrassMaterial>>,
    mut styles: ResMut<Assets<GrassStyle>>,
) {
    commands.spawn((
        Camera3d::default(),
        FreeCamera::default(),
        Transform::from_xyz(-20.0, 9.0, 34.0).looking_at(Vec3::new(0.0, 1.0, 0.0), Vec3::Y),
    ));
    commands.spawn((
        DirectionalLight {
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(4.0, 8.0, 3.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // The terrain and its grass are one entity: the field takes the mesh's
    // transform, so the grass lies where the mesh is drawn, turn included.
    // The terrain fades to the grass colour past the last blade.
    let style = GrassStyle::MEADOW;
    let terrain = meshes.add(hills());
    commands.spawn((
        Mesh3d(terrain.clone()),
        MeshMaterial3d(materials.add(FarGrassMaterial {
            base: StandardMaterial {
                base_color: Color::srgb(0.2, 0.15, 0.09),
                perceptual_roughness: 0.95,
                ..default()
            },
            extension: FarGrass::new(&style),
        })),
        GrassField {
            size: Vec2::splat(SIZE),
            style: styles.add(style),
            ground: GrassGround::mesh(terrain),
            ..default()
        },
        Transform::from_rotation(Quat::from_rotation_y(30f32.to_radians())),
    ));
}
