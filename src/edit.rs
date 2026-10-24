//! Runtime edits: grass cleared, painted, mowed and regrown while the game
//! runs, and read back on the CPU for gameplay.
//!
//! A field's density mask says where grass grows and how tall: red is
//! density, green a height multiplier (`GrassField::density_map`). Edits
//! change that mask and nothing else, so the shaders draw an edited field
//! exactly as they draw a painted one, and a game that edits nothing pays
//! nothing.
//!
//! - **One copy, on the CPU.** A field's first edit gives it an edit layer:
//!   its mask packed as the GPU reads it (`packed`), or a full mask of
//!   [`EDIT_TEXEL`] texels for a field that had none. The authored image is
//!   never written; it is what `GrassEditOp::Restore` and regrowth return to.
//! - **Only what changed goes up.** The rows an edit touched are written
//!   into the mask buffer the shaders already read ([`MaskPatches`]), so an
//!   edit costs its own texels, not the field's. Nothing is read back from
//!   the GPU.
//! - **Queries are cheap.** The mask is on the CPU, so [`GrassSampler`]
//!   answers "how much grass is here, and how tall?" without the GPU.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use bevy::app::{App, PostUpdate};
use bevy::asset::{AssetEvent, AssetId, Assets, Handle, RenderAssetUsages};
use bevy::ecs::message::{Message, MessageReader};
use bevy::ecs::prelude::*;
use bevy::ecs::system::SystemParam;
use bevy::image::Image;
use bevy::log::warn;
use bevy::math::{EulerRot, UVec2, Vec2, Vec3, Vec3Swizzles, Vec4Swizzles};
use bevy::platform::time::Instant;
use bevy::reflect::Reflect;
use bevy::reflect::std_traits::ReflectDefault;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::time::Time;
use bevy::transform::components::{GlobalTransform, Transform};

use crate::api::GrassField;
use crate::fields::{FieldFrame, FieldSlot, FieldTables, TablesDirty};
use crate::gpu_layout::NO_MAP;
use crate::ground::HOLE;
use crate::packed::{self, PackedImages};
use crate::shader_model::{bare_noise, gust_noise_table, hash2, unit};
use crate::{GrassStats, GrassSystems};

/// Texel size, in metres, of the mask a field without one gets on its first
/// edit: one trample cell, about a footprint.
pub(crate) const EDIT_TEXEL: f32 = 0.25;

/// Most texels per side of that mask; a larger field gets coarser texels.
/// At this size the mask is 8 MB.
pub(crate) const MAX_EDIT_TEXELS: u32 = 2048;

/// Side of a regrowth tile, in texels. Regrowth visits only the tiles an edit
/// lowered.
const TILE: u32 = 16;

/// A tile regrows at most this often, in seconds: steps of a few frames'
/// growth rather than one every frame.
const REGROW_INTERVAL: f64 = 0.1;

/// Texels regrowth visits per frame, over every field: about 0.15 ms on the
/// development CPU. A larger lowered area is visited in turns, each tile
/// growing by the time since its last visit, so it regrows at the same rate,
/// in coarser steps.
const REGROW_BUDGET: u32 = 8192;

/// A brush that moves further than this in one frame, in metres, was
/// teleported: it edits where it lands, not along the way.
const MAX_SWEEP: f32 = 16.0;

/// Edits kept for one field while its mask loads. More are dropped with a
/// warning: a mask that never loads must not grow memory forever.
const MAX_WAITING: usize = 4096;

/// The softness of an edit or brush unless it says otherwise.
const DEFAULT_SOFTNESS: f32 = 0.3;

/// The area an edit covers, in world X and Z (`translation.xz()`).
#[derive(Debug, Clone, Copy, PartialEq, Reflect)]
#[reflect(Debug, PartialEq)]
pub enum GrassBrushShape {
    /// A disc.
    Circle {
        /// Its centre.
        center: Vec2,
        /// Its radius, in metres.
        radius: f32,
    },
    /// Every point within `radius` of the segment from `from` to `to`: a
    /// path, a track, a firebreak.
    Stroke {
        /// Where the stroke starts.
        from: Vec2,
        /// Where it ends.
        to: Vec2,
        /// Half its width, in metres.
        radius: f32,
    },
    /// A rectangle: a building's plot, a field's edge.
    Rect {
        /// Its centre.
        center: Vec2,
        /// Half its size along its own X and Z.
        half_size: Vec2,
        /// Its turn about Y, in radians, as `Quat::from_rotation_y(yaw)`
        /// turns things: a building's own yaw.
        yaw: f32,
    },
}

impl GrassBrushShape {
    /// A disc of `radius` metres around `center`.
    #[must_use]
    pub fn circle(center: Vec2, radius: f32) -> Self {
        Self::Circle { center, radius }
    }

    /// The points within `radius` metres of the segment from `from` to `to`.
    #[must_use]
    pub fn stroke(from: Vec2, to: Vec2, radius: f32) -> Self {
        Self::Stroke { from, to, radius }
    }

    /// A rectangle `half_size` either side of `center`, turned `yaw` radians
    /// about Y.
    #[must_use]
    pub fn rect(center: Vec2, half_size: Vec2, yaw: f32) -> Self {
        Self::Rect {
            center,
            half_size,
            yaw,
        }
    }
}

/// What an edit does to the grass under it.
///
/// Density and height are each 0 to 1 of what the field grows there without
/// edits at full mask: density 0 is bare ground, height 0.5 is half-height
/// blades. Thinning removes blades in a fixed order, so grass thinned a bit
/// at a time thins evenly and never flickers.
#[derive(Debug, Clone, Copy, PartialEq, Reflect)]
#[reflect(Debug, PartialEq)]
pub enum GrassEditOp {
    /// Moves the density toward this value, up or down: 0 clears the ground,
    /// 1 grows the style's full density.
    Density(f32),
    /// Moves the height toward this value, up or down.
    Height(f32),
    /// Cuts blades taller than this share of their full height down to it,
    /// and leaves shorter ones: mowing, grazing, a scythe.
    Mow(f32),
    /// Moves density and height back toward the field's own mask (full grass
    /// for a field without one): an undo, or regrowth at once.
    Restore,
}

/// A change to the grass: a message any system can write.
///
/// ```no_run
/// # use bevy::prelude::*;
/// # use bevy_grass::prelude::*;
/// // Clear the ground under a building as it is placed.
/// fn place_building(mut edits: MessageWriter<GrassEdit>, placed: Query<&Transform, Added<Building>>) {
///     for transform in &placed {
///         let yaw = transform.rotation.to_euler(EulerRot::YXZ).0;
///         edits.write(GrassEdit::clear(GrassBrushShape::rect(
///             transform.translation.xz(),
///             Vec2::new(6.0, 4.0),
///             yaw,
///         )));
///     }
/// }
/// # #[derive(Component)] struct Building;
/// ```
///
/// Edits are applied in [`GrassSystems::Edit`], in the order they were
/// written, to every field under the shape (or only [`GrassEdit::field`]).
/// They reach the screen the same frame, and [`GrassSampler`] sees them as
/// soon as they are applied. An edit for a field whose mask has not loaded
/// waits for it.
///
/// A field remembers its edits until it is despawned or its
/// [`density_map`](GrassField::density_map) changes (a different image, or
/// that image modified or reloaded), which starts it again from the new mask.
#[derive(Message, Debug, Clone, Copy, PartialEq, Reflect)]
#[reflect(Debug, PartialEq)]
pub struct GrassEdit {
    /// Where.
    pub shape: GrassBrushShape,
    /// What.
    pub op: GrassEditOp,
    /// How far the edit goes, 0 to 1.
    ///
    /// At 1, the default, it sets the grass to the op's target inside the
    /// shape, fading to the grass as it was across the soft edge. Doing it
    /// again changes nothing, so an edit sent every frame while the mouse
    /// drags paints a stroke the way a paint program does.
    ///
    /// Below 1 it moves the grass that share of the range from none to full,
    /// and repeated edits add up: a path worn in a little at every passing,
    /// a patch burnt over several seconds. Steps smaller than the mask's
    /// 1/255 still add up.
    pub strength: f32,
    /// The outer share of the shape that fades out: 0 for a hard edge, 1 to
    /// fade from the centre. A share of the radius, or of the smaller half
    /// size of a rectangle. 0.3 by default.
    pub softness: f32,
    /// Only this field. `None` (the default) edits every field under the
    /// shape, so overlapping fields are cleared together.
    pub field: Option<Entity>,
}

impl GrassEdit {
    /// `op` over `shape`, at full strength and the default softness.
    #[must_use]
    pub fn new(shape: GrassBrushShape, op: GrassEditOp) -> Self {
        Self {
            shape,
            op,
            strength: 1.0,
            softness: DEFAULT_SOFTNESS,
            field: None,
        }
    }

    /// Removes the grass: paths, plots, burnt ground.
    #[must_use]
    pub fn clear(shape: GrassBrushShape) -> Self {
        Self::new(shape, GrassEditOp::Density(0.0))
    }

    /// Grows grass, or thins it, to `density` (0 to 1).
    #[must_use]
    pub fn paint(shape: GrassBrushShape, density: f32) -> Self {
        Self::new(shape, GrassEditOp::Density(density))
    }

    /// Cuts the grass to `height`, a share of its full height (0 to 1).
    #[must_use]
    pub fn mow(shape: GrassBrushShape, height: f32) -> Self {
        Self::new(shape, GrassEditOp::Mow(height))
    }

    /// Puts back the grass the field had before any edit.
    #[must_use]
    pub fn restore(shape: GrassBrushShape) -> Self {
        Self::new(shape, GrassEditOp::Restore)
    }

    /// With [`GrassEdit::strength`] `strength`.
    #[must_use]
    pub fn with_strength(self, strength: f32) -> Self {
        Self { strength, ..self }
    }

    /// With [`GrassEdit::softness`] `softness`.
    #[must_use]
    pub fn with_softness(self, softness: f32) -> Self {
        Self { softness, ..self }
    }

    /// On `field` only.
    #[must_use]
    pub fn on_field(self, field: Entity) -> Self {
        Self {
            field: Some(field),
            ..self
        }
    }
}

/// Edits the grass wherever the entity goes: a lawnmower, a plough, a fire,
/// a path worn in by feet.
///
/// ```no_run
/// # use bevy::prelude::*;
/// # use bevy_grass::prelude::*;
/// # fn setup(mut commands: Commands) {
/// // A mower: cuts a 1.2 m swath to a fifth of the grass's height.
/// commands.spawn((
///     Transform::default(),
///     GrassBrush::new(GrassEditOp::Mow(0.2), 0.6),
/// ));
/// // Villagers: each passing wears the grass down a little, so paths form
/// // where they walk most.
/// commands.spawn((
///     Transform::default(),
///     GrassBrush::new(GrassEditOp::Density(0.0), 0.4).with_rate(0.3),
/// ));
/// # }
/// ```
///
/// Without a [`rate`](GrassBrush::rate) the brush works at once, along the
/// whole way it moved since the last frame, so a fast brush leaves no gaps; a
/// brush that stands still does nothing more. With one, it works on the disc
/// under it for as long as it stays, a fire burning in place.
///
/// A brush edits at the entity's position, at any height: remove the
/// component (or the entity) to stop it. Add a [`GrassBrushFootprint`] to edit
/// a rectangle instead of a disc.
#[derive(Component, Debug, Clone, Copy, PartialEq, Reflect)]
#[reflect(Component, Default, Debug, PartialEq)]
#[require(Transform)]
pub struct GrassBrush {
    /// What it does.
    pub op: GrassEditOp,
    /// Its radius, in metres.
    pub radius: f32,
    /// The outer share of the radius that fades out (see
    /// [`GrassEdit::softness`]).
    pub softness: f32,
    /// How fast it works, as the most it moves the grass per second, 0 to 1
    /// of the range from none to full: 0.25 takes four seconds. `None` (the
    /// default) works at once.
    pub rate: Option<f32>,
}

impl GrassBrush {
    /// A brush of `radius` metres doing `op` at once.
    #[must_use]
    pub fn new(op: GrassEditOp, radius: f32) -> Self {
        Self {
            op,
            radius,
            softness: DEFAULT_SOFTNESS,
            rate: None,
        }
    }

    /// The same brush, working at `rate` per second.
    #[must_use]
    pub fn with_rate(self, rate: f32) -> Self {
        Self {
            rate: Some(rate),
            ..self
        }
    }
}

impl Default for GrassBrush {
    /// Clears a disc of half a metre.
    fn default() -> Self {
        Self::new(GrassEditOp::Density(0.0), 0.5)
    }
}

/// The rectangle a [`GrassBrush`] edits instead of a disc: a mower's deck, a
/// tank's tracks, a plough, a bulldozer's blade.
///
/// ```no_run
/// # use bevy::prelude::*;
/// # use bevy_grass::prelude::*;
/// # fn setup(mut commands: Commands) {
/// // A ride-on mower whose deck is 1.4 m wide (along its X) and 0.8 m long.
/// commands.spawn((
///     Transform::default(),
///     GrassBrush::new(GrassEditOp::Mow(0.2), 0.0),
///     GrassBrushFootprint::new(Vec2::new(0.7, 0.4)),
/// ));
/// # }
/// ```
///
/// The rectangle lies under the entity: `half_size` either side of `center`
/// along the entity's own X and Z, turned and scaled with its transform. A
/// tilt is left out, so the rectangle stays flat on the ground. Working at
/// once, the brush covers everything the rectangle slid over since the last
/// frame, however fast it goes; with a rate, the rectangle where it stands.
///
/// For a vehicle that should leave two tracks rather than one wide strip, give
/// each track a child entity with its own brush and footprint.
///
/// With a footprint, the brush's `radius` is not used; its op, softness and
/// rate are, and the softness is a share of the smaller half size.
#[derive(Component, Debug, Clone, Copy, PartialEq, Reflect)]
#[reflect(Component, Default, Debug, PartialEq)]
#[require(GrassBrush)]
pub struct GrassBrushFootprint {
    /// The rectangle's centre, in the entity's local space.
    pub center: Vec3,
    /// Half its size along the entity's local X and Z, in local units (the
    /// transform's scale applies).
    pub half_size: Vec2,
}

impl GrassBrushFootprint {
    /// A rectangle `half_size` either side of the entity's origin.
    #[must_use]
    pub fn new(half_size: Vec2) -> Self {
        Self {
            center: Vec3::ZERO,
            half_size,
        }
    }
}

impl Default for GrassBrushFootprint {
    /// A 1 m square under the entity.
    fn default() -> Self {
        Self::new(Vec2::splat(0.5))
    }
}

/// Grass that grows back: on a field, edits that cleared, thinned or mowed it
/// undo themselves over time, back to the field's own mask.
///
/// ```no_run
/// # use bevy::prelude::*;
/// # use bevy_grass::prelude::*;
/// # fn setup(mut commands: Commands, mut styles: ResMut<Assets<GrassStyle>>) {
/// commands.spawn((
///     GrassField::new(Vec2::splat(64.0), GrassStyle::LAWN, &mut styles),
///     // Mowed lawn is back to full height in about a minute; paths through
///     // it never close.
///     GrassRegrowth { density_rate: 0.0, height_rate: 0.015 },
/// ));
/// # }
/// ```
///
/// Regrowth only raises the grass: grass painted above the field's mask
/// stays. It runs on the game's clock, so it pauses when the game does.
#[derive(Component, Debug, Clone, Copy, PartialEq, Reflect)]
#[reflect(Component, Default, Debug, PartialEq)]
pub struct GrassRegrowth {
    /// Density regained per second, 0 to 1 of the range from bare to full:
    /// 0.02, the default, regrows bare ground in 50 s. 0 keeps cleared ground
    /// bare.
    pub density_rate: f32,
    /// Height regained per second, the same way. 0.02 by default.
    pub height_rate: f32,
}

impl Default for GrassRegrowth {
    fn default() -> Self {
        Self {
            density_rate: 0.02,
            height_rate: 0.02,
        }
    }
}

/// The grass at one point of one field, from [`GrassSampler`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GrassSample {
    /// The field's entity.
    pub field: Entity,
    /// How much of the field's grass grows here, 0 (bare) to 1 (its full
    /// density): its density mask with every edit, and the style's bare
    /// patches. What is there, not what a camera draws, so not thinned for
    /// distance or quality.
    pub density: f32,
    /// Blades per square metre here: the style's density (with the field's
    /// `density_scale`) times `density`.
    pub blades_per_m2: f32,
    /// How tall the grass is here, as a share of its full height: the mask's
    /// height with every edit. Mowed grass is below 1.
    pub height_scale: f32,
    /// A typical blade's height here, in metres: midway between the style's
    /// shortest and tallest blade (with the field's `height_scale`), times
    /// `height_scale`.
    pub height: f32,
    /// The tallest blade of any of the field's species here, in metres.
    pub tallest: f32,
}

/// Reads the grass on the CPU: how much grows at a point, and how tall, with
/// every edit. For gameplay: hiding in tall grass, footstep sounds, where
/// animals graze, whether a fire can spread.
///
/// ```no_run
/// # use bevy::prelude::*;
/// # use bevy_grass::prelude::*;
/// # #[derive(Component)] struct Player;
/// # #[derive(Component)] struct Hidden;
/// fn hide_in_tall_grass(
///     grass: GrassSampler,
///     player: Single<(Entity, &Transform), With<Player>>,
///     mut commands: Commands,
/// ) {
///     let (entity, transform) = *player;
///     let cover = grass.sample(transform.translation.xz());
///     if cover.is_some_and(|grass| grass.density > 0.5 && grass.height > 1.0) {
///         commands.entity(entity).insert(Hidden);
///     } else {
///         commands.entity(entity).remove::<Hidden>();
///     }
/// }
/// ```
///
/// A sample costs about a microsecond per field, whatever the field's size.
/// It reads the fields as the last [`GrassSystems::Rebuild`] left them and
/// the edits as of the last [`GrassSystems::Edit`]: a system ordered after
/// `GrassSystems::Edit` sees this frame's edits.
#[derive(SystemParam)]
pub struct GrassSampler<'w> {
    tables: Res<'w, FieldTables>,
    edits: Res<'w, EditLayers>,
}

impl std::fmt::Debug for GrassSampler<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("GrassSampler")
            .field("fields", &self.tables.slots.len())
            .field("edited", &self.edits.layers.len())
            .finish()
    }
}

impl GrassSampler<'_> {
    /// The grass at `point` (world X and Z), of the field with the tallest
    /// grass there if several overlap. `None` outside every field that is
    /// drawn (a hidden field has no grass).
    #[must_use]
    pub fn sample(&self, point: Vec2) -> Option<GrassSample> {
        let cover = |sample: &GrassSample| {
            if sample.density > 0.0 {
                sample.height
            } else {
                0.0
            }
        };
        self.samples(point)
            .max_by(|a, b| cover(a).total_cmp(&cover(b)))
    }

    /// The grass at `point` (world X and Z) in each field there.
    pub fn samples(&self, point: Vec2) -> impl Iterator<Item = GrassSample> + '_ {
        self.tables
            .slots
            .iter()
            .filter_map(move |slot| self.sample_field(slot, point))
    }

    /// `field`'s density mask as it is now, edits and all, as an RGBA image
    /// (red density, green height, linear): save it with a game, or keep what
    /// was painted in the game as the field's new
    /// [`density_map`](GrassField::density_map). `None` for a field with no
    /// mask and no edits, or not drawn yet.
    #[must_use]
    pub fn mask_image(&self, field: Entity) -> Option<Image> {
        let (words, offset, size) = if let Some(layer) = self.edits.layers.get(&field) {
            (layer.words.as_slice(), 0, layer.size)
        } else {
            let slot = self.tables.slots.iter().find(|slot| slot.entity == field)?;
            let entry = self.tables.fields.get(slot.first)?;
            if entry.placement.z == NO_MAP {
                return None;
            }
            (
                self.tables.masks.as_slice(),
                entry.placement.z,
                entry.map_sizes.xy(),
            )
        };
        let mut data = Vec::with_capacity((size.x * size.y * 4) as usize);
        for index in 0..size.x * size.y {
            let (red, green) = texel(words, offset + index);
            data.extend_from_slice(&[red, green, 0, 255]);
        }
        Some(Image::new(
            Extent3d {
                width: size.x,
                height: size.y,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            data,
            TextureFormat::Rgba8Unorm,
            RenderAssetUsages::default(),
        ))
    }

    fn sample_field(&self, slot: &FieldSlot, point: Vec2) -> Option<GrassSample> {
        let entry = self.tables.fields.get(slot.first)?;
        if entry.access.x == 0 {
            return None;
        }
        let size = entry.extent.xy();
        let local = entry.local_of(point);
        if !(local.cmpge(Vec2::ZERO).all() && local.cmplt(size).all()) {
            return None;
        }
        let uv = local / size;
        let mask = match self.edits.layers.get(&slot.entity) {
            Some(layer) => sample_mask(&layer.words, 0, layer.size, uv),
            None if entry.placement.z == NO_MAP => Vec2::ONE,
            None => sample_mask(
                &self.tables.masks,
                entry.placement.z,
                entry.map_sizes.xy(),
                uv,
            ),
        };
        // Bare patches, as `evaluate` in `generate.wesl` cuts them.
        let bare = if entry.tufts.z >= 0.0 {
            let threshold = entry.tufts.z;
            let noise = bare_noise(gust_noise_table(), point, entry.tufts.w);
            smoothstep(threshold - 0.05, threshold + 0.05, noise)
        } else {
            1.0
        };
        // No blade grows where the ground has a hole: past a mesh's edge, or
        // where a height function gave no height.
        let ground = if entry.placement.w != NO_MAP
            && ground_is_hole(
                &self.tables.heights,
                entry.placement.w,
                entry.map_sizes.zw(),
                uv,
            ) {
            0.0
        } else {
            1.0
        };
        let density = mask.x * bare * ground;
        Some(GrassSample {
            field: slot.entity,
            density,
            blades_per_m2: entry.shape.w * density,
            height_scale: mask.y,
            height: f32::midpoint(entry.shape.x, entry.shape.y) * mask.y,
            tallest: entry.mix.w * mask.y,
        })
    }
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A bilinear sample of a packed mask: density and height. Twin of
/// `sample_mask` in `generate.wesl`.
fn sample_mask(words: &[u32], offset: u32, size: UVec2, uv: Vec2) -> Vec2 {
    let texels = size.as_vec2();
    let p = (uv * texels - 0.5).clamp(Vec2::ZERO, texels - 1.0);
    let base = p.floor().as_uvec2();
    let t = p - p.floor();
    let next = (base + 1).min(size - 1);
    let at = |x: u32, y: u32| {
        let (red, green) = texel(words, offset + y * size.x + x);
        Vec2::new(f32::from(red), f32::from(green)) / 255.0
    };
    let top = at(base.x, base.y).lerp(at(next.x, base.y), t.x);
    let bottom = at(base.x, next.y).lerp(at(next.x, next.y), t.x);
    top.lerp(bottom, t.y)
}

/// Whether the ground of a heightmap has a hole at `uv`: any of the four
/// texels the bilinear sample reads is one. The hole test of `ground_height`
/// in `generate.wesl`, its twin.
fn ground_is_hole(heights: &[f32], offset: u32, size: UVec2, uv: Vec2) -> bool {
    let texels = size.as_vec2();
    let p = (uv * texels - 0.5).clamp(Vec2::ZERO, texels - 1.0);
    let base = p.floor().as_uvec2();
    let next = (base + 1).min(size - 1);
    let at = |x: u32, y: u32| heights[(offset + y * size.x + x) as usize];
    [
        at(base.x, base.y),
        at(next.x, base.y),
        at(base.x, next.y),
        at(next.x, next.y),
    ]
    .into_iter()
    .any(|height| height < HOLE * 0.5)
}

/// Texel `index` of a packed mask: red and green.
fn texel(words: &[u32], index: u32) -> (u8, u8) {
    let half = words[(index / 2) as usize] >> ((index % 2) * 16);
    ((half & 0xff) as u8, ((half >> 8) & 0xff) as u8)
}

fn set_texel(words: &mut [u32], index: u32, (red, green): (u8, u8)) {
    let shift = (index % 2) * 16;
    let word = &mut words[(index / 2) as usize];
    *word = (*word & !(0xffff << shift)) | ((u32::from(red) | (u32::from(green) << 8)) << shift);
}

/// One field's mask as edited, in the packed form the GPU reads.
#[derive(Debug)]
pub(crate) struct Layer {
    pub(crate) size: UVec2,
    pub(crate) words: Vec<u32>,
    /// The mask the edits started from; `None` is full grass everywhere.
    original: Option<Arc<Vec<u32>>>,
    /// The density map the layer was made from, if the field has one.
    source: Option<AssetId<Image>>,
    /// Each row's changed texels, first and one past the last, not yet
    /// uploaded; `start >= end` for a clean row.
    dirty: Vec<(u32, u32)>,
    any_dirty: bool,
    /// One bit per tile of `TILE`² texels that may hold grass below the
    /// original, for regrowth.
    lowered: Vec<u64>,
    any_lowered: bool,
    /// Seconds of game time the layer has seen, for regrowth.
    clock: f64,
    /// When each tile last regrew, on `clock`.
    grown: Vec<f64>,
    /// The word of `lowered` regrowth goes on from next frame.
    cursor: usize,
}

/// A clean row.
const CLEAN: (u32, u32) = (u32::MAX, 0);

impl Layer {
    fn new(size: UVec2, original: Option<Arc<Vec<u32>>>, source: Option<AssetId<Image>>) -> Self {
        let words = match &original {
            Some(words) => words.as_ref().clone(),
            None => vec![u32::MAX; packed::mask_words(size.x * size.y) as usize],
        };
        let tiles = size.x.div_ceil(TILE) * size.y.div_ceil(TILE);
        Self {
            size,
            words,
            original,
            source,
            dirty: vec![CLEAN; size.y as usize],
            any_dirty: false,
            lowered: vec![0; tiles.div_ceil(64) as usize],
            any_lowered: false,
            clock: 0.0,
            grown: vec![0.0; tiles as usize],
            cursor: 0,
        }
    }

    fn original(&self, index: u32) -> (u8, u8) {
        self.original
            .as_ref()
            .map_or((255, 255), |words| texel(words, index))
    }

    fn tile_of(&self, x: u32, y: u32) -> u32 {
        (y / TILE) * self.size.x.div_ceil(TILE) + x / TILE
    }

    /// Records that texel (`x`, `y`) changed, and whether it is now below
    /// the original.
    fn mark(&mut self, x: u32, y: u32, lowered: bool) {
        let row = &mut self.dirty[y as usize];
        *row = (row.0.min(x), row.1.max(x + 1));
        self.any_dirty = true;
        if lowered {
            let tile = self.tile_of(x, y);
            let word = &mut self.lowered[(tile / 64) as usize];
            if *word & (1 << (tile % 64)) == 0 {
                // It grows from now, not from whenever it last grew.
                self.grown[tile as usize] = self.clock;
            }
            *word |= 1 << (tile % 64);
            self.any_lowered = true;
        }
    }

    /// Finds every tile below the original again, for regrowth whose rates
    /// changed: a channel that did not regrow dropped its tiles.
    fn find_lowered(&mut self) {
        self.lowered.fill(0);
        self.any_lowered = false;
        let texels = self.size.x * self.size.y;
        for word in 0..self.words.len() {
            // Most of a field is as it was: two texels at a time.
            let same = match &self.original {
                Some(original) => self.words[word] == original[word],
                None => self.words[word] == u32::MAX,
            };
            if same {
                continue;
            }
            for index in (word as u32 * 2..word as u32 * 2 + 2).filter(|i| *i < texels) {
                let (now, original) = (texel(&self.words, index), self.original(index));
                if now.0 < original.0 || now.1 < original.1 {
                    let tile = self.tile_of(index % self.size.x, index / self.size.x);
                    self.lowered[(tile / 64) as usize] |= 1 << (tile % 64);
                    self.any_lowered = true;
                }
            }
        }
        self.grown.fill(self.clock);
    }

    /// Applies one edit; returns the texels changed.
    fn apply(&mut self, frame: &FieldFrame, edit: &Edit, seed: u32) -> u64 {
        if frame.size.cmple(Vec2::ZERO).any() || edit.strength <= 0.0 {
            return 0;
        }
        let texel_size = frame.size / self.size.as_vec2();
        let (low, high) = edit.shape.bounds();
        // The brush's box in the field's own axes, then the texels whose
        // centres it holds.
        let corners = [
            low,
            high,
            Vec2::new(low.x, high.y),
            Vec2::new(high.x, low.y),
        ]
        .map(|corner| frame.local_of(corner));
        let local_low = corners.iter().copied().fold(Vec2::MAX, Vec2::min);
        let local_high = corners.iter().copied().fold(Vec2::MIN, Vec2::max);
        let start = (local_low / texel_size - 0.5)
            .ceil()
            .max(Vec2::ZERO)
            .as_uvec2();
        let end = ((local_high / texel_size - 0.5).floor() + 1.0)
            .max(Vec2::ZERO)
            .as_uvec2()
            .min(self.size);
        let mut changed = 0;
        for y in start.y..end.y {
            for x in start.x..end.x {
                let local = (Vec2::new(x as f32, y as f32) + 0.5) * texel_size;
                let weight = edit.weight(frame.world_of(local));
                if weight <= 0.0 {
                    continue;
                }
                let index = y * self.size.x + x;
                let now = texel(&self.words, index);
                let original = self.original(index);
                let how = if edit.strength >= 1.0 {
                    Move::Set(weight)
                } else {
                    Move::Step(edit.strength * weight * 255.0, unit(hash2(index, seed)))
                };
                let next = edit.op.apply(now, original, how);
                if next != now {
                    set_texel(&mut self.words, index, next);
                    self.mark(x, y, next.0 < original.0 || next.1 < original.1);
                    changed += 1;
                }
            }
        }
        changed
    }

    /// Raises lowered texels toward the original at `rates` (density and
    /// height, texel units per second), visiting tiles from where it left off
    /// until `budget` texels are visited. Returns the texels changed and
    /// visited. A channel whose rate is 0 does not regrow, and does not keep
    /// its tile on the list.
    fn regrow(&mut self, rates: Vec2, budget: u32, seed: u32) -> (u64, u32) {
        let columns = self.size.x.div_ceil(TILE);
        let words = self.lowered.len();
        let (mut changed, mut visited) = (0, 0);
        for turn in 0..words {
            let word = (self.cursor + turn) % words;
            if visited >= budget {
                self.cursor = word;
                break;
            }
            let mut bits = self.lowered[word];
            while bits != 0 {
                let bit = bits.trailing_zeros();
                bits &= bits - 1;
                let tile = word as u32 * 64 + bit;
                let elapsed = self.clock - self.grown[tile as usize];
                if elapsed < REGROW_INTERVAL {
                    continue;
                }
                self.grown[tile as usize] = self.clock;
                let step = rates * elapsed as f32;
                let (tile_x, tile_y) = (tile % columns, tile / columns);
                let mut below = false;
                for y in tile_y * TILE..((tile_y + 1) * TILE).min(self.size.y) {
                    for x in tile_x * TILE..((tile_x + 1) * TILE).min(self.size.x) {
                        let index = y * self.size.x + x;
                        let now = texel(&self.words, index);
                        let original = self.original(index);
                        let dither = unit(hash2(index, seed));
                        let grow = |value: u8, goal: u8, step: f32| {
                            if value < goal && step > 0.0 {
                                toward(value, f32::from(goal), step, dither)
                            } else {
                                value
                            }
                        };
                        let next = (
                            grow(now.0, original.0, step.x),
                            grow(now.1, original.1, step.y),
                        );
                        if next != now {
                            set_texel(&mut self.words, index, next);
                            self.mark(x, y, false);
                            changed += 1;
                        }
                        below |= (rates.x > 0.0 && next.0 < original.0)
                            || (rates.y > 0.0 && next.1 < original.1);
                        visited += 1;
                    }
                }
                if !below {
                    self.lowered[word] &= !(1 << bit);
                }
            }
        }
        self.any_lowered = self.lowered.iter().any(|word| *word != 0);
        (changed, visited)
    }

    /// Moves the changed rows into `patches`, for a layer whose first texel
    /// is word `base` of the mask table.
    fn drain_into(&mut self, base: u32, patches: &mut PatchList) {
        let width = self.size.x;
        for y in 0..self.size.y {
            let (start, end) = std::mem::replace(&mut self.dirty[y as usize], CLEAN);
            if start >= end {
                continue;
            }
            let (first_texel, last_texel) = (y * width + start, y * width + end - 1);
            let (first, last) = (first_texel / 2, last_texel / 2 + 1);
            patches.push(base, first, &self.words[first as usize..last as usize]);
        }
        self.any_dirty = false;
    }
}

/// How an edit moves a texel toward its target.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Move {
    /// At full strength: to the edit's profile, which is the target where
    /// the weight is 1 and fades to no change where it is 0. Never further
    /// than the profile, and never back, so the same edit twice is the same
    /// edit once: a brush's strokes overlap from frame to frame, and a soft
    /// edge stays as soft as it was drawn.
    Set(f32),
    /// Weaker: a step of this many texel units, rounded by the dither (see
    /// `toward`). Steps add up.
    Step(f32, f32),
}

impl Move {
    fn apply(self, value: u8, goal: f32) -> u8 {
        match self {
            Self::Set(weight) => limit(value, goal, weight),
            Self::Step(step, dither) => toward(value, goal, step, dither),
        }
    }
}

/// `value` brought within the profile of an edit toward `goal` at `weight`
/// (texel units): lowering, a ceiling that falls from the top of the range
/// at weight 0 to the goal at weight 1; raising, a floor that rises from the
/// bottom to the goal.
fn limit(value: u8, goal: f32, weight: f32) -> u8 {
    let now = f32::from(value);
    let goal = goal.clamp(0.0, 255.0);
    let next = if now > goal {
        now.min((goal + (255.0 - goal) * (1.0 - weight)).round())
    } else {
        now.max((goal * weight).round())
    };
    next as u8
}

/// `value` moved toward `goal` (both in texel units) by at most `step`,
/// rounded at random in proportion, so steps smaller than one unit still
/// add up over many frames. Once within `step` it lands on the goal, so a
/// texel at its target stays put instead of flickering a unit up and down.
fn toward(value: u8, goal: f32, step: f32, dither: f32) -> u8 {
    let now = f32::from(value);
    let gap = goal - now;
    let next = if gap.abs() <= step {
        goal.round()
    } else if gap > 0.0 {
        (now + step + dither).floor().min(goal.round())
    } else {
        (now - step + dither).floor().max(goal.round())
    };
    next.clamp(0.0, 255.0) as u8
}

impl GrassEditOp {
    /// A texel's red and green after this op moved them `how`, given what
    /// they were before any edit.
    fn apply(self, (red, green): (u8, u8), original: (u8, u8), how: Move) -> (u8, u8) {
        let goal = |share: f32| share.clamp(0.0, 1.0) * 255.0;
        match self {
            Self::Density(density) => (how.apply(red, goal(density)), green),
            Self::Height(height) => (red, how.apply(green, goal(height))),
            Self::Mow(height) if f32::from(green) > goal(height) => {
                (red, how.apply(green, goal(height)))
            }
            Self::Mow(_) => (red, green),
            Self::Restore => (
                how.apply(red, f32::from(original.0)),
                how.apply(green, f32::from(original.1)),
            ),
        }
    }

    /// Whether the op's numbers can be used: a NaN would clear the grass.
    fn is_finite(self) -> bool {
        match self {
            Self::Density(value) | Self::Height(value) | Self::Mow(value) => value.is_finite(),
            Self::Restore => true,
        }
    }
}

/// A shape ready to weigh texels.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Shape {
    Disc {
        center: Vec2,
        radius: f32,
    },
    Segment {
        from: Vec2,
        to: Vec2,
        radius: f32,
    },
    Rect {
        center: Vec2,
        half: Vec2,
        axis: Vec2,
    },
    /// A `Rect` slid from `from` to `to` without turning: what a box brush
    /// covered between two frames.
    Slid {
        from: Vec2,
        to: Vec2,
        half: Vec2,
        axis: Vec2,
    },
}

impl Shape {
    fn of(shape: GrassBrushShape) -> Option<Self> {
        let shape = match shape {
            GrassBrushShape::Circle { center, radius } => Self::Disc {
                center,
                radius: radius.max(0.0),
            },
            GrassBrushShape::Stroke { from, to, radius } => Self::Segment {
                from,
                to,
                radius: radius.max(0.0),
            },
            GrassBrushShape::Rect {
                center,
                half_size,
                yaw,
            } => {
                let (sin, cos) = yaw.sin_cos();
                Self::Rect {
                    center,
                    half: half_size.abs(),
                    axis: Vec2::new(cos, -sin),
                }
            }
        };
        shape.is_finite().then_some(shape)
    }

    /// Whether every number is finite: a NaN would edit nothing, or
    /// everything.
    fn is_finite(&self) -> bool {
        let finite = |v: Vec2| v.is_finite();
        match *self {
            Self::Disc { center, radius } => finite(center) && radius.is_finite(),
            Self::Segment { from, to, radius } => finite(from) && finite(to) && radius.is_finite(),
            Self::Rect { center, half, axis } => finite(center) && finite(half) && finite(axis),
            Self::Slid {
                from,
                to,
                half,
                axis,
            } => finite(from) && finite(to) && finite(half) && finite(axis),
        }
    }

    /// The world box around the shape.
    fn bounds(&self) -> (Vec2, Vec2) {
        match *self {
            Self::Disc { center, radius } => (center - radius, center + radius),
            Self::Segment { from, to, radius } => (from.min(to) - radius, from.max(to) + radius),
            Self::Rect { center, half, axis } => {
                let reach = axis.abs() * half.x + axis.perp().abs() * half.y;
                (center - reach, center + reach)
            }
            Self::Slid {
                from,
                to,
                half,
                axis,
            } => {
                let reach = axis.abs() * half.x + axis.perp().abs() * half.y;
                (from.min(to) - reach, from.max(to) + reach)
            }
        }
    }

    /// How far `point` is inside the shape (negative outside), and how far
    /// its edge fades at softness 1.
    fn depth(&self, point: Vec2) -> (f32, f32) {
        match *self {
            Self::Disc { center, radius } => (radius - point.distance(center), radius),
            Self::Segment { from, to, radius } => {
                let along = to - from;
                let length = along.length_squared();
                let t = if length > 0.0 {
                    (point - from).dot(along) / length
                } else {
                    0.0
                };
                let nearest = from + along * t.clamp(0.0, 1.0);
                (radius - point.distance(nearest), radius)
            }
            Self::Rect { center, half, axis } => {
                let offset = point - center;
                let q = Vec2::new(offset.dot(axis).abs(), offset.dot(axis.perp()).abs());
                ((half - q).min_element(), half.min_element())
            }
            Self::Slid {
                from,
                to,
                half,
                axis,
            } => (
                slid_depth(point, (from, to), half, axis),
                half.min_element(),
            ),
        }
    }
}

/// How deep `point` is in a rectangle (`half` along `axis` and its
/// perpendicular) at its deepest as the rectangle slides from `ends.0` to
/// `ends.1`.
///
/// Along the slide, the depth from each pair of sides is a tent in the slide's
/// fraction `t`, and the depth is the smaller of the two: its highest point is
/// at an end, at a tent's peak, or where the two tents cross. Those are all
/// tried, so the answer is exact.
fn slid_depth(point: Vec2, (from, to): (Vec2, Vec2), half: Vec2, axis: Vec2) -> f32 {
    let local = |v: Vec2| Vec2::new(v.dot(axis), v.dot(axis.perp()));
    let (q, d) = (local(point - from), local(to - from));
    let depth = |t: f32| (half - (q - d * t).abs()).min_element();
    let mut deepest = depth(0.0).max(depth(1.0));
    let mut consider = |t: f32| {
        if t > 0.0 && t < 1.0 {
            deepest = deepest.max(depth(t));
        }
    };
    // The peaks: a side pair's middle passes the point.
    if d.x != 0.0 {
        consider(q.x / d.x);
    }
    if d.y != 0.0 {
        consider(q.y / d.y);
    }
    // The crossings: half.x - s(q.x - t d.x) = half.y - r(q.y - t d.y).
    for s in [-1.0, 1.0] {
        for r in [-1.0, 1.0] {
            let slope = s * d.x - r * d.y;
            if slope != 0.0 {
                consider((half.y - r * q.y - half.x + s * q.x) / slope);
            }
        }
    }
    deepest
}

/// One edit as applied: a `GrassEdit`, or a brush's work this frame.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Edit {
    shape: Shape,
    op: GrassEditOp,
    strength: f32,
    softness: f32,
    field: Option<Entity>,
}

impl Edit {
    fn of(edit: &GrassEdit) -> Option<Self> {
        let usable = edit.op.is_finite() && edit.strength.is_finite();
        Some(Self {
            shape: Shape::of(edit.shape).filter(|_| usable)?,
            op: edit.op,
            strength: edit.strength.clamp(0.0, 1.0),
            softness: finite_share(edit.softness),
            field: edit.field,
        })
    }

    /// A brush's work this frame over `shape`.
    fn brush(shape: Shape, brush: &GrassBrush, strength: f32) -> Option<Self> {
        let usable = shape.is_finite() && brush.op.is_finite() && strength.is_finite();
        usable.then(|| Self {
            shape,
            op: brush.op,
            strength: strength.clamp(0.0, 1.0),
            softness: finite_share(brush.softness),
            field: None,
        })
    }

    /// How much of the edit a point gets: 1 inside, fading to 0 across the
    /// soft edge.
    fn weight(&self, point: Vec2) -> f32 {
        let (depth, extent) = self.shape.depth(point);
        if depth < 0.0 {
            return 0.0;
        }
        let band = self.softness * extent;
        if band <= 0.0 {
            1.0
        } else {
            smoothstep(0.0, band, depth)
        }
    }

    /// Whether the shape reaches into the field.
    fn touches(&self, frame: &FieldFrame) -> bool {
        let (low, high) = self.shape.bounds();
        let corners = [
            low,
            high,
            Vec2::new(low.x, high.y),
            Vec2::new(high.x, low.y),
        ]
        .map(|corner| frame.local_of(corner));
        let local_low = corners.iter().copied().fold(Vec2::MAX, Vec2::min);
        let local_high = corners.iter().copied().fold(Vec2::MIN, Vec2::max);
        local_high.cmpge(Vec2::ZERO).all() && local_low.cmple(frame.size).all()
    }
}

fn finite_share(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// Every field's edit layer, and the edits waiting for a field's mask.
#[derive(Resource, Debug, Default)]
pub(crate) struct EditLayers {
    pub(crate) layers: HashMap<Entity, Layer>,
    waiting: HashMap<Entity, Vec<Edit>>,
    /// Some layer changed since the tables last joined the masks from them
    /// (see `images_for`).
    pub(crate) ahead: bool,
    /// Counts passes, for the random rounding.
    pass: u32,
    /// Fields already warned about for too many waiting edits.
    warned: bool,
}

impl EditLayers {
    /// `entity`'s layer, if it was made from the mask `field` has now.
    pub(crate) fn layer_of(&self, entity: Entity, field: &GrassField) -> Option<&Layer> {
        let source = field.density_map.as_ref().map(Handle::id);
        self.layers
            .get(&entity)
            .filter(|layer| layer.source == source)
    }

    /// `entity`'s layer, made now if it has none, or `None` while its mask
    /// has not loaded.
    fn layer_mut(
        &mut self,
        entity: Entity,
        field: &GrassField,
        frame: &FieldFrame,
        sources: &mut Sources,
    ) -> Option<&mut Layer> {
        if self.layer_of(entity, field).is_none() {
            let layer = if let Some(handle) = &field.density_map {
                let image = sources.images.get(handle)?;
                let packed = sources.cache.mask(handle.id(), image);
                Layer::new(image.size(), Some(packed), Some(handle.id()))
            } else {
                // A mask of its own, which the tables must make room for.
                sources.dirty.mark_fields();
                let texels = (frame.size / EDIT_TEXEL).ceil().as_uvec2();
                Layer::new(
                    texels.clamp(UVec2::ONE, UVec2::splat(MAX_EDIT_TEXELS)),
                    None,
                    None,
                )
            };
            self.layers.insert(entity, layer);
        }
        self.layers.get_mut(&entity)
    }

    /// Applies `edit` to the field, or keeps it until the field's mask has
    /// loaded. Edits for a field keep their order.
    fn apply(
        &mut self,
        (entity, field, frame): (Entity, &GrassField, &FieldFrame),
        edit: &Edit,
        sources: &mut Sources,
        seed: u32,
    ) -> u64 {
        if self.waiting.contains_key(&entity) {
            self.keep(entity, *edit);
            return 0;
        }
        if let Some(layer) = self.layer_mut(entity, field, frame, sources) {
            let changed = layer.apply(frame, edit, seed);
            self.ahead |= changed > 0;
            changed
        } else {
            self.keep(entity, *edit);
            0
        }
    }

    fn keep(&mut self, entity: Entity, edit: Edit) {
        let waiting = self.waiting.entry(entity).or_default();
        if waiting.len() < MAX_WAITING {
            waiting.push(edit);
        } else if !std::mem::replace(&mut self.warned, true) {
            warn!(
                "bevy_grass: {MAX_WAITING} edits are waiting for the density map of the \
                 GrassField {entity} to load; further edits to it are dropped until it does."
            );
        }
    }

    fn seed(&mut self) -> u32 {
        self.pass = self.pass.wrapping_add(1);
        hash2(self.pass, 0xed17)
    }
}

/// What making a layer reads: the images, their packed forms, and the
/// tables, which a new mask must be added to.
struct Sources<'a> {
    images: &'a Assets<Image>,
    cache: &'a mut PackedImages,
    dirty: &'a mut TablesDirty,
}

/// Runs of the mask table to write on the GPU: each run's first word in
/// the table and its length, and all their words back to back.
#[derive(Debug, Default)]
pub(crate) struct PatchList {
    pub(crate) runs: Vec<(u32, u32)>,
    pub(crate) words: Vec<u32>,
}

impl PatchList {
    /// Adds words `first..` of a layer whose first word is `base` in the
    /// table, joined to the run before when they touch.
    fn push(&mut self, base: u32, first: u32, words: &[u32]) {
        let at = base + first;
        let end = at + words.len() as u32;
        if let Some((run_at, run_len)) = self.runs.last_mut() {
            let run_end = *run_at + *run_len;
            if at <= run_end && run_end <= end {
                // Rows of an odd width share a word; it is the same word.
                self.words
                    .extend_from_slice(&words[(run_end - at) as usize..]);
                *run_len = end - *run_at;
                return;
            }
        }
        self.runs.push((at, words.len() as u32));
        self.words.extend_from_slice(words);
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.runs.is_empty()
    }
}

/// This frame's mask writes, for the render world.
#[derive(Resource, Debug, Default, Clone)]
pub(crate) struct MaskPatches(pub(crate) Arc<PatchList>);

/// Registers the edit types and systems. Part of the tables' plugin.
pub(crate) fn plugin(app: &mut App) {
    app.add_message::<GrassEdit>()
        .init_resource::<EditLayers>()
        .init_resource::<MaskPatches>()
        .add_systems(
            PostUpdate,
            (forget_stale_layers, apply_edits, regrow)
                .chain()
                .in_set(GrassSystems::Edit),
        );
}

/// Drops the layers of despawned fields, of fields whose density map is
/// another image now, and of images that changed, which start again from the
/// new mask. Regrowth whose rates changed looks at every tile again.
pub(crate) fn forget_stale_layers(
    mut edits: ResMut<EditLayers>,
    mut removed: RemovedComponents<GrassField>,
    changed: Query<(Entity, &GrassField), Changed<GrassField>>,
    mut images: MessageReader<AssetEvent<Image>>,
    regrowth: Query<Entity, Changed<GrassRegrowth>>,
) {
    for entity in removed.read() {
        edits.layers.remove(&entity);
        edits.waiting.remove(&entity);
    }
    for (entity, field) in &changed {
        if edits.layers.contains_key(&entity) && edits.layer_of(entity, field).is_none() {
            edits.layers.remove(&entity);
        }
    }
    for event in images.read() {
        if let AssetEvent::Modified { id } | AssetEvent::Removed { id } = event {
            edits.layers.retain(|_, layer| layer.source != Some(*id));
        }
    }
    for entity in &regrowth {
        if let Some(layer) = edits.layers.get_mut(&entity) {
            layer.find_lowered();
        }
    }
}

/// Applies the edits written since the last pass, then every brush's work
/// this frame.
pub(crate) fn apply_edits(
    mut messages: MessageReader<GrassEdit>,
    brushes: Query<(
        Entity,
        &GrassBrush,
        &GlobalTransform,
        Option<&GrassBrushFootprint>,
    )>,
    fields: Query<(Entity, &GrassField, &GlobalTransform)>,
    (images, mut cache, mut dirty): (
        Res<Assets<Image>>,
        ResMut<PackedImages>,
        ResMut<TablesDirty>,
    ),
    (mut edits, mut stats): (ResMut<EditLayers>, ResMut<GrassStats>),
    time: Res<Time>,
    mut swept: Local<HashMap<Entity, (Vec2, Vec2)>>,
) {
    let started = Instant::now();
    let mut queue: Vec<Edit> = messages.read().filter_map(Edit::of).collect();
    swept.retain(|entity, _| brushes.contains(*entity));
    let elapsed = time.delta_secs();
    for (entity, brush, transform, footprint) in &brushes {
        // Where the brush is, which way it faces, and the rectangle's half
        // size if it has one.
        let (at, axis, half) = match footprint {
            Some(footprint) => {
                let scale = transform.scale();
                let (yaw, _, _) = transform.rotation().to_euler(EulerRot::YXZ);
                let (sin, cos) = yaw.sin_cos();
                (
                    transform.transform_point(footprint.center).xz(),
                    Vec2::new(cos, -sin),
                    Some(footprint.half_size.abs() * Vec2::new(scale.x, scale.z).abs()),
                )
            }
            None => (transform.translation().xz(), Vec2::X, None),
        };
        let radius = brush.radius.max(0.0);
        let here = |center| match half {
            Some(half) => Shape::Rect { center, half, axis },
            None => Shape::Disc { center, radius },
        };
        let (shape, strength) = match brush.rate {
            // Working for as long as it stays: the shape under it, by this
            // frame's share.
            Some(rate) => (here(at), rate * elapsed),
            // Working at once: everything it passed over since last frame.
            None => match swept.insert(entity, (at, axis)) {
                Some(last) if last == (at, axis) => continue,
                Some((from, _)) if from.distance(at) <= MAX_SWEEP => {
                    let shape = match half {
                        Some(half) => Shape::Slid {
                            from,
                            to: at,
                            half,
                            axis,
                        },
                        None => Shape::Segment {
                            from,
                            to: at,
                            radius,
                        },
                    };
                    (shape, 1.0)
                }
                _ => (here(at), 1.0),
            },
        };
        queue.extend(Edit::brush(shape, brush, strength));
    }

    let mut changed = 0;
    let mut applied = 0;
    if !queue.is_empty() || !edits.waiting.is_empty() {
        let mut sources = Sources {
            images: &images,
            cache: &mut cache,
            dirty: &mut dirty,
        };
        // Edits waiting for a mask go first, in their order.
        let waiting = std::mem::take(&mut edits.waiting);
        for (entity, list) in waiting {
            let Ok((_, field, transform)) = fields.get(entity) else {
                continue;
            };
            let frame = FieldFrame::of(field, transform);
            for edit in &list {
                let seed = edits.seed();
                changed += edits.apply((entity, field, &frame), edit, &mut sources, seed);
            }
            applied += list.len() as u64;
        }
        for edit in &queue {
            let seed = edits.seed();
            for (entity, field, transform) in &fields {
                if edit.field.is_some_and(|only| only != entity) {
                    continue;
                }
                let frame = FieldFrame::of(field, transform);
                if edit.touches(&frame) {
                    changed += edits.apply((entity, field, &frame), edit, &mut sources, seed);
                }
            }
        }
        applied += queue.len() as u64;
    }

    let waiting = edits.waiting.values().map(Vec::len).sum();
    let counted = EditStats {
        applied: stats.edits.applied + applied,
        texels_changed: stats.edits.texels_changed + changed,
        edited_fields: edits.layers.len(),
        waiting,
        last_apply: if applied > 0 {
            started.elapsed()
        } else {
            Duration::ZERO
        },
        ..stats.edits
    };
    if counted != stats.edits {
        stats.edits = counted;
    }
}

/// Grows the grass of fields with [`GrassRegrowth`] back toward their masks.
pub(crate) fn regrow(
    fields: Query<(Entity, &GrassRegrowth)>,
    mut edits: ResMut<EditLayers>,
    time: Res<Time>,
    mut stats: ResMut<GrassStats>,
) {
    let started = Instant::now();
    let (mut changed, mut budget) = (0, REGROW_BUDGET);
    for (entity, regrowth) in &fields {
        let seed = edits.seed();
        let Some(layer) = edits.layers.get_mut(&entity) else {
            continue;
        };
        layer.clock += time.delta_secs_f64();
        if !layer.any_lowered || budget == 0 {
            continue;
        }
        let rates = Vec2::new(
            finite_rate(regrowth.density_rate),
            finite_rate(regrowth.height_rate),
        );
        let (texels, visited) = layer.regrow(rates * 255.0, budget, seed);
        budget = budget.saturating_sub(visited);
        changed += texels;
        edits.ahead |= texels > 0;
    }
    if budget < REGROW_BUDGET {
        stats.edits.texels_changed += changed;
        stats.edits.last_apply += started.elapsed();
    }
}

fn finite_rate(rate: f32) -> f32 {
    if rate.is_finite() { rate.max(0.0) } else { 0.0 }
}

/// Moves the rows edits changed into this frame's patches, for each layer
/// the tables hold; a layer the tables do not hold yet (a new mask waiting
/// for a rebuild) keeps its rows until they do.
pub(crate) fn collect_patches(
    mut edits: ResMut<EditLayers>,
    tables: Res<FieldTables>,
    mut patches: ResMut<MaskPatches>,
    mut stats: ResMut<GrassStats>,
) {
    if !edits.layers.values().any(|layer| layer.any_dirty) {
        if !patches.0.is_empty() {
            *patches = MaskPatches::default();
        }
        return;
    }
    let mut list = PatchList::default();
    for slot in tables.slots.iter() {
        let Some(layer) = edits.layers.get_mut(&slot.entity) else {
            continue;
        };
        let Some(entry) = tables.fields.get(slot.first) else {
            continue;
        };
        if !layer.any_dirty || entry.placement.z == NO_MAP || entry.map_sizes.xy() != layer.size {
            continue;
        }
        // A mask starts on a whole word.
        layer.drain_into(entry.placement.z / 2, &mut list);
    }
    stats.edits.bytes_uploaded += list.words.len() as u64 * 4;
    patches.0 = Arc::new(list);
}

/// What edits are doing, in [`GrassStats::edits`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Reflect)]
#[reflect(Default, Debug, PartialEq)]
pub struct EditStats {
    /// Edits applied so far: each [`GrassEdit`], and each brush's work in a
    /// frame.
    pub applied: u64,
    /// Mask texels changed so far, by edits and regrowth.
    pub texels_changed: u64,
    /// Bytes of mask written to the GPU for edits so far: only the rows
    /// edits touched.
    pub bytes_uploaded: u64,
    /// Fields with edits.
    pub edited_fields: usize,
    /// Edits waiting for a field's density map to load.
    pub waiting: usize,
    /// CPU time of this frame's edits and regrowth.
    pub last_apply: Duration,
}

#[cfg(test)]
mod tests;
