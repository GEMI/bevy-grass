//! The generation pass against its CPU mirror, on a real GPU.
//!
//! Renders a few headless frames, reads one camera's instances back, and
//! compares them with what the `mirror` functions place for the same view.
//! Needs a GPU, so it is ignored by default:
//!
//! ```text
//! cargo test -- --ignored gpu
//! ```

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use bevy::camera::RenderTarget;
use bevy::math::{Vec2, Vec3, Vec4};
use bevy::prelude::*;
use bevy::render::pipelined_rendering::PipelinedRenderingPlugin;
use bevy::render::render_resource::{
    BufferDescriptor, BufferUsages, CommandEncoderDescriptor, MapMode, TextureFormat,
};
use bevy::render::renderer::{RenderDevice, RenderQueue};
use bevy::render::settings::{WgpuLimits, WgpuSettings};
use bevy::render::{RenderApp, RenderPlugin};
use bevy::time::TimeUpdateStrategy;
use bevy::window::ExitCondition;
use bevy::winit::WinitPlugin;

use super::platform::STORAGE_BUFFER_BUDGET;
use super::resources::GrassShared;
use super::view::{GrassViewBuffers, ViewUniformData};
use crate::GrassPlugin;
use crate::GrassWind;
use crate::api::{GrassField, GrassGround, GrassInteractor, GrassInteractorFootprint, GrassStyle};
use crate::fields::{CHUNK_SIZE, FieldTables};
use crate::mirror;
use crate::shader_model::{self, ChunkLayout, DistanceLod, TILE_POINTS};

/// Copies a buffer to the CPU, waiting for the GPU.
fn read_buffer(
    device: &RenderDevice,
    queue: &RenderQueue,
    source: &bevy::render::render_resource::Buffer,
    size: u64,
) -> Vec<u32> {
    let staging = device.create_buffer(&BufferDescriptor {
        label: Some("grass_test_staging"),
        size,
        usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor::default());
    encoder.copy_buffer_to_buffer(source, 0, &staging, 0, size);
    queue.submit([encoder.finish()]);
    staging
        .slice(..)
        .map_async(MapMode::Read, |result| result.expect("map"));
    device
        .poll(bevy::render::render_resource::PollType::wait_indefinitely())
        .expect("poll");
    let words =
        bytemuck::cast_slice(&staging.slice(..).get_mapped_range().expect("mapped")).to_vec();
    staging.unmap();
    words
}

/// Every instance the GPU generated for the first grass view, all four
/// words, and the view uniform they were generated with.
fn gpu_words(app: &mut App) -> Option<(ViewUniformData, Vec<[u32; 4]>)> {
    gpu_words_of(app, None)
}

/// The same for one camera, named by its main-world entity; `None`: the
/// first grass view.
fn gpu_words_of(app: &mut App, camera: Option<Entity>) -> Option<(ViewUniformData, Vec<[u32; 4]>)> {
    let world = app.sub_app_mut(RenderApp).world_mut();
    let device = world.resource::<RenderDevice>().clone();
    let queue = world.resource::<RenderQueue>().clone();
    let mut views = world.query::<(&bevy::render::sync_world::MainEntity, &GrassViewBuffers)>();
    let (_, buffers) = views
        .iter(world)
        .find(|(main, _)| camera.is_none_or(|camera| main.id() == camera))?;
    let uniform = *buffers.uniform.get();
    let counters = read_buffer(&device, &queue, &buffers.counters, 16);
    let instances = read_buffer(
        &device,
        &queue,
        buffers.instances_buffer(),
        buffers.instances_buffer().size(),
    );
    let mut out = Vec::new();
    for (bucket, &count) in counters.iter().take(3).enumerate() {
        let base = uniform.bucket_base[bucket] as usize;
        let count = count.min(uniform.bucket_capacity[bucket]) as usize;
        for i in base..base + count {
            out.push([
                instances[i * 4],
                instances[i * 4 + 1],
                instances[i * 4 + 2],
                instances[i * 4 + 3],
            ]);
        }
    }
    Some((uniform, out))
}

/// Words 0–2 of every instance (word 3 is the wind, which samples a
/// texture), and the view uniform.
fn gpu_instances(app: &mut App) -> Option<(ViewUniformData, Vec<[u32; 3]>)> {
    gpu_instances_of(app, None)
}

/// The same for one camera, named by its main-world entity.
fn gpu_instances_of(
    app: &mut App,
    camera: impl Into<Option<Entity>>,
) -> Option<(ViewUniformData, Vec<[u32; 3]>)> {
    let (uniform, words) = gpu_words_of(app, camera.into())?;
    Some((uniform, words.iter().map(|w| [w[0], w[1], w[2]]).collect()))
}

/// Each instance's root (world XZ) and current wind-and-trample bend.
fn gpu_bends(app: &mut App) -> Vec<(Vec2, Vec2)> {
    let Some((view, words)) = gpu_words(app) else {
        return Vec::new();
    };
    let origin = view.instance_origin;
    let position = |q: u32, center: f32| ((q as f32 / 65535.0) - 0.5) * 2.0 * origin.w + center;
    words
        .iter()
        .map(|w| {
            let root = Vec2::new(
                position(w[0] & 0xffff, origin.x),
                position(w[0] >> 16, origin.z),
            );
            let bend = |bits: u32| mirror::unpack_bend(bits).0;
            (root, Vec2::new(bend(w[3] & 0xffff), bend(w[3] >> 16)))
        })
        .collect()
}

fn box_visible(planes: &[Vec4; 6], lo: Vec3, hi: Vec3) -> bool {
    planes.iter().all(|plane| {
        let n = plane.truncate();
        let corner = Vec3::select(n.cmpgt(Vec3::ZERO), hi, lo);
        n.dot(corner) + plane.w >= 0.0
    })
}

fn sphere_visible(planes: &[Vec4; 6], center: Vec3, radius: f32) -> bool {
    planes
        .iter()
        .all(|plane| plane.truncate().dot(center) + plane.w >= -radius)
}

fn quantize(value: f32, max: f32) -> u32 {
    (value.clamp(0.0, 1.0) * max).round() as u32
}

/// What `generate.wesl` should produce for a flat field with no mask and
/// no shadow light, built from the mirror functions.
fn cpu_instances(tables: &FieldTables, view: &ViewUniformData) -> Vec<[u32; 3]> {
    let lod_table = DistanceLod {
        radius: view.radius,
        lod_start: [view.lod1_start, view.lod2_start],
        band: view.lod_band,
        fade_start: view.lod_density.w,
        density_at: [view.lod_density.x, view.lod_density.y],
        mid_range_density: view.lod_density.z,
    };
    let camera = view.camera_position.truncate();
    let origin = view.instance_origin;
    let mut out = Vec::new();
    for chunk in tables.chunks.iter() {
        let field = tables.fields[chunk.ids.x as usize];
        let tallest = field.shape.y;
        // The box around the chunk's square, turned with its field.
        let corner = field.world_of(Vec2::new(chunk.bounds.x, chunk.bounds.y));
        let (side_x, side_z) = (
            field.x_axis() * CHUNK_SIZE,
            field.x_axis().perp() * CHUNK_SIZE,
        );
        let low = corner + side_x.min(Vec2::ZERO) + side_z.min(Vec2::ZERO);
        let high = corner + side_x.max(Vec2::ZERO) + side_z.max(Vec2::ZERO);
        let lo = Vec3::new(low.x - tallest, chunk.bounds.z, low.y - tallest);
        let hi = Vec3::new(high.x + tallest, chunk.bounds.w + tallest, high.y + tallest);
        if !box_visible(&view.planes, lo, hi) {
            continue;
        }
        let nearest = camera.clamp(lo, hi).distance(camera);
        if nearest >= view.radius {
            continue;
        }
        let layout = ChunkLayout {
            tiles_per_side: field.placement.y,
            tile_size: field.extent.w,
            candidate_density: TILE_POINTS as f32 / (field.extent.w * field.extent.w),
        };
        let chunk_keep = mirror::keep_fraction(
            field.shape.w,
            view.density_scale * lod_table.falloff(nearest),
            &layout,
        );
        for index in 0..layout.candidates_needed(chunk_keep) {
            let candidate = mirror::candidate(
                [chunk.ids.y, chunk.ids.z],
                field.placement.x,
                &layout,
                index,
            );
            let local = Vec2::new(chunk.bounds.x, chunk.bounds.y) + candidate.local;
            if local.x >= field.extent.x || local.y >= field.extent.y {
                continue;
            }
            let at = field.world_of(local);
            let root = Vec3::new(at.x, field.min_corner.y, at.y);
            let distance = root.distance(camera);
            let falloff = lod_table.falloff(distance);
            if candidate.rank
                >= mirror::keep_fraction(field.shape.w, view.density_scale * falloff, &layout)
            {
                continue;
            }
            let pick = shader_model::unit(shader_model::hash(candidate.seed ^ 0x68));
            let height = field.shape.x + pick * (field.shape.y - field.shape.x);
            if height < 0.01 || !sphere_visible(&view.planes, root + Vec3::Y * height * 0.5, height)
            {
                continue;
            }
            let lod = lod_table.lod(
                distance,
                shader_model::unit(shader_model::hash(candidate.seed ^ 0x10d)),
            );
            let width = field.shape.z
                * (0.75 + 0.5 * shader_model::unit(shader_model::hash(candidate.seed ^ 0x77)))
                * mirror::coverage_width(falloff, view.limits.x);
            let relative = root - origin.truncate();
            let qx = quantize(relative.x / (2.0 * origin.w) + 0.5, 65535.0);
            let qz = quantize(relative.z / (2.0 * origin.w) + 0.5, 65535.0);
            let qy = quantize(relative.y / (2.0 * view.limits.w) + 0.5, 65535.0);
            let facing = (shader_model::unit(shader_model::hash(candidate.seed ^ 0x66)).fract()
                * 256.0) as u32
                & 0xff;
            out.push([
                qx | (qz << 16),
                qy | (facing << 16) | ((candidate.seed >> 24) << 24),
                quantize(height / field.shape.y, 255.0)
                    | (quantize(width / (mirror::WIDTH_RANGE * field.shape.z), 255.0) << 8)
                    | (chunk.ids.x << 16)
                    | (lod << 24)
                    | (u32::from(mirror::clump_of(Vec2::new(root.x, root.z))) << 26),
            ]);
        }
    }
    out
}

/// A headless app with one camera rendering to an image and one field.
fn headless_app() -> App {
    headless_app_with(Transform::from_xyz(3.0, 0.5, -4.0), GrassGround::Flat)
}

/// [`headless_app`] with the field at `transform`, on `ground`.
///
/// The device offers no more storage buffers per shader stage than
/// `STORAGE_BUFFER_BUDGET`, as most of the devices grass is built for do, so
/// every GPU test also checks that the pipelines fit it. A limit changes what
/// wgpu validates, not what the GPU computes.
fn headless_app_with(transform: Transform, ground: GrassGround) -> App {
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: None,
                exit_condition: ExitCondition::DontExit,
                ..default()
            })
            .set(RenderPlugin {
                render_creation: WgpuSettings {
                    constrained_limits: Some(WgpuLimits {
                        max_storage_buffers_per_shader_stage: STORAGE_BUFFER_BUDGET,
                        ..WgpuLimits::unlimited()
                    }),
                    ..default()
                }
                .into(),
                ..default()
            })
            .disable::<WinitPlugin>()
            .disable::<PipelinedRenderingPlugin>(),
    )
    .add_plugins(GrassPlugin::default())
    .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
        1.0 / 60.0,
    )));

    let world = app.world_mut();
    let target = world
        .resource_mut::<Assets<Image>>()
        .add(Image::new_target_texture(
            640,
            360,
            TextureFormat::Rgba8UnormSrgb,
            None,
        ));
    let style = world
        .resource_mut::<Assets<GrassStyle>>()
        .add(GrassStyle::MEADOW);
    world.spawn((
        Camera3d::default(),
        RenderTarget::Image(target.into()),
        Transform::from_xyz(-5.0, 1.7, 30.0).looking_at(Vec3::new(5.0, 0.0, 0.0), Vec3::Y),
    ));
    world.spawn((
        GrassField {
            size: Vec2::new(70.0, 50.0),
            style,
            ground,
            ..default()
        },
        transform,
    ));
    app.finish();
    app.cleanup();
    app
}

fn generation_matches_the_cpu_mirror(settings: crate::GrassSettings) {
    generation_matches_the_cpu_mirror_in(headless_app(), settings);
}

fn generation_matches_the_cpu_mirror_in(mut app: App, settings: crate::GrassSettings) {
    app.insert_resource(settings);

    // Shaders compile asynchronously; wait until a frame has produced grass.
    let mut result = None;
    for _ in 0..600 {
        app.update();
        if let Some((view, gpu)) = gpu_instances(&mut app)
            && !gpu.is_empty()
        {
            result = Some((view, gpu));
            break;
        }
    }
    let (view, gpu) = result.expect("the GPU produced no grass in 600 frames");
    // The shaders were handed what was asked for.
    assert_eq!(view.radius, settings.radius);
    assert_eq!(view.lod1_start, settings.radius * settings.lod1_start);
    assert_eq!(view.lod2_start, settings.radius * settings.lod2_start);
    assert_eq!(view.lod_band, settings.lod_blend);
    assert_eq!(
        view.lod_density,
        Vec4::new(
            settings.lod2_density,
            settings.fade_density,
            settings.mid_range_density,
            settings.fade_start
        )
    );
    assert_eq!(view.limits.x, settings.max_widening);
    let tables = app.world().resource::<FieldTables>().clone();
    let cpu = cpu_instances(&tables, &view);

    // Everything but the position must match exactly. Positions may differ
    // by one 16-bit step (millimetres): the GPU fuses multiply-adds that the
    // CPU rounds separately.
    let key = |w: &[u32; 3]| (w[1] >> 16, w[2]);
    let unpack = |w: &[u32; 3]| [w[0] & 0xffff, w[0] >> 16, w[1] & 0xffff];
    let mut gpu_by_key: HashMap<(u32, u32), Vec<[u32; 3]>> = HashMap::new();
    for w in &gpu {
        gpu_by_key.entry(key(w)).or_default().push(unpack(w));
    }
    let close = |a: [u32; 3], b: [u32; 3]| a.iter().zip(b).all(|(x, y)| x.abs_diff(y) <= 1);
    let mut shared = 0;
    let mut missing = Vec::new();
    for w in &cpu {
        let found = gpu_by_key
            .get(&key(w))
            .and_then(|candidates| candidates.iter().position(|p| close(*p, unpack(w))));
        match found {
            Some(i) => {
                shared += 1;
                gpu_by_key.get_mut(&key(w)).unwrap().swap_remove(i);
            }
            None => missing.push(*w),
        }
    }
    eprintln!(
        "GPU {} blades, CPU {} blades, {} match ({:.3}% of the larger)",
        gpu.len(),
        cpu.len(),
        shared,
        100.0 * shared as f64 / gpu.len().max(cpu.len()) as f64
    );
    for w in missing.iter().take(5) {
        eprintln!("CPU blade not on the GPU: {w:08x?}");
    }
    let gpu_set: HashSet<[u32; 3]> = gpu.iter().copied().collect();
    assert!(
        cpu.len() > 10_000,
        "the test scene should place many blades"
    );
    assert_eq!(gpu_set.len(), gpu.len(), "the GPU produced a blade twice");
    // Floating-point differences at a threshold (a LOD band, the radius, the
    // frustum edge) may flip a handful of blades; anything more is a bug.
    let tolerance = cpu.len() / 500;
    assert!(
        cpu.len() - shared <= tolerance,
        "{} CPU blades missing on the GPU",
        cpu.len() - shared
    );
    assert!(
        gpu.len() - shared <= tolerance,
        "{} GPU blades not placed by the CPU",
        gpu.len() - shared
    );
}

/// Blades the first grass view generated, `frames` frames from now.
fn blades_after(app: &mut App, frames: usize) -> usize {
    for _ in 0..frames {
        app.update();
    }
    gpu_instances(app).map_or(0, |(_, instances)| instances.len())
}

/// The view uniform and blade count of the first grass view, `frames` frames
/// from now.
fn view_after(app: &mut App, frames: usize) -> (ViewUniformData, usize) {
    for _ in 0..frames {
        app.update();
    }
    let (uniform, instances) = gpu_instances(app).expect("a grass view");
    (uniform, instances.len())
}

/// Updates until the first grass view holds blades: the shaders compile
/// asynchronously, so how many frames that takes varies.
fn wait_for_grass(app: &mut App) {
    for _ in 0..600 {
        app.update();
        if gpu_instances(app).is_some_and(|(_, instances)| !instances.is_empty()) {
            return;
        }
    }
    panic!("no grass in 600 frames");
}

/// Blades per level of detail among a view's instances.
fn blades_per_lod(app: &mut App) -> [usize; 3] {
    let (_, instances) = gpu_instances(app).expect("a grass view");
    let mut counts = [0; 3];
    for words in instances {
        counts[((words[2] >> 24) & 3) as usize] += 1;
    }
    counts
}

/// The index count of each bucket's indirect draw for the first grass view.
fn draw_index_counts(app: &mut App) -> [u32; 3] {
    let world = app.sub_app_mut(RenderApp).world_mut();
    let device = world.resource::<RenderDevice>().clone();
    let queue = world.resource::<RenderQueue>().clone();
    let mut views = world.query::<&GrassViewBuffers>();
    let buffers = views.iter(world).next().expect("a grass view");
    // Three `DrawIndexedIndirectArgs` of five words; the first is the count.
    let words = read_buffer(&device, &queue, &buffers.draw_args, 60);
    [words[0], words[5], words[10]]
}

/// Standard deviation of the bend along x of every blade the first view
/// holds.
fn bend_spread(app: &mut App) -> f32 {
    let bends: Vec<f32> = gpu_bends(app).into_iter().map(|(_, bend)| bend.x).collect();
    assert!(
        bends.len() > 10_000,
        "the test scene should place many blades"
    );
    let mean = bends.iter().sum::<f32>() / bends.len() as f32;
    (bends.iter().map(|b| (b - mean).powi(2)).sum::<f32>() / bends.len() as f32).sqrt()
}

mod edit;
mod generation;
mod look;
mod settings;
mod stats;
mod trample;
mod visibility;
mod wind;
