//! Golden images: every preset rendered headless, at a fixed camera and wind
//! time, and compared with the images in `tests/golden/<renderer>/`. A shader
//! change that alters the picture fails the check; one that does not passes.
//!
//! ```text
//! cargo run --example golden               # compare; exit code 1 on a mismatch
//! cargo run --example golden -- --bless    # write the goldens for this renderer
//! ```
//!
//! Goldens are per renderer, because GPUs round differently: CI renders with
//! a software rasterizer, Mesa's lavapipe on Linux (`llvmpipe`) or WARP on
//! Windows (`warp`), which are the same on every machine. To use WARP here:
//!
//! ```text
//! WGPU_BACKEND=dx12 WGPU_FORCE_FALLBACK_ADAPTER=1 cargo run --example golden
//! ```
//!
//! A mismatch writes the new image and a difference image next to each other
//! in `target/golden/`.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use bevy::app::ScheduleRunnerPlugin;
use bevy::camera::RenderTarget;
use bevy::light::CascadeShadowConfigBuilder;
use bevy::prelude::*;
use bevy::render::gpu_readback::{Readback, ReadbackComplete};
use bevy::render::render_resource::{PipelineCache, TextureFormat, TextureUsages};
use bevy::render::renderer::RenderAdapterInfo;
use bevy::render::{Render, RenderApp, RenderPlugin, RenderSystems};
use bevy::time::TimeUpdateStrategy;
use bevy::window::ExitCondition;
use bevy::winit::WinitPlugin;
use bevy_grass::prelude::*;

/// Size of each image. A multiple of 64 pixels wide, so rows need no padding.
const WIDTH: u32 = 384;
const HEIGHT: u32 = 256;
/// Each preset's patch sits this far from the others, so no camera sees two.
const SPACING: f32 = 400.0;
/// Frames the pipelines must have been ready before the capture.
const SETTLE_FRAMES: u32 = 8;

/// The largest mean difference per channel (0–255) an image may have. A
/// renderer draws the same picture every run (the clean runs measured
/// exactly 0), so this is tight: darkening only the ambient light by 10%
/// moved the mean by 0.02–0.09 and must fail.
const MAX_MEAN_DIFFERENCE: f64 = 0.01;
/// The largest share of pixels that may differ by more than `LOUD`.
const MAX_LOUD_SHARE: f64 = 0.0005;
const LOUD: u8 = 16;

fn main() {
    let bless = std::env::args().any(|arg| arg == "--bless");
    let ready = PipelinesReady::default();
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: None,
                exit_condition: ExitCondition::DontExit,
                ..default()
            })
            .set(RenderPlugin {
                // Compile every pipeline the frame it is asked for, so the
                // capture never catches grass that is not there yet.
                synchronous_pipeline_compilation: true,
                ..default()
            })
            .disable::<WinitPlugin>(),
    )
    .add_plugins(ScheduleRunnerPlugin::run_loop(Duration::ZERO))
    .add_plugins(GrassPlugin {
        // Fourteen cameras; each sees one small patch.
        instance_capacity: Some(60_000),
        ..default()
    })
    // The wind's clock stands still at 0, so the picture is the same however
    // long the GPU takes.
    .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO))
    .insert_resource(Golden {
        bless,
        ready: ready.clone(),
        settled: 0,
        pending: 0,
        failures: Vec::new(),
        captured: false,
    })
    .add_systems(Startup, setup)
    .add_systems(Update, capture);
    if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
        render_app
            .insert_resource(ready)
            .add_systems(Render, watch_pipelines.in_set(RenderSystems::Cleanup));
    }
    app.run();
}

/// Set by the render world when no pipeline is still compiling.
#[derive(Resource, Clone, Default)]
struct PipelinesReady(Arc<AtomicBool>);

fn watch_pipelines(cache: Res<PipelineCache>, ready: Res<PipelinesReady>) {
    ready.0.store(
        cache.waiting_pipelines().next().is_none(),
        Ordering::Release,
    );
}

#[derive(Resource)]
struct Golden {
    bless: bool,
    ready: PipelinesReady,
    settled: u32,
    pending: usize,
    failures: Vec<String>,
    captured: bool,
}

/// One preset's camera and its image.
#[derive(Component)]
struct Shot {
    name: String,
    image: Handle<Image>,
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut styles: ResMut<Assets<GrassStyle>>,
) {
    commands.spawn((
        DirectionalLight {
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::YXZ, -0.6, -0.6, 0.0)),
        CascadeShadowConfigBuilder {
            num_cascades: 1,
            maximum_distance: 30.0,
            ..default()
        }
        .build(),
    ));
    let soil = materials.add(StandardMaterial {
        base_color: Color::srgb(0.20, 0.15, 0.09),
        perceptual_roughness: 0.95,
        ..default()
    });
    let plane = meshes.add(Plane3d::default().mesh().size(12.0, 12.0));
    let person = meshes.add(Capsule3d::new(0.25, 1.3));
    let blue = materials.add(Color::srgb(0.15, 0.32, 0.75));

    for (index, (name, style)) in GrassStyle::presets().enumerate() {
        let at = Vec3::new(index as f32 * SPACING, 0.0, 0.0);
        let mut image =
            Image::new_target_texture(WIDTH, HEIGHT, TextureFormat::Rgba8UnormSrgb, None);
        image.texture_descriptor.usage |= TextureUsages::COPY_SRC;
        let image = images.add(image);
        commands.spawn((
            Camera3d::default(),
            RenderTarget::from(image.clone()),
            // One camera's shadow cascade covers only its own patch.
            Transform::from_translation(at + Vec3::new(-3.0, 2.0, 5.5))
                .looking_at(at + Vec3::new(0.0, 0.4, 0.0), Vec3::Y),
            Shot {
                name: name.to_lowercase(),
                image,
            },
        ));
        commands.spawn((
            Mesh3d(plane.clone()),
            MeshMaterial3d(soil.clone()),
            Transform::from_translation(at),
        ));
        commands.spawn((
            Mesh3d(person.clone()),
            MeshMaterial3d(blue.clone()),
            Transform::from_translation(at + Vec3::new(0.8, 0.9, 1.0)),
        ));
        commands.spawn((
            GrassField {
                size: Vec2::splat(8.0),
                style: styles.add(style),
                ..default()
            },
            Transform::from_translation(at),
        ));
    }
}

fn capture(
    mut commands: Commands,
    mut golden: ResMut<Golden>,
    shots: Query<&Shot>,
    adapter: Option<Res<RenderAdapterInfo>>,
) {
    if golden.captured {
        return;
    }
    if golden.ready.0.load(Ordering::Acquire) {
        golden.settled += 1;
    } else {
        golden.settled = 0;
    }
    if golden.settled < SETTLE_FRAMES {
        return;
    }
    golden.captured = true;
    let renderer = renderer_name(adapter.as_deref());
    info!("capturing {} presets with {renderer}", shots.iter().count());
    for shot in &shots {
        golden.pending += 1;
        let name = shot.name.clone();
        let renderer = renderer.clone();
        commands
            .spawn(Readback::texture(shot.image.clone()))
            .observe(
                move |event: On<ReadbackComplete>,
                      mut commands: Commands,
                      mut golden: ResMut<Golden>| {
                    commands.entity(event.entity).despawn();
                    if golden.pending == 0 {
                        return;
                    }
                    let result = check(&name, &renderer, &event.data, golden.bless);
                    if let Err(failure) = result {
                        error!("{failure}");
                        golden.failures.push(failure);
                    }
                    golden.pending -= 1;
                    if golden.pending == 0 {
                        let code = if golden.failures.is_empty() {
                            info!(
                                "golden images: all match ({})",
                                if golden.bless { "blessed" } else { "checked" }
                            );
                            0
                        } else {
                            error!("golden images: {} differ", golden.failures.len());
                            1
                        };
                        // Straight out, not through `AppExit`: on WARP the
                        // renderer's shutdown never finished (measured: the
                        // result in 2 minutes, then no exit in 17), and the
                        // result is already written.
                        std::process::exit(code);
                    }
                },
            );
    }
}

/// A short, stable name for the renderer, for the golden folder.
fn renderer_name(adapter: Option<&RenderAdapterInfo>) -> String {
    let Some(adapter) = adapter else {
        return "unknown".into();
    };
    let name = adapter.name.to_lowercase();
    if name.contains("llvmpipe") {
        "llvmpipe".into()
    } else if name.contains("basic render") || name.contains("warp") {
        "warp".into()
    } else {
        name.chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect::<String>()
            .split('-')
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join("-")
    }
}

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Compares one capture with its golden, or writes it when blessing.
fn check(name: &str, renderer: &str, data: &[u8], bless: bool) -> Result<(), String> {
    let stride = data.len() / HEIGHT as usize;
    let mut pixels = Vec::with_capacity((WIDTH * HEIGHT * 4) as usize);
    for row in data.chunks(stride).take(HEIGHT as usize) {
        pixels.extend_from_slice(&row[..(WIDTH * 4) as usize]);
    }
    let rendered = image::RgbaImage::from_raw(WIDTH, HEIGHT, pixels)
        .ok_or_else(|| format!("{name}: readback has the wrong size"))?;
    let golden_path = root()
        .join("tests/golden")
        .join(renderer)
        .join(format!("{name}.png"));
    if bless {
        save(&rendered, &golden_path)?;
        return Ok(());
    }
    let golden = image::open(&golden_path)
        .map_err(|error| {
            format!(
                "{name}: no golden at {} ({error}); run with --bless to make one",
                golden_path.display()
            )
        })?
        .to_rgba8();
    let mut total = 0u64;
    let mut loud = 0u64;
    let mut difference = image::RgbaImage::new(WIDTH, HEIGHT);
    for ((a, b), out) in rendered
        .pixels()
        .zip(golden.pixels())
        .zip(difference.pixels_mut())
    {
        let most = (0..3).map(|c| a[c].abs_diff(b[c])).max().unwrap_or(0);
        total += (0..3).map(|c| u64::from(a[c].abs_diff(b[c]))).sum::<u64>();
        loud += u64::from(most > LOUD);
        let shown = most.saturating_mul(8);
        *out = image::Rgba([shown, shown, shown, 255]);
    }
    let pixels = f64::from(WIDTH * HEIGHT);
    let mean = total as f64 / (pixels * 3.0);
    let loud_share = loud as f64 / pixels;
    if mean <= MAX_MEAN_DIFFERENCE && loud_share <= MAX_LOUD_SHARE {
        info!(
            "{name}: matches (mean {mean:.3}, {:.3}% loud)",
            loud_share * 100.0
        );
        return Ok(());
    }
    let out = root().join("target/golden");
    save(&rendered, &out.join(format!("{name}.png")))?;
    save(&difference, &out.join(format!("{name}-diff.png")))?;
    Err(format!(
        "{name}: differs from its golden (mean {mean:.3} > {MAX_MEAN_DIFFERENCE} or {:.3}% \
         of pixels off by more than {LOUD} > {:.3}%); see target/golden/",
        loud_share * 100.0,
        MAX_LOUD_SHARE * 100.0
    ))
}

fn save(image: &image::RgbaImage, path: &Path) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    image
        .save(path)
        .map_err(|e| format!("{}: {e}", path.display()))
}
