//! Per-view state. Every camera that draws grass gets its own instance
//! buffers, so split-screen doubles blade memory but not CPU cost.

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex};

use bevy::camera::Camera3d;
use bevy::camera::visibility::RenderLayers;
use bevy::ecs::prelude::*;
use bevy::log::{debug, warn};
use bevy::math::{IVec2, IVec4, Mat4, UVec4, Vec3, Vec4};
use bevy::render::render_resource::{
    BindGroup, BindGroupEntries, Buffer, BufferDescriptor, BufferUsages, MapMode, PipelineCache,
    ShaderType, UniformBuffer,
};
use bevy::render::renderer::{RenderDevice, RenderQueue};
use bevy::render::view::ExtractedView;

use super::resources::{ExtractedGrass, GrassLayouts, GrassShared};
use crate::GrassSettings;
use crate::api::{GrassCamera, NoGrass};
use crate::fields::{has_unsupported_layer, layer_mask};
use crate::shader_model::DistanceLod;
use crate::stats::GpuReadings;
use crate::trample;

/// Bytes per instance.
const INSTANCE_SIZE: u64 = 16;
/// Bytes per `DrawIndexedIndirectArgs`.
pub(crate) const DRAW_ARGS_SIZE: u64 = 20;
/// `fixed_lod` value meaning "pick by distance".
const DISTANCE_LOD: u32 = 3;
/// The vertical range instance positions are stored over, in metres either
/// side of the view's origin, at least: a view whose horizontal extent is
/// larger uses that instead.
const MIN_HEIGHT_RANGE: f32 = 256.0;

impl crate::GrassShadows {
    /// Directional cascades grass is drawn into, counted from the nearest.
    pub(crate) fn cascades(self) -> usize {
        match self {
            crate::GrassShadows::Off => 0,
            crate::GrassShadows::Near => 1,
            crate::GrassShadows::Full => 2,
        }
    }

    /// Buckets drawn into shadow maps, counted from LOD 0. Orthographic views
    /// put every blade in bucket 0, so they always cast all of them.
    pub(crate) fn buckets(self) -> u64 {
        match self {
            crate::GrassShadows::Off | crate::GrassShadows::Near => 1,
            crate::GrassShadows::Full => 2,
        }
    }
}

/// Mirrors `View` in `common.wesl`.
#[derive(Debug, Clone, Copy, Default, ShaderType)]
pub(crate) struct ViewUniformData {
    pub planes: [Vec4; 6],
    pub camera_position: Vec4,
    pub camera_forward: Vec4,
    pub shadow_offset: Vec4,
    pub instance_origin: Vec4,
    pub radius: f32,
    pub lod1_start: f32,
    pub lod2_start: f32,
    pub lod_band: f32,
    pub density_scale: f32,
    pub fixed_lod: u32,
    pub max_visible_chunks: u32,
    pub interactor_count: u32,
    /// The render layers the view draws, one bit each.
    pub layers: u32,
    /// Narrowest a blade is drawn, in pixels.
    pub min_width_px: f32,
    pub bucket_base: UVec4,
    pub bucket_capacity: UVec4,
    pub bucket_lod: UVec4,
    /// The trample window's lowest cell this frame (xy) and last frame (zw).
    pub trample_origin: IVec4,
    /// Cell size in metres, and how much flattening recovers this frame.
    pub trample_step: Vec4,
    /// Direction to the first directional light, w 1 when there is one.
    pub sun: Vec4,
    /// The distance table's densities: the share kept where LOD 2 starts and
    /// where the last fade starts, the extra share kept from LOD 1 on, and
    /// where the last fade starts as a share of the radius.
    pub lod_density: Vec4,
    /// The most a thinned blade widens, the largest interactor radius, how
    /// far an edge-on blade turns toward the camera, and the vertical range
    /// of stored positions.
    pub limits: Vec4,
}

/// Where the overflow check is in its cycle.
const READBACK_IDLE: u8 = 0;
const READBACK_COPIED: u8 = 1;
const READBACK_MAPPING: u8 = 2;
const READBACK_READY: u8 = 3;

/// A view's GPU buffers.
#[derive(Component)]
pub(crate) struct GrassViewBuffers {
    pub uniform: UniformBuffer<ViewUniformData>,
    visible: Buffer,
    pub counters: Buffer,
    pub dispatch: Buffer,
    instances: Buffer,
    pub draw_args: Buffer,
    /// The trample window: one packed push per cell (see `trample.rs`).
    trample: Buffer,
    /// This frame's interactors, `(x, z, radius, strength)`.
    interactors: Buffer,
    /// Interactors in `interactors` this frame.
    pub interactor_count: u32,
    /// The window's lowest cell last frame; `None` before the first frame.
    trample_origin: Option<IVec2>,
    /// The side of a window cell last frame, in metres. When it changes, the
    /// old cells mean something else and the window starts again.
    trample_cell: f32,
    /// Copy of `counters`, read back every few seconds to warn about overflow.
    pub readback: Buffer,
    pub readback_state: Arc<AtomicU8>,
    readback_values: Arc<Mutex<[u32; 4]>>,
    frames_since_readback: u32,
    /// Frames between readbacks; 0 for never. Follows the settings.
    readback_interval: u32,
    capacities: [u32; 3],
    /// Interactors `interactors` has room for.
    max_interactors: u32,
    /// Entries in `visible`.
    max_slices: u32,
    warned: bool,
}

/// A view's bind groups.
#[derive(Component, Default)]
pub(crate) struct GrassViewBindGroups {
    pub compute: Option<BindGroup>,
    pub dispatch: Option<BindGroup>,
    pub draw: Option<BindGroup>,
}

impl GrassViewBuffers {
    fn new(device: &RenderDevice, settings: &GrassSettings) -> Self {
        let capacities = [
            settings.instance_capacity,
            settings.instance_capacity / 2,
            settings.instance_capacity / 2,
        ];
        let total: u32 = capacities.iter().sum();
        let buffer = |label: &'static str, size: u64, usage: BufferUsages| {
            device.create_buffer(&BufferDescriptor {
                label: Some(label),
                size,
                usage,
                mapped_at_creation: false,
            })
        };
        Self {
            uniform: UniformBuffer::default(),
            visible: buffer(
                "grass_visible_chunks",
                // A `VisibleChunk`: one slice of a visible chunk, 16 bytes.
                u64::from(settings.max_chunk_slices) * 16,
                BufferUsages::STORAGE,
            ),
            counters: buffer(
                "grass_counters",
                16,
                BufferUsages::STORAGE | BufferUsages::COPY_DST | BufferUsages::COPY_SRC,
            ),
            dispatch: buffer(
                "grass_dispatch",
                12,
                BufferUsages::STORAGE | BufferUsages::INDIRECT,
            ),
            instances: buffer(
                "grass_instances",
                u64::from(total) * INSTANCE_SIZE,
                // The GPU readback test copies instances out.
                if cfg!(test) {
                    BufferUsages::STORAGE | BufferUsages::COPY_SRC
                } else {
                    BufferUsages::STORAGE
                },
            ),
            draw_args: buffer(
                "grass_draw_args",
                3 * DRAW_ARGS_SIZE,
                // The GPU readback test copies the arguments out.
                if cfg!(test) {
                    BufferUsages::STORAGE | BufferUsages::INDIRECT | BufferUsages::COPY_SRC
                } else {
                    BufferUsages::STORAGE | BufferUsages::INDIRECT
                },
            ),
            // Zeroed on creation: nothing is flattened yet.
            trample: buffer(
                "grass_trample",
                (trample::TEXELS * trample::TEXELS) as u64 * 4,
                BufferUsages::STORAGE,
            ),
            interactors: buffer(
                "grass_interactors",
                u64::from(settings.max_interactors) * 16,
                BufferUsages::STORAGE | BufferUsages::COPY_DST,
            ),
            interactor_count: 0,
            trample_origin: None,
            trample_cell: settings.trample_cell_size,
            readback: buffer(
                "grass_readback",
                16,
                BufferUsages::MAP_READ | BufferUsages::COPY_DST,
            ),
            readback_state: Arc::new(AtomicU8::new(READBACK_IDLE)),
            readback_values: Arc::new(Mutex::new([0; 4])),
            frames_since_readback: 0,
            readback_interval: settings.readback_interval,
            capacities,
            max_interactors: settings.max_interactors,
            max_slices: settings.max_chunk_slices,
            warned: false,
        }
    }

    /// Whether these buffers have the room `settings` ask for. When a game
    /// changes the capacity at runtime they do not, and are made again.
    fn fits(&self, settings: &GrassSettings) -> bool {
        self.capacities[0] == settings.instance_capacity
            && self.max_interactors == settings.max_interactors
            && self.max_slices == settings.max_chunk_slices
    }

    /// The instance buffer, for the GPU readback test.
    #[cfg(test)]
    pub(crate) fn instances_buffer(&self) -> &Buffer {
        &self.instances
    }

    /// Whether this frame's compute should copy the counters for the check.
    pub(crate) fn wants_readback(&self) -> bool {
        self.readback_interval > 0
            && self.frames_since_readback >= self.readback_interval
            && self.readback_state.load(Ordering::Acquire) == READBACK_IDLE
    }

    /// Called after the copy is recorded.
    pub(crate) fn readback_copied(&mut self) {
        self.frames_since_readback = 0;
        self.readback_state
            .store(READBACK_COPIED, Ordering::Release);
    }
}

/// Inward-facing, normalized frustum planes of a clip-from-world matrix.
/// Bevy uses reversed depth, so clip `z` runs from `w` (near) down to 0 (far);
/// an infinite far plane comes out degenerate and is replaced by one that
/// passes everything.
pub(crate) fn frustum_planes(clip_from_world: &Mat4) -> [Vec4; 6] {
    let row = |i| clip_from_world.row(i);
    let raw = [
        row(3) + row(0),
        row(3) - row(0),
        row(3) + row(1),
        row(3) - row(1),
        row(3) - row(2),
        row(2),
    ];
    raw.map(|plane| {
        let length = plane.truncate().length();
        if length < 1e-6 {
            Vec4::W
        } else {
            plane / length
        }
    })
}

/// Creates buffers for new grass views and writes every view's uniform.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub(crate) fn prepare_views(
    mut commands: Commands,
    mut views: Query<
        (
            Entity,
            &ExtractedView,
            Option<&RenderLayers>,
            Option<&GrassCamera>,
            Option<&mut GrassViewBuffers>,
        ),
        (With<Camera3d>, Without<NoGrass>),
    >,
    mut warned_layers: Local<bool>,
    mut gone: RemovedComponents<GrassViewBuffers>,
    readings: Res<GpuReadings>,
    extracted: Res<ExtractedGrass>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
) {
    // A camera that is gone stops counting in the stats.
    for entity in gone.read() {
        readings.forget(entity);
    }
    for (entity, view, layers, camera, buffers) in &mut views {
        // New views, and views whose capacity the game changed, get fresh
        // buffers; the bind groups that held the old ones go with them.
        let Some(mut buffers) = buffers.filter(|buffers| buffers.fits(&extracted.settings)) else {
            commands.entity(entity).insert((
                GrassViewBuffers::new(&device, &extracted.settings),
                GrassViewBindGroups::default(),
            ));
            continue;
        };
        // The resource's settings, with this camera's own over them.
        let settings = &extracted.settings.for_camera(camera);
        let mut data = view_uniform(
            view,
            settings,
            &buffers.capacities,
            extracted
                .shadow_light
                .filter(|_| settings.shadows.cascades() > 0),
        );
        data.sun = extracted.sun.map_or(Vec4::ZERO, |sun| sun.extend(1.0));
        data.layers = layer_mask(layers);
        if has_unsupported_layer(layers) && !*warned_layers {
            *warned_layers = true;
            warn!(
                "bevy_grass: a camera is on a render layer above 31; grass supports layers 0 \
                 to 31, so the others are ignored."
            );
        }
        update_trample(&mut buffers, &mut data, view, settings, &extracted, &queue);
        buffers.uniform.set(data);
        buffers.uniform.write_buffer(&device, &queue);
        buffers.readback_interval = settings.readback_interval;
        buffers.frames_since_readback += 1;
        check_readback(&mut buffers, entity, &readings);
    }
}

fn view_uniform(
    view: &ExtractedView,
    settings: &GrassSettings,
    capacities: &[u32; 3],
    shadow_light: Option<Vec3>,
) -> ViewUniformData {
    let world_from_view = view.world_from_view.to_matrix();
    let clip_from_world = view
        .clip_from_world
        .unwrap_or_else(|| view.clip_from_view * world_from_view.inverse());
    let orthographic = view.clip_from_view.w_axis.w == 1.0;
    let position = view.world_from_view.translation();
    let forward = view.world_from_view.forward().as_vec3();
    let pixel_scale = 2.0 / (view.clip_from_view.x_axis.x * view.viewport.z.max(1) as f32);

    let lod = DistanceLod::from_settings(settings);
    // Positions are quantized to 16 bits over ±extent around an origin that
    // moves in whole quantization steps, so blades never shimmer as the
    // camera moves. Heights use the same steps, over at least 256 m.
    let extent = if orthographic {
        settings.orthographic_extent
    } else {
        settings.radius + 16.0
    };
    let height_range = extent.max(MIN_HEIGHT_RANGE);
    let step = 2.0 * extent / 65_535.0;
    let snap = step * 128.0;
    let origin = (position / snap).round() * snap;

    let shadow_offset = shadow_light.map_or(Vec4::ZERO, |light| {
        let down = (-light.y).max(0.2);
        Vec4::new(light.x / down, 0.0, light.z / down, 1.0)
    });
    let bucket_base = UVec4::new(0, capacities[0], capacities[0] + capacities[1], 0);
    let (fixed_lod, bucket_lod) = if orthographic {
        (
            settings.orthographic_lod,
            UVec4::new(settings.orthographic_lod, 1, 2, 0),
        )
    } else {
        (DISTANCE_LOD, UVec4::new(0, 1, 2, 0))
    };
    ViewUniformData {
        planes: frustum_planes(&clip_from_world),
        camera_position: position.extend(if orthographic { 1.0 } else { 0.0 }),
        camera_forward: forward.extend(pixel_scale),
        shadow_offset,
        instance_origin: origin.extend(extent),
        radius: lod.radius,
        lod1_start: lod.lod_start[0],
        lod2_start: lod.lod_start[1],
        lod_band: lod.band,
        density_scale: settings.density_scale,
        fixed_lod,
        max_visible_chunks: settings.max_chunk_slices,
        interactor_count: 0,
        layers: 1,
        min_width_px: settings.min_width_px,
        bucket_base,
        bucket_capacity: UVec4::new(capacities[0], capacities[1], capacities[2], 0),
        bucket_lod,
        trample_origin: IVec4::ZERO,
        trample_step: Vec4::ZERO,
        sun: Vec4::ZERO,
        lod_density: Vec4::new(
            lod.density_at[0],
            lod.density_at[1],
            lod.mid_range_density,
            lod.fade_start,
        ),
        limits: Vec4::new(
            settings.max_widening,
            settings.max_interactor_radius,
            settings.turn_to_camera,
            height_range,
        ),
    }
}

/// Moves the view's trample window, uploads the interactors it stamps this
/// frame and fills the uniform's trample fields.
fn update_trample(
    buffers: &mut GrassViewBuffers,
    data: &mut ViewUniformData,
    view: &ExtractedView,
    settings: &GrassSettings,
    extracted: &ExtractedGrass,
    queue: &RenderQueue,
) {
    let orthographic = view.clip_from_view.w_axis.w == 1.0;
    let center = trample::focus(
        view.world_from_view.translation(),
        view.world_from_view.forward().as_vec3(),
        orthographic,
        &extracted.tables,
    );
    let cell = settings.trample_cell_size;
    let origin = trample::window_origin(center, cell);
    // Before the first frame, and when the cells change size, every slot
    // counts as newly in the window, which clears it.
    if buffers.trample_cell != cell {
        buffers.trample_cell = cell;
        buffers.trample_origin = None;
    }
    let previous = buffers
        .trample_origin
        .unwrap_or(origin + IVec2::splat(trample::TEXELS * 4));
    buffers.trample_origin = Some(origin);

    let stamped = trample::select_interactors(
        &extracted.interactors,
        center,
        settings.max_interactors,
        cell,
        settings.max_interactor_radius,
    );
    if !stamped.is_empty() {
        queue.write_buffer(&buffers.interactors, 0, bytemuck::cast_slice(&stamped));
    }
    buffers.interactor_count = stamped.len() as u32;

    let elapsed = (extracted.frame.time - extracted.frame.previous_time).max(0.0);
    data.interactor_count = buffers.interactor_count;
    data.trample_origin = IVec4::new(origin.x, origin.y, previous.x, previous.y);
    data.trample_step = Vec4::new(cell, elapsed / settings.trample_recovery, 0.0, 0.0);
}

/// Advances the overflow check: maps a copied buffer, or reports a mapped one.
fn check_readback(buffers: &mut GrassViewBuffers, view: Entity, readings: &GpuReadings) {
    match buffers.readback_state.load(Ordering::Acquire) {
        READBACK_COPIED => {
            buffers
                .readback_state
                .store(READBACK_MAPPING, Ordering::Release);
            let buffer = buffers.readback.clone();
            let state = buffers.readback_state.clone();
            let values = buffers.readback_values.clone();
            buffers
                .readback
                .slice(..)
                .map_async(MapMode::Read, move |result| {
                    if result.is_ok() {
                        if let Ok(bytes) = buffer.slice(..).get_mapped_range() {
                            let words: &[u32] = bytemuck::cast_slice(&bytes);
                            if let Ok(mut out) = values.lock() {
                                out.copy_from_slice(&words[..4]);
                            }
                        }
                        buffer.unmap();
                    }
                    state.store(READBACK_READY, Ordering::Release);
                });
        }
        READBACK_READY => {
            let counts = buffers
                .readback_values
                .lock()
                .map(|v| *v)
                .unwrap_or_default();
            buffers
                .readback_state
                .store(READBACK_IDLE, Ordering::Release);
            debug!(
                "bevy_grass: {} visible chunk slices; blades per bucket {} / {} / {}",
                counts[3], counts[0], counts[1], counts[2]
            );
            let over: Vec<String> = (0..3)
                .filter(|&b| counts[b] > buffers.capacities[b])
                .map(|b| {
                    format!(
                        "bucket {b}: {} blades for {}",
                        counts[b], buffers.capacities[b]
                    )
                })
                .collect();
            let overflowed = !over.is_empty() || counts[3] > buffers.max_slices;
            readings.record(view, counts, overflowed);
            if !over.is_empty() && !buffers.warned {
                buffers.warned = true;
                warn!(
                    "bevy_grass: instance buffers overflowed ({}); extra blades are dropped. Raise \
                     GrassSettings::instance_capacity or lower the style density.",
                    over.join(", ")
                );
            }
            if counts[3] > buffers.max_slices && !buffers.warned {
                buffers.warned = true;
                warn!(
                    "bevy_grass: {} visible chunk slices for {}; the grass of the rest is \
                     dropped. Lower the style density or the grass radius, or raise \
                     GrassSettings::max_chunk_slices.",
                    counts[3], buffers.max_slices
                );
            }
        }
        _ => {}
    }
}

/// Builds each view's compute and draw bind groups.
pub(crate) fn prepare_view_bind_groups(
    mut views: Query<(&GrassViewBuffers, &mut GrassViewBindGroups)>,
    shared: Res<GrassShared>,
    layouts: Res<GrassLayouts>,
    pipeline_cache: Res<PipelineCache>,
    device: Res<RenderDevice>,
) {
    let (Some(frame), Some(fields)) = (shared.frame.binding(), shared.fields.binding()) else {
        return;
    };
    for (buffers, mut groups) in &mut views {
        let Some(uniform) = buffers.uniform.binding() else {
            continue;
        };
        groups.compute = Some(device.create_bind_group(
            "grass_view",
            &pipeline_cache.get_bind_group_layout(&layouts.view),
            &BindGroupEntries::sequential((
                uniform.clone(),
                buffers.visible.as_entire_binding(),
                buffers.counters.as_entire_binding(),
                buffers.instances.as_entire_binding(),
                buffers.draw_args.as_entire_binding(),
                buffers.trample.as_entire_binding(),
                buffers.interactors.as_entire_binding(),
            )),
        ));
        groups.dispatch = Some(device.create_bind_group(
            "grass_dispatch",
            &pipeline_cache.get_bind_group_layout(&layouts.dispatch),
            &BindGroupEntries::single(buffers.dispatch.as_entire_binding()),
        ));
        groups.draw = Some(device.create_bind_group(
            "grass_draw",
            &pipeline_cache.get_bind_group_layout(&layouts.draw),
            &BindGroupEntries::sequential((
                frame.clone(),
                uniform,
                buffers.instances.as_entire_binding(),
                fields.clone(),
                &shared.gust_view,
                &shared.gust_sampler,
            )),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frustum_planes_contain_what_the_camera_sees() {
        use bevy::camera::{CameraProjection, PerspectiveProjection};
        use bevy::transform::components::Transform;
        let projection = PerspectiveProjection {
            fov: 1.0,
            aspect_ratio: 16.0 / 9.0,
            near: 0.1,
            ..Default::default()
        }
        .get_clip_from_view();
        let view = Transform::from_xyz(0.0, 2.0, 10.0)
            .looking_at(Vec3::ZERO, Vec3::Y)
            .to_matrix()
            .inverse();
        let planes = frustum_planes(&(projection * view));
        let inside = |p: Vec3| {
            planes
                .iter()
                .all(|plane| plane.truncate().dot(p) + plane.w >= 0.0)
        };
        assert!(inside(Vec3::ZERO));
        assert!(inside(Vec3::new(0.0, 0.0, -500.0)));
        assert!(!inside(Vec3::new(0.0, 0.0, 20.0)), "behind the camera");
        assert!(!inside(Vec3::new(100.0, 0.0, 0.0)), "far to the side");
    }
}
