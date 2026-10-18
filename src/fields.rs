//! The CPU's whole steady-state job: describing the fields.
//!
//! When a field, its style, its mask or its heightmap changes, the field
//! table, the chunk table and the packed masks and heights are rebuilt and
//! handed to the render world. Nothing here runs per blade or per frame; the
//! GPU decides which blades exist (`shaders/generate.wesl`).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use bevy::app::{App, PostUpdate};
use bevy::asset::{Asset, AssetEvent, AssetEventSystems, AssetId, Assets};
use bevy::camera::visibility::{InheritedVisibility, RenderLayers, VisibilitySystems};
use bevy::color::{ColorToComponents, LinearRgba};
use bevy::ecs::message::MessageReader;
use bevy::ecs::prelude::*;
use bevy::image::Image;
use bevy::log::warn;
use bevy::math::{UVec2, UVec4, Vec2, Vec3, Vec4};
use bevy::platform::time::Instant;
use bevy::render::render_resource::ShaderType;
use bevy::transform::TransformSystems;
use bevy::transform::components::GlobalTransform;

use crate::api::{
    GrassBlade, GrassField, GrassFieldOverrides, GrassGround, GrassSpecies, GrassStyle,
};
use crate::packed::{self, PackedImages};
use crate::shader_model::{self, ChunkLayout};
use crate::stats::TableStats;
use crate::style::MAX_SPECIES;
use crate::{GrassStats, GrassSystems};

/// Marks a field without a mask or heightmap.
pub(crate) const NO_MAP: u32 = u32::MAX;

/// Side of a chunk, in metres.
///
/// One size for every view, because the chunk table is shared. T2 in the plan
/// suggested 8 m chunks for orthographic views; per-blade frustum culling
/// rejects what a larger chunk lets through, so this stays 16 m until a
/// measurement says otherwise.
pub(crate) const CHUNK_SIZE: f32 = 16.0;

/// Entries beyond this index cannot be addressed by an instance's 8-bit
/// field index. Every species of a field is an entry.
const MAX_ENTRIES: usize = 256;

/// One species of one field as the shaders see it. Mirrors `Field` in
/// `common.wesl`.
///
/// A field is one entry per species, in a row: its style's own blades first,
/// then each of the style's `species`. Chunks point at the first; a blade
/// points at its species' entry. Every entry repeats the field's placement,
/// so the shaders read whichever they hold.
#[derive(Debug, Clone, Copy, PartialEq, Default, ShaderType)]
pub(crate) struct FieldGpu {
    /// xyz: the field's minimum corner at its base height.
    pub min_corner: Vec4,
    /// x, y: size along X and Z; z: heightmap scale; w: tile size.
    pub extent: Vec4,
    /// x: shortest blade, y: tallest blade, z: root width, w: blades per m².
    pub shape: Vec4,
    /// x: lean, y: stiffness, w: root occlusion.
    pub motion: Vec4,
    pub root_color: Vec4,
    pub tip_color: Vec4,
    /// x: seed, y: tiles per chunk side, z: mask offset, w: heightmap offset
    /// (offsets in texels; `NO_MAP` when absent).
    pub placement: UVec4,
    /// Mask width and height, heightmap width and height, in texels.
    pub map_sizes: UVec4,
    /// rgb: the ground's colour; w: how far roots blend toward it.
    pub ground: Vec4,
    /// x: specular, y: toon bands (0: physically based).
    pub look: Vec4,
    /// x: this species' chance of taking a blade the species listed before
    /// it left; y: its clustering; z: species in the field and w: the tallest
    /// blade of any of them (both on the field's first entry).
    pub mix: Vec4,
    /// x: twist, in turns; y: droop; z: fold; w: head, a share of the length.
    pub form: Vec4,
    /// rgb: head colour; w: head width.
    pub head: Vec4,
    /// rgb: patch colour over tip colour; w: patch amount.
    pub patches: Vec4,
    /// x: patch size; y: dry tips; z: dead blades.
    pub variation: Vec4,
    /// x: tuft pull; y: tuft size; z: the bare-patch noise threshold, or -1
    /// for none; w: bare patch size.
    pub tufts: Vec4,
    /// x: the render layers the field is drawn on, one bit each; 0 while the
    /// field is hidden. A view draws the field when its own layers share a bit.
    pub access: UVec4,
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
    /// Bumped on every change; the render world uploads when it changes.
    pub version: u64,
    /// Bumped when the chunks, masks or heightmaps change, which is most of
    /// the upload. A field only shown or hidden bumps `version` alone.
    pub structure_version: u64,
    pub fields: Arc<Vec<FieldGpu>>,
    /// Where each field's entries are in `fields`, and what they were built
    /// from.
    pub slots: Arc<Vec<FieldSlot>>,
    /// The image of each mask in `masks`, in order.
    pub mask_ids: Arc<Vec<AssetId<Image>>>,
    /// The image of each heightmap in `heights`, in order.
    pub height_ids: Arc<Vec<AssetId<Image>>>,
    pub chunks: Arc<Vec<ChunkGpu>>,
    /// Every mask, two R8G8 texels per word, low half first.
    pub masks: Arc<Vec<u32>>,
    /// Every heightmap, one normalized height per texel.
    pub heights: Arc<Vec<f32>>,
    /// The optional shading terms some field uses (`LookFeatures`).
    pub features: LookFeatures,
}

impl FieldTables {
    /// The highest ground under `point` (world X and Z) in the chunk of the
    /// field that holds it, or `None` outside every field. A flat field is
    /// exact; on a heightmap it is the top of the 16 m chunk.
    pub(crate) fn highest_ground_at(&self, point: Vec2) -> Option<f32> {
        let mut first_chunk = 0usize;
        for slot in self.slots.iter() {
            let field = self.fields.get(slot.first)?;
            let size = Vec2::new(field.extent.x, field.extent.y);
            let counts = (size / CHUNK_SIZE).ceil().as_uvec2().max(UVec2::ONE);
            let local = point - Vec2::new(field.min_corner.x, field.min_corner.z);
            if local.cmpge(Vec2::ZERO).all() && local.cmplt(size).all() {
                let cell = (local / CHUNK_SIZE).as_uvec2().min(counts - UVec2::ONE);
                let index = first_chunk + (cell.y * counts.x + cell.x) as usize;
                return self.chunks.get(index).map(|chunk| chunk.bounds.w);
            }
            first_chunk += (counts.x * counts.y) as usize;
        }
        None
    }
}

/// The entries of one field in [`FieldTables::fields`], and what they were
/// built from, so a rebuild can tell what has to be built again.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FieldSlot {
    pub entity: Entity,
    pub first: usize,
    pub entries: usize,
    entry: EntryKey,
    chunk: ChunkKey,
}

/// Everything a field's entries depend on besides the contents of its style:
/// while this and the style are the same, so are the entries.
#[derive(Debug, Clone, PartialEq)]
struct EntryKey {
    style: AssetId<GrassStyle>,
    size: Vec2,
    corner: Vec3,
    access: u32,
    /// The mask, and its offset in the mask table, in texels.
    mask: Option<(AssetId<Image>, u32)>,
    /// The heightmap, its offset in the height table and its scale.
    height: Option<(AssetId<Image>, u32, f32)>,
    species: usize,
    overrides: GrassFieldOverrides,
}

/// Everything a field's chunks depend on besides the contents of its
/// heightmap.
#[derive(Debug, Clone, PartialEq)]
struct ChunkKey {
    size: Vec2,
    base: f32,
    height: Option<(AssetId<Image>, f32)>,
    /// The index of the field's first entry, which its chunks point at.
    first: usize,
}

/// Optional shading terms, compiled into the grass shaders only when some
/// field's style uses them, so the others cost nothing.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct LookFeatures(u8);

impl LookFeatures {
    pub(crate) const GROUND_TINT: Self = Self(1);
    pub(crate) const SHEEN: Self = Self(2);
    pub(crate) const TOON: Self = Self(4);
    /// Blade shape: twist, droop, fold and heads.
    pub(crate) const SHAPE: Self = Self(8);
    /// Colour variation: patches, dry tips and dead blades.
    pub(crate) const VARIATION: Self = Self(16);

    /// The terms a field of `style` uses for its species `look` (the style
    /// itself for its own blades).
    pub(crate) fn of(style: &GrassStyle, look: &GrassBlade) -> Self {
        let mut features = Self::default();
        if style.ground_tint.amount > 0.0 {
            features.0 |= Self::GROUND_TINT.0;
        }
        if look.specular > 0.0 {
            features.0 |= Self::SHEEN.0;
        }
        if style.toon_bands > 0 {
            features.0 |= Self::TOON.0;
        }
        if look.twist > 0.0 || look.droop > 0.0 || look.fold > 0.0 || look.head_length > 0.0 {
            features.0 |= Self::SHAPE.0;
        }
        let variation = &style.variation;
        if variation.patch_amount > 0.0 || variation.dry_tips > 0.0 || variation.dead_share > 0.0 {
            features.0 |= Self::VARIATION.0;
        }
        features
    }

    /// The terms that change where blades are, which is all a shadow map
    /// needs.
    pub(crate) fn geometry(self) -> Self {
        Self(self.0 & Self::SHAPE.0)
    }

    pub(crate) fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// The shader definitions that compile the terms in.
    pub(crate) fn shader_defs(self) -> impl Iterator<Item = &'static str> {
        [
            (Self::GROUND_TINT, "GRASS_GROUND_TINT"),
            (Self::SHEEN, "GRASS_SHEEN"),
            (Self::TOON, "GRASS_TOON"),
            (Self::SHAPE, "GRASS_SHAPE"),
            (Self::VARIATION, "GRASS_VARIATION"),
        ]
        .into_iter()
        .filter(move |(feature, _)| self.contains(*feature))
        .map(|(_, def)| def)
    }
}

impl std::ops::BitOr for LookFeatures {
    type Output = Self;

    fn bitor(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

/// Registers the tables and the systems that keep them current, in
/// [`GrassSystems`] order.
pub(crate) fn plugin(app: &mut App) {
    app.init_resource::<FieldTables>()
        .init_resource::<TablesDirty>()
        .init_resource::<GrassStats>()
        .init_resource::<PackedImages>()
        .configure_sets(
            PostUpdate,
            (GrassSystems::DetectChanges, GrassSystems::Rebuild)
                .chain()
                .after(TransformSystems::Propagate)
                .after(VisibilitySystems::VisibilityPropagate)
                .after(AssetEventSystems),
        )
        .add_systems(
            PostUpdate,
            (
                (warn_unsupported, detect_changes).in_set(GrassSystems::DetectChanges),
                rebuild.in_set(GrassSystems::Rebuild),
            ),
        );
}

/// What the tables need.
#[derive(Resource, Debug, Default)]
pub(crate) struct TablesDirty {
    /// A field was added, removed, changed or moved.
    fields: bool,
    /// Styles in use that changed.
    styles: HashSet<AssetId<GrassStyle>>,
    /// Masks and heightmaps in use that changed.
    images: HashSet<AssetId<Image>>,
    /// Which fields are drawn changed, which only the entries show.
    access: bool,
}

impl TablesDirty {
    /// Whether the tables have to be looked at again, not merely patched.
    fn rebuild(&self) -> bool {
        self.fields || !self.styles.is_empty() || !self.images.is_empty()
    }
}

/// A field whose settings or position changed. `With<GrassField>` matters:
/// without it, every moving camera or character would rebuild the tables.
type FieldChanged = (
    With<GrassField>,
    Or<(
        Changed<GrassField>,
        Changed<GlobalTransform>,
        Changed<GrassFieldOverrides>,
    )>,
);

/// A field shown, hidden, or moved to other render layers.
type AccessChanged = (
    With<GrassField>,
    Or<(Changed<InheritedVisibility>, Changed<RenderLayers>)>,
);

/// The number of render layers a view or field can name.
const LAYER_BITS: usize = 32;

/// `layers` as the bit mask the shaders test; a missing component is layer 0,
/// as for meshes. Layers beyond 31 do not fit and are left out.
pub(crate) fn layer_mask(layers: Option<&RenderLayers>) -> u32 {
    layers.map_or(1, |layers| {
        layers
            .iter()
            .filter(|&layer| layer < LAYER_BITS)
            .fold(0, |mask, layer| mask | 1 << layer)
    })
}

/// Whether `layers` names a layer the shaders cannot test.
pub(crate) fn has_unsupported_layer(layers: Option<&RenderLayers>) -> bool {
    layers.is_some_and(|layers| layers.iter().any(|layer| layer >= LAYER_BITS))
}

/// What a field's `access` word holds: its layers, or nothing while hidden.
fn access(visible: bool, layers: Option<&RenderLayers>) -> u32 {
    if visible { layer_mask(layers) } else { 0 }
}

/// A field's size on the ground: `size` times the transform's X and Z scale.
fn scaled_size(field: &GrassField, transform: &GlobalTransform) -> Vec2 {
    let scale = transform.scale();
    field.size * Vec2::new(scale.x, scale.z).abs()
}

/// Whether a transform turns anything. Fields cannot rotate yet.
fn is_rotated(transform: &GlobalTransform) -> bool {
    transform
        .rotation()
        .angle_between(bevy::math::Quat::IDENTITY)
        > 1e-3
}

/// A field whose transform or layers changed, for `warn_unsupported`.
type TransformOrLayersChanged = (
    With<GrassField>,
    Or<(Changed<GlobalTransform>, Changed<RenderLayers>)>,
);

/// True the first time `unsupported` holds for `entity`, so each field is
/// warned about once; a field that fixes the problem and breaks it again is
/// warned about again.
fn first_time(seen: &mut HashSet<Entity>, entity: Entity, unsupported: bool) -> bool {
    if unsupported {
        seen.insert(entity)
    } else {
        seen.remove(&entity);
        false
    }
}

/// Warns, once per field, about the things a field's entity can ask for that
/// are not supported.
pub(crate) fn warn_unsupported(
    fields: Query<(Entity, &GlobalTransform, Option<&RenderLayers>), TransformOrLayersChanged>,
    mut rotated: Local<HashSet<Entity>>,
    mut layered: Local<HashSet<Entity>>,
) {
    for (entity, transform, layers) in &fields {
        if first_time(&mut rotated, entity, is_rotated(transform)) {
            warn!(
                "bevy_grass: the GrassField {entity} is rotated, but fields cannot rotate yet; \
                 it is drawn unrotated. Leave its rotation at identity and use GrassField::size \
                 and the transform's scale to shape it."
            );
        }
        if first_time(&mut layered, entity, has_unsupported_layer(layers)) {
            warn!(
                "bevy_grass: the GrassField {entity} is on a render layer above 31; grass \
                 supports layers 0 to 31, so the others are ignored."
            );
        }
    }
}

/// The asset an event is about.
fn event_id<A: Asset>(event: &AssetEvent<A>) -> AssetId<A> {
    match event {
        AssetEvent::Added { id }
        | AssetEvent::Modified { id }
        | AssetEvent::LoadedWithDependencies { id }
        | AssetEvent::Removed { id }
        | AssetEvent::Unused { id } => *id,
    }
}

/// Marks the tables for a rebuild when anything they depend on changes.
pub(crate) fn detect_changes(
    mut dirty: ResMut<TablesDirty>,
    changed: Query<(), FieldChanged>,
    access_changed: Query<(), AccessChanged>,
    (mut removed, mut removed_overrides): (
        RemovedComponents<GrassField>,
        RemovedComponents<GrassFieldOverrides>,
    ),
    mut styles: MessageReader<AssetEvent<GrassStyle>>,
    mut images: MessageReader<AssetEvent<Image>>,
    fields: Query<&GrassField>,
) {
    // Only styles and images some field uses matter.
    for event in styles.read() {
        let id = event_id(event);
        if fields.iter().any(|field| field.style.id() == id) {
            dirty.styles.insert(id);
        }
    }
    for event in images.read() {
        let id = event_id(event);
        let used = fields.iter().any(|field| {
            field
                .density_map
                .as_ref()
                .is_some_and(|mask| mask.id() == id)
                || matches!(&field.ground, GrassGround::Heightmap { image, .. } if image.id() == id)
        });
        if used {
            dirty.images.insert(id);
        }
    }
    let (gone, overrides_gone) = (removed.read().count(), removed_overrides.read().count());
    if !changed.is_empty() || gone > 0 || overrides_gone > 0 {
        dirty.fields = true;
    }
    if !access_changed.is_empty() {
        dirty.access = true;
    }
}

/// What `rebuild` reads of each field.
type FieldInputs<'a> = (
    Entity,
    &'a GrassField,
    &'a GlobalTransform,
    &'a InheritedVisibility,
    Option<&'a RenderLayers>,
    Option<&'a GrassFieldOverrides>,
);

/// Where a field is and which views draw it.
struct Placement {
    size: Vec2,
    /// The minimum corner, at the field's base height.
    corner: Vec3,
    access: u32,
}

impl Placement {
    fn of(
        field: &GrassField,
        transform: &GlobalTransform,
        visibility: &InheritedVisibility,
        layers: Option<&RenderLayers>,
    ) -> Self {
        let size = scaled_size(field, transform);
        let origin = transform.translation();
        Self {
            size,
            corner: Vec3::new(origin.x - size.x * 0.5, origin.y, origin.z - size.y * 0.5),
            access: access(visibility.get(), layers),
        }
    }
}

/// Shows or hides the fields already in the tables, without touching their
/// chunks or images.
fn patch_access(tables: &mut FieldTables, fields: &Query<FieldInputs>) {
    let mut entries = tables.fields.as_ref().clone();
    let mut slots = tables.slots.as_ref().clone();
    for slot in &mut slots {
        let Ok((_, _, _, visibility, layers, _)) = fields.get(slot.entity) else {
            continue;
        };
        let bits = access(visibility.get(), layers);
        slot.entry.access = bits;
        for entry in &mut entries[slot.first..slot.first + slot.entries] {
            entry.access.x = bits;
        }
    }
    tables.fields = Arc::new(entries);
    tables.slots = Arc::new(slots);
    tables.version += 1;
}

/// What `rebuild` reads of each field, once its style and images have loaded.
struct Input<'a> {
    entity: Entity,
    placement: Placement,
    overrides: GrassFieldOverrides,
    style_id: AssetId<GrassStyle>,
    style: &'a GrassStyle,
    mask: Option<(AssetId<Image>, &'a Image)>,
    height: Option<(AssetId<Image>, &'a Image, f32)>,
}

/// An input placed in the tables: which entries it gets, and where its images
/// sit in the mask and height tables.
struct Plan<'a> {
    input: &'a Input<'a>,
    /// The style's species that fit, after its own blades.
    species: &'a [GrassSpecies],
    /// The index of the field's first entry.
    first: usize,
    /// Offsets of its mask and heightmap, in texels.
    mask_offset: u32,
    height_offset: u32,
}

impl Plan<'_> {
    fn entry_key(&self) -> EntryKey {
        let input = self.input;
        EntryKey {
            style: input.style_id,
            size: input.placement.size,
            corner: input.placement.corner,
            access: input.placement.access,
            mask: input.mask.map(|(id, _)| (id, self.mask_offset)),
            height: input
                .height
                .map(|(id, _, scale)| (id, self.height_offset, scale)),
            species: self.species.len(),
            overrides: input.overrides,
        }
    }

    fn chunk_key(&self) -> ChunkKey {
        ChunkKey {
            size: self.input.placement.size,
            base: self.input.placement.corner.y,
            height: self.input.height.map(|(id, _, scale)| (id, scale)),
            first: self.first,
        }
    }

    /// Whether the field's heightmap is one of `changed`.
    fn uses_height(&self, changed: &HashSet<AssetId<Image>>) -> bool {
        self.input
            .height
            .is_some_and(|(id, ..)| changed.contains(&id))
    }

    /// Whether the field's mask or heightmap is one of `changed`.
    fn uses(&self, changed: &HashSet<AssetId<Image>>) -> bool {
        self.input.mask.is_some_and(|(id, _)| changed.contains(&id))
            || self
                .input
                .height
                .is_some_and(|(id, ..)| changed.contains(&id))
    }
}

/// Reads each field with its style, mask and heightmap, or `None` while any of
/// them has not loaded, so a field never appears at the wrong density or
/// height and then jumps.
fn gather<'a>(
    sorted: &[FieldInputs<'a>],
    styles: &'a Assets<GrassStyle>,
    images: &'a Assets<Image>,
) -> Option<Vec<Input<'a>>> {
    let mut inputs = Vec::with_capacity(sorted.len());
    for (entity, field, transform, visibility, layers, overrides) in sorted {
        let mask = match &field.density_map {
            Some(handle) => Some((handle.id(), images.get(handle)?)),
            None => None,
        };
        let height = match &field.ground {
            GrassGround::Flat => None,
            GrassGround::Heightmap {
                image,
                height_scale,
            } => Some((image.id(), images.get(image)?, *height_scale)),
        };
        inputs.push(Input {
            entity: *entity,
            placement: Placement::of(field, transform, visibility, *layers),
            overrides: overrides.copied().unwrap_or_default(),
            style_id: field.style.id(),
            style: styles.get(&field.style)?,
            mask,
            height,
        });
    }
    Some(inputs)
}

/// What the limits left out.
#[derive(Debug, Default, Clone, Copy)]
struct Dropped {
    fields: usize,
    species: usize,
}

/// Which entries each field gets, and where its images sit in the tables.
/// Fields beyond the table's 256 entries are left out.
fn plan<'a>(inputs: &'a [Input<'a>]) -> (Vec<Plan<'a>>, Dropped) {
    let mut plans = Vec::with_capacity(inputs.len());
    let mut dropped = Dropped::default();
    let (mut entry_cursor, mut mask_words, mut height_texels) = (0usize, 0u32, 0u32);
    for (index, input) in inputs.iter().enumerate() {
        let species = &input.style.species[..input.style.species.len().min(MAX_SPECIES - 1)];
        dropped.species += input.style.species.len() - species.len();
        if species.len() < input.style.species.len() {
            warn!(
                "bevy_grass mixes at most {MAX_SPECIES} species in a field; {} of a style's {} \
                 species are drawn",
                species.len(),
                input.style.species.len()
            );
        }
        if entry_cursor + 1 + species.len() > MAX_ENTRIES {
            warn!(
                "bevy_grass draws at most {MAX_ENTRIES} fields, each species of a field \
                 counting as one; some fields are not drawn"
            );
            dropped.fields = inputs.len() - index;
            break;
        }
        plans.push(Plan {
            input,
            species,
            first: entry_cursor,
            mask_offset: mask_words * 2,
            height_offset: height_texels,
        });
        entry_cursor += 1 + species.len();
        if let Some((_, image)) = input.mask {
            let size = image.size();
            mask_words += packed::mask_words(size.x * size.y);
        }
        if let Some((_, image, _)) = input.height {
            let size = image.size();
            height_texels += size.x * size.y;
        }
    }
    (plans, dropped)
}

/// The masks and heightmaps of the tables, kept while the same images are used
/// in the same order and none changed, and otherwise joined from the packed
/// images, each read at most once (see `PackedImages`).
struct Images {
    masks: Arc<Vec<u32>>,
    heights: Arc<Vec<f32>>,
    mask_ids: Arc<Vec<AssetId<Image>>>,
    height_ids: Arc<Vec<AssetId<Image>>>,
    masks_same: bool,
    heights_same: bool,
}

fn images_for(
    plans: &[Plan],
    old: &FieldTables,
    changed: &HashSet<AssetId<Image>>,
    cache: &mut PackedImages,
) -> Images {
    let mask_ids: Vec<_> = plans
        .iter()
        .filter_map(|plan| plan.input.mask.map(|(id, _)| id))
        .collect();
    let height_ids: Vec<_> = plans
        .iter()
        .filter_map(|plan| plan.input.height.map(|(id, ..)| id))
        .collect();
    let untouched = |ids: &[AssetId<Image>]| !ids.iter().any(|id| changed.contains(id));
    let masks_same = *old.mask_ids == mask_ids && untouched(&mask_ids);
    let heights_same = *old.height_ids == height_ids && untouched(&height_ids);

    let masks = if masks_same {
        old.masks.clone()
    } else {
        let parts: Vec<_> = plans
            .iter()
            .filter_map(|plan| plan.input.mask)
            .map(|(id, image)| cache.mask(id, image))
            .collect();
        packed::join(&parts)
    };
    let heights = if heights_same {
        old.heights.clone()
    } else {
        let parts: Vec<_> = plans
            .iter()
            .filter_map(|plan| plan.input.height)
            .map(|(id, image, _)| cache.heights(id, image))
            .collect();
        packed::join(&parts)
    };
    cache.retain(&mask_ids, &height_ids);
    Images {
        masks,
        heights,
        mask_ids: if masks_same {
            old.mask_ids.clone()
        } else {
            Arc::new(mask_ids)
        },
        height_ids: if heights_same {
            old.height_ids.clone()
        } else {
            Arc::new(height_ids)
        },
        masks_same,
        heights_same,
    }
}

/// The chunks, kept (and the second value true) while no field resized, moved
/// up or down, changed heightmap or shifted in the tables.
fn chunks_for(
    plans: &[Plan],
    old: &FieldTables,
    changed: &HashSet<AssetId<Image>>,
    cache: &mut PackedImages,
) -> (Arc<Vec<ChunkGpu>>, bool) {
    let same =
        old.slots.len() == plans.len()
            && old.slots.iter().zip(plans).all(|(slot, plan)| {
                slot.entity == plan.input.entity && slot.chunk == plan.chunk_key()
            })
            && !plans.iter().any(|plan| plan.uses_height(changed));
    if same {
        return (old.chunks.clone(), true);
    }
    let mut chunks = Vec::new();
    for plan in plans {
        let heights = plan
            .input
            .height
            .map(|(id, image, _)| cache.heights(id, image));
        field_chunks(plan, heights.as_ref().map(|h| h.as_slice()), &mut chunks);
    }
    (Arc::new(chunks), false)
}

/// The field table, with the slots that say what each entry was built from.
struct Built {
    fields: Vec<FieldGpu>,
    slots: Vec<FieldSlot>,
    features: LookFeatures,
    /// Entries worked out again, not kept.
    rewritten: u64,
}

/// Each field's entries: kept when its style, place and images are the same
/// as when they were built, worked out again otherwise.
fn entries_for(
    plans: &[Plan],
    old: &FieldTables,
    changed_styles: &HashSet<AssetId<GrassStyle>>,
    changed_images: &HashSet<AssetId<Image>>,
) -> Built {
    let previous: HashMap<Entity, &FieldSlot> =
        old.slots.iter().map(|slot| (slot.entity, slot)).collect();
    let mut built = Built {
        fields: Vec::with_capacity(old.fields.len()),
        slots: Vec::with_capacity(plans.len()),
        features: LookFeatures::default(),
        rewritten: 0,
    };
    for plan in plans {
        let (entry, chunk) = (plan.entry_key(), plan.chunk_key());
        let count = 1 + plan.species.len();
        let kept = previous.get(&plan.input.entity).filter(|slot| {
            slot.entry == entry
                && slot.entries == count
                && !changed_styles.contains(&plan.input.style_id)
                && !plan.uses(changed_images)
        });
        if let Some(slot) = kept {
            built
                .fields
                .extend_from_slice(&old.fields[slot.first..slot.first + slot.entries]);
        } else {
            built.fields.extend(field_entries(plan));
            built.rewritten += count as u64;
        }
        built.slots.push(FieldSlot {
            entity: plan.input.entity,
            first: plan.first,
            entries: count,
            entry,
            chunk,
        });
        let style = plan.input.style;
        for look in std::iter::once(&style.blade).chain(plan.species.iter().map(|s| &s.blade)) {
            built.features = built.features | LookFeatures::of(style, look);
        }
    }
    built
}

/// Brings the tables up to date with what changed, building only what that
/// needs, once every input has loaded.
///
/// - A mask or heightmap is read from its image only when that image changed
///   (`PackedImages`).
/// - A field's entries are rewritten only when its style changed, or it moved,
///   or its images did.
/// - The chunks, masks and heightmaps are kept (and not uploaded again) while
///   no field resized, moved up or down, or changed images. A style change
///   alone, a colour changing every frame for a season or the time of day,
///   rewrites the entries of the fields using it and nothing else.
/// - A pass that finds the tables unchanged changes nothing.
pub(crate) fn rebuild(
    mut dirty: ResMut<TablesDirty>,
    mut tables: ResMut<FieldTables>,
    fields: Query<FieldInputs>,
    styles: Res<Assets<GrassStyle>>,
    images: Res<Assets<Image>>,
    mut cache: ResMut<PackedImages>,
    mut stats: ResMut<GrassStats>,
) {
    if !dirty.rebuild() {
        if dirty.access {
            dirty.access = false;
            patch_access(&mut tables, &fields);
        }
        return;
    }
    let mut sorted: Vec<_> = fields.iter().collect();
    sorted.sort_by_key(|(entity, ..)| *entity);
    let Some(inputs) = gather(&sorted, &styles, &images) else {
        return;
    };
    // This pass reads every field's visibility too, and takes care of
    // everything that changed.
    let changed_styles = std::mem::take(&mut dirty.styles);
    let changed_images = std::mem::take(&mut dirty.images);
    dirty.fields = false;
    dirty.access = false;

    let started = Instant::now();
    cache.forget(&changed_images);
    let old = (*tables).clone();
    let (plans, dropped) = plan(&inputs);
    let images = images_for(&plans, &old, &changed_images, &mut cache);
    let (chunks, chunks_same) = chunks_for(&plans, &old, &changed_images, &mut cache);
    let built = entries_for(&plans, &old, &changed_styles, &changed_images);

    let layout_changed = !(images.masks_same && images.heights_same && chunks_same);
    let mut counted = stats.tables;
    counted.entries_rewritten += built.rewritten;
    counted.images_read = cache.reads;
    if !layout_changed && built.fields == *old.fields && built.slots == *old.slots {
        // Something changed that no field shows: another style, say. There is
        // nothing to upload.
        if counted != stats.tables {
            stats.tables = counted;
        }
        return;
    }
    let (fields_len, chunks_len) = (built.slots.len(), chunks.len());
    *tables = FieldTables {
        version: old.version + 1,
        structure_version: old.structure_version + u64::from(layout_changed),
        fields: Arc::new(built.fields),
        slots: Arc::new(built.slots),
        chunks,
        masks: images.masks,
        heights: images.heights,
        mask_ids: images.mask_ids,
        height_ids: images.height_ids,
        features: built.features,
    };
    stats.tables = TableStats {
        fields: fields_len,
        chunks: chunks_len,
        fields_dropped: dropped.fields,
        species_dropped: dropped.species,
        rebuilds: counted.rebuilds + 1,
        layout_rebuilds: counted.layout_rebuilds + u64::from(layout_changed),
        last_rebuild: started.elapsed(),
        ..counted
    };
}

/// The entry of one look of a field: `field_entry` (where the field is and
/// what belongs to the whole field) with `look`'s blades. `style` is the
/// field's own style, whose density, toon bands and patch
/// colour every species shares.
fn look_entry(
    field_entry: &FieldGpu,
    style: &GrassStyle,
    look: &GrassBlade,
    overrides: &GrassFieldOverrides,
    mix: Vec4,
) -> FieldGpu {
    let (root, tip) = (linear(look.root_color), linear(look.tip_color));
    let patch = linear(style.variation.patch_color).truncate();
    let tint = overrides
        .tint
        .map_or(Vec3::ONE, |tint| linear(tint).truncate());
    let height = overrides.height_scale.max(0.0);
    FieldGpu {
        shape: Vec4::new(
            look.height.x * height,
            look.height.y * height,
            look.width,
            style.density * overrides.density_scale.max(0.0),
        ),
        motion: Vec4::new(look.lean, look.stiffness, 0.0, look.root_occlusion),
        root_color: (root.truncate() * tint).extend(root.w),
        // The tip colour's alpha carries the translucency.
        tip_color: (tip.truncate() * tint).extend(look.translucency),
        look: Vec4::new(
            look.specular.clamp(0.0, 1.0),
            style.toon_bands as f32,
            0.0,
            0.0,
        ),
        mix,
        form: Vec4::new(
            look.twist.max(0.0),
            look.droop.clamp(0.0, 1.0),
            look.fold.clamp(0.0, 1.0),
            look.head_length.clamp(0.0, 0.9),
        ),
        head: linear(look.head_color)
            .truncate()
            .extend(look.head_width.max(0.0)),
        // In a patch the whole blade is scaled by how far its tip colour
        // moves toward the patch colour.
        patches: (patch / tip.truncate().max(Vec3::splat(1e-3)))
            .extend(style.variation.patch_amount.clamp(0.0, 1.0)),
        ..*field_entry
    }
}

/// A field's entries: its own blades first, then each species, all repeating
/// the field's placement.
fn field_entries(plan: &Plan) -> Vec<FieldGpu> {
    let Input {
        placement,
        style,
        mask,
        height,
        ..
    } = plan.input;
    let Placement {
        size,
        corner,
        access,
    } = *placement;
    let overrides = &plan.input.overrides;
    let layout =
        ChunkLayout::for_density(CHUNK_SIZE, style.density * overrides.density_scale.max(0.0));
    let (mask_offset, mask_size) = match mask {
        Some((_, image)) => (plan.mask_offset, image.size()),
        None => (NO_MAP, UVec2::ZERO),
    };
    let (height_offset, height_size, height_scale) = match height {
        Some((_, image, scale)) => (plan.height_offset, image.size(), *scale),
        None => (NO_MAP, UVec2::ZERO, 0.0),
    };

    let variation = &style.variation;
    let field_entry = FieldGpu {
        min_corner: corner.extend(0.0),
        extent: Vec4::new(size.x, size.y, height_scale, layout.tile_size),
        // The seed comes from where the field is, not from its entity, so
        // the same field looks the same however the scene was spawned.
        placement: UVec4::new(
            shader_model::hash2(corner.x.to_bits(), corner.z.to_bits()),
            layout.tiles_per_side,
            mask_offset,
            height_offset,
        ),
        map_sizes: UVec4::new(mask_size.x, mask_size.y, height_size.x, height_size.y),
        ground: linear(style.ground_tint.color)
            .truncate()
            .extend(style.ground_tint.amount.clamp(0.0, 1.0)),
        variation: Vec4::new(
            variation.patch_size.max(0.01),
            variation.dry_tips.clamp(0.0, 1.0),
            variation.dead_share.clamp(0.0, 1.0),
            0.0,
        ),
        tufts: Vec4::new(
            variation.tufts.clamp(0.0, 1.0),
            variation.tuft_spacing.max(0.01),
            if variation.bare_share > 0.0 {
                shader_model::bare_threshold(variation.bare_share)
            } else {
                -1.0
            },
            variation.bare_patch_size.max(0.01),
        ),
        access: UVec4::new(access, 0, 0, 0),
        ..FieldGpu::default()
    };
    let tallest = plan
        .species
        .iter()
        .map(|species| species.blade.height.y)
        .fold(style.blade.height.y, f32::max)
        * overrides.height_scale.max(0.0);
    let chances = species_chances(plan.species);
    let looks = std::iter::once((&style.blade, 1.0, 0.0)).chain(
        plan.species
            .iter()
            .zip(&chances)
            .map(|(species, chance)| (&species.blade, *chance, species.clustering)),
    );
    looks
        .map(|(look, chance, clustering)| {
            let mix = Vec4::new(
                chance,
                clustering.clamp(0.0, 1.0),
                (1 + plan.species.len()) as f32,
                tallest,
            );
            look_entry(&field_entry, style, look, overrides, mix)
        })
        .collect()
}

/// A field's chunks: the 16 m squares of it, each with the lowest and highest
/// ground in it. `heights` is the field's heightmap, if it has one.
fn field_chunks(plan: &Plan, heights: Option<&[f32]>, out: &mut Vec<ChunkGpu>) {
    let Placement { size, corner, .. } = plan.input.placement;
    let height_scale = plan.input.height.map_or(0.0, |(_, _, scale)| scale);
    let height_size = plan
        .input
        .height
        .map_or(UVec2::ZERO, |(_, image, _)| image.size());
    let counts = (size / CHUNK_SIZE).ceil().as_uvec2();
    for chunk_z in 0..counts.y {
        for chunk_x in 0..counts.x {
            let local = Vec2::new(chunk_x as f32, chunk_z as f32) * CHUNK_SIZE;
            let (low, high) = match heights {
                None => (corner.y, corner.y),
                Some(heights) => {
                    let (low, high) =
                        height_range(heights, height_size, local / size, CHUNK_SIZE / size);
                    (
                        corner.y + low * height_scale,
                        corner.y + high * height_scale,
                    )
                }
            };
            out.push(ChunkGpu {
                bounds: Vec4::new(local.x, local.y, low, high),
                ids: UVec4::new(plan.first as u32, chunk_x, chunk_z, 0),
            });
        }
    }
}

fn linear(color: bevy::color::Color) -> Vec4 {
    Vec4::from_array(LinearRgba::from(color).to_f32_array())
}

/// Each species' chance of taking a blade that the species listed before it
/// left. Shares adding up to more than 1 are scaled down to 1.
fn species_chances(species: &[GrassSpecies]) -> Vec<f32> {
    let total: f32 = species.iter().map(|s| s.share.max(0.0)).sum();
    let scale = if total > 1.0 { 1.0 / total } else { 1.0 };
    let mut left = 1.0;
    species
        .iter()
        .map(|s| {
            let share = s.share.max(0.0) * scale;
            let chance = if left > 1e-6 {
                (share / left).min(1.0)
            } else {
                0.0
            };
            left -= share;
            chance
        })
        .collect()
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
    use bevy::app::{App, PostUpdate};
    use bevy::asset::{AssetApp, AssetPlugin, Handle};
    use bevy::camera::visibility::{Visibility, VisibilitySystems};
    use bevy::transform::components::Transform;

    use super::*;

    /// Stands in for Bevy's visibility propagation, which needs the whole
    /// renderer: an entity is visible unless it or an ancestor up to the
    /// nearest `Visible` is `Hidden`. The full-app GPU test runs the real one.
    fn propagate(
        entities: Query<Entity, With<Visibility>>,
        visibilities: Query<(&Visibility, Option<&ChildOf>)>,
        mut inherited: Query<&mut InheritedVisibility>,
    ) {
        for entity in &entities {
            let mut visible = true;
            let mut current = entity;
            while let Ok((visibility, parent)) = visibilities.get(current) {
                match (visibility, parent) {
                    (Visibility::Hidden, _) => visible = false,
                    (Visibility::Inherited, Some(parent)) => {
                        current = parent.parent();
                        continue;
                    }
                    _ => {}
                }
                break;
            }
            let mut inherited = inherited.get_mut(entity).unwrap();
            if inherited.get() != visible {
                *inherited = if visible {
                    InheritedVisibility::VISIBLE
                } else {
                    InheritedVisibility::HIDDEN
                };
            }
        }
    }

    /// The systems exactly as the plugin registers and orders them.
    fn app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<GrassStyle>()
            .init_asset::<Image>()
            .add_systems(
                PostUpdate,
                propagate.in_set(VisibilitySystems::VisibilityPropagate),
            )
            .add_plugins(plugin);
        app
    }

    /// A 32 m field of the meadow style, and the app's tables after it built.
    fn spawn_field(app: &mut App, extra: impl Bundle) -> Entity {
        let style = app
            .world_mut()
            .resource_mut::<Assets<GrassStyle>>()
            .add(GrassStyle::MEADOW);
        app.world_mut()
            .spawn((
                GrassField {
                    size: Vec2::splat(32.0),
                    style,
                    ..Default::default()
                },
                GlobalTransform::IDENTITY,
                extra,
            ))
            .id()
    }

    /// Runs until the style's `Added` event, which arrives a frame late, has
    /// been seen and the tables are quiet.
    fn settle(app: &mut App) {
        app.update();
        app.update();
    }

    /// What the render world would draw the first field with.
    fn access_of(app: &App) -> u32 {
        app.world().resource::<FieldTables>().fields[0].access.x
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
    fn species_chances_give_each_its_share() {
        let species = |shares: &[f32]| -> Vec<GrassSpecies> {
            shares
                .iter()
                .map(|share| GrassSpecies::new(GrassBlade::CLOVER, *share))
                .collect()
        };
        // The chance of each, times what the ones before left, is its share.
        let shares = [0.1, 0.06, 0.03];
        let chances = species_chances(&species(&shares));
        let mut left = 1.0;
        for (share, chance) in shares.iter().zip(&chances) {
            assert!((left * chance - share).abs() < 1e-6);
            left *= 1.0 - chance;
        }
        // Too much in total: scaled to fill every blade, in proportion.
        let chances = species_chances(&species(&[0.8, 0.8]));
        assert!((chances[0] - 0.5).abs() < 1e-6 && (chances[1] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn every_species_is_an_entry_after_its_field() {
        let mut app = app();
        let style = app
            .world_mut()
            .resource_mut::<Assets<GrassStyle>>()
            .add(GrassStyle::SUMMER_WILD);
        app.world_mut().spawn((
            GrassField {
                size: Vec2::splat(32.0),
                style,
                ..Default::default()
            },
            GlobalTransform::IDENTITY,
        ));
        app.update();
        let tables = app.world().resource::<FieldTables>();
        let count = 1 + GrassStyle::SUMMER_WILD.species.len();
        assert_eq!(tables.fields.len(), count);
        assert!(tables.chunks.iter().all(|chunk| chunk.ids.x == 0));
        let first = tables.fields[0];
        assert_eq!(first.mix.z, count as f32);
        assert_eq!(first.mix.w, GrassBlade::SEED_HEADS.height.y);
        // The seed heads' entry has their look and the field's placement.
        let heads = tables.fields[1];
        assert_eq!(heads.shape.y, GrassBlade::SEED_HEADS.height.y);
        assert_eq!(heads.form.w, GrassBlade::SEED_HEADS.head_length);
        assert_eq!(heads.placement, first.placement);
        assert_eq!(heads.tufts, first.tufts);
        assert!(tables.features.contains(LookFeatures::SHAPE));
        assert!(tables.features.contains(LookFeatures::VARIATION));
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

    #[test]
    fn a_hidden_field_is_not_drawn_and_showing_it_rebuilds_nothing_else() {
        let mut app = app();
        let field = spawn_field(&mut app, ());
        settle(&mut app);
        assert_eq!(access_of(&app), 1, "a visible field is on layer 0");
        let before = app.world().resource::<FieldTables>().clone();

        app.world_mut().entity_mut(field).insert(Visibility::Hidden);
        app.update();
        let hidden = app.world().resource::<FieldTables>().clone();
        assert_eq!(access_of(&app), 0, "a hidden field is not drawn");
        assert_eq!(hidden.version, before.version + 1);
        assert_eq!(
            hidden.structure_version, before.structure_version,
            "hiding a field must not rebuild its chunks and images"
        );
        assert!(Arc::ptr_eq(&hidden.chunks, &before.chunks));
        assert!(Arc::ptr_eq(&hidden.masks, &before.masks));
        assert!(Arc::ptr_eq(&hidden.heights, &before.heights));

        app.world_mut()
            .entity_mut(field)
            .insert(Visibility::Visible);
        app.update();
        assert_eq!(access_of(&app), 1);
        let shown = app.world().resource::<FieldTables>();
        assert_eq!(shown.structure_version, before.structure_version);
        assert_eq!(shown.chunks.len(), 4, "the chunks were there all along");
    }

    #[test]
    fn a_field_under_a_hidden_parent_is_hidden_too() {
        let mut app = app();
        let parent = app.world_mut().spawn(Visibility::Visible).id();
        let field = spawn_field(&mut app, ChildOf(parent));
        app.update();
        assert_eq!(access_of(&app), 1);

        app.world_mut()
            .entity_mut(parent)
            .insert(Visibility::Hidden);
        app.update();
        assert_eq!(access_of(&app), 0, "hiding a level hides the grass in it");
        // The field itself still says `Inherited`.
        assert_eq!(
            app.world().get::<Visibility>(field),
            Some(&Visibility::Inherited)
        );

        app.world_mut()
            .entity_mut(parent)
            .insert(Visibility::Visible);
        app.update();
        assert_eq!(access_of(&app), 1);
    }

    #[test]
    fn a_field_is_drawn_on_the_render_layers_it_names() {
        assert_eq!(layer_mask(None), 0b1, "no component: layer 0, like a mesh");
        assert_eq!(layer_mask(Some(&RenderLayers::layer(3))), 0b1000);
        assert_eq!(
            layer_mask(Some(&RenderLayers::from_layers(&[0, 5, 31]))),
            1 | 1 << 5 | 1 << 31
        );
        assert_eq!(layer_mask(Some(&RenderLayers::none())), 0);
        let beyond = RenderLayers::from_layers(&[2, 40]);
        assert_eq!(
            layer_mask(Some(&beyond)),
            0b100,
            "layers above 31 are left out"
        );
        assert!(has_unsupported_layer(Some(&beyond)));
        assert!(!has_unsupported_layer(Some(&RenderLayers::layer(31))));
        assert!(!has_unsupported_layer(None));

        let mut app = app();
        let field = spawn_field(&mut app, RenderLayers::layer(2));
        settle(&mut app);
        assert_eq!(access_of(&app), 0b100);
        let structure = app.world().resource::<FieldTables>().structure_version;

        app.world_mut()
            .entity_mut(field)
            .insert(RenderLayers::from_layers(&[1, 2]));
        app.update();
        assert_eq!(access_of(&app), 0b110);
        assert_eq!(
            app.world().resource::<FieldTables>().structure_version,
            structure,
            "changing layers must not rebuild chunks and images"
        );

        // Hidden wins over any layer.
        app.world_mut().entity_mut(field).insert(Visibility::Hidden);
        app.update();
        assert_eq!(access_of(&app), 0);
    }

    #[test]
    fn every_species_of_a_hidden_field_is_hidden() {
        let mut app = app();
        let style = app
            .world_mut()
            .resource_mut::<Assets<GrassStyle>>()
            .add(GrassStyle::SUMMER_WILD);
        app.world_mut().spawn((
            GrassField {
                size: Vec2::splat(32.0),
                style,
                ..Default::default()
            },
            GlobalTransform::IDENTITY,
            Visibility::Hidden,
        ));
        app.update();
        let tables = app.world().resource::<FieldTables>();
        assert!(tables.fields.len() > 1);
        assert!(tables.fields.iter().all(|entry| entry.access.x == 0));
    }

    #[test]
    fn a_field_is_as_big_as_its_size_times_its_scale() {
        let mut app = app();
        let field = spawn_field(&mut app, ());
        app.update();
        let tables = app.world().resource::<FieldTables>();
        assert_eq!(
            tables.fields[0].extent.truncate().truncate(),
            Vec2::splat(32.0)
        );
        assert_eq!(tables.chunks.len(), 4);

        // A scaled `Plane3d` mesh is bigger; so is a scaled field. It stays
        // centred on its translation, and Y scale does nothing.
        *app.world_mut().get_mut::<GlobalTransform>(field).unwrap() = GlobalTransform::from(
            Transform::from_xyz(10.0, 2.0, -6.0).with_scale(Vec3::new(2.0, 5.0, 0.5)),
        );
        app.update();
        let tables = app.world().resource::<FieldTables>();
        let entry = tables.fields[0];
        assert_eq!(entry.extent.truncate().truncate(), Vec2::new(64.0, 16.0));
        assert_eq!(
            entry.min_corner.truncate(),
            Vec3::new(10.0 - 32.0, 2.0, -6.0 - 8.0)
        );
        assert_eq!(tables.chunks.len(), 4, "four chunks across, one deep");
        assert!(tables.chunks.iter().all(|chunk| chunk.ids.z == 0));
    }

    #[test]
    fn rotation_is_noticed_and_each_field_is_warned_about_once() {
        assert!(!is_rotated(&GlobalTransform::IDENTITY));
        assert!(!is_rotated(&GlobalTransform::from(
            Transform::from_xyz(3.0, 1.0, 2.0).with_scale(Vec3::new(2.0, 1.0, 3.0))
        )));
        assert!(is_rotated(&GlobalTransform::from(
            Transform::from_rotation(bevy::math::Quat::from_rotation_y(0.3))
        )));
        // Scale alone is not rotation, even when it is not uniform.
        assert!(!is_rotated(&GlobalTransform::from(Transform::from_scale(
            Vec3::new(1.0, 4.0, 0.1)
        ))));

        let mut world = World::new();
        let (a, b) = (world.spawn_empty().id(), world.spawn_empty().id());
        let mut seen = HashSet::new();
        assert!(first_time(&mut seen, a, true));
        assert!(!first_time(&mut seen, a, true), "once per field");
        assert!(first_time(&mut seen, b, true), "each field has its own");
        assert!(
            !first_time(&mut seen, a, false),
            "nothing wrong, nothing to say"
        );
        assert!(first_time(&mut seen, a, true), "broken again after a fix");
    }
    /// The version of the tables, now.
    fn version(app: &App) -> u64 {
        app.world().resource::<FieldTables>().version
    }

    /// Moves the first field once, when run.
    fn move_the_field(
        mut moved: Local<bool>,
        mut fields: Query<&mut GlobalTransform, With<GrassField>>,
    ) {
        if !*moved {
            *moved = true;
            for mut transform in &mut fields {
                *transform = GlobalTransform::from_xyz(7.0, 0.0, 0.0);
            }
        }
    }

    #[test]
    fn a_system_ordered_before_the_plugins_sets_is_seen_the_same_frame() {
        let mut app = app();
        spawn_field(&mut app, ());
        settle(&mut app);
        let before = version(&app);
        app.add_systems(
            PostUpdate,
            move_the_field.before(GrassSystems::DetectChanges),
        );
        app.update();
        assert_eq!(
            version(&app),
            before + 1,
            "the move was in the tables in the frame it happened"
        );
        assert_eq!(
            app.world().resource::<FieldTables>().fields[0].min_corner.x,
            7.0 - 16.0
        );
    }

    #[test]
    fn a_system_ordered_after_the_plugins_sets_is_seen_the_next_frame() {
        let mut app = app();
        spawn_field(&mut app, ());
        settle(&mut app);
        let before = version(&app);
        app.add_systems(PostUpdate, move_the_field.after(GrassSystems::Rebuild));
        app.update();
        assert_eq!(version(&app), before, "too late for this frame");
        app.update();
        assert_eq!(version(&app), before + 1, "and in the next");
    }

    #[test]
    fn an_asset_edited_before_the_asset_events_is_rebuilt_the_same_frame() {
        #[derive(Resource)]
        struct Painted(Handle<GrassStyle>);

        fn thin_the_grass(painted: Res<Painted>, mut styles: ResMut<Assets<GrassStyle>>) {
            styles.get_mut(&painted.0).unwrap().density = 3.0;
        }

        let mut app = app();
        let field = spawn_field(&mut app, ());
        settle(&mut app);
        let style = app.world().get::<GrassField>(field).unwrap().style.clone();
        assert_eq!(
            app.world().resource::<FieldTables>().fields[0].shape.w,
            60.0
        );
        app.insert_resource(Painted(style));
        app.add_systems(PostUpdate, thin_the_grass.before(AssetEventSystems));
        app.update();
        assert_eq!(
            app.world().resource::<FieldTables>().fields[0].shape.w,
            3.0,
            "an edit made before the asset events must not wait a frame"
        );
    }
    fn stats(app: &App) -> TableStats {
        app.world().resource::<GrassStats>().tables
    }

    #[test]
    fn rebuilds_are_counted_and_timed_and_showing_a_field_is_not_one() {
        let mut app = app();
        assert_eq!(stats(&app).rebuilds, 0);
        let field = spawn_field(&mut app, ());
        settle(&mut app);
        let built = stats(&app);
        assert!(built.rebuilds >= 1, "{built:?}");
        assert_eq!((built.fields, built.chunks), (1, 4));
        assert_eq!((built.fields_dropped, built.species_dropped), (0, 0));
        assert!(built.last_rebuild > std::time::Duration::ZERO);

        // Moving the field is a rebuild; hiding it only patches the tables.
        *app.world_mut().get_mut::<GlobalTransform>(field).unwrap() =
            GlobalTransform::from_xyz(5.0, 0.0, 0.0);
        app.update();
        assert_eq!(stats(&app).rebuilds, built.rebuilds + 1);
        app.world_mut().entity_mut(field).insert(Visibility::Hidden);
        app.update();
        assert_eq!(
            stats(&app).rebuilds,
            built.rebuilds + 1,
            "hiding is not a rebuild"
        );
        assert_eq!(
            stats(&app).fields,
            1,
            "a hidden field is still in the tables"
        );
    }

    #[test]
    fn the_limits_that_drop_content_are_visible_to_the_game() {
        // 256 entries: 260 one-species fields leave the last four out.
        let mut app = app();
        for _ in 0..260 {
            spawn_field(&mut app, ());
        }
        settle(&mut app);
        let built = stats(&app);
        assert_eq!(built.fields, MAX_ENTRIES);
        assert_eq!(built.fields_dropped, 4);
        assert_eq!(
            app.world().resource::<FieldTables>().fields.len(),
            MAX_ENTRIES
        );

        // 16 species per field: twenty asked for, fifteen besides its own
        // blades drawn.
        let mut app = self::app();
        let many: Vec<GrassSpecies> = (0..20)
            .map(|_| GrassSpecies::new(GrassBlade::CLOVER, 0.01))
            .collect();
        let style = app
            .world_mut()
            .resource_mut::<Assets<GrassStyle>>()
            .add(GrassStyle::MEADOW.with_species(many));
        app.world_mut().spawn((
            GrassField {
                size: Vec2::splat(16.0),
                style,
                ..Default::default()
            },
            GlobalTransform::IDENTITY,
        ));
        settle(&mut app);
        let built = stats(&app);
        assert_eq!(built.species_dropped, 20 - (MAX_SPECIES - 1));
        assert_eq!(built.fields_dropped, 0);
    }
    // ---- incremental rebuilds -------------------------------------------

    use bevy::asset::RenderAssetUsages;
    use bevy::color::Color;
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

    fn mask_image(size: u32, value: u8) -> Image {
        Image::new_fill(
            Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            &[value, 255, 0, 255],
            TextureFormat::Rgba8Unorm,
            RenderAssetUsages::default(),
        )
    }

    fn add_image(app: &mut App, size: u32, value: u8) -> Handle<Image> {
        app.world_mut()
            .resource_mut::<Assets<Image>>()
            .add(mask_image(size, value))
    }

    fn add_style(app: &mut App, style: GrassStyle) -> Handle<GrassStyle> {
        app.world_mut()
            .resource_mut::<Assets<GrassStyle>>()
            .add(style)
    }

    fn spawn_with(app: &mut App, field: GrassField) -> Entity {
        app.world_mut()
            .spawn((field, GlobalTransform::IDENTITY))
            .id()
    }

    fn tables_now(app: &App) -> FieldTables {
        app.world().resource::<FieldTables>().clone()
    }

    fn edit_style(app: &mut App, style: &Handle<GrassStyle>, edit: impl FnOnce(&mut GrassStyle)) {
        let mut styles = app.world_mut().resource_mut::<Assets<GrassStyle>>();
        // Dropping the guard is what announces the change.
        let mut guard = styles.get_mut(style).expect("the style");
        edit(&mut guard);
    }

    /// A field of 32 m with a mask and a heightmap.
    fn rich_field(
        app: &mut App,
        style: &Handle<GrassStyle>,
    ) -> (Entity, Handle<Image>, Handle<Image>) {
        let mask = add_image(app, 64, 200);
        let heightmap = add_image(app, 64, 128);
        let field = spawn_with(
            app,
            GrassField {
                size: Vec2::splat(32.0),
                style: style.clone(),
                density_map: Some(mask.clone()),
                ground: GrassGround::Heightmap {
                    image: heightmap.clone(),
                    height_scale: 5.0,
                },
            },
        );
        (field, mask, heightmap)
    }

    #[test]
    fn a_colour_only_style_change_is_a_uniform_update() {
        let mut app = app();
        let style = add_style(&mut app, GrassStyle::MEADOW);
        rich_field(&mut app, &style);
        settle(&mut app);
        let before = tables_now(&app);
        let at_start = stats(&app);
        assert_eq!(at_start.images_read, 2, "the mask and the heightmap, once");

        // Retune the colours every frame, as a day/night or season blend does.
        for step in 0..30 {
            let t = step as f32 / 30.0;
            edit_style(&mut app, &style, |style| {
                style.blade.tip_color = Color::srgb(0.2 + 0.6 * t, 0.6 - 0.3 * t, 0.1);
                style.ground_tint.color = Color::srgb(0.1 + 0.2 * t, 0.1, 0.05);
            });
            app.update();
        }
        let after = tables_now(&app);
        let now = stats(&app);

        assert_eq!(after.version, before.version + 30, "every change is shown");
        assert_eq!(
            after.structure_version, before.structure_version,
            "and none of them is a layout rebuild"
        );
        assert!(
            Arc::ptr_eq(&after.chunks, &before.chunks),
            "chunks are kept"
        );
        assert!(Arc::ptr_eq(&after.masks, &before.masks), "masks are kept");
        assert!(
            Arc::ptr_eq(&after.heights, &before.heights),
            "heights are kept"
        );
        assert_eq!(
            now.images_read, at_start.images_read,
            "no image was read again"
        );
        assert_eq!(now.layout_rebuilds, at_start.layout_rebuilds);
        assert_eq!(now.rebuilds, at_start.rebuilds + 30);
        assert_eq!(now.entries_rewritten, at_start.entries_rewritten + 30);

        // And the colour really is the last one.
        let t = 29.0 / 30.0;
        let last = linear(Color::srgb(0.2 + 0.6 * t, 0.6 - 0.3 * t, 0.1));
        assert_eq!(after.fields[0].tip_color.truncate(), last.truncate());
        assert_ne!(after.fields[0].tip_color, before.fields[0].tip_color);
    }

    #[test]
    fn a_big_mask_is_read_once_however_often_the_style_changes() {
        let mut app = app();
        let style = add_style(&mut app, GrassStyle::MEADOW);
        let mask = add_image(&mut app, 2048, 200);
        spawn_with(
            &mut app,
            GrassField {
                size: Vec2::splat(64.0),
                style: style.clone(),
                density_map: Some(mask),
                ..Default::default()
            },
        );
        app.update();
        let first = stats(&app);
        assert_eq!(first.images_read, 1);
        let initial = first.last_rebuild;

        let mut slowest = std::time::Duration::ZERO;
        for step in 0..20 {
            edit_style(&mut app, &style, |style| {
                style.blade.tip_color = Color::srgb(0.1 + step as f32 * 0.04, 0.5, 0.1);
            });
            app.update();
            slowest = slowest.max(stats(&app).last_rebuild);
        }
        assert_eq!(stats(&app).images_read, 1, "4 million texels, read once");
        assert!(
            slowest * 4 < initial,
            "an update took {slowest:?} against {initial:?} for the first build, which read the mask"
        );
    }

    #[test]
    fn only_the_image_that_changed_is_read_again() {
        let mut app = app();
        let style = add_style(&mut app, GrassStyle::MEADOW);
        let mask_a = add_image(&mut app, 32, 50);
        let mask_b = add_image(&mut app, 32, 60);
        let heightmap_b = add_image(&mut app, 32, 128);
        spawn_with(
            &mut app,
            GrassField {
                size: Vec2::splat(16.0),
                style: style.clone(),
                density_map: Some(mask_a.clone()),
                ..Default::default()
            },
        );
        spawn_with(
            &mut app,
            GrassField {
                size: Vec2::splat(16.0),
                style: style.clone(),
                density_map: Some(mask_b),
                ground: GrassGround::Heightmap {
                    image: heightmap_b,
                    height_scale: 3.0,
                },
            },
        );
        settle(&mut app);
        let before = tables_now(&app);
        let at_start = stats(&app);
        assert_eq!(at_start.images_read, 3);

        // Paint one texel of the first mask.
        app.world_mut()
            .resource_mut::<Assets<Image>>()
            .get_mut(&mask_a)
            .unwrap()
            .set_color_at(0, 0, Color::linear_rgb(1.0, 1.0, 0.0))
            .unwrap();
        app.update();
        let after = tables_now(&app);
        let now = stats(&app);

        assert_eq!(now.images_read, at_start.images_read + 1, "only that image");
        assert!(
            Arc::ptr_eq(&after.heights, &before.heights),
            "heights untouched"
        );
        assert!(
            Arc::ptr_eq(&after.chunks, &before.chunks),
            "chunks untouched"
        );
        assert!(!Arc::ptr_eq(&after.masks, &before.masks));
        assert_eq!(
            after.structure_version,
            before.structure_version + 1,
            "the masks upload"
        );
        // The painted texel is in the table, and nothing else moved. (Fields
        // are in the tables in entity order, which is not spawn order, so look
        // for what changed rather than where.)
        let differing = |a: &[u32], b: &[u32]| a.iter().zip(b).filter(|(a, b)| a != b).count();
        assert_eq!(before.masks.len(), after.masks.len());
        assert_eq!(
            differing(&before.masks, &after.masks),
            1,
            "exactly the painted word changed"
        );
        assert_eq!(
            now.entries_rewritten,
            at_start.entries_rewritten + 1,
            "its field only"
        );
        let changed_entries = before
            .fields
            .iter()
            .zip(after.fields.iter())
            .filter(|(a, b)| a != b)
            .count();
        assert_eq!(
            changed_entries, 0,
            "the mask kept its size and place, so no entry differs"
        );
    }

    #[test]
    fn only_the_entries_of_the_changed_style_are_rewritten() {
        let mut app = app();
        let plain = add_style(&mut app, GrassStyle::MEADOW);
        let wild = add_style(&mut app, GrassStyle::SUMMER_WILD);
        let species = GrassStyle::SUMMER_WILD.species.len();
        let one = |app: &mut App, style: &Handle<GrassStyle>| {
            spawn_with(
                app,
                GrassField {
                    size: Vec2::splat(16.0),
                    style: style.clone(),
                    ..Default::default()
                },
            )
        };
        one(&mut app, &plain);
        one(&mut app, &wild);
        one(&mut app, &plain);
        settle(&mut app);
        let before = tables_now(&app);
        let start = stats(&app).entries_rewritten;
        let wild_entries = (1 + species) as u64;
        assert_eq!(before.fields.len() as u64, 2 + wild_entries);

        edit_style(&mut app, &plain, |style| style.blade.stiffness = 0.9);
        app.update();
        let after = tables_now(&app);
        assert_eq!(
            stats(&app).entries_rewritten,
            start + 2,
            "the two plain fields"
        );
        let changed = |a: &FieldTables, b: &FieldTables| {
            a.fields
                .iter()
                .zip(b.fields.iter())
                .filter(|(a, b)| a != b)
                .count()
        };
        assert_eq!(changed(&after, &before), 2, "and only their entries differ");

        edit_style(&mut app, &wild, |style| style.blade.stiffness = 0.9);
        app.update();
        assert_eq!(
            stats(&app).entries_rewritten,
            start + 2 + wild_entries,
            "the wild field and each of its species"
        );
    }

    #[test]
    fn a_change_no_field_shows_changes_nothing() {
        let mut app = app();
        let used = add_style(&mut app, GrassStyle::MEADOW);
        let unused = add_style(&mut app, GrassStyle::LAWN);
        spawn_with(
            &mut app,
            GrassField {
                size: Vec2::splat(16.0),
                style: used.clone(),
                ..Default::default()
            },
        );
        settle(&mut app);
        let (tables, counted) = (tables_now(&app), stats(&app));

        // A style no field uses.
        edit_style(&mut app, &unused, |style| style.density = 1.0);
        app.update();
        assert_eq!(version(&app), tables.version);
        assert_eq!(stats(&app), counted);

        // Touching the used style without changing it: the entries are
        // worked out again, find themselves the same, and nothing is uploaded.
        edit_style(&mut app, &used, |_| ());
        app.update();
        assert_eq!(version(&app), tables.version);
        assert_eq!(stats(&app).rebuilds, counted.rebuilds);
        assert_eq!(stats(&app).entries_rewritten, counted.entries_rewritten + 1);
    }

    #[test]
    fn moving_a_field_reads_no_image_and_a_sideways_move_uploads_no_layout() {
        let mut app = app();
        let style = add_style(&mut app, GrassStyle::MEADOW);
        let (field, ..) = rich_field(&mut app, &style);
        settle(&mut app);
        let before = tables_now(&app);
        let reads = stats(&app).images_read;

        *app.world_mut().get_mut::<GlobalTransform>(field).unwrap() =
            GlobalTransform::from_xyz(40.0, 0.0, -10.0);
        app.update();
        let sideways = tables_now(&app);
        assert_eq!(stats(&app).images_read, reads);
        assert_eq!(sideways.version, before.version + 1);
        assert_eq!(
            sideways.structure_version, before.structure_version,
            "chunk bounds are relative to the field, so a sideways move changes only its entries"
        );
        assert!(Arc::ptr_eq(&sideways.masks, &before.masks));
        assert_eq!(sideways.fields[0].min_corner.x, 40.0 - 16.0);

        // Up is different: the chunks hold the ground's height.
        *app.world_mut().get_mut::<GlobalTransform>(field).unwrap() =
            GlobalTransform::from_xyz(40.0, 3.0, -10.0);
        app.update();
        let up = tables_now(&app);
        assert_eq!(up.structure_version, sideways.structure_version + 1);
        assert_eq!(stats(&app).images_read, reads, "still no image read");
        assert!(Arc::ptr_eq(&up.masks, &before.masks), "and the masks stay");
        assert!(up.chunks[0].bounds.z > sideways.chunks[0].bounds.z);
    }

    #[test]
    fn a_field_can_differ_from_its_style_without_a_style_of_its_own() {
        let mut app = app();
        let style = add_style(&mut app, GrassStyle::MEADOW);
        let spawn = |app: &mut App| {
            spawn_with(
                app,
                GrassField {
                    size: Vec2::splat(32.0),
                    style: style.clone(),
                    ..Default::default()
                },
            )
        };
        let (plain, changed) = (spawn(&mut app), spawn(&mut app));
        settle(&mut app);
        let entry_of = |app: &App, entity: Entity| {
            let tables = tables_now(app);
            let slot = tables.slots.iter().find(|s| s.entity == entity).unwrap();
            tables.fields[slot.first]
        };
        let before = entry_of(&app, changed);
        assert_eq!(before.shape, entry_of(&app, plain).shape);

        app.world_mut()
            .entity_mut(changed)
            .insert(GrassFieldOverrides {
                tint: Some(Color::linear_rgb(2.0, 1.0, 0.5)),
                density_scale: 0.5,
                height_scale: 0.5,
            });
        app.update();
        let now = entry_of(&app, changed);
        let blade = GrassStyle::MEADOW.blade;
        assert_eq!(now.shape.x, blade.height.x * 0.5);
        assert_eq!(now.shape.y, blade.height.y * 0.5);
        assert_eq!(now.shape.z, before.shape.z, "widths are not scaled");
        assert_eq!(now.shape.w, GrassStyle::MEADOW.density * 0.5);
        assert_eq!(
            now.mix.w,
            before.mix.w * 0.5,
            "the tallest blade, for culling"
        );
        let (root, tip) = (linear(blade.root_color), linear(blade.tip_color));
        assert_eq!(
            now.root_color.truncate(),
            root.truncate() * Vec3::new(2.0, 1.0, 0.5)
        );
        assert_eq!(
            now.tip_color.truncate(),
            tip.truncate() * Vec3::new(2.0, 1.0, 0.5)
        );
        assert_eq!(now.tip_color.w, before.tip_color.w, "translucency is kept");
        assert_eq!(now.head, before.head, "heads are not tinted");
        assert_eq!(
            now.placement.y,
            ChunkLayout::for_density(CHUNK_SIZE, GrassStyle::MEADOW.density * 0.5).tiles_per_side,
            "fewer blades need fewer tiles"
        );
        assert_eq!(entry_of(&app, plain), before, "the other field");

        // Nothing else was rewritten: one field's entry, no images.
        let rewritten = stats(&app).entries_rewritten;
        app.world_mut()
            .get_mut::<GrassFieldOverrides>(changed)
            .unwrap()
            .tint = None;
        app.update();
        assert_eq!(stats(&app).entries_rewritten, rewritten + 1);
        assert_eq!(entry_of(&app, changed).root_color, before.root_color);

        // Taking the component off puts the field back as it was.
        app.world_mut()
            .entity_mut(changed)
            .remove::<GrassFieldOverrides>();
        app.update();
        assert_eq!(entry_of(&app, changed), before);

        // Nonsense is clamped, not drawn.
        app.world_mut()
            .entity_mut(changed)
            .insert(GrassFieldOverrides {
                tint: None,
                density_scale: -1.0,
                height_scale: -3.0,
            });
        app.update();
        let clamped = entry_of(&app, changed);
        assert_eq!((clamped.shape.y, clamped.shape.w), (0.0, 0.0));
    }

    #[test]
    fn the_highest_ground_under_a_point_is_found_in_the_right_chunk() {
        let mut app = app();
        let style = add_style(&mut app, GrassStyle::MEADOW);
        let field = |app: &mut App, size: Vec2, x: f32, y: f32| {
            let id = spawn_with(
                app,
                GrassField {
                    size,
                    style: style.clone(),
                    ..Default::default()
                },
            );
            *app.world_mut().get_mut::<GlobalTransform>(id).unwrap() =
                GlobalTransform::from_xyz(x, y, 0.0);
            id
        };
        // 40 × 20 m (3 × 2 chunks) at height 3, and 16 × 16 m at height 7.
        field(&mut app, Vec2::new(40.0, 20.0), 0.0, 3.0);
        field(&mut app, Vec2::splat(16.0), 100.0, 7.0);
        settle(&mut app);
        let tables = tables_now(&app);
        let at = |x: f32, z: f32| tables.highest_ground_at(Vec2::new(x, z));
        // Which field is first in the tables depends on the entities; either
        // way each point finds its own field's height.
        assert_eq!(at(0.0, 0.0), Some(3.0));
        assert_eq!(
            at(19.9, 9.9),
            Some(3.0),
            "the far corner of the first field"
        );
        assert_eq!(at(-19.9, -9.9), Some(3.0), "and the near one");
        assert_eq!(at(100.0, 0.0), Some(7.0));
        assert_eq!(at(107.9, -7.9), Some(7.0));
        assert_eq!(at(20.1, 0.0), None, "beside the first field");
        assert_eq!(at(0.0, 10.1), None, "past its edge");
        assert_eq!(at(0.0, 50.0), None);
    }

    /// The tables worked out from nothing, for what the app holds now.
    fn from_scratch(app: &mut App) -> FieldTables {
        let kept = tables_now(app);
        *app.world_mut().resource_mut::<FieldTables>() = FieldTables::default();
        *app.world_mut().resource_mut::<PackedImages>() = PackedImages::default();
        app.world_mut().resource_mut::<TablesDirty>().fields = true;
        app.update();
        let fresh = tables_now(app);
        // Carry on from where the incremental tables were, as the caller's
        // next step must continue from them.
        *app.world_mut().resource_mut::<FieldTables>() = kept;
        fresh
    }

    fn same_tables(incremental: &FieldTables, fresh: &FieldTables, step: &str) {
        assert_eq!(incremental.fields, fresh.fields, "{step}: fields");
        assert_eq!(incremental.chunks, fresh.chunks, "{step}: chunks");
        assert_eq!(incremental.masks, fresh.masks, "{step}: masks");
        assert_eq!(incremental.heights, fresh.heights, "{step}: heights");
        assert_eq!(incremental.slots, fresh.slots, "{step}: slots");
        assert_eq!(incremental.mask_ids, fresh.mask_ids, "{step}: mask ids");
        assert_eq!(
            incremental.height_ids, fresh.height_ids,
            "{step}: height ids"
        );
        assert_eq!(incremental.features, fresh.features, "{step}: features");
    }

    #[test]
    fn building_incrementally_gives_exactly_what_building_from_scratch_gives() {
        // Every kind of change, in a world with several fields, masks,
        // heightmaps and species; after each the incremental tables must equal
        // those worked out from nothing. A stale cache would show here.
        let mut app = app();
        let plain = add_style(&mut app, GrassStyle::MEADOW);
        let wild = add_style(&mut app, GrassStyle::SUMMER_WILD);
        let (rich, mask, heightmap) = rich_field(&mut app, &plain);
        let small_mask = add_image(&mut app, 16, 90);
        let second = spawn_with(
            &mut app,
            GrassField {
                size: Vec2::new(48.0, 20.0),
                style: wild.clone(),
                density_map: Some(small_mask.clone()),
                ..Default::default()
            },
        );
        settle(&mut app);
        let check = |app: &mut App, step: &str| {
            app.update();
            app.update();
            let fresh = from_scratch(app);
            same_tables(&tables_now(app), &fresh, step);
        };
        check(&mut app, "start");

        edit_style(&mut app, &plain, |style| {
            style.blade.tip_color = Color::srgb(0.9, 0.5, 0.1);
        });
        check(&mut app, "colour");
        edit_style(&mut app, &plain, |style| style.density = 20.0);
        check(&mut app, "density, which changes the tile layout");
        edit_style(&mut app, &wild, |style| {
            style.species = Vec::new().into();
        });
        check(&mut app, "species removed: entries shift");
        edit_style(&mut app, &wild, |style| {
            *style = GrassStyle::AUTUMN_WILD;
        });
        check(&mut app, "species added: entries shift back");

        *app.world_mut().get_mut::<GlobalTransform>(second).unwrap() =
            GlobalTransform::from_xyz(-30.0, 1.0, 5.0);
        check(&mut app, "moved");
        app.world_mut().get_mut::<GrassField>(rich).unwrap().size = Vec2::new(64.0, 16.0);
        check(&mut app, "resized");

        app.world_mut()
            .resource_mut::<Assets<Image>>()
            .get_mut(&mask)
            .unwrap()
            .set_color_at(3, 3, Color::linear_rgb(0.0, 1.0, 0.0))
            .unwrap();
        check(&mut app, "mask painted");
        app.world_mut()
            .resource_mut::<Assets<Image>>()
            .get_mut(&heightmap)
            .unwrap()
            .set_color_at(5, 5, Color::linear_rgb(1.0, 0.0, 0.0))
            .unwrap();
        check(&mut app, "heightmap painted");

        // A different mask on a field, with a different size.
        let bigger = add_image(&mut app, 128, 30);
        app.world_mut()
            .get_mut::<GrassField>(second)
            .unwrap()
            .density_map = Some(bigger);
        check(&mut app, "mask swapped for a bigger one");
        app.world_mut()
            .get_mut::<GrassField>(second)
            .unwrap()
            .density_map = None;
        check(&mut app, "mask removed");

        let third = spawn_with(
            &mut app,
            GrassField {
                size: Vec2::splat(24.0),
                style: plain.clone(),
                density_map: Some(small_mask),
                ..Default::default()
            },
        );
        check(&mut app, "a field added, sharing a mask");
        app.world_mut()
            .entity_mut(second)
            .insert(GrassFieldOverrides {
                tint: Some(Color::srgb(1.2, 1.0, 0.6)),
                density_scale: 0.5,
                height_scale: 1.5,
            });
        check(&mut app, "overrides added");
        app.world_mut()
            .get_mut::<GrassFieldOverrides>(second)
            .unwrap()
            .tint = None;
        check(&mut app, "tint taken off");
        app.world_mut()
            .entity_mut(second)
            .remove::<GrassFieldOverrides>();
        check(&mut app, "overrides removed");
        app.world_mut().entity_mut(rich).despawn();
        check(&mut app, "a field removed");
        app.world_mut().entity_mut(third).insert(Visibility::Hidden);
        app.update();
        let fresh = from_scratch(&mut app);
        same_tables(&tables_now(&app), &fresh, "hidden");
    }
}
