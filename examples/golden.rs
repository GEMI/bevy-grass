//! Golden images: every preset rendered headless, at a fixed camera and wind
//! time, and compared with the images in `tests/golden/<renderer>/`. A shader
//! change that alters the picture fails the check; one that does not passes.
//!
//! A second set, the render paths (`path_*.png`), draws one preset through
//! each way Bevy can render a camera: without MSAA, with each prepass, with
//! TAA, deferred, HDR with bloom, orthographic and without grass shadows.
//! These are the paths that break first when Bevy changes, and they break
//! silently: a shader or pipeline that fails draws no grass and logs an
//! `ERROR`. So the run also fails when anything but this check logs one.
//!
//! ```text
//! cargo run --example golden               # compare; exit code 1 on a mismatch
//! cargo run --example golden -- --bless    # write the goldens for this renderer
//! cargo run --example golden -- --max-storage-buffers 10   # as a 10-buffer device
//! ```
//!
//! Goldens are per renderer, because GPUs round differently: CI renders with
//! a software rasterizer, Mesa's lavapipe on Linux or WARP on Windows, which
//! are the same on every machine. WARP uses `tests/golden/warp/`. Lavapipe is
//! a compiler as much as a renderer, so its folder is named after the Mesa and
//! LLVM builds (`llvmpipe-mesa-...-llvm-...`): a runner with a different Mesa
//! or LLVM gets a missing-golden failure, not a comparison against another
//! compiler's output. To use WARP here:
//!
//! ```text
//! WGPU_BACKEND=dx12 WGPU_FORCE_FALLBACK_ADAPTER=1 cargo run --example golden
//! ```
//!
//! To bless the Linux goldens, run the `golden-bless` CI job (Actions → CI →
//! Run workflow → `bless_goldens`), download its artifact and commit the
//! `llvmpipe-...` folder it contains.
//!
//! A mismatch writes the new image and a difference image next to each other
//! in `target/golden/`.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use bevy::anti_alias::taa::TemporalAntiAliasing;
use bevy::app::ScheduleRunnerPlugin;
use bevy::camera::{Hdr, RenderTarget, ScalingMode};
use bevy::core_pipeline::prepass::{
    DeferredPrepass, DepthPrepass, MotionVectorPrepass, NormalPrepass,
};
use bevy::ecs::system::EntityCommands;
use bevy::light::CascadeShadowConfigBuilder;
use bevy::log::LogPlugin;
use bevy::log::tracing::{Event, Level};
use bevy::log::tracing_subscriber::Layer;
use bevy::log::tracing_subscriber::layer::Context;
use bevy::log::tracing_subscriber::registry::Registry;
use bevy::material::OpaqueRendererMethod;
use bevy::pbr::DefaultOpaqueRendererMethod;
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;
use bevy::render::gpu_readback::{Readback, ReadbackComplete};
use bevy::render::render_resource::{PipelineCache, TextureFormat, TextureUsages};
use bevy::render::renderer::RenderAdapterInfo;
use bevy::render::settings::{WgpuLimits, WgpuSettings};
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
/// The largest difference per channel that counts as rendering noise rather
/// than a changed picture. llvmpipe compiles its shaders for the host CPU, so
/// the same runner image rounds one level differently on an Intel Xeon than on
/// an AMD EPYC: every blade edge moves by a hair and a few percent of channels
/// differ by 1. Ignoring that keeps the check identical across runner hosts
/// while a real change, which moves pixels by more than one level, still fails.
const NOISE: u8 = 1;

/// One way of rendering a camera, and what it adds to the camera.
struct RenderPath {
    name: &'static str,
    camera: fn(&mut EntityCommands),
}

/// The render paths, each drawn with [`PATH_STYLE`]. Forward with 4× MSAA is
/// Bevy's default and every preset's path; it is here too, so the set can be
/// compared within itself.
const RENDER_PATHS: [RenderPath; 8] = [
    RenderPath {
        name: "forward",
        camera: |_| {},
    },
    RenderPath {
        name: "forward_no_msaa",
        camera: |camera| {
            camera.insert(Msaa::Off);
        },
    },
    RenderPath {
        name: "depth_prepass",
        camera: |camera| {
            camera.insert(DepthPrepass);
        },
    },
    RenderPath {
        name: "every_prepass_taa",
        camera: |camera| {
            camera.insert((
                Msaa::Off,
                DepthPrepass,
                NormalPrepass,
                MotionVectorPrepass,
                TemporalAntiAliasing::default(),
            ));
        },
    },
    RenderPath {
        name: "deferred",
        camera: |camera| {
            camera.insert((
                Msaa::Off,
                DepthPrepass,
                MotionVectorPrepass,
                DeferredPrepass,
            ));
        },
    },
    RenderPath {
        name: "hdr_bloom",
        camera: |camera| {
            camera.insert((Hdr, Bloom::NATURAL));
        },
    },
    RenderPath {
        name: "orthographic",
        camera: |camera| {
            camera.insert(Projection::from(OrthographicProjection {
                scaling_mode: ScalingMode::FixedVertical {
                    viewport_height: 5.0,
                },
                ..OrthographicProjection::default_3d()
            }));
        },
    },
    RenderPath {
        name: "no_grass_shadows",
        camera: |camera| {
            camera.insert(GrassCamera {
                shadows: Some(GrassShadows::Off),
                ..default()
            });
        },
    },
];

/// The preset every render path draws.
const PATH_STYLE: GrassStyle = GrassStyle::MEADOW;

/// `ERROR` events logged by anything but this check: a shader that does not
/// compile, a pipeline that does not validate, a wgpu error.
static LOGGED_ERRORS: AtomicUsize = AtomicUsize::new(0);

/// Counts `ERROR` events into [`LOGGED_ERRORS`].
struct CountErrors;

impl Layer<Registry> for CountErrors {
    fn on_event(&self, event: &Event<'_>, _context: Context<'_, Registry>) {
        let metadata = event.metadata();
        if *metadata.level() == Level::ERROR && metadata.target() != module_path!() {
            LOGGED_ERRORS.fetch_add(1, Ordering::Relaxed);
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let bless = args.iter().any(|arg| arg == "--bless");
    // The device's storage buffers per shader stage, lowered to what a weaker
    // device or browser offers: the same images must come out.
    let max_storage_buffers = args
        .iter()
        .position(|arg| arg == "--max-storage-buffers")
        .and_then(|at| args.get(at + 1)?.parse().ok());
    let ready = PipelinesReady::default();
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(LogPlugin {
                custom_layer: |_| Some(Box::new(CountErrors)),
                ..default()
            })
            .set(WindowPlugin {
                primary_window: None,
                exit_condition: ExitCondition::DontExit,
                ..default()
            })
            .set(RenderPlugin {
                render_creation: WgpuSettings {
                    constrained_limits: max_storage_buffers.map(|max| WgpuLimits {
                        max_storage_buffers_per_shader_stage: max,
                        ..WgpuLimits::unlimited()
                    }),
                    ..default()
                }
                .into(),
                // Compile every pipeline the frame it is asked for, so the
                // capture never catches grass that is not there yet.
                synchronous_pipeline_compilation: true,
                ..default()
            })
            .disable::<WinitPlugin>(),
    )
    .add_plugins(ScheduleRunnerPlugin::run_loop(Duration::ZERO))
    .add_plugins(GrassPlugins.set(GrassPlugin {
        settings: GrassSettings {
            // One camera per preset, each seeing one 8 m patch: at most
            // 16,000 blades (LAWN, 250 per m²). With 60,000 per camera, WARP
            // crashed once there were 34 cameras.
            instance_capacity: 24_000,
            ..default()
        },
        ..default()
    }))
    // The wind's clock stands still at 0, so the picture is the same however
    // long the GPU takes.
    .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO))
    // Opaque materials render deferred where a camera has a deferred
    // prepass, which only the `deferred` path has. Every other patch's
    // materials are forward, as in a forward game: a camera without a
    // deferred prepass does not draw a deferred material at all.
    .insert_resource(DefaultOpaqueRendererMethod::deferred())
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
    let plane = meshes.add(Plane3d::default().mesh().size(12.0, 12.0));
    let person = meshes.add(Capsule3d::new(0.25, 1.3));
    let mut soil_and_person = |method: OpaqueRendererMethod| {
        let soil = materials.add(StandardMaterial {
            base_color: Color::srgb(0.20, 0.15, 0.09),
            perceptual_roughness: 0.95,
            opaque_render_method: method,
            ..default()
        });
        let blue = materials.add(StandardMaterial {
            base_color: Color::srgb(0.15, 0.32, 0.75),
            opaque_render_method: method,
            ..default()
        });
        (soil, blue)
    };
    let forward = soil_and_person(OpaqueRendererMethod::Forward);
    let deferred = soil_and_person(OpaqueRendererMethod::Auto);

    let presets = GrassStyle::presets().map(|(name, style)| (name.to_lowercase(), style, None));
    let paths = RENDER_PATHS
        .iter()
        .map(|path| (format!("path_{}", path.name), PATH_STYLE, Some(path)));
    for (index, (name, style, path)) in presets.chain(paths).enumerate() {
        let at = Vec3::new(index as f32 * SPACING, 0.0, 0.0);
        let mut image =
            Image::new_target_texture(WIDTH, HEIGHT, TextureFormat::Rgba8UnormSrgb, None);
        image.texture_descriptor.usage |= TextureUsages::COPY_SRC;
        let image = images.add(image);
        let mut camera = commands.spawn((
            Camera3d::default(),
            RenderTarget::from(image.clone()),
            // One camera's shadow cascade covers only its own patch.
            Transform::from_translation(at + Vec3::new(-3.0, 2.0, 5.5))
                .looking_at(at + Vec3::new(0.0, 0.4, 0.0), Vec3::Y),
            Shot { name, image },
        ));
        if let Some(path) = path {
            (path.camera)(&mut camera);
        }
        let (soil, blue) = if path.is_some_and(|path| path.name == "deferred") {
            &deferred
        } else {
            &forward
        };
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
    info!("capturing {} images with {renderer}", shots.iter().count());
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
                        let errors = LOGGED_ERRORS.load(Ordering::Relaxed);
                        if errors > 0 {
                            let failure = format!(
                                "{errors} ERROR messages in the log: a shader, pipeline or \
                                 GPU call failed, see above"
                            );
                            error!("{failure}");
                            golden.failures.push(failure);
                        }
                        let code = if golden.failures.is_empty() {
                            info!(
                                "golden images: all match ({})",
                                if golden.bless { "blessed" } else { "checked" }
                            );
                            0
                        } else {
                            error!("golden images: {} failures", golden.failures.len());
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
        // Lavapipe is Mesa's software Vulkan driver, built with LLVM. Both can
        // change how it rounds even when the shaders do not, so keep both
        // versions in the folder name.
        let identity = format!("{} {}", adapter.driver_info, adapter.name);
        let version = slug(&identity);
        if version.is_empty() {
            "llvmpipe".into()
        } else {
            format!("llvmpipe-{version}")
        }
    } else if name.contains("basic render") || name.contains("warp") {
        "warp".into()
    } else {
        slug(&name)
    }
}

/// `text` as a lower-case ASCII path component, at most 80 characters.
fn slug(text: &str) -> String {
    let slug: String = text
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    slug.chars().take(80).collect::<String>()
}

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Compares one capture with its golden, or writes it when blessing.
fn check(capture: &str, renderer: &str, data: &[u8], bless: bool) -> Result<(), String> {
    let stride = data.len() / HEIGHT as usize;
    let mut pixels = Vec::with_capacity((WIDTH * HEIGHT * 4) as usize);
    for row in data.chunks(stride).take(HEIGHT as usize) {
        pixels.extend_from_slice(&row[..(WIDTH * 4) as usize]);
    }
    let actual = image::RgbaImage::from_raw(WIDTH, HEIGHT, pixels)
        .ok_or_else(|| format!("{capture}: readback has the wrong size"))?;
    let golden_path = root()
        .join("tests/golden")
        .join(renderer)
        .join(format!("{capture}.png"));
    if bless {
        save(&actual, &golden_path)?;
        return Ok(());
    }
    let golden = match image::open(&golden_path) {
        Ok(golden) => golden.to_rgba8(),
        Err(error) => {
            // Keep the capture, so CI can hand it over as the first golden.
            let kept = root().join("target/golden").join(format!("{capture}.png"));
            save(&actual, &kept)?;
            return Err(format!(
                "{capture}: no golden at {} ({error}); run with --bless to make one, \
                 or take the image from target/golden/",
                golden_path.display()
            ));
        }
    };
    let mut total = 0u64;
    let mut loud = 0u64;
    let mut difference = image::RgbaImage::new(WIDTH, HEIGHT);
    for ((a, b), out) in actual
        .pixels()
        .zip(golden.pixels())
        .zip(difference.pixels_mut())
    {
        let most = (0..3).map(|c| a[c].abs_diff(b[c])).max().unwrap_or(0);
        total += (0..3)
            .map(|c| u64::from(a[c].abs_diff(b[c]).saturating_sub(NOISE)))
            .sum::<u64>();
        loud += u64::from(most > LOUD);
        let shown = most.saturating_mul(8);
        *out = image::Rgba([shown, shown, shown, 255]);
    }
    let pixels = f64::from(WIDTH * HEIGHT);
    let mean = total as f64 / (pixels * 3.0);
    let loud_share = loud as f64 / pixels;
    if mean <= MAX_MEAN_DIFFERENCE && loud_share <= MAX_LOUD_SHARE {
        info!(
            "{capture}: matches (mean {mean:.3}, {:.3}% loud)",
            loud_share * 100.0
        );
        return Ok(());
    }
    let out = root().join("target/golden");
    save(&actual, &out.join(format!("{capture}.png")))?;
    save(&difference, &out.join(format!("{capture}-diff.png")))?;
    Err(format!(
        "{capture}: differs from its golden (mean {mean:.3} > {MAX_MEAN_DIFFERENCE} or {:.3}% \
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
