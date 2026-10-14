//! The reference benchmark: a 2 × 2 km heightmap terrain covered in meadow,
//! flown through on a fixed path, with every frame's timings written to CSV.
//!
//! The path is driven by the frame counter, not the clock, so every run sees
//! the same frames. Compare runs on the same machine only.
//!
//! ```text
//! cargo run --example bench                       # perspective flythrough, High tier
//! cargo run --example bench -- --ortho            # DEMO ARPG GAME's camera, light and blades panning the same terrain
//! cargo run --example bench -- --tier medium --csv target/bench-medium.csv
//! cargo run --example bench -- --no-grass         # the same frames without grass
//! cargo run --example bench -- --interactors 512  # a horde trampling around the view
//! cargo run --example bench -- --screenshot shots/bench.png   # the last frame
//! cargo run --example bench -- --sheen --tint --toon  # add optional look terms
//! ```
//!
//! The styles are the M3 look, without the optional terms (sheen, ground tint,
//! toon bands), so runs stay comparable across versions; each term has a
//! flag, to measure what it costs.
//!
//! Columns: frame, frame time, and the GPU time of trampling, grass culling,
//! blade generation and the whole main opaque pass (terrain included), in ms. The
//! grass's share of the main pass is its time minus a `--no-grass` run's.
//! Shadow-pass time is not in the CSV: Bevy records it as two spans a frame,
//! so read it from `BENCH_PERF=1`'s log instead.

mod common;

use std::fmt::Write as _;

use bevy::asset::RenderAssetUsages;
use bevy::camera::ScalingMode;
use bevy::diagnostic::{DiagnosticPath, DiagnosticsStore};
use bevy::light::CascadeShadowConfigBuilder;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::diagnostic::RenderDiagnosticsPlugin;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::time::Real;
use bevy_grass::prelude::*;

const TERRAIN_SIZE: f32 = 2000.0;
const HEIGHT_SCALE: f32 = 40.0;
const HEIGHTMAP_TEXELS: u32 = 1024;
const MESH_CELLS: u32 = 512;
const FRAMES: u32 = 1500;
/// Frames before measuring: pipeline compilation and the first table upload.
const WARMUP: u32 = 120;

#[derive(Resource, Debug, Clone)]
struct Bench {
    orthographic: bool,
    grass: bool,
    interactors: u32,
    /// The optional look terms, from `--sheen`, `--tint` and `--toon`.
    sheen: bool,
    tint: bool,
    toon: bool,
    /// What the camera looks at this frame, in world XZ.
    focus: Vec2,
    csv_path: String,
    rows: Vec<[f32; 5]>,
    frame: u32,
}

fn main() {
    let mut orthographic = false;
    let mut grass = true;
    let mut interactors = 0;
    let mut screenshot = None;
    let mut quality = GrassQuality::High;
    let mut csv_path = "target/bench.csv".to_string();
    let (mut sheen, mut tint, mut toon) = (false, false, false);
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--ortho" => orthographic = true,
            "--no-grass" => grass = false,
            "--interactors" => {
                interactors = args
                    .next()
                    .and_then(|n| n.parse().ok())
                    .expect("--interactors needs a count");
            }
            "--screenshot" => screenshot = args.next(),
            "--sheen" => sheen = true,
            "--tint" => tint = true,
            "--toon" => toon = true,
            "--csv" => csv_path = args.next().expect("--csv needs a path"),
            "--tier" => {
                quality = match args.next().as_deref() {
                    Some("low") => GrassQuality::Low,
                    Some("medium") => GrassQuality::Medium,
                    Some("high") => GrassQuality::High,
                    Some("ultra") => GrassQuality::Ultra,
                    other => panic!("unknown tier {other:?}; use low, medium, high or ultra"),
                }
            }
            other => panic!("unknown argument {other}"),
        }
    }
    let mut app = App::new();
    common::ExampleArgs {
        frames: Some(FRAMES),
        screenshot,
        perf: std::env::var_os("BENCH_PERF").is_some(),
        ..Default::default()
    }
    .install(&mut app, "bevy_grass: benchmark");
    app.add_plugins(GrassPlugin {
        quality,
        max_interactors: Some(interactors.max(64)),
        ..default()
    });
    if !app.is_plugin_added::<RenderDiagnosticsPlugin>() {
        app.add_plugins(RenderDiagnosticsPlugin);
    }
    app.insert_resource(Bench {
        orthographic,
        grass,
        interactors,
        sheen,
        tint,
        toon,
        focus: Vec2::ZERO,
        csv_path,
        rows: Vec::new(),
        frame: 0,
    })
    .add_systems(Startup, setup)
    .add_systems(Update, (fly, move_interactors, record).chain())
    .run();
}

/// The terrain: a few long, smooth waves, 0 to 1.
fn terrain_height(x: f32, z: f32) -> f32 {
    let h = 0.45 * (x / 180.0).sin() * (z / 220.0).cos()
        + 0.3 * ((x + z) / 95.0).sin()
        + 0.12 * (x / 37.0 - z / 53.0).sin()
        + 0.05 * (x / 11.0 + z / 13.0).sin();
    (h + 0.92) / 1.84
}

fn world_height(x: f32, z: f32) -> f32 {
    terrain_height(x, z) * HEIGHT_SCALE
}

fn heightmap() -> Image {
    let mut data = Vec::with_capacity((HEIGHTMAP_TEXELS * HEIGHTMAP_TEXELS * 4) as usize);
    for y in 0..HEIGHTMAP_TEXELS {
        for x in 0..HEIGHTMAP_TEXELS {
            let u = (x as f32 + 0.5) / HEIGHTMAP_TEXELS as f32;
            let v = (y as f32 + 0.5) / HEIGHTMAP_TEXELS as f32;
            let h = terrain_height((u - 0.5) * TERRAIN_SIZE, (v - 0.5) * TERRAIN_SIZE);
            data.extend_from_slice(&h.to_le_bytes());
        }
    }
    Image::new(
        Extent3d {
            width: HEIGHTMAP_TEXELS,
            height: HEIGHTMAP_TEXELS,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::R32Float,
        RenderAssetUsages::MAIN_WORLD,
    )
}

fn terrain_mesh() -> Mesh {
    let n = MESH_CELLS + 1;
    let mut positions = Vec::with_capacity((n * n) as usize);
    let mut normals = Vec::with_capacity((n * n) as usize);
    for j in 0..n {
        for i in 0..n {
            let x = (i as f32 / MESH_CELLS as f32 - 0.5) * TERRAIN_SIZE;
            let z = (j as f32 / MESH_CELLS as f32 - 0.5) * TERRAIN_SIZE;
            positions.push([x, world_height(x, z), z]);
            let e = 0.5;
            let normal = Vec3::new(
                world_height(x - e, z) - world_height(x + e, z),
                2.0 * e,
                world_height(x, z - e) - world_height(x, z + e),
            )
            .normalize();
            normals.push(normal.to_array());
        }
    }
    let mut indices = Vec::with_capacity((MESH_CELLS * MESH_CELLS * 6) as usize);
    for j in 0..MESH_CELLS {
        for i in 0..MESH_CELLS {
            let a = j * n + i;
            indices.extend_from_slice(&[a, a + n, a + 1, a + 1, a + n, a + n + 1]);
        }
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_indices(Indices::U32(indices))
}

fn setup(
    mut commands: Commands,
    bench: Res<Bench>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut styles: ResMut<Assets<GrassStyle>>,
    mut images: ResMut<Assets<Image>>,
) {
    let mut camera = commands.spawn(Camera3d::default());
    if bench.orthographic {
        camera.insert(Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::FixedVertical {
                viewport_height: 14.64,
            },
            ..OrthographicProjection::default_3d()
        }));
    }
    let mut sun = commands.spawn((
        DirectionalLight {
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::YXZ, 2.0, -0.6, 0.0)),
    ));
    if bench.orthographic {
        // DEMO ARPG GAME's single shadow map around the 40 m camera distance (ADR-060).
        sun.insert(
            CascadeShadowConfigBuilder {
                num_cascades: 1,
                minimum_distance: 25.0,
                maximum_distance: 55.0,
                ..default()
            }
            .build(),
        );
    }
    commands.spawn((
        Mesh3d(meshes.add(terrain_mesh())),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.18, 0.14, 0.08),
            perceptual_roughness: 0.95,
            ..default()
        })),
    ));
    for _ in 0..bench.interactors {
        commands.spawn(GrassInteractor::default());
    }
    if !bench.grass {
        return;
    }
    // DEMO ARPG GAME's blades are wider and fewer than a perspective game's, with a
    // 2-pixel minimum width (the `arpg_camera` example's style).
    let style = if bench.orthographic {
        GrassStyle {
            height: Vec2::new(0.4, 0.9),
            width: 0.05,
            // 36, not 45: the look approved at gate G1 drew 36 per m² while a
            // rounding bug capped 45 (fixed in the perf pass).
            density: 36.0,
            min_width_px: 2.0,
            ..GrassStyle::TALL_PRAIRIE
        }
    } else {
        GrassStyle::MEADOW
    };
    let style = GrassStyle {
        specular: if bench.sheen { 0.4 } else { 0.0 },
        ground_tint: if bench.tint { 0.3 } else { 0.0 },
        toon_bands: if bench.toon { 3 } else { 0 },
        ..style
    };
    commands.spawn(GrassField {
        size: Vec2::splat(TERRAIN_SIZE),
        style: styles.add(style),
        ground: GrassGround::Heightmap {
            image: images.add(heightmap()),
            height_scale: HEIGHT_SCALE,
        },
        ..default()
    });
}

/// Moves the camera along the fixed path for this frame.
fn fly(mut bench: ResMut<Bench>, mut cameras: Query<&mut Transform, With<Camera3d>>) {
    bench.frame += 1;
    let t = bench.frame as f32 / FRAMES as f32;
    let Ok(mut transform) = cameras.single_mut() else {
        return;
    };
    // A quarter of a 600 m loop: about 940 m of ground, 0.6 m per frame.
    let angle = t * std::f32::consts::FRAC_PI_2;
    let at = Vec2::new(600.0 * angle.cos(), 600.0 * angle.sin());
    if bench.orthographic {
        // DEMO ARPG GAME's camera: 30° below the horizon, 45° yaw, 40 m back.
        let rotation =
            Quat::from_euler(EulerRot::YXZ, 45f32.to_radians(), -30f32.to_radians(), 0.0);
        let target = Vec3::new(at.x, world_height(at.x, at.y), at.y);
        *transform = Transform::from_rotation(rotation);
        transform.translation = target - transform.forward() * 40.0;
        bench.focus = at;
    } else {
        let ahead = Vec2::new(-angle.sin(), angle.cos());
        let eye = Vec3::new(at.x, world_height(at.x, at.y) + 1.8, at.y);
        let look = eye + Vec3::new(ahead.x, -0.12, ahead.y);
        *transform = Transform::from_translation(eye).looking_at(look, Vec3::Y);
        bench.focus = at + ahead * 12.0;
    }
}

/// Walks the interactors in rings around what the camera looks at, each at
/// its own speed, so they leave trails across the view.
fn move_interactors(
    bench: Res<Bench>,
    mut interactors: Query<&mut Transform, With<GrassInteractor>>,
) {
    let count = interactors.iter().len().max(1) as f32;
    let frame = bench.frame as f32;
    for (i, mut transform) in interactors.iter_mut().enumerate() {
        let i = i as f32;
        let ring = 1.0 + 12.0 * (i / count).sqrt();
        let angle = i * 2.399 + frame * 0.01 * (1.0 + i % 3.0);
        let at = bench.focus + Vec2::from_angle(angle) * ring;
        transform.translation = Vec3::new(at.x, world_height(at.x, at.y), at.y);
    }
}

fn gpu_ms(store: &DiagnosticsStore, path: &'static str) -> f32 {
    store
        .get(&DiagnosticPath::const_new(path))
        .and_then(|d| d.measurement())
        .map_or(f32::NAN, |m| m.value as f32)
}

/// Records this frame and, at the end, writes the CSV and a summary.
fn record(mut bench: ResMut<Bench>, store: Res<DiagnosticsStore>, time: Res<Time<Real>>) {
    if bench.frame <= WARMUP {
        return;
    }
    let row = [
        time.delta_secs() * 1000.0,
        gpu_ms(&store, "render/grass_trample/elapsed_gpu"),
        gpu_ms(&store, "render/grass_cull/elapsed_gpu"),
        gpu_ms(&store, "render/grass_generate/elapsed_gpu"),
        gpu_ms(&store, "render/main_opaque_pass_3d/elapsed_gpu"),
    ];
    bench.rows.push(row);
    if bench.frame + 1 != FRAMES {
        return;
    }
    let mut csv = String::from(
        "frame,frame_ms,trample_gpu_ms,cull_gpu_ms,generate_gpu_ms,main_pass_gpu_ms\n",
    );
    for (i, row) in bench.rows.iter().enumerate() {
        let _ = writeln!(
            csv,
            "{},{:.4},{:.4},{:.4},{:.4},{:.4}",
            i as u32 + WARMUP + 1,
            row[0],
            row[1],
            row[2],
            row[3],
            row[4]
        );
    }
    if let Err(error) = std::fs::write(&bench.csv_path, csv) {
        error!("could not write {}: {error}", bench.csv_path);
    }
    let names = [
        "frame",
        "grass trample",
        "grass cull",
        "grass generate",
        "main pass",
    ];
    let mut summary = format!(
        "benchmark ({}{}), {} frames:",
        if bench.orthographic {
            "orthographic"
        } else {
            "perspective"
        },
        if bench.grass { "" } else { ", no grass" },
        bench.rows.len()
    );
    for (column, name) in names.iter().enumerate() {
        let mut values: Vec<f32> = bench
            .rows
            .iter()
            .map(|r| r[column])
            .filter(|v| v.is_finite())
            .collect();
        if values.is_empty() {
            continue;
        }
        values.sort_by(f32::total_cmp);
        let at = |p: f32| values[((values.len() - 1) as f32 * p) as usize];
        let _ = write!(
            summary,
            "\n  {name:<15} p50 {:6.3} ms   p95 {:6.3} ms",
            at(0.5),
            at(0.95)
        );
    }
    info!("{summary}\n  written to {}", bench.csv_path);
}
