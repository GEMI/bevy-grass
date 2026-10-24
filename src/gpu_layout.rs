//! The field and chunk tables' GPU layout: the contract between the field
//! logic and `shaders/common.wesl`.
//!
//! The field logic (`fields.rs`, `edit.rs`) builds its tables directly in
//! this layout and the render world uploads them as they are, shared as
//! `Arc`s: converting from a layout of the logic's own would cost a copy of
//! every table on every rebuild for nothing a port needs. So the logic
//! depends on the GPU layout, but only here: a change to the layout, or to
//! how Bevy derives `ShaderType`, is a change to this file and its WESL
//! twins, `Field`, `Chunk` and `NO_MAP` in `common.wesl`.

use bevy::math::{UVec4, Vec4};
use bevy::render::render_resource::ShaderType;

/// Marks a field without a mask or heightmap. The WESL twin is `NO_MAP` in
/// `common.wesl`.
pub(crate) const NO_MAP: u32 = u32::MAX;

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
    /// x: lean, y: stiffness, z: how much less the wind moves the field's
    /// blades, `1 - GrassFieldOverrides::wind_scale` (so 0, an entry's
    /// default, is the full wind), w: root occlusion.
    pub motion: Vec4,
    pub root_color: Vec4,
    pub tip_color: Vec4,
    /// x: seed, y: tiles per chunk side, z: mask offset, w: heightmap offset
    /// (offsets in texels; `NO_MAP` when absent). On the GPU the heightmaps
    /// follow the masks in one buffer, so the render world adds the masks'
    /// length in words to `w` when it uploads the table.
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
    /// x, y: the field's own X axis in world XZ, which turns with the field
    /// about Y; its Z axis is the perpendicular (`Vec2::perp`). Positions in
    /// the field are along these axes from `min_corner`.
    pub axis: Vec4,
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
