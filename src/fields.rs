//! The CPU's whole steady-state job: describing the fields.
//!
//! When a field, its style, its mask or its heightmap changes, the field
//! table, the chunk table and the packed masks and heights are rebuilt and
//! handed to the render world. Nothing here runs per blade or per frame; the
//! GPU decides which blades exist (`shaders/generate.wesl`).

use std::sync::Arc;

use bevy::asset::{AssetEvent, Assets};
use bevy::color::{ColorToComponents, LinearRgba};
use bevy::ecs::message::MessageReader;
use bevy::ecs::prelude::*;
use bevy::image::Image;
use bevy::log::warn;
use bevy::math::{UVec2, UVec4, Vec2, Vec3, Vec4};
use bevy::render::render_resource::ShaderType;
use bevy::transform::components::GlobalTransform;

use crate::api::{GrassField, GrassGround, GrassStyle};
use crate::mirror::{self, ChunkLayout};

/// Marks a field without a mask or heightmap.
pub(crate) const NO_MAP: u32 = u32::MAX;

/// Side of a chunk, in metres.
///
/// One size for every view, because the chunk table is shared. T2 in the plan
/// suggested 8 m chunks for orthographic views; per-blade frustum culling
/// rejects what a larger chunk lets through, so this stays 16 m until a
/// measurement says otherwise.
pub(crate) const CHUNK_SIZE: f32 = 16.0;

/// Fields beyond this index cannot be addressed by the 8-bit field index.
const MAX_FIELDS: usize = 256;

/// One field as the shaders see it. Mirrors `Field` in `common.wesl`.
#[derive(Debug, Clone, Copy, PartialEq, Default, ShaderType)]
pub(crate) struct FieldGpu {
    /// xyz: the field's minimum corner at its base height.
    pub min_corner: Vec4,
    /// x, y: size along X and Z; z: heightmap scale; w: tile size.
    pub extent: Vec4,
    /// x: shortest blade, y: tallest blade, z: root width, w: blades per m².
    pub shape: Vec4,
    /// x: lean, y: stiffness, z: minimum width in pixels, w: root occlusion.
    pub motion: Vec4,
    pub root_color: Vec4,
    pub tip_color: Vec4,
    /// x: seed, y: tiles per chunk side, z: mask offset, w: heightmap offset
    /// (offsets in texels; `NO_MAP` when absent).
    pub placement: UVec4,
    /// Mask width and height, heightmap width and height, in texels.
    pub map_sizes: UVec4,
}

/// One chunk. Mirrors `Chunk` in `common.wesl`: 32 bytes.
#[derive(Debug, Clone, Copy, PartialEq, Default, ShaderType)]
pub(crate) struct ChunkGpu {
    /// x, y: offset of the chunk's minimum corner inside its field, in metres;
    /// z, w: lowest and highest ground in the chunk, world Y.
    pub bounds: Vec4,
    /// x: field index, y, z: chunk coordinates in the field.
    pub ids: UVec4,
}

/// Everything the GPU needs to know about the fields. Replaced, never edited,
/// so the render world can keep the previous version while a new one builds.
#[derive(Resource, Debug, Default, Clone)]
pub(crate) struct FieldTables {
    /// Bumped on every rebuild; the render world uploads when it changes.
    pub version: u64,
    pub fields: Arc<Vec<FieldGpu>>,
    pub chunks: Arc<Vec<ChunkGpu>>,
    /// Every mask, two R8G8 texels per word, low half first.
    pub masks: Arc<Vec<u32>>,
    /// Every heightmap, one normalized height per texel.
    pub heights: Arc<Vec<f32>>,
}

/// Whether the tables need a rebuild.
#[derive(Resource, Debug, Default)]
pub(crate) struct TablesDirty(bool);

/// A field whose settings or position changed. `With<GrassField>` matters:
/// without it, every moving camera or character would rebuild the tables.
type FieldChanged = (
    With<GrassField>,
    Or<(Changed<GrassField>, Changed<GlobalTransform>)>,
);

/// Marks the tables for a rebuild when anything they depend on changes.
pub(crate) fn detect_changes(
    mut dirty: ResMut<TablesDirty>,
    changed: Query<(), FieldChanged>,
    mut removed: RemovedComponents<GrassField>,
    mut styles: MessageReader<AssetEvent<GrassStyle>>,
    mut images: MessageReader<AssetEvent<Image>>,
    fields: Query<&GrassField>,
) {
    let image_changed = images.read().any(|event| {
        let id = match event {
            AssetEvent::Added { id }
            | AssetEvent::Modified { id }
            | AssetEvent::LoadedWithDependencies { id }
            | AssetEvent::Removed { id }
            | AssetEvent::Unused { id } => *id,
        };
        fields.iter().any(|field| {
            field.density.as_ref().is_some_and(|mask| mask.id() == id)
                || matches!(&field.ground, GrassGround::Heightmap { image, .. } if image.id() == id)
        })
    });
    if !changed.is_empty()
        || removed.read().count() > 0
        || styles.read().count() > 0
        || image_changed
    {
        dirty.0 = true;
    }
}

/// Rebuilds the tables when something changed and every input has loaded.
pub(crate) fn rebuild(
    mut dirty: ResMut<TablesDirty>,
    mut tables: ResMut<FieldTables>,
    fields: Query<(Entity, &GrassField, &GlobalTransform)>,
    styles: Res<Assets<GrassStyle>>,
    images: Res<Assets<Image>>,
) {
    if !dirty.0 {
        return;
    }
    let mut sorted: Vec<_> = fields.iter().collect();
    sorted.sort_by_key(|(entity, ..)| *entity);
    if sorted.len() > MAX_FIELDS {
        warn!(
            "bevy_grass draws at most {MAX_FIELDS} fields; {} exist",
            sorted.len()
        );
        sorted.truncate(MAX_FIELDS);
    }

    // Wait for every style, mask and heightmap, so a field never appears at
    // the wrong density or height and then jumps.
    let mut inputs = Vec::with_capacity(sorted.len());
    for (_, field, transform) in &sorted {
        let Some(style) = styles.get(&field.style) else {
            return;
        };
        let mask = match &field.density {
            Some(handle) => match images.get(handle) {
                Some(image) => Some(image),
                None => return,
            },
            None => None,
        };
        let heightmap = match &field.ground {
            GrassGround::Flat => None,
            GrassGround::Heightmap {
                image,
                height_scale,
            } => match images.get(image) {
                Some(image) => Some((image, *height_scale)),
                None => return,
            },
        };
        inputs.push((*field, *transform, style, mask, heightmap));
    }
    dirty.0 = false;

    let mut builder = TableBuilder::default();
    for (index, (field, transform, style, mask, heightmap)) in inputs.iter().enumerate() {
        builder.add_field(index as u32, field, transform, style, *mask, *heightmap);
    }
    *tables = FieldTables {
        version: tables.version + 1,
        fields: Arc::new(builder.fields),
        chunks: Arc::new(builder.chunks),
        masks: Arc::new(builder.masks),
        heights: Arc::new(builder.heights),
    };
}

/// The tables being built, one field at a time.
#[derive(Default)]
struct TableBuilder {
    fields: Vec<FieldGpu>,
    chunks: Vec<ChunkGpu>,
    masks: Vec<u32>,
    heights: Vec<f32>,
}

impl TableBuilder {
    fn add_field(
        &mut self,
        index: u32,
        field: &GrassField,
        transform: &GlobalTransform,
        style: &GrassStyle,
        mask: Option<&Image>,
        heightmap: Option<(&Image, f32)>,
    ) {
        let corner = min_corner(field, transform);
        let layout = ChunkLayout::for_density(CHUNK_SIZE, style.density);
        let (mask_offset, mask_size) = match mask {
            Some(image) => {
                let offset = self.masks.len() as u32 * 2;
                (offset, pack_mask(image, &mut self.masks))
            }
            None => (NO_MAP, UVec2::ZERO),
        };
        let (height_offset, height_size, height_scale) = match heightmap {
            Some((image, scale)) => {
                let offset = self.heights.len() as u32;
                (offset, read_heights(image, &mut self.heights), scale)
            }
            None => (NO_MAP, UVec2::ZERO, 0.0),
        };

        self.fields.push(FieldGpu {
            min_corner: corner.extend(0.0),
            extent: Vec4::new(field.size.x, field.size.y, height_scale, layout.tile_size),
            shape: Vec4::new(style.height.x, style.height.y, style.width, style.density),
            motion: Vec4::new(
                style.lean,
                style.stiffness,
                style.min_width_px,
                style.root_occlusion,
            ),
            root_color: linear(style.root_color),
            // The tip colour's alpha carries the translucency.
            tip_color: linear(style.tip_color)
                .truncate()
                .extend(style.translucency),
            // The seed comes from where the field is, not from its entity, so
            // the same field looks the same however the scene was spawned.
            placement: UVec4::new(
                mirror::hash2(corner.x.to_bits(), corner.z.to_bits()),
                layout.tiles_per_side,
                mask_offset,
                height_offset,
            ),
            map_sizes: UVec4::new(mask_size.x, mask_size.y, height_size.x, height_size.y),
        });

        let counts = (field.size / CHUNK_SIZE).ceil().as_uvec2();
        for chunk_z in 0..counts.y {
            for chunk_x in 0..counts.x {
                let local = Vec2::new(chunk_x as f32, chunk_z as f32) * CHUNK_SIZE;
                let (low, high) = if height_offset == NO_MAP {
                    (corner.y, corner.y)
                } else {
                    let (low, high) = height_range(
                        &self.heights[height_offset as usize..],
                        height_size,
                        local / field.size,
                        CHUNK_SIZE / field.size,
                    );
                    (
                        corner.y + low * height_scale,
                        corner.y + high * height_scale,
                    )
                };
                self.chunks.push(ChunkGpu {
                    bounds: Vec4::new(local.x, local.y, low, high),
                    ids: UVec4::new(index, chunk_x, chunk_z, 0),
                });
            }
        }
    }
}

fn linear(color: bevy::color::Color) -> Vec4 {
    Vec4::from_array(LinearRgba::from(color).to_f32_array())
}

fn min_corner(field: &GrassField, transform: &GlobalTransform) -> Vec3 {
    let origin = transform.translation();
    Vec3::new(
        origin.x - field.size.x * 0.5,
        origin.y,
        origin.z - field.size.y * 0.5,
    )
}

/// Appends a mask as R8G8 texels, two per word, and returns its size. The
/// word array is padded so every mask starts on a whole word.
fn pack_mask(image: &Image, out: &mut Vec<u32>) -> UVec2 {
    let size = image.size();
    let mut half = None;
    for y in 0..size.y {
        for x in 0..size.x {
            let texel = image.get_color_at(x, y).map_or(0xffff, |color| {
                let c = LinearRgba::from(color);
                let r = (c.red.clamp(0.0, 1.0) * 255.0).round() as u32;
                let g = (c.green.clamp(0.0, 1.0) * 255.0).round() as u32;
                r | (g << 8)
            });
            match half.take() {
                None => half = Some(texel),
                Some(low) => out.push(low | (texel << 16)),
            }
        }
    }
    if let Some(low) = half {
        out.push(low);
    }
    size
}

/// Appends a heightmap's red channel, normalized, and returns its size.
fn read_heights(image: &Image, out: &mut Vec<f32>) -> UVec2 {
    let size = image.size();
    for y in 0..size.y {
        for x in 0..size.x {
            out.push(
                image
                    .get_color_at(x, y)
                    .map_or(0.0, |c| LinearRgba::from(c).red),
            );
        }
    }
    size
}

/// Lowest and highest normalized height over a UV rectangle, one texel wider
/// on every side so bilinear sampling at the edges is covered.
fn height_range(heights: &[f32], size: UVec2, uv_min: Vec2, uv_size: Vec2) -> (f32, f32) {
    let texels = size.as_vec2();
    let lo = ((uv_min * texels).floor().as_ivec2() - 1)
        .max(bevy::math::IVec2::ZERO)
        .as_uvec2();
    let hi = (((uv_min + uv_size) * texels).ceil().as_ivec2() + 1)
        .min(size.as_ivec2() - 1)
        .max(bevy::math::IVec2::ZERO)
        .as_uvec2();
    let mut range = (f32::MAX, f32::MIN);
    for y in lo.y..=hi.y {
        for x in lo.x..=hi.x {
            let h = heights[(y * size.x + x) as usize];
            range = (range.0.min(h), range.1.max(h));
        }
    }
    if range.0 > range.1 { (0.0, 0.0) } else { range }
}

#[cfg(test)]
mod tests {
    use bevy::MinimalPlugins;
    use bevy::app::{App, Update};
    use bevy::asset::{AssetApp, AssetPlugin};
    use bevy::transform::components::Transform;

    use super::*;

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<GrassStyle>()
            .init_asset::<Image>()
            .init_resource::<FieldTables>()
            .init_resource::<TablesDirty>()
            .add_systems(Update, (detect_changes, rebuild).chain());
        app
    }

    #[test]
    fn only_a_field_moving_rebuilds_the_tables() {
        let mut app = app();
        let style = app
            .world_mut()
            .resource_mut::<Assets<GrassStyle>>()
            .add(GrassStyle::MEADOW);
        let field = app
            .world_mut()
            .spawn((
                GrassField {
                    size: Vec2::splat(32.0),
                    style,
                    ..Default::default()
                },
                GlobalTransform::IDENTITY,
            ))
            .id();
        let camera = app
            .world_mut()
            .spawn((Transform::IDENTITY, GlobalTransform::IDENTITY))
            .id();
        app.update();
        app.update();
        let built = app.world().resource::<FieldTables>().version;
        assert!(built > 0, "the first field builds the tables");
        assert_eq!(app.world().resource::<FieldTables>().chunks.len(), 4);

        for step in 1..5 {
            *app.world_mut().get_mut::<GlobalTransform>(camera).unwrap() =
                GlobalTransform::from_xyz(step as f32, 0.0, 0.0);
            app.update();
        }
        assert_eq!(
            app.world().resource::<FieldTables>().version,
            built,
            "a moving camera rebuilt the tables"
        );

        *app.world_mut().get_mut::<GlobalTransform>(field).unwrap() =
            GlobalTransform::from_xyz(5.0, 0.0, 0.0);
        app.update();
        assert_eq!(app.world().resource::<FieldTables>().version, built + 1);
    }

    #[test]
    fn a_chunk_height_range_covers_every_texel_it_touches() {
        // A 4 × 4 heightmap with one tall texel in the corner.
        let mut heights = vec![0.2; 16];
        heights[15] = 0.9;
        let size = UVec2::new(4, 4);
        let (low, high) = height_range(&heights, size, Vec2::new(0.5, 0.5), Vec2::splat(0.5));
        assert_eq!((low, high), (0.2, 0.9));
        let (_, high) = height_range(&heights, size, Vec2::ZERO, Vec2::splat(0.25));
        assert_eq!(high, 0.2);
    }
}
