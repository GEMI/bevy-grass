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
use bevy::math::{EulerRot, Quat, UVec2, UVec4, Vec2, Vec3, Vec4};
use bevy::platform::time::Instant;
use bevy::transform::TransformSystems;
use bevy::transform::components::GlobalTransform;

use crate::api::{
    GrassBlade, GrassField, GrassFieldOverrides, GrassGround, GrassSpecies, GrassStyle,
};
use crate::edit::{self, EditLayers};
use crate::gpu_layout::{ChunkGpu, FieldGpu, NO_MAP};
use crate::ground::{HOLE, MeshGrounds, bake_mesh_grounds};
use crate::packed::{self, PackedImages};
use crate::shader_model::{self, ChunkLayout};
use crate::stats::TableStats;
use crate::style::MAX_SPECIES;
use crate::{GrassStats, GrassSystems};

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

impl FieldGpu {
    /// The field's X axis in world XZ. An entry built by hand, with no axis,
    /// is unturned.
    pub(crate) fn x_axis(&self) -> Vec2 {
        let axis = Vec2::new(self.axis.x, self.axis.y);
        if axis == Vec2::ZERO { Vec2::X } else { axis }
    }

    /// A point of the field, in metres along its axes from its minimum
    /// corner, in world XZ. Twin of `field_to_world` in `generate.wesl`.
    /// Only the debug gizmos and the tests draw from the GPU table.
    #[cfg(any(test, feature = "debug"))]
    pub(crate) fn world_of(&self, local: Vec2) -> Vec2 {
        Vec2::new(self.min_corner.x, self.min_corner.z) + turn(self.x_axis(), local)
    }

    /// The point of the field at `point` (world XZ), in metres along its axes
    /// from its minimum corner.
    pub(crate) fn local_of(&self, point: Vec2) -> Vec2 {
        let axis = self.x_axis();
        let offset = point - Vec2::new(self.min_corner.x, self.min_corner.z);
        Vec2::new(offset.dot(axis), offset.dot(axis.perp()))
    }
}

/// `v`, given along the axes of a field whose X axis is `axis`, in world XZ.
/// Unturned (`axis` = X) it is `v` exactly.
fn turn(axis: Vec2, v: Vec2) -> Vec2 {
    axis * v.x + axis.perp() * v.y
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
    /// What each mask in `masks` is, in order.
    pub mask_ids: Arc<Vec<MaskKey>>,
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
            let local = field.local_of(point);
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

impl FieldTables {
    /// The ground below `point` (world X and Z) as [`highest_ground_at`]
    /// finds it, or, outside every field, the first field's height, or 0.
    ///
    /// [`highest_ground_at`]: FieldTables::highest_ground_at
    pub(crate) fn ground_below(&self, point: Vec2) -> f32 {
        self.highest_ground_at(point)
            .or_else(|| self.fields.first().map(|field| field.min_corner.y))
            .unwrap_or(0.0)
    }
}

/// What one mask in the tables is. An image keeps its key when it is edited,
/// so a field's first edit does not move anything in the tables; a field
/// without an image gets a mask of its own on its first edit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum MaskKey {
    /// The field's density map, edited or not.
    Image(AssetId<Image>),
    /// The edit layer of a field that has no density map.
    Edited(Entity),
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
    axis: Vec2,
    access: u32,
    /// The mask, and its offset in the mask table, in texels.
    mask: Option<(MaskKey, u32)>,
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

    /// Every optional look term with the shader definition that compiles it
    /// in: the one list `shader_defs` and the tests both read, so a term
    /// cannot be given a definition in one place and not the other.
    pub(crate) const TERMS: [(Self, &'static str); 5] = [
        (Self::GROUND_TINT, "GRASS_GROUND_TINT"),
        (Self::SHEEN, "GRASS_SHEEN"),
        (Self::TOON, "GRASS_TOON"),
        (Self::SHAPE, "GRASS_SHAPE"),
        (Self::VARIATION, "GRASS_VARIATION"),
    ];

    /// The shader definitions that compile the terms in.
    pub(crate) fn shader_defs(self) -> impl Iterator<Item = &'static str> {
        Self::TERMS
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
        .init_resource::<MeshGrounds>()
        .init_resource::<GrassStats>()
        .init_resource::<PackedImages>()
        .configure_sets(
            PostUpdate,
            (
                GrassSystems::DetectChanges,
                GrassSystems::Edit,
                GrassSystems::Rebuild,
            )
                .chain()
                .after(TransformSystems::Propagate)
                .after(VisibilitySystems::VisibilityPropagate)
                .after(AssetEventSystems),
        )
        .add_systems(
            PostUpdate,
            (
                (
                    warn_unsupported,
                    warn_missing_style,
                    (bake_mesh_grounds, detect_changes).chain(),
                )
                    .in_set(GrassSystems::DetectChanges),
                (rebuild, edit::collect_patches)
                    .chain()
                    .in_set(GrassSystems::Rebuild),
            ),
        );
    edit::plugin(app);
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
    /// Marks the fields as changed: a field's ground was baked, say.
    pub(crate) fn mark_fields(&mut self) {
        self.fields = true;
    }

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

/// How many frames a field may have no style before it is worth a warning:
/// long enough for a field spawned in `Startup` to have its style added, short
/// enough to answer "why is nothing drawing?" while the user is looking.
const MISSING_STYLE_FRAMES: u8 = 3;

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

/// Where a field lies on the ground, worked out from its components exactly
/// as the tables work it out, for code that cannot wait for a rebuild.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct FieldFrame {
    /// The minimum corner (the field's own −X, −Z corner), in world XZ.
    pub corner: Vec2,
    /// The field's X axis in world XZ.
    pub axis: Vec2,
    /// Its size on the ground, scale applied.
    pub size: Vec2,
}

impl FieldFrame {
    pub(crate) fn of(field: &GrassField, transform: &GlobalTransform) -> Self {
        let size = scaled_size(field, transform);
        let axis = field_axis(transform);
        let origin = transform.translation();
        Self {
            corner: Vec2::new(origin.x, origin.z) + turn(axis, -size * 0.5),
            axis,
            size,
        }
    }

    /// A point of the field, in metres along its axes from its minimum
    /// corner, in world XZ.
    pub(crate) fn world_of(&self, local: Vec2) -> Vec2 {
        self.corner + turn(self.axis, local)
    }

    /// The point of the field at `point` (world XZ), in metres along its axes
    /// from its minimum corner.
    pub(crate) fn local_of(&self, point: Vec2) -> Vec2 {
        let offset = point - self.corner;
        Vec2::new(offset.dot(self.axis), offset.dot(self.axis.perp()))
    }
}

/// A field's X axis in world XZ: its transform's turn about Y. Grass grows
/// upright, so a tilt is left out.
fn field_axis(transform: &GlobalTransform) -> Vec2 {
    let (yaw, _, _) = transform.rotation().to_euler(EulerRot::YXZ);
    let (sin, cos) = yaw.sin_cos();
    // An unturned field gets exactly X, so it is placed exactly as before.
    if sin == 0.0 && cos > 0.0 {
        Vec2::X
    } else {
        Vec2::new(cos, -sin)
    }
}

/// Whether a transform tilts the field out of the XZ plane, which a field
/// does not follow.
fn is_tilted(transform: &GlobalTransform) -> bool {
    let rotation = transform.rotation();
    let (yaw, _, _) = rotation.to_euler(EulerRot::YXZ);
    rotation.angle_between(Quat::from_rotation_y(yaw)) > 1e-3
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
    mut tilted: Local<HashSet<Entity>>,
    mut layered: Local<HashSet<Entity>>,
) {
    for (entity, transform, layers) in &fields {
        if first_time(&mut tilted, entity, is_tilted(transform)) {
            warn!(
                "bevy_grass: the GrassField {entity} is tilted; grass grows upright, so only \
                 its turn about Y is used. For a slope, use GrassGround::Heightmap or \
                 GrassGround::Mesh."
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

/// Warns, once per field, when a field still has the empty default style
/// handle after a few frames. Such a handle never loads, so the field waits
/// forever and nothing is drawn; the warning says how to give it a style.
pub(crate) fn warn_missing_style(
    fields: Query<(Entity, &GrassField)>,
    styles: Res<Assets<GrassStyle>>,
    mut waiting: Local<HashMap<Entity, u8>>,
) {
    waiting.retain(|entity, _| fields.contains(*entity));
    let default = AssetId::<GrassStyle>::default();
    for (entity, field) in &fields {
        let missing = field.style.id() == default && !styles.contains(&field.style);
        if !missing {
            waiting.remove(&entity);
            continue;
        }
        let frames = waiting.entry(entity).or_insert(0);
        *frames = frames.saturating_add(1);
        if *frames == MISSING_STYLE_FRAMES {
            warn!(
                "bevy_grass: the GrassField {entity} has no style, so nothing is drawn; use \
                 styles.add(GrassStyle::MEADOW) or load a .grass.ron, or construct it with \
                 GrassField::new(size, GrassStyle::MEADOW, &mut styles)."
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
    /// The minimum corner (the field's own −X, −Z corner), at the field's
    /// base height.
    corner: Vec3,
    /// The field's X axis in world XZ.
    axis: Vec2,
    access: u32,
}

impl Placement {
    fn of(
        field: &GrassField,
        transform: &GlobalTransform,
        visibility: InheritedVisibility,
        layers: Option<&RenderLayers>,
    ) -> Self {
        let frame = FieldFrame::of(field, transform);
        Self {
            size: frame.size,
            corner: Vec3::new(frame.corner.x, transform.translation().y, frame.corner.y),
            axis: frame.axis,
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
    mask: Option<MaskInput<'a>>,
    height: Option<(AssetId<Image>, &'a Image, f32)>,
}

/// A field's mask: its image, its edit layer, or both, in which case the
/// layer's texels are the ones drawn.
struct MaskInput<'a> {
    key: MaskKey,
    size: UVec2,
    image: Option<&'a Image>,
    edited: Option<&'a [u32]>,
}

impl MaskInput<'_> {
    /// The image the mask was read from, if any.
    fn image_id(&self) -> Option<AssetId<Image>> {
        match self.key {
            MaskKey::Image(id) => Some(id),
            MaskKey::Edited(_) => None,
        }
    }

    /// The texels, packed (see `PackedImages::mask`).
    fn words(&self, cache: &mut PackedImages) -> Arc<Vec<u32>> {
        match (self.edited, self.image, self.key) {
            (Some(words), ..) => Arc::new(words.to_vec()),
            (None, Some(image), MaskKey::Image(id)) => cache.mask(id, image),
            _ => Arc::default(),
        }
    }
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
            axis: input.placement.axis,
            access: input.placement.access,
            mask: input.mask.as_ref().map(|mask| (mask.key, self.mask_offset)),
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
        let mask = self.input.mask.as_ref().and_then(MaskInput::image_id);
        mask.is_some_and(|id| changed.contains(&id))
            || self
                .input
                .height
                .is_some_and(|(id, ..)| changed.contains(&id))
    }
}

/// Reads each field with its style, mask and heightmap, or `None` while any of
/// them has not loaded (or a mesh ground has not been baked), so a field
/// never appears at the wrong density or height and then jumps. An edited
/// field's mask is its edit layer.
fn gather<'a>(
    sorted: &[FieldInputs<'a>],
    styles: &'a Assets<GrassStyle>,
    images: &'a Assets<Image>,
    grounds: &MeshGrounds,
    edits: &'a EditLayers,
) -> Option<Vec<Input<'a>>> {
    let mut inputs = Vec::with_capacity(sorted.len());
    for (entity, field, transform, visibility, layers, overrides) in sorted {
        let layer = edits.layer_of(*entity, field);
        let mask = match &field.density_map {
            Some(handle) => {
                let image = images.get(handle)?;
                Some(MaskInput {
                    key: MaskKey::Image(handle.id()),
                    size: image.size(),
                    image: Some(image),
                    edited: layer
                        .filter(|layer| layer.size == image.size())
                        .map(|layer| layer.words.as_slice()),
                })
            }
            None => layer.map(|layer| MaskInput {
                key: MaskKey::Edited(*entity),
                size: layer.size,
                image: None,
                edited: Some(layer.words.as_slice()),
            }),
        };
        let height = match &field.ground {
            GrassGround::Flat => None,
            GrassGround::Heightmap {
                image,
                height_scale,
            } => Some((image.id(), images.get(image)?, *height_scale)),
            // A baked ground is in the mesh's units, which the transform's Y
            // scale turns into metres, as it does for the mesh itself.
            GrassGround::Mesh { .. } => {
                let image = grounds.image(*entity)?;
                Some((image.id(), images.get(image)?, transform.scale().y))
            }
        };
        inputs.push(Input {
            entity: *entity,
            placement: Placement::of(field, transform, **visibility, *layers),
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
        if let Some(mask) = &input.mask {
            mask_words += packed::mask_words(mask.size.x * mask.size.y);
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
/// images, each read at most once (see `PackedImages`), and the edit layers.
struct Images {
    masks: Arc<Vec<u32>>,
    heights: Arc<Vec<f32>>,
    mask_ids: Arc<Vec<MaskKey>>,
    height_ids: Arc<Vec<AssetId<Image>>>,
    masks_same: bool,
    heights_same: bool,
}

/// `edits_ahead` says the edit layers changed since the masks were last
/// joined. Edits reach the GPU as patches, so stale masks are kept while
/// nothing else is uploaded; but if the chunks or heightmaps are uploaded
/// (`chunks_same` false), the masks go with them and are joined afresh.
fn images_for(
    plans: &[Plan],
    old: &FieldTables,
    changed: &HashSet<AssetId<Image>>,
    cache: &mut PackedImages,
    (chunks_same, edits_ahead): (bool, bool),
) -> Images {
    let mask_ids: Vec<_> = plans
        .iter()
        .filter_map(|plan| plan.input.mask.as_ref().map(|mask| mask.key))
        .collect();
    let height_ids: Vec<_> = plans
        .iter()
        .filter_map(|plan| plan.input.height.map(|(id, ..)| id))
        .collect();
    let untouched = |ids: &[AssetId<Image>]| !ids.iter().any(|id| changed.contains(id));
    let mask_images: Vec<_> = plans
        .iter()
        .filter_map(|plan| plan.input.mask.as_ref().and_then(MaskInput::image_id))
        .collect();
    let heights_same = *old.height_ids == height_ids && untouched(&height_ids);
    let refresh = edits_ahead && !(chunks_same && heights_same);
    let masks_same = *old.mask_ids == mask_ids && untouched(&mask_images) && !refresh;

    let masks = if masks_same {
        old.masks.clone()
    } else {
        let parts: Vec<_> = plans
            .iter()
            .filter_map(|plan| plan.input.mask.as_ref())
            .map(|mask| mask.words(cache))
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
    cache.retain(&mask_images, &height_ids);
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
    (styles, images, grounds): (
        Res<Assets<GrassStyle>>,
        Res<Assets<Image>>,
        Res<MeshGrounds>,
    ),
    (mut cache, mut edits): (ResMut<PackedImages>, ResMut<EditLayers>),
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
    let edits_ahead = edits.ahead;
    let Some(inputs) = gather(&sorted, &styles, &images, &grounds, &edits) else {
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
    let (chunks, chunks_same) = chunks_for(&plans, &old, &changed_images, &mut cache);
    let images = images_for(
        &plans,
        &old,
        &changed_images,
        &mut cache,
        (chunks_same, edits_ahead),
    );
    let built = entries_for(&plans, &old, &changed_styles, &changed_images);
    drop(plans);
    drop(inputs);
    if !images.masks_same {
        // The masks were joined from the layers as they are now.
        edits.ahead = false;
    }

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
        motion: Vec4::new(
            look.lean,
            look.stiffness,
            1.0 - overrides.wind_scale.max(0.0),
            look.root_occlusion,
        ),
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
        axis,
        access,
    } = *placement;
    let overrides = &plan.input.overrides;
    let layout =
        ChunkLayout::for_density(CHUNK_SIZE, style.density * overrides.density_scale.max(0.0));
    let (mask_offset, mask_size) = match mask {
        Some(mask) => (plan.mask_offset, mask.size),
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
        axis: axis.extend(0.0).extend(0.0),
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
/// on every side so bilinear sampling at the edges is covered. Holes in a
/// mesh ground hold no blades, so they are left out.
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
            if h > HOLE * 0.5 {
                range = (range.0.min(h), range.1.max(h));
            }
        }
    }
    if range.0 > range.1 { (0.0, 0.0) } else { range }
}

#[cfg(test)]
mod tests;
