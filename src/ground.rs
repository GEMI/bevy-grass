//! Mesh grounds: a mesh baked once, on the CPU, into a heightmap that the GPU
//! reads like any other (`GrassGround::Mesh`).
//!
//! The bake is an `Image` in `Assets<Image>`, one height per texel in the
//! mesh's own units, so everything after it (packing, chunk bounds, upload,
//! sampling) is the heightmap path. It is baked again only when the mesh
//! asset, the field's size or the cell size changes: moving, turning or
//! scaling the field reuses it.

use std::collections::{HashMap, HashSet};

use bevy::asset::{AssetEvent, AssetId, Assets, Handle, RenderAssetUsages};
use bevy::ecs::message::MessageReader;
use bevy::ecs::prelude::*;
use bevy::image::Image;
use bevy::log::warn;
use bevy::math::{UVec2, Vec2, Vec3};
use bevy::mesh::{Mesh, MeshAccessError, PrimitiveTopology};
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use crate::api::{GrassField, GrassGround};
use crate::fields::TablesDirty;

/// The height a baked ground has where the mesh does not reach: no blade
/// grows there. Twin of `HOLE` in `generate.wesl`.
pub(crate) const HOLE: f32 = -1e30;

/// Most texels along either side of a baked ground: 4096 by 4096 heights is
/// 64 MB, on the CPU and again on the GPU.
pub(crate) const MAX_TEXELS: u32 = 4096;

/// The baked ground of every field on a mesh.
#[derive(Resource, Debug, Default)]
pub(crate) struct MeshGrounds {
    baked: HashMap<Entity, Baked>,
}

impl MeshGrounds {
    /// The heightmap baked for the field `entity`, once there is one.
    pub(crate) fn image(&self, entity: Entity) -> Option<&Handle<Image>> {
        self.baked.get(&entity).map(|baked| &baked.image)
    }
}

#[derive(Debug)]
struct Baked {
    key: BakeKey,
    image: Handle<Image>,
}

/// What a bake depends on besides the contents of the mesh.
#[derive(Debug, Clone, Copy, PartialEq)]
struct BakeKey {
    mesh: AssetId<Mesh>,
    size: Vec2,
    cell_size: f32,
}

/// Why a mesh could not be baked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BakeError {
    /// The mesh kept no CPU copy (its `RenderAssetUsages` has no
    /// `MAIN_WORLD`).
    Extracted,
    /// The mesh has no `Float32x3` positions.
    NoPositions,
    /// Not triangles.
    Topology(PrimitiveTopology),
}

impl std::fmt::Display for BakeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Extracted => write!(
                f,
                "it kept no CPU copy; load or build it with RenderAssetUsages::MAIN_WORLD"
            ),
            Self::NoPositions => write!(f, "it has no Float32x3 positions"),
            Self::Topology(topology) => {
                write!(
                    f,
                    "it is a {topology:?}; only triangle lists and strips are baked"
                )
            }
        }
    }
}

/// Texels across a ground of `size` (metres before scale) at `cell_size`.
pub(crate) fn texels(size: Vec2, cell_size: f32) -> UVec2 {
    (size / cell_size.max(1e-3))
        .ceil()
        .as_uvec2()
        .clamp(UVec2::ONE, UVec2::splat(MAX_TEXELS))
}

/// The heights of `mesh` over a field of `size` centred on the mesh's origin,
/// `texels` across, laid out like a heightmap (first row at −Z): the highest
/// surface over each texel's centre, in the mesh's units, or [`HOLE`] where
/// the mesh does not reach.
pub(crate) fn bake(mesh: &Mesh, size: Vec2, texels: UVec2) -> Result<Vec<f32>, BakeError> {
    let topology = mesh.primitive_topology();
    if !matches!(
        topology,
        PrimitiveTopology::TriangleList | PrimitiveTopology::TriangleStrip
    ) {
        return Err(BakeError::Topology(topology));
    }
    let positions = match mesh.try_attribute(Mesh::ATTRIBUTE_POSITION) {
        Ok(values) => values.as_float3().ok_or(BakeError::NoPositions)?,
        Err(MeshAccessError::ExtractedToRenderWorld) => return Err(BakeError::Extracted),
        Err(MeshAccessError::NotFound) => return Err(BakeError::NoPositions),
    };
    let order: Vec<usize> = match mesh.try_indices_option() {
        Ok(Some(indices)) => indices.iter().collect(),
        Ok(None) => (0..positions.len()).collect(),
        Err(_) => return Err(BakeError::Extracted),
    };
    let triangles: Vec<[usize; 3]> = match topology {
        PrimitiveTopology::TriangleStrip => order.windows(3).map(|w| [w[0], w[1], w[2]]).collect(),
        _ => order.as_chunks::<3>().0.to_vec(),
    };

    // Each vertex in texel units, where texel (i, j) has its centre at
    // (i, j), with its height.
    let scale = texels.as_vec2() / size.max(Vec2::splat(1e-6));
    let to_texels = |p: [f32; 3]| {
        let at = (Vec2::new(p[0], p[2]) + size * 0.5) * scale - 0.5;
        Vec3::new(at.x, at.y, p[1])
    };
    let mut heights = vec![HOLE; (texels.x * texels.y) as usize];
    let last = texels.as_vec2() - 1.0;
    for [a, b, c] in triangles {
        let (Some(&a), Some(&b), Some(&c)) = (positions.get(a), positions.get(b), positions.get(c))
        else {
            continue;
        };
        let (a, b, c) = (to_texels(a), to_texels(b), to_texels(c));
        let area = edge(a, b, c);
        // Walls and slivers seen edge-on from above hold no ground.
        if area.abs() < 1e-9 {
            continue;
        }
        let low = a.min(b).min(c).truncate().ceil().max(Vec2::ZERO);
        let high = a.max(b).max(c).truncate().floor().min(last);
        if low.cmpgt(high).any() {
            continue;
        }
        // Points on a shared edge belong to both triangles: no cracks.
        let slack = -1e-4 * area.abs();
        for j in low.y as u32..=high.y as u32 {
            for i in low.x as u32..=high.x as u32 {
                let p = Vec3::new(i as f32, j as f32, 0.0);
                let (wa, wb, wc) = (edge(b, c, p), edge(c, a, p), edge(a, b, p));
                let inside = if area > 0.0 {
                    wa >= slack && wb >= slack && wc >= slack
                } else {
                    wa <= -slack && wb <= -slack && wc <= -slack
                };
                if inside {
                    let height = (wa * a.z + wb * b.z + wc * c.z) / area;
                    let texel = &mut heights[(j * texels.x + i) as usize];
                    *texel = texel.max(height);
                }
            }
        }
    }
    Ok(heights)
}

/// Twice the signed area of the triangle `a`, `b`, `p` in XY.
fn edge(a: Vec3, b: Vec3, p: Vec3) -> f32 {
    (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x)
}

/// `heights` as an image the heightmap path reads: one `R32Float` texel each,
/// kept on the CPU only.
fn heights_image(heights: &[f32], texels: UVec2) -> Image {
    Image::new(
        Extent3d {
            width: texels.x,
            height: texels.y,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        heights.iter().flat_map(|h| h.to_le_bytes()).collect(),
        TextureFormat::R32Float,
        RenderAssetUsages::MAIN_WORLD,
    )
}

/// Bakes the ground of every field on a mesh that has none yet, or whose
/// mesh, size or cell size changed, and drops the bakes of fields gone or no
/// longer on a mesh. A mesh that cannot be baked gets a ground of holes, so
/// its field draws nothing instead of holding every other field back, and one
/// warning.
pub(crate) fn bake_mesh_grounds(
    fields: Query<(Entity, &GrassField)>,
    meshes: Res<Assets<Mesh>>,
    mut events: MessageReader<AssetEvent<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut grounds: ResMut<MeshGrounds>,
    mut dirty: ResMut<TablesDirty>,
    mut warned: Local<HashSet<Entity>>,
) {
    let changed: HashSet<AssetId<Mesh>> = events
        .read()
        .filter_map(|event| match event {
            AssetEvent::Added { id }
            | AssetEvent::Modified { id }
            | AssetEvent::LoadedWithDependencies { id } => Some(*id),
            AssetEvent::Removed { .. } | AssetEvent::Unused { .. } => None,
        })
        .collect();
    let before = grounds.baked.len();
    grounds.baked.retain(|entity, _| {
        fields
            .get(*entity)
            .is_ok_and(|(_, field)| matches!(field.ground, GrassGround::Mesh { .. }))
    });
    warned.retain(|entity| grounds.baked.contains_key(entity) || fields.contains(*entity));
    if grounds.baked.len() != before {
        dirty.mark_fields();
    }
    for (entity, field) in &fields {
        let GrassGround::Mesh { mesh, cell_size } = &field.ground else {
            continue;
        };
        let key = BakeKey {
            mesh: mesh.id(),
            size: field.size.abs(),
            cell_size: *cell_size,
        };
        let current = grounds
            .baked
            .get(&entity)
            .is_some_and(|baked| baked.key == key && !changed.contains(&key.mesh));
        if current {
            continue;
        }
        let Some(mesh) = meshes.get(mesh) else {
            continue;
        };
        let texels = texels(key.size, key.cell_size);
        let heights = bake(mesh, key.size, texels).unwrap_or_else(|error| {
            if warned.insert(entity) {
                warn!(
                    "bevy_grass: the mesh ground of the GrassField {entity} cannot be baked: \
                     {error}. The field draws nothing."
                );
            }
            vec![HOLE; (texels.x * texels.y) as usize]
        });
        let image = images.add(heights_image(&heights, texels));
        grounds.baked.insert(entity, Baked { key, image });
        dirty.mark_fields();
    }
}

#[cfg(test)]
pub(crate) mod tests;
