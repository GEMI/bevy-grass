//! Grass on terrain built from code: an island from a height function, drawn
//! as 16 chunk meshes, with one field over all of it.
//!
//! The chunks keep no CPU copy of their meshes, as many games build them, so
//! `GrassGround::mesh` could not bake them, and one field per chunk would be
//! sixteen fields. The field is given the height function instead, and a
//! density function keeps grass out of the sea, off the beach and off the
//! cliffs. Both are sampled once, at spawn.
//!
//! ```text
//! cargo run --release --example code_terrain
//! cargo run --release --example code_terrain -- --fixed-time --screenshot shots/code_terrain.png
//! ```

mod common;

use bevy::asset::RenderAssetUsages;
use bevy::camera_controller::free_camera::{FreeCamera, FreeCameraPlugin};
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;
use bevy_grass::prelude::*;

/// Side of the square the island sits in, and of the field over it.
const SIZE: f32 = 240.0;
/// Chunks along each side of the terrain.
const CHUNKS: u32 = 4;
/// Grass grows only this far above the sea.
const BEACH: f32 = 1.2;
/// Steepest ground grass grows on, as rise over run: about 40°.
const STEEPEST: f32 = 0.85;

fn main() {
    let mut app = App::new();
    common::ExampleArgs::parse().install(&mut app, "bevy_grass: terrain built from code");
    app.add_plugins((GrassPlugins, FreeCameraPlugin))
        .add_systems(Startup, setup)
        .run();
}

/// Height of the island above the sea at `point` (world X and Z): a dome
/// that falls into the sea about 95 m out, with hills and a cliff on it.
fn height(point: Vec2) -> f32 {
    let dome = 14.0 * (1.0 - (point.length() / 95.0).powi(2));
    let hills = 2.5 * (point.x * 0.06).sin() * (point.y * 0.045).cos()
        + 1.2 * (point.x * 0.11 + point.y * 0.13).sin();
    // A cliff along a ridge north of the centre.
    let cliff = 3.0 * (point.y + 30.0).clamp(0.0, 2.0) * (1.0 - (point.x / 60.0).powi(2)).max(0.0);
    dome + hills + cliff
}

/// How steep the ground is at `point`, as rise over run.
fn slope(point: Vec2) -> f32 {
    let step = 0.5;
    let dx = height(point + Vec2::X * step) - height(point - Vec2::X * step);
    let dz = height(point + Vec2::Y * step) - height(point - Vec2::Y * step);
    Vec2::new(dx, dz).length() / (2.0 * step)
}

/// One chunk of the terrain, centred on `center`, as a game's world builder
/// might make it: render-world only, so there is no CPU copy to bake.
fn chunk(center: Vec2, side: f32) -> Mesh {
    let mut mesh = Mesh::from(Plane3d::default().mesh().size(side, side).subdivisions(59));
    if let Some(VertexAttributeValues::Float32x3(positions)) =
        mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION)
    {
        for position in positions.iter_mut() {
            position[1] = height(center + Vec2::new(position[0], position[2]));
        }
    }
    mesh.compute_smooth_normals();
    mesh.asset_usage = RenderAssetUsages::RENDER_WORLD;
    mesh
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut ground: ResMut<Assets<FarGrassMaterial>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut styles: ResMut<Assets<GrassStyle>>,
    mut images: ResMut<Assets<Image>>,
) {
    commands.spawn((
        Camera3d::default(),
        FreeCamera::default(),
        Transform::from_xyz(-45.0, 22.0, -125.0).looking_at(Vec3::new(0.0, 6.0, -40.0), Vec3::Y),
    ));
    commands.spawn((
        DirectionalLight {
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(4.0, 8.0, 3.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // The sea.
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(1000.0, 1000.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.10, 0.25, 0.32),
            perceptual_roughness: 0.15,
            ..default()
        })),
    ));

    // The terrain, in chunks, fading to the grass colour past the last blade.
    let style = GrassStyle::MEADOW;
    let terrain = ground.add(FarGrassMaterial {
        base: StandardMaterial {
            base_color: Color::srgb(0.2, 0.15, 0.09),
            perceptual_roughness: 0.95,
            ..default()
        },
        extension: FarGrass::new(&style),
    });
    let side = SIZE / CHUNKS as f32;
    for z in 0..CHUNKS {
        for x in 0..CHUNKS {
            let center = (Vec2::new(x as f32, z as f32) + 0.5) * side - SIZE * 0.5;
            commands.spawn((
                Mesh3d(meshes.add(chunk(center, side))),
                MeshMaterial3d(terrain.clone()),
                Transform::from_xyz(center.x, 0.0, center.y),
            ));
        }
    }

    // One field over the whole island. It sits at the origin unturned, so
    // its points are world X and Z, and the functions read the same terrain
    // as the chunks.
    commands.spawn(
        GrassField::new(Vec2::splat(SIZE), style, &mut styles)
            .with_ground_from_fn(&mut images, 0.5, height)
            .with_density_from_fn(&mut images, 0.5, |point| {
                let above_beach = ((height(point) - BEACH) / 1.0).clamp(0.0, 1.0);
                let gentle = ((STEEPEST - slope(point)) / 0.2).clamp(0.0, 1.0);
                above_beach * gentle
            }),
    );
}
