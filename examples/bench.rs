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
//! cargo run --example bench -- --species --tufts --shape --variation  # add variation
//! cargo run --example bench -- --prepass  # a depth prepass on the camera
//! cargo run --profile release-lto --example bench -- --single-threaded  # the fastest frame on few-core CPUs
//! cargo run --example bench -- --tier competitive --target-hz 240 --machine t1200
//!     # the 240 Hz acceptance gate: exits non-zero when frame p95 exceeds 4.167 ms
//! cargo run --example bench -- --tier competitive --target-hz 240 --baseline-csv target/no-grass.csv
//!     # also report grass cost against a previous --no-grass run
//! ```
//!
//! The 240 Hz gate is also driven by `tools/bench_240.py`, which runs the
//! three reference configurations (High, Competitive and no-grass) for a
//! named machine and both scenes. CI runners cannot enforce GPU frame times
//! reliably, so the gate is manual or nightly on the labelled machines
//! described in `docs/performance.md`.
//!
//! What sets the frame-time floor, beyond grass (see "The frame-time floor"
//! in `docs/performance.md`): `--no-plugin` leaves `GrassPlugin` out
//! entirely, `--no-gpu-timing` leaves out the GPU timestamp and statistics
//! queries (the GPU columns are then empty), `--no-terrain` and
//! `--no-shadows` take the terrain mesh and the sun's shadows away.
//!
//! The styles are the M3 look, without the optional terms (sheen, ground tint,
//! toon bands) or any variation, so runs stay comparable across versions;
//! each term and each kind of variation has a flag, to measure what it
//! costs. The variation flags take `GrassStyle::SUMMER_WILD`'s settings:
//! `--species` its plants (whose heads also turn on the shape code),
//! `--tufts` its tufts and bare patches, `--shape` its twist, droop and fold,
//! `--variation` its colour patches, dry tips and dead blades.
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
use bevy::core_pipeline::prepass::DepthPrepass;
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

/// The pass/fail frame budget, from `--target-hz` or `--budget-ms`.
#[derive(Debug, Clone, Copy)]
struct FrameBudget {
    ms: f32,
    /// Set when the budget came from `--target-hz`, for the report.
    hz: Option<f32>,
}

impl FrameBudget {
    fn from_hz(hz: f32) -> Self {
        assert!(
            hz.is_finite() && hz > 0.0,
            "--target-hz must be a positive number"
        );
        Self {
            ms: 1000.0 / hz,
            hz: Some(hz),
        }
    }

    fn from_ms(ms: f32) -> Self {
        assert!(
            ms.is_finite() && ms > 0.0,
            "--budget-ms must be a positive number"
        );
        Self { ms, hz: None }
    }

    fn description(self) -> String {
        self.hz.map_or_else(
            || format!("{:.3} ms", self.ms),
            |hz| format!("{hz:.0} Hz ({:.3} ms)", self.ms),
        )
    }
}

#[derive(Resource, Debug, Clone)]
struct Bench {
    orthographic: bool,
    grass: bool,
    /// The terrain mesh and the sun's shadows, both on unless `--no-terrain`
    /// or `--no-shadows`.
    terrain: bool,
    shadows: bool,
    /// A depth prepass on the camera, from `--prepass`.
    prepass: bool,
    interactors: u32,
    /// The optional look terms, from `--sheen`, `--tint` and `--toon`.
    sheen: bool,
    tint: bool,
    toon: bool,
    /// The kinds of variation, from `--species`, `--tufts`, `--shape` and
    /// `--variation`.
    species: bool,
    tufts: bool,
    shape: bool,
    variation: bool,
    /// What the camera looks at this frame, in world XZ.
    focus: Vec2,
    /// A frame-time budget to test `p95` against, from `--target-hz` or
    /// `--budget-ms`.
    budget: Option<FrameBudget>,
    /// A name for the reference machine, included in the summary and results.
    machine: String,
    /// A previous `--no-grass` CSV, for the finite grass-cost percentiles.
    baseline_csv: Option<String>,
    csv_path: String,
    rows: Vec<[f32; 6]>,
    frame: u32,
}

#[derive(Debug, Clone)]
struct Cli {
    orthographic: bool,
    grass: bool,
    interactors: u32,
    screenshot: Option<String>,
    quality: GrassQuality,
    csv_path: String,
    budget: Option<FrameBudget>,
    machine: String,
    baseline_csv: Option<String>,
    sheen: bool,
    tint: bool,
    toon: bool,
    species: bool,
    tufts: bool,
    shape: bool,
    variation: bool,
    single_threaded: bool,
    plugin: bool,
    gpu_timing: bool,
    terrain: bool,
    shadows: bool,
    prepass: bool,
}

impl Default for Cli {
    fn default() -> Self {
        Self {
            orthographic: false,
            grass: true,
            interactors: 0,
            screenshot: None,
            quality: GrassQuality::High,
            csv_path: "target/bench.csv".to_string(),
            budget: None,
            machine: "unspecified".to_string(),
            baseline_csv: None,
            sheen: false,
            tint: false,
            toon: false,
            species: false,
            tufts: false,
            shape: false,
            variation: false,
            single_threaded: false,
            plugin: true,
            gpu_timing: true,
            terrain: true,
            shadows: true,
            prepass: false,
        }
    }
}

fn parse_cli() -> Cli {
    let mut cli = Cli::default();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--ortho" => cli.orthographic = true,
            "--no-grass" => cli.grass = false,
            "--interactors" => {
                cli.interactors = args
                    .next()
                    .and_then(|n| n.parse().ok())
                    .expect("--interactors needs a count");
            }
            "--screenshot" => cli.screenshot = args.next(),
            "--sheen" => cli.sheen = true,
            "--tint" => cli.tint = true,
            "--toon" => cli.toon = true,
            "--species" => cli.species = true,
            "--tufts" => cli.tufts = true,
            "--shape" => cli.shape = true,
            "--variation" => cli.variation = true,
            "--single-threaded" => cli.single_threaded = true,
            "--no-plugin" => cli.plugin = false,
            "--no-gpu-timing" => cli.gpu_timing = false,
            "--no-terrain" => cli.terrain = false,
            "--no-shadows" => cli.shadows = false,
            "--prepass" => cli.prepass = true,
            "--csv" => cli.csv_path = args.next().expect("--csv needs a path"),
            "--baseline-csv" => {
                cli.baseline_csv = Some(args.next().expect("--baseline-csv needs a path"));
            }
            "--machine" => cli.machine = args.next().expect("--machine needs a name"),
            "--target-hz" => {
                let hz = args
                    .next()
                    .and_then(|n| n.parse().ok())
                    .expect("--target-hz needs a number");
                cli.budget = Some(FrameBudget::from_hz(hz));
            }
            "--budget-ms" => {
                let ms = args
                    .next()
                    .and_then(|n| n.parse().ok())
                    .expect("--budget-ms needs a number");
                cli.budget = Some(FrameBudget::from_ms(ms));
            }
            "--tier" => {
                cli.quality = match args.next().as_deref() {
                    Some("competitive" | "240") => GrassQuality::Competitive,
                    Some("low") => GrassQuality::Low,
                    Some("medium") => GrassQuality::Medium,
                    Some("high") => GrassQuality::High,
                    Some("ultra") => GrassQuality::Ultra,
                    other => panic!(
                        "unknown tier {other:?}; use competitive, low, medium, high or ultra"
                    ),
                };
            }
            other => panic!("unknown argument {other}"),
        }
    }
    cli
}

fn main() {
    let cli = parse_cli();
    let mut app = App::new();
    common::ExampleArgs {
        frames: Some(FRAMES),
        screenshot: cli.screenshot.clone(),
        perf: std::env::var_os("BENCH_PERF").is_some(),
        single_threaded: cli.single_threaded,
        ..Default::default()
    }
    .install(&mut app, "bevy_grass: benchmark");
    if cli.plugin {
        app.add_plugins(GrassPlugins.set(GrassPlugin {
            settings: GrassSettings {
                max_interactors: cli.interactors.max(64),
                ..cli.quality.settings()
            },
        }));
    }
    if cli.gpu_timing && !app.is_plugin_added::<RenderDiagnosticsPlugin>() {
        app.add_plugins(RenderDiagnosticsPlugin);
    }
    app.insert_resource(Bench {
        orthographic: cli.orthographic,
        grass: cli.grass && cli.plugin,
        terrain: cli.terrain,
        prepass: cli.prepass,
        shadows: cli.shadows,
        interactors: cli.interactors,
        sheen: cli.sheen,
        tint: cli.tint,
        toon: cli.toon,
        species: cli.species,
        tufts: cli.tufts,
        shape: cli.shape,
        variation: cli.variation,
        focus: Vec2::ZERO,
        budget: cli.budget,
        machine: cli.machine,
        baseline_csv: cli.baseline_csv,
        csv_path: cli.csv_path,
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

/// The style of the benchmark's field: DEMO ARPG GAME's or a perspective game's, with
/// the kinds of variation the flags ask for.
fn bench_style(bench: &Bench) -> GrassStyle {
    // DEMO ARPG GAME's blades are wider and fewer than a perspective game's (the
    // `arpg_camera` example's style); its camera asks for 2-pixel minimum width.
    let style = if bench.orthographic {
        GrassStyle {
            blade: GrassBlade {
                height: Vec2::new(0.4, 0.9),
                width: 0.05,
                ..GrassStyle::TALL_PRAIRIE.blade
            },
            // 36, not 45: the look approved at gate G1 drew 36 per m² while a
            // rounding bug capped 45 (fixed in the perf pass).
            density: 36.0,
            ..GrassStyle::TALL_PRAIRIE
        }
    } else {
        GrassStyle::MEADOW
    };
    let wild = GrassStyle::SUMMER_WILD;
    let mut style = GrassStyle {
        blade: GrassBlade {
            specular: if bench.sheen { 0.4 } else { 0.0 },
            ..style.blade
        },
        ground_tint: GroundTint {
            amount: if bench.tint { 0.3 } else { 0.0 },
            ..style.ground_tint
        },
        toon_bands: if bench.toon { 3 } else { 0 },
        ..style
    };
    if bench.species {
        style.species.clone_from(&wild.species);
    }
    if bench.tufts {
        let variation = &mut style.variation;
        (variation.tufts, variation.tuft_spacing) =
            (wild.variation.tufts, wild.variation.tuft_spacing);
        (variation.bare_share, variation.bare_patch_size) =
            (wild.variation.bare_share, wild.variation.bare_patch_size);
    }
    if bench.shape {
        let blade = &mut style.blade;
        (blade.twist, blade.droop, blade.fold) =
            (wild.blade.twist, wild.blade.droop, wild.blade.fold);
    }
    if bench.variation {
        let variation = &mut style.variation;
        (variation.patch_color, variation.patch_amount) =
            (wild.variation.patch_color, wild.variation.patch_amount);
        (variation.dry_tips, variation.dead_share) =
            (wild.variation.dry_tips, wild.variation.dead_share);
    }
    style
}

fn setup(
    mut commands: Commands,
    bench: Res<Bench>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    styles: Option<ResMut<Assets<GrassStyle>>>,
    mut images: ResMut<Assets<Image>>,
) {
    let mut camera = commands.spawn(Camera3d::default());
    if bench.prepass {
        camera.insert(DepthPrepass);
    }
    if bench.orthographic {
        // DEMO ARPG GAME's 2-pixel minimum blade width.
        camera.insert(GrassCamera {
            min_width_px: Some(2.0),
            ..default()
        });
        camera.insert(Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::FixedVertical {
                viewport_height: 14.64,
            },
            ..OrthographicProjection::default_3d()
        }));
    }
    let mut sun = commands.spawn((
        DirectionalLight {
            shadow_maps_enabled: bench.shadows,
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
    if bench.terrain {
        commands.spawn((
            Mesh3d(meshes.add(terrain_mesh())),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgb(0.18, 0.14, 0.08),
                perceptual_roughness: 0.95,
                ..default()
            })),
        ));
    }
    for _ in 0..bench.interactors {
        commands.spawn(GrassInteractor::default());
    }
    let Some(mut styles) = styles.filter(|_| bench.grass) else {
        return;
    };
    let style = bench_style(&bench);
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

/// p50, p95 and the worst of the finite values, or `None` when there are none.
fn percentiles(values: impl IntoIterator<Item = f32>) -> Option<(f32, f32, f32)> {
    let mut values: Vec<f32> = values
        .into_iter()
        .filter(|value| value.is_finite())
        .collect();
    if values.is_empty() {
        return None;
    }
    values.sort_by(f32::total_cmp);
    let at = |p: f32| values[((values.len() - 1) as f32 * p) as usize];
    Some((at(0.5), at(0.95), values[values.len() - 1]))
}

/// The `frame_ms` column of a CSV written by this example.
fn read_frame_csv(path: &str) -> Result<Vec<f32>, String> {
    let text =
        std::fs::read_to_string(path).map_err(|error| format!("could not read {path}: {error}"))?;
    let mut lines = text.lines();
    let header = lines.next().ok_or_else(|| format!("{path} is empty"))?;
    let column = header
        .split(',')
        .position(|name| name.trim() == "frame_ms")
        .ok_or_else(|| format!("{path} has no frame_ms column"))?;
    let mut frames = Vec::new();
    for (index, line) in lines.enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let value = line
            .split(',')
            .nth(column)
            .ok_or_else(|| format!("{path}:{} has no frame_ms value", index + 2))?
            .trim()
            .parse::<f32>()
            .map_err(|error| format!("{path}:{}: {error}", index + 2))?;
        frames.push(value);
    }
    Ok(frames)
}

/// Records this frame and, at the end, writes the CSV and a summary.
fn record(
    mut bench: ResMut<Bench>,
    store: Res<DiagnosticsStore>,
    time: Res<Time<Real>>,
    mut exit: MessageWriter<AppExit>,
) {
    if bench.frame <= WARMUP {
        return;
    }
    let row = [
        time.delta_secs() * 1000.0,
        gpu_ms(&store, "render/grass_trample/elapsed_gpu"),
        gpu_ms(&store, "render/grass_cull/elapsed_gpu"),
        gpu_ms(&store, "render/grass_generate/elapsed_gpu"),
        gpu_ms(&store, "render/main_opaque_pass_3d/elapsed_gpu"),
        gpu_ms(&store, "render/per_view_shadow_pass/elapsed_gpu"),
    ];
    bench.rows.push(row);
    if bench.frame + 1 != FRAMES {
        return;
    }
    write_csv(&bench);
    let (summary, failed) = report(&bench);
    info!("{summary}\n  written to {}", bench.csv_path);
    if failed {
        // A gate failure is not a shader or asset error. Use a warning and a
        // non-zero exit code so `bench_ab.py` keeps rejecting only real errors.
        warn!(
            "frame budget {} not met on machine {}; see {}",
            bench
                .budget
                .map_or_else(String::new, FrameBudget::description),
            bench.machine,
            bench.csv_path
        );
        exit.write(AppExit::error());
    }
}

fn write_csv(bench: &Bench) {
    let mut csv = String::from(
        "frame,frame_ms,trample_gpu_ms,cull_gpu_ms,generate_gpu_ms,main_pass_gpu_ms,shadow_gpu_ms\n",
    );
    for (i, row) in bench.rows.iter().enumerate() {
        let _ = writeln!(
            csv,
            "{},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4}",
            i as u32 + WARMUP + 1,
            row[0],
            row[1],
            row[2],
            row[3],
            row[4],
            row[5]
        );
    }
    if let Err(error) = std::fs::write(&bench.csv_path, csv) {
        error!("could not write {}: {error}", bench.csv_path);
    }
}

/// The text summary and whether a set frame budget was missed.
fn report(bench: &Bench) -> (String, bool) {
    let names = [
        ("frame", 0),
        ("grass trample", 1),
        ("grass cull", 2),
        ("grass generate", 3),
        ("main pass", 4),
        ("shadow pass", 5),
    ];
    let frame_percentiles = percentiles(bench.rows.iter().map(|row| row[0]));
    let mut summary = format!(
        "benchmark ({}{}), machine {}, {} frames:",
        if bench.orthographic {
            "orthographic"
        } else {
            "perspective"
        },
        if bench.grass { "" } else { ", no grass" },
        bench.machine,
        bench.rows.len()
    );
    for (name, column) in names {
        let Some((p50, p95, _)) = percentiles(bench.rows.iter().map(|row| row[column])) else {
            continue;
        };
        let _ = write!(
            summary,
            "\n  {name:<15} p50 {p50:6.3} ms   p95 {p95:6.3} ms"
        );
    }
    if let Some((p50, p95, _)) = grass_cost(bench) {
        let _ = write!(
            summary,
            "\n  grass cost      p50 {p50:6.3} ms   p95 {p95:6.3} ms"
        );
    }
    let Some(budget) = bench.budget else {
        return (summary, false);
    };
    let Some((p50, p95, _)) = frame_percentiles else {
        let _ = write!(
            summary,
            "\n  budget          {}: no frame samples -> FAIL",
            budget.description()
        );
        return (summary, true);
    };
    let pass = p95 <= budget.ms;
    let _ = write!(
        summary,
        "\n  budget          {}: frame p50 {p50:.3} ms, p95 {p95:.3} ms -> {}",
        budget.description(),
        if pass { "PASS" } else { "FAIL" }
    );
    (summary, !pass)
}

/// Grass cost is a paired difference against a `--no-grass` run of the same
/// frames. The two runs must have the same number of rows.
fn grass_cost(bench: &Bench) -> Option<(f32, f32, f32)> {
    let path = bench.baseline_csv.as_deref()?;
    match read_frame_csv(path) {
        Ok(baseline) if baseline.len() == bench.rows.len() => percentiles(
            bench
                .rows
                .iter()
                .map(|row| row[0])
                .zip(baseline)
                .map(|(with_grass, without_grass)| with_grass - without_grass),
        ),
        Ok(baseline) => {
            warn!(
                "baseline {path} has {} frames, this run has {}; no grass cost",
                baseline.len(),
                bench.rows.len()
            );
            None
        }
        Err(error) => {
            warn!("{error}; no grass cost");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_240_hz_budget_is_4_167_ms() {
        let budget = FrameBudget::from_hz(240.0);
        assert!((budget.ms - 4.166_667).abs() < 1e-3);
        assert_eq!(budget.description(), "240 Hz (4.167 ms)");
    }

    #[test]
    fn percentiles_ignore_non_finite_values() {
        let values = [1.0, f32::NAN, 2.0, 3.0, 4.0, f32::INFINITY, 5.0];
        let (p50, p95, worst) = percentiles(values).expect("finite values");
        assert!((p50 - 3.0).abs() < 1e-6);
        // The existing benchmark's floor index: with five values, p95 is the
        // fourth. Changing that would change every historical number.
        assert!((p95 - 4.0).abs() < 1e-6);
        assert!((worst - 5.0).abs() < 1e-6);
    }

    #[test]
    fn a_missing_frame_column_is_an_error() {
        let path = std::env::temp_dir().join("bevy_grass_bench_test.csv");
        std::fs::write(&path, "frame,other\n1,2\n").expect("write test csv");
        let error = read_frame_csv(path.to_str().expect("utf-8 temp path"))
            .expect_err("no frame_ms column");
        assert!(error.contains("frame_ms"), "{error}");
        let _ = std::fs::remove_file(path);
    }
}
