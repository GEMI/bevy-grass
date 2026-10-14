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
use bevy::render::RenderApp;
use bevy::render::pipelined_rendering::PipelinedRenderingPlugin;
use bevy::render::render_resource::{
    BufferDescriptor, BufferUsages, CommandEncoderDescriptor, MapMode, TextureFormat,
};
use bevy::render::renderer::{RenderDevice, RenderQueue};
use bevy::time::TimeUpdateStrategy;
use bevy::window::ExitCondition;
use bevy::winit::WinitPlugin;

use super::view::{GrassViewBuffers, ViewUniformData};
use crate::GrassPlugin;
use crate::api::{GrassField, GrassInteractor, GrassStyle, Wind};
use crate::fields::{CHUNK_SIZE, FieldTables};
use crate::mirror::{self, ChunkLayout, DistanceLod, TILE_POINTS};

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
    let world = app.sub_app_mut(RenderApp).world_mut();
    let device = world.resource::<RenderDevice>().clone();
    let queue = world.resource::<RenderQueue>().clone();
    let mut views = world.query::<&GrassViewBuffers>();
    let buffers = views.iter(world).next()?;
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
    let (uniform, words) = gpu_words(app)?;
    Some((uniform, words.iter().map(|w| [w[0], w[1], w[2]]).collect()))
}

/// Each instance's root (world XZ) and current wind-and-trample bend.
fn gpu_bends(app: &mut App) -> Vec<(Vec2, Vec2)> {
    let Some((view, words)) = gpu_words(app) else {
        return Vec::new();
    };
    let origin = view.instance_origin;
    let position = |q: u32, center: f32| ((q as f32 / 65535.0) - 0.5) * 2.0 * origin.w + center;
    // Offset binary, as `snorm8` in `common.wesl` writes it.
    let snorm = |byte: u32| (byte & 0xff) as f32 / 255.0 * 2.0 - 1.0;
    words
        .iter()
        .map(|w| {
            let root = Vec2::new(
                position(w[0] & 0xffff, origin.x),
                position(w[0] >> 16, origin.z),
            );
            (root, Vec2::new(snorm(w[3]), snorm(w[3] >> 8)))
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
    };
    let camera = view.camera_position.truncate();
    let origin = view.instance_origin;
    let mut out = Vec::new();
    for chunk in tables.chunks.iter() {
        let field = tables.fields[chunk.ids.x as usize];
        let tallest = field.shape.y;
        let corner = Vec2::new(field.min_corner.x, field.min_corner.z)
            + Vec2::new(chunk.bounds.x, chunk.bounds.y);
        let lo = Vec3::new(corner.x - tallest, chunk.bounds.z, corner.y - tallest);
        let hi = Vec3::new(
            corner.x + CHUNK_SIZE + tallest,
            chunk.bounds.w + tallest,
            corner.y + CHUNK_SIZE + tallest,
        );
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
            let root = Vec3::new(
                field.min_corner.x + local.x,
                field.min_corner.y,
                field.min_corner.z + local.y,
            );
            let distance = root.distance(camera);
            let falloff = lod_table.falloff(distance);
            if candidate.rank
                >= mirror::keep_fraction(field.shape.w, view.density_scale * falloff, &layout)
            {
                continue;
            }
            let pick = mirror::unit(mirror::hash(candidate.seed ^ 0x68));
            let height = field.shape.x + pick * (field.shape.y - field.shape.x);
            if height < 0.01 || !sphere_visible(&view.planes, root + Vec3::Y * height * 0.5, height)
            {
                continue;
            }
            let lod = lod_table.lod(distance, mirror::unit(mirror::hash(candidate.seed ^ 0x10d)));
            let width = field.shape.z
                * (0.75 + 0.5 * mirror::unit(mirror::hash(candidate.seed ^ 0x77)))
                * mirror::coverage_width(falloff);
            let relative = root - origin.truncate();
            let qx = quantize(relative.x / (2.0 * origin.w) + 0.5, 65535.0);
            let qz = quantize(relative.z / (2.0 * origin.w) + 0.5, 65535.0);
            let qy = quantize(relative.y / (2.0 * mirror::HEIGHT_RANGE) + 0.5, 65535.0);
            let facing =
                (mirror::unit(mirror::hash(candidate.seed ^ 0x66)).fract() * 256.0) as u32 & 0xff;
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
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: None,
                exit_condition: ExitCondition::DontExit,
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
            ..default()
        },
        Transform::from_xyz(3.0, 0.5, -4.0),
    ));
    app.finish();
    app.cleanup();
    app
}

#[test]
#[ignore = "needs a GPU: cargo test -- --ignored gpu"]
fn gpu_generation_matches_the_cpu_mirror() {
    let mut app = headless_app();

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

#[test]
#[ignore = "needs a GPU: cargo test -- --ignored gpu"]
fn gpu_trampled_grass_lies_down_stays_put_and_recovers() {
    let mut app = headless_app();
    // Without wind an untouched blade stands nearly straight.
    app.insert_resource(Wind {
        strength: 0.0,
        ..Wind::CALM
    });
    let at = Vec2::new(4.0, 12.0);
    let interactor = app
        .world_mut()
        .spawn((
            GrassInteractor {
                radius: 1.0,
                strength: 1.0,
            },
            Transform::from_xyz(at.x, 0.5, at.y),
        ))
        .id();
    // Bends of the blades whose roots are within `reach` of the interactor.
    let near = |app: &mut App, reach: f32| -> Vec<(Vec2, Vec2)> {
        gpu_bends(app)
            .into_iter()
            .filter(|(root, _)| root.distance(at) < reach)
            .collect()
    };
    let mut ready = false;
    for _ in 0..600 {
        app.update();
        if near(&mut app, 0.5).len() > 20 {
            ready = true;
            break;
        }
    }
    assert!(
        ready,
        "no grass appeared around the interactor in 600 frames"
    );

    let flattened = near(&mut app, 0.5);
    for (root, bend) in &flattened {
        assert!(
            bend.length() > 0.8,
            "a blade at {root} under the interactor bends only {bend}"
        );
        let away = *root - at;
        if away.length() > 0.2 {
            assert!(
                bend.dot(away.normalize()) > 0.6,
                "a blade at {root} leans {bend}, not away"
            );
        }
    }
    let untouched: Vec<f32> = gpu_bends(&mut app)
        .into_iter()
        .filter(|(root, _)| root.distance(at) > 3.0)
        .map(|(_, bend)| bend.length())
        .collect();
    assert!(
        untouched.iter().all(|&b| b < 0.1),
        "blades far away bend too"
    );

    // The interactor leaves and the camera moves a non-whole number of cells:
    // the trail stays where it was made, a little weaker.
    app.world_mut().despawn(interactor);
    let mut cameras = app.world_mut().query::<(&mut Transform, &Camera3d)>();
    for (mut transform, _) in cameras.iter_mut(app.world_mut()) {
        transform.translation += Vec3::new(6.3, 0.0, -2.9);
    }
    for _ in 0..6 {
        app.update();
    }
    let trail = near(&mut app, 0.5);
    assert!(!trail.is_empty());
    assert!(
        trail.iter().all(|(_, bend)| bend.length() > 0.6),
        "the trail moved with the camera or faded too fast"
    );

    // After 2 s of game time the grass stands again.
    for _ in 0..120 {
        app.update();
    }
    let recovered = near(&mut app, 0.5);
    assert!(!recovered.is_empty());
    assert!(
        recovered.iter().all(|(_, bend)| bend.length() < 0.1),
        "flattened grass did not recover"
    );
}
