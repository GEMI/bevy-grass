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

use crate::api::{GrassField, GrassGround, GrassSpecies, GrassStyle};
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

/// Entries beyond this index cannot be addressed by an instance's 8-bit
/// field index. Every species of a field is an entry.
const MAX_ENTRIES: usize = 256;

/// Most species one field mixes, its style's own blades included.
pub(crate) const MAX_SPECIES: usize = 16;

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
    /// x: lean, y: stiffness, z: minimum width in pixels, w: root occlusion.
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
    /// The optional shading terms some field uses (`LookFeatures`).
    pub features: LookFeatures,
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
    pub(crate) fn of(style: &GrassStyle, look: &GrassStyle) -> Self {
        let mut features = Self::default();
        if style.ground_tint > 0.0 {
            features.0 |= Self::GROUND_TINT.0;
        }
        if look.specular > 0.0 {
            features.0 |= Self::SHEEN.0;
        }
        if style.toon_bands > 0 {
            features.0 |= Self::TOON.0;
        }
        if look.twist > 0.0 || look.droop > 0.0 || look.fold > 0.0 || look.head > 0.0 {
            features.0 |= Self::SHAPE.0;
        }
        if style.patch_amount > 0.0 || style.dry_tips > 0.0 || style.dead_blades > 0.0 {
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
    let mut features = LookFeatures::default();
    for (field, transform, style, mask, heightmap) in &inputs {
        let species = &style.species[..style.species.len().min(MAX_SPECIES - 1)];
        if species.len() < style.species.len() {
            warn!(
                "bevy_grass mixes at most {MAX_SPECIES} species in a field; {} of a style's {} \
                 species are drawn",
                species.len(),
                style.species.len()
            );
        }
        if builder.fields.len() + 1 + species.len() > MAX_ENTRIES {
            warn!(
                "bevy_grass draws at most {MAX_ENTRIES} fields, each species of a field \
                 counting as one; some fields are not drawn"
            );
            break;
        }
        builder.add_field(field, transform, style, species, *mask, *heightmap);
        for look in std::iter::once(*style).chain(species.iter().map(|s| &s.style)) {
            features = features | LookFeatures::of(style, look);
        }
    }
    *tables = FieldTables {
        version: tables.version + 1,
        fields: Arc::new(builder.fields),
        chunks: Arc::new(builder.chunks),
        masks: Arc::new(builder.masks),
        heights: Arc::new(builder.heights),
        features,
    };
}

/// The tables being built, one field at a time.
/// The entry of one look of a field: `field_entry` (where the field is and
/// what belongs to the whole field) with `look`'s blades. `style` is the
/// field's own style, whose density, minimum width, toon bands and patch
/// colour every species shares.
fn look_entry(field_entry: FieldGpu, style: &GrassStyle, look: &GrassStyle, mix: Vec4) -> FieldGpu {
    let tip = linear(look.tip_color);
    let patch = linear(style.patch_color).truncate();
    FieldGpu {
        shape: Vec4::new(look.height.x, look.height.y, look.width, style.density),
        motion: Vec4::new(
            look.lean,
            look.stiffness,
            style.min_width_px,
            look.root_occlusion,
        ),
        root_color: linear(look.root_color),
        // The tip colour's alpha carries the translucency.
        tip_color: tip.truncate().extend(look.translucency),
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
            look.head.clamp(0.0, 0.9),
        ),
        head: linear(look.head_color)
            .truncate()
            .extend(look.head_width.max(0.0)),
        // In a patch the whole blade is scaled by how far its tip colour
        // moves toward the patch colour.
        patches: (patch / tip.truncate().max(Vec3::splat(1e-3)))
            .extend(style.patch_amount.clamp(0.0, 1.0)),
        ..field_entry
    }
}

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
        field: &GrassField,
        transform: &GlobalTransform,
        style: &GrassStyle,
        species: &[GrassSpecies],
        mask: Option<&Image>,
        heightmap: Option<(&Image, f32)>,
    ) {
        let index = self.fields.len() as u32;
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

        let field_entry = FieldGpu {
            min_corner: corner.extend(0.0),
            extent: Vec4::new(field.size.x, field.size.y, height_scale, layout.tile_size),
            // The seed comes from where the field is, not from its entity, so
            // the same field looks the same however the scene was spawned.
            placement: UVec4::new(
                mirror::hash2(corner.x.to_bits(), corner.z.to_bits()),
                layout.tiles_per_side,
                mask_offset,
                height_offset,
            ),
            map_sizes: UVec4::new(mask_size.x, mask_size.y, height_size.x, height_size.y),
            ground: linear(style.ground_color)
                .truncate()
                .extend(style.ground_tint.clamp(0.0, 1.0)),
            variation: Vec4::new(
                style.patch_size.max(0.01),
                style.dry_tips.clamp(0.0, 1.0),
                style.dead_blades.clamp(0.0, 1.0),
                0.0,
            ),
            tufts: Vec4::new(
                style.tufts.clamp(0.0, 1.0),
                style.tuft_size.max(0.01),
                if style.bare_patches > 0.0 {
                    mirror::bare_threshold(style.bare_patches)
                } else {
                    -1.0
                },
                style.bare_patch_size.max(0.01),
            ),
            ..FieldGpu::default()
        };
        let tallest = species
            .iter()
            .map(|species| species.style.height.y)
            .fold(style.height.y, f32::max);
        let chances = species_chances(species);
        let looks = std::iter::once((style, 1.0, 0.0)).chain(
            species
                .iter()
                .zip(&chances)
                .map(|(species, chance)| (&species.style, *chance, species.clustering)),
        );
        for (look, chance, clustering) in looks {
            let mix = Vec4::new(
                chance,
                clustering.clamp(0.0, 1.0),
                (1 + species.len()) as f32,
                tallest,
            );
            self.fields.push(look_entry(field_entry, style, look, mix));
        }

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
    fn species_chances_give_each_its_share() {
        let species = |shares: &[f32]| -> Vec<GrassSpecies> {
            shares
                .iter()
                .map(|share| GrassSpecies::new(GrassStyle::CLOVER, *share))
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
        assert_eq!(first.mix.w, GrassStyle::SEED_HEADS.height.y);
        // The seed heads' entry has their look and the field's placement.
        let heads = tables.fields[1];
        assert_eq!(heads.shape.y, GrassStyle::SEED_HEADS.height.y);
        assert_eq!(heads.form.w, GrassStyle::SEED_HEADS.head);
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
}
