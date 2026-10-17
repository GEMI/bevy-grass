//! GPU resources shared by every view: the field and chunk tables, masks,
//! heightmaps, the blue-noise tile, the gust texture, the shared index
//! buffer and the per-frame uniform. Uploaded only when the fields change.

use std::sync::Arc;

use crate::clock::GrassClock;
use bevy::asset::Handle;
use bevy::ecs::prelude::*;
use bevy::light::DirectionalLight;
use bevy::math::{UVec4, Vec2, Vec3, Vec4};
use bevy::render::Extract;
use bevy::render::render_resource::binding_types::{
    sampler, storage_buffer_read_only_sized, storage_buffer_sized, texture_2d, uniform_buffer,
};
use bevy::render::render_resource::{
    AddressMode, BindGroup, BindGroupEntries, BindGroupLayoutDescriptor, BindGroupLayoutEntries,
    Buffer, BufferInitDescriptor, BufferUsages, Extent3d, FilterMode, PipelineCache, Sampler,
    SamplerBindingType, SamplerDescriptor, ShaderStages, ShaderType, StorageBuffer,
    TextureDataOrder, TextureDescriptor, TextureDimension, TextureFormat, TextureSampleType,
    TextureUsages, TextureView, TextureViewDescriptor, UniformBuffer,
};
use bevy::render::renderer::{RenderDevice, RenderQueue};
use bevy::render::view::{ViewUniform, ViewUniforms};
use bevy::shader::Shader;
use bevy::transform::components::GlobalTransform;

use super::trample;
use super::view::ViewUniformData;
use crate::GrassSettings;
use crate::api::{GrassInteractor, GrassSun, GrassWind};
use crate::fields::{CHUNK_SIZE, ChunkGpu, FieldGpu, FieldTables};
use crate::mirror;
use crate::sun::{SunCandidate, pick_sun};

/// Side of the gust texture, in texels.
pub(crate) const GUST_SIZE: u32 = 256;

/// The embedded shaders, loaded in the main app and handed to the render app.
#[derive(Resource, Debug, Clone)]
pub(crate) struct GrassShaders {
    pub main: Handle<Shader>,
    pub prepass: Handle<Shader>,
    pub shadow: Handle<Shader>,
    pub generate: Handle<Shader>,
    pub trample: Handle<Shader>,
}

/// Mirrors `Frame` in `common.wesl`.
#[derive(Debug, Clone, Copy, Default, ShaderType)]
pub(crate) struct FrameUniform {
    pub time: f32,
    pub previous_time: f32,
    pub wind_strength: f32,
    pub gust_scale: f32,
    pub gust_speed: f32,
    pub chunk_size: f32,
    pub chunk_count: u32,
    pub _pad0: u32,
    pub wind_direction: Vec2,
    pub gust_offset: Vec2,
    pub previous_gust_offset: Vec2,
    pub sway: f32,
    pub sway_speed: f32,
    pub flutter: f32,
    pub gust_contrast: f32,
    pub lod_rows: UVec4,
}

/// Mirrors `LodTable` in `common.wesl`.
#[derive(Debug, Clone, Copy, Default, ShaderType)]
struct LodTable {
    index_count: UVec4,
    first_index: UVec4,
}

/// What the main world handed over this frame.
#[derive(Resource, Debug, Default)]
pub(crate) struct ExtractedGrass {
    pub tables: FieldTables,
    pub frame: FrameUniform,
    /// Direction the first shadow-casting directional light shines, if any.
    pub shadow_light: Option<Vec3>,
    /// Direction to the light the deferred G-buffer treats as the sun: the
    /// first shadow-casting directional light, or else the first one.
    pub sun: Option<Vec3>,
    /// Every interactor, as `(x, z, radius, strength)`.
    pub interactors: Vec<Vec4>,
    /// This frame's settings, in range.
    pub settings: GrassSettings,
}

/// A directional light, for choosing the one grass follows.
type SunLight = (
    Entity,
    &'static DirectionalLight,
    &'static GlobalTransform,
    Has<GrassSun>,
);

/// Copies the tables (only when rebuilt, by reference), the wind, the clock,
/// the shadow-casting light and mode, and the interactors.
pub(crate) fn extract_grass(
    mut extracted: ResMut<ExtractedGrass>,
    tables: Extract<Res<FieldTables>>,
    wind: Extract<Res<GrassWind>>,
    clock: Extract<Res<GrassClock>>,
    lights: Extract<Query<SunLight>>,
    interactors: Extract<Query<(&GrassInteractor, &GlobalTransform)>>,
    settings: Extract<Res<GrassSettings>>,
) {
    extracted.settings = settings.sanitized();
    let rows = extracted.settings.blade_rows;
    if tables.version != extracted.tables.version {
        extracted.tables = tables.clone();
    }
    extracted.frame = FrameUniform {
        time: clock.time,
        previous_time: clock.previous_time,
        wind_strength: wind.strength,
        gust_scale: wind.gust_scale,
        gust_speed: wind.gust_speed,
        chunk_size: CHUNK_SIZE,
        chunk_count: extracted.tables.chunks.len() as u32,
        wind_direction: wind.direction,
        gust_offset: clock.gust_offset,
        previous_gust_offset: clock.previous_gust_offset,
        sway: wind.sway.max(0.0),
        sway_speed: wind.sway_speed.max(0.0),
        flutter: wind.flutter.max(0.0),
        gust_contrast: wind.gust_contrast.max(0.0),
        lod_rows: UVec4::new(rows[0], rows[1], rows[2], 0),
        ..Default::default()
    };
    // The light grass follows: the one marked `GrassSun`, or else the first
    // that casts shadows. The G-buffer's sun is that light, or else the first.
    let (shadow_light, sun) =
        pick_sun(
            lights
                .iter()
                .map(|(entity, light, transform, marked)| SunCandidate {
                    entity,
                    casts: light.shadow_maps_enabled,
                    marked,
                    shines: transform.forward().as_vec3(),
                }),
        );
    extracted.shadow_light = shadow_light;
    extracted.sun = sun.map(|shines| -shines);
    extracted.interactors.clear();
    let tables = &extracted.tables;
    let stamps: Vec<Vec4> = interactors
        .iter()
        .map(|(interactor, transform)| {
            let at = transform.translation();
            // Only an interactor with a height limit needs the ground.
            let ground = interactor
                .max_height
                .and_then(|_| tables.highest_ground_at(Vec2::new(at.x, at.z)));
            let reach = trample::reach(interactor.max_height, at.y, ground);
            Vec4::new(at.x, at.z, interactor.radius, interactor.strength * reach)
        })
        .collect();
    extracted.interactors = stamps;
}

/// Bind group layouts, shared by the pipelines and the bind groups.
#[derive(Resource, Debug, Clone)]
pub(crate) struct GrassLayouts {
    /// Group 0 of the compute passes: everything shared by the views.
    pub shared: BindGroupLayoutDescriptor,
    /// Group 1 of the compute passes: one view's buffers.
    pub view: BindGroupLayoutDescriptor,
    /// Group 2 of `size_dispatch`: the view's indirect dispatch arguments.
    pub dispatch: BindGroupLayoutDescriptor,
    /// Group 2 of every grass draw: one view's instances.
    pub draw: BindGroupLayoutDescriptor,
    /// Group 0 of the shadow pass: the light view's uniform.
    pub shadow_view: BindGroupLayoutDescriptor,
    /// Group 1 of the shadow pass, empty so the grass group stays at 2.
    pub empty: BindGroupLayoutDescriptor,
}

impl GrassLayouts {
    fn new() -> Self {
        let read = || storage_buffer_read_only_sized(false, None);
        let read_write = || storage_buffer_sized(false, None);
        let shared = BindGroupLayoutDescriptor::new(
            "grass_shared",
            &BindGroupLayoutEntries::sequential(
                ShaderStages::COMPUTE,
                (
                    uniform_buffer::<FrameUniform>(false),
                    read(),
                    read(),
                    read(),
                    read(),
                    read(),
                    texture_2d(TextureSampleType::Float { filterable: true }),
                    sampler(SamplerBindingType::Filtering),
                    uniform_buffer::<LodTable>(false),
                    read(),
                    // The tuft table.
                    read(),
                ),
            ),
        );
        let view = BindGroupLayoutDescriptor::new(
            "grass_view",
            &BindGroupLayoutEntries::sequential(
                ShaderStages::COMPUTE,
                (
                    uniform_buffer::<ViewUniformData>(false),
                    read_write(),
                    read_write(),
                    read_write(),
                    read_write(),
                    // Trample cells and interactors.
                    read_write(),
                    read_write(),
                ),
            ),
        );
        let dispatch = BindGroupLayoutDescriptor::new(
            "grass_dispatch",
            &BindGroupLayoutEntries::single(ShaderStages::COMPUTE, read_write()),
        );
        let draw = BindGroupLayoutDescriptor::new(
            "grass_draw",
            &BindGroupLayoutEntries::sequential(
                ShaderStages::VERTEX_FRAGMENT,
                (
                    uniform_buffer::<FrameUniform>(false),
                    uniform_buffer::<ViewUniformData>(false),
                    read(),
                    read(),
                    // The noise the colour patches come from.
                    texture_2d(TextureSampleType::Float { filterable: true }),
                    sampler(SamplerBindingType::Filtering),
                ),
            ),
        );
        let shadow_view = BindGroupLayoutDescriptor::new(
            "grass_shadow_view",
            &BindGroupLayoutEntries::single(
                ShaderStages::VERTEX,
                uniform_buffer::<ViewUniform>(true),
            ),
        );
        let empty = BindGroupLayoutDescriptor::new("grass_empty", &[]);
        Self {
            shared,
            view,
            dispatch,
            draw,
            shadow_view,
            empty,
        }
    }
}

/// The shared GPU buffers.
#[derive(Resource)]
pub(crate) struct GrassShared {
    pub frame: UniformBuffer<FrameUniform>,
    pub fields: StorageBuffer<Vec<FieldGpu>>,
    chunks: StorageBuffer<Vec<ChunkGpu>>,
    masks: StorageBuffer<Vec<u32>>,
    heights: StorageBuffer<Vec<f32>>,
    tile: Buffer,
    clumps: Buffer,
    tufts: Buffer,
    lods: UniformBuffer<LodTable>,
    /// All three LODs' indices, back to back.
    pub indices: Buffer,
    /// The rows per LOD `indices` was built for.
    blade_rows: [u32; 3],
    pub gust_view: TextureView,
    pub gust_sampler: Sampler,
    uploaded_version: u64,
    uploaded_structure: u64,
    /// Chunks in the table; zero means no grass anywhere.
    pub chunk_count: u32,
}

/// Bind groups shared by every view, rebuilt when their buffers might move.
#[derive(Resource, Default)]
pub(crate) struct GrassSharedBindGroups {
    pub compute: Option<BindGroup>,
    pub shadow_view: Option<BindGroup>,
    pub empty: Option<BindGroup>,
}

/// Creates every shared resource once the render device exists.
pub(crate) fn init_shared(
    mut commands: Commands,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
) {
    let rows = GrassSettings::HIGH.blade_rows;
    let (indices, table) = blade_indices(&device, rows);
    let mut lods = UniformBuffer::from(table);
    lods.write_buffer(&device, &queue);

    let tile_points: Vec<[f32; 2]> = mirror::blue_noise_tile()
        .iter()
        .map(Vec2::to_array)
        .collect();
    let tile = device.create_buffer_with_data(&BufferInitDescriptor {
        label: Some("grass_blue_noise_tile"),
        contents: bytemuck::cast_slice(&tile_points),
        usage: BufferUsages::STORAGE,
    });

    let clumps = device.create_buffer_with_data(&BufferInitDescriptor {
        label: Some("grass_clumps"),
        contents: bytemuck::cast_slice(mirror::clump_table()),
        usage: BufferUsages::STORAGE,
    });

    let tufts = device.create_buffer_with_data(&BufferInitDescriptor {
        label: Some("grass_tufts"),
        contents: bytemuck::cast_slice(mirror::tuft_table()),
        usage: BufferUsages::STORAGE,
    });

    let gust = device.create_texture_with_data(
        &queue,
        &TextureDescriptor {
            label: Some("grass_gust_noise"),
            size: Extent3d {
                width: GUST_SIZE,
                height: GUST_SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::R8Unorm,
            usage: TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        },
        TextureDataOrder::LayerMajor,
        &gust_noise(GUST_SIZE),
    );
    let gust_view = gust.create_view(&TextureViewDescriptor::default());
    let gust_sampler = device.create_sampler(&SamplerDescriptor {
        label: Some("grass_gust_sampler"),
        address_mode_u: AddressMode::Repeat,
        address_mode_v: AddressMode::Repeat,
        mag_filter: FilterMode::Linear,
        min_filter: FilterMode::Linear,
        ..Default::default()
    });

    let mut shared = GrassShared {
        frame: UniformBuffer::default(),
        fields: StorageBuffer::default(),
        chunks: StorageBuffer::default(),
        masks: StorageBuffer::default(),
        heights: StorageBuffer::default(),
        tile,
        clumps,
        tufts,
        lods,
        indices,
        blade_rows: rows,
        gust_view,
        gust_sampler,
        uploaded_version: u64::MAX,
        uploaded_structure: u64::MAX,
        chunk_count: 0,
    };
    upload_tables(&mut shared, &FieldTables::default(), &device, &queue);
    commands.insert_resource(GrassLayouts::new());
    commands.insert_resource(shared);
}

/// Storage bindings may not be empty, so an empty table uploads one default
/// element that nothing ever indexes.
fn non_empty<T: Clone + Default>(items: &Arc<Vec<T>>) -> Vec<T> {
    if items.is_empty() {
        vec![T::default()]
    } else {
        items.as_ref().clone()
    }
}

fn upload_tables(
    shared: &mut GrassShared,
    tables: &FieldTables,
    device: &RenderDevice,
    queue: &RenderQueue,
) {
    shared.uploaded_version = tables.version;
    shared.fields.set(non_empty(&tables.fields));
    shared.fields.write_buffer(device, queue);
    // Showing or hiding a field changes its entries only; the chunks and the
    // images, most of the bytes, stay where they are.
    if shared.uploaded_structure == tables.structure_version {
        return;
    }
    shared.uploaded_structure = tables.structure_version;
    shared.chunk_count = tables.chunks.len() as u32;
    shared.chunks.set(non_empty(&tables.chunks));
    shared.chunks.write_buffer(device, queue);
    shared.masks.set(non_empty(&tables.masks));
    shared.masks.write_buffer(device, queue);
    shared.heights.set(non_empty(&tables.heights));
    shared.heights.write_buffer(device, queue);
}

/// The index buffer of blades with `rows` rows per level of detail, all three
/// back to back, and the table of where each begins.
fn blade_indices(device: &RenderDevice, rows: [u32; 3]) -> (Buffer, LodTable) {
    let mut index_data = Vec::new();
    let mut index_count = UVec4::ZERO;
    let mut first_index = UVec4::ZERO;
    for (lod, rows) in rows.iter().enumerate() {
        let lod_indices = mirror::lod_indices(*rows);
        first_index[lod] = index_data.len() as u32;
        index_count[lod] = lod_indices.len() as u32;
        index_data.extend(lod_indices);
    }
    let indices = device.create_buffer_with_data(&BufferInitDescriptor {
        label: Some("grass_indices"),
        contents: bytemuck::cast_slice(&index_data),
        usage: BufferUsages::INDEX,
    });
    (
        indices,
        LodTable {
            index_count,
            first_index,
        },
    )
}

/// Uploads rebuilt tables and this frame's uniform.
pub(crate) fn prepare_shared(
    mut shared: ResMut<GrassShared>,
    extracted: Res<ExtractedGrass>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
) {
    if extracted.tables.version != shared.uploaded_version {
        upload_tables(&mut shared, &extracted.tables, &device, &queue);
    }
    // A game that changes the blades' rows gets new meshes, once.
    if extracted.settings.blade_rows != shared.blade_rows {
        let (indices, table) = blade_indices(&device, extracted.settings.blade_rows);
        shared.indices = indices;
        shared.blade_rows = extracted.settings.blade_rows;
        shared.lods.set(table);
        shared.lods.write_buffer(&device, &queue);
    }
    shared.frame.set(extracted.frame);
    shared.frame.write_buffer(&device, &queue);
}

/// Rebuilds the shared bind groups. Cheap, and a write can reallocate the
/// uniform and storage buffers, so this runs every frame.
pub(crate) fn prepare_shared_bind_groups(
    mut groups: ResMut<GrassSharedBindGroups>,
    shared: Res<GrassShared>,
    layouts: Res<GrassLayouts>,
    pipeline_cache: Res<PipelineCache>,
    device: Res<RenderDevice>,
    view_uniforms: Res<ViewUniforms>,
) {
    let bindings = (
        shared.frame.binding(),
        shared.fields.binding(),
        shared.chunks.binding(),
        shared.masks.binding(),
        shared.heights.binding(),
        shared.lods.binding(),
    );
    if let (Some(frame), Some(fields), Some(chunks), Some(masks), Some(heights), Some(lods)) =
        bindings
    {
        groups.compute = Some(device.create_bind_group(
            "grass_shared",
            &pipeline_cache.get_bind_group_layout(&layouts.shared),
            &BindGroupEntries::sequential((
                frame,
                fields,
                chunks,
                masks,
                heights,
                shared.tile.as_entire_binding(),
                &shared.gust_view,
                &shared.gust_sampler,
                lods,
                shared.clumps.as_entire_binding(),
                shared.tufts.as_entire_binding(),
            )),
        ));
    }
    if groups.empty.is_none() {
        groups.empty = Some(device.create_bind_group(
            "grass_empty",
            &pipeline_cache.get_bind_group_layout(&layouts.empty),
            &[],
        ));
    }
    groups.shadow_view = view_uniforms.uniforms.binding().map(|view| {
        device.create_bind_group(
            "grass_shadow_view",
            &pipeline_cache.get_bind_group_layout(&layouts.shadow_view),
            &BindGroupEntries::single(view),
        )
    });
}

/// Tileable gust noise: three octaves of value noise on lattices that divide
/// the texture, so it wraps without a seam. Values span the full 0–255 range.
pub(crate) fn gust_noise(size: u32) -> Vec<u8> {
    const OCTAVES: [(u32, f32); 3] = [(4, 0.6), (8, 0.28), (16, 0.12)];
    let lattice = |period: u32, octave: u32, x: u32, y: u32| {
        mirror::unit(mirror::hash2(mirror::hash2(x % period, y % period), octave))
    };
    let smooth = |t: f32| t * t * (3.0 - 2.0 * t);
    let mut values = Vec::with_capacity((size * size) as usize);
    for y in 0..size {
        for x in 0..size {
            let mut value = 0.0;
            for (octave, (period, weight)) in OCTAVES.iter().enumerate() {
                let fx = x as f32 / size as f32 * *period as f32;
                let fy = y as f32 / size as f32 * *period as f32;
                let (ix, iy) = (fx.floor() as u32, fy.floor() as u32);
                let (tx, ty) = (smooth(fx.fract()), smooth(fy.fract()));
                let o = octave as u32;
                let top =
                    lattice(*period, o, ix, iy) * (1.0 - tx) + lattice(*period, o, ix + 1, iy) * tx;
                let bottom = lattice(*period, o, ix, iy + 1) * (1.0 - tx)
                    + lattice(*period, o, ix + 1, iy + 1) * tx;
                value += (top * (1.0 - ty) + bottom * ty) * weight;
            }
            values.push(value);
        }
    }
    let (low, high) = values
        .iter()
        .fold((f32::MAX, f32::MIN), |(lo, hi), v| (lo.min(*v), hi.max(*v)));
    let span = (high - low).max(1e-6);
    values
        .iter()
        .map(|v| ((v - low) / span * 255.0).round() as u8)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gust_noise_wraps_without_a_seam() {
        let size = 64;
        let noise = gust_noise(size);
        let at = |x: u32, y: u32| i32::from(noise[(y * size + x) as usize]);
        // The step across the wrap is no larger than the largest step inside.
        let inner = (0..size - 1)
            .flat_map(|y| (0..size - 1).map(move |x| (x, y)))
            .map(|(x, y)| {
                (at(x, y) - at(x + 1, y))
                    .abs()
                    .max((at(x, y) - at(x, y + 1)).abs())
            })
            .max()
            .unwrap();
        for i in 0..size {
            assert!((at(size - 1, i) - at(0, i)).abs() <= inner);
            assert!((at(i, size - 1) - at(i, 0)).abs() <= inner);
        }
    }

    #[test]
    fn gust_noise_uses_the_full_range() {
        let noise = gust_noise(64);
        assert_eq!(noise.iter().min(), Some(&0));
        assert_eq!(noise.iter().max(), Some(&255));
    }
}
