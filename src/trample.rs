//! The CPU side of trampling: where each camera's trample window sits, and
//! which interactors it stamps. The GPU side (decay, stamp, sampling) is in
//! `shaders/generate.wesl`.
//!
//! Each camera owns a square of `TEXELS × TEXELS` cells, each
//! `GrassSettings::trample_cell_size` metres across (0.25 by default, so the
//! window covers 64 m), centred near what the camera looks at. Cells are addressed
//! toroidally (a world cell's slot is its coordinates modulo `TEXELS`), so
//! when the window moves no data is copied: the decay pass clears only the
//! slots whose world cell has just come into the window, and trails stay in
//! world space.

use bevy::math::{IVec2, Mat3, Vec2, Vec3, Vec3Swizzles, Vec4, Vec4Swizzles};
use bevy::transform::components::GlobalTransform;

use crate::GrassInteractorFootprint;
use crate::GrassSettings;
use crate::fields::{CHUNK_SIZE, FieldTables};

/// Cells along each side of the window. The WESL twin is `TRAMPLE_TEXELS`.
/// It is a power of two, which is what makes a cell's slot a mask.
pub(crate) const TEXELS: i32 = 256;

/// Side of the window, in metres, for cells of `cell` metres.
pub(crate) fn window_side(cell: f32) -> f32 {
    TEXELS as f32 * cell
}

/// The window's lowest cell for a window of `cell`-metre cells centred on
/// `center` (world XZ).
pub(crate) fn window_origin(center: Vec2, cell: f32) -> IVec2 {
    (center / cell).floor().as_ivec2() - IVec2::splat(TEXELS / 2)
}

/// The point the trample window centres on, in world XZ.
///
/// A perspective camera stands in its grass, so the window centres on it. An
/// orthographic camera looks at the ground from far away (DEMO ARPG GAME's from
/// 40 m), so the window centres where its view direction meets the ground:
/// the ray is intersected with the height of the chunk below it, three times,
/// which converges on any terrain smooth over a chunk.
pub(crate) fn focus(
    position: Vec3,
    forward: Vec3,
    orthographic: bool,
    tables: &FieldTables,
) -> Vec2 {
    if !orthographic || forward.y > -1e-3 {
        return position.xz();
    }
    let mut ground = tables
        .fields
        .first()
        .map_or(0.0, |field| field.min_corner.y);
    let mut point = position.xz();
    for _ in 0..3 {
        let along = (ground - position.y) / forward.y;
        point = (position + forward * along).xz();
        if let Some(height) = chunk_height_at(tables, point) {
            ground = height;
        }
    }
    point
}

/// The middle of the height range of the chunk containing `point`, if any.
fn chunk_height_at(tables: &FieldTables, point: Vec2) -> Option<f32> {
    tables.chunks.iter().find_map(|chunk| {
        let field = tables.fields.get(chunk.ids.x as usize)?;
        let low = field.min_corner.xz() + Vec2::new(chunk.bounds.x, chunk.bounds.y);
        let inside = point.cmpge(low).all() && point.cmplt(low + Vec2::splat(CHUNK_SIZE)).all();
        inside.then(|| f32::midpoint(chunk.bounds.z, chunk.bounds.w))
    })
}

/// How strongly an interactor pushes, as a share of its own strength, given
/// the highest `ground` under it (if it is over a field) and the `max_height`
/// it may be above that: 1 at the ground, fading to 0 over the upper half of
/// the range, 1 when the interactor has no limit or no ground is known.
pub(crate) fn reach(max_height: Option<f32>, feet: f32, ground: Option<f32>) -> f32 {
    let (Some(max), Some(ground)) = (max_height, ground) else {
        return 1.0;
    };
    let max = max.max(1e-3);
    let t = (((feet - ground) - 0.5 * max) / (0.5 * max)).clamp(0.0, 1.0);
    1.0 - t * t * (3.0 - 2.0 * t)
}

/// One interactor's stamp, as `trample_stamp` reads it: three `vec4`s.
///
/// A disc is `disc` alone (x, z, radius, strength), the rest zero. A box (a
/// [`GrassInteractorFootprint`] on the ground) also has its `shape` (its axis
/// on the ground, then its core half extents along and across it) and its
/// `motion` (how it moves on the ground, 0 to 1 m/s-ish, then its rounding
/// and the margin its push fades over); its `disc` radius is how far from
/// the centre it pushes at all. A margin above 0 is what makes it a box.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Stamp {
    pub(crate) disc: Vec4,
    pub(crate) shape: Vec4,
    pub(crate) motion: Vec4,
}

impl Stamp {
    /// A disc of `radius` at world `(x, z)`.
    pub(crate) fn disc(x: f32, z: f32, radius: f32, strength: f32) -> Self {
        Self {
            disc: Vec4::new(x, z, radius, strength),
            ..Self::default()
        }
    }

    /// Whether this is a box.
    pub(crate) fn is_box(&self) -> bool {
        self.motion.w > 0.0
    }

    /// The stamp as the buffer holds it.
    pub(crate) fn gpu(self) -> [Vec4; 3] {
        [self.disc, self.shape, self.motion]
    }
}

/// How fast a box must move on the ground, in metres a second, for the grass
/// under it to lie the way it moves. Slower, the grass is pushed more and
/// more out from the box's edges, as under a box at rest.
pub(crate) const MOVING_SPEED: f32 = 0.5;

/// The stamp of a [`GrassInteractorFootprint`] under `transform`, pushing
/// with `strength`, moving on the ground by `motion` (0 to 1, see
/// [`MOVING_SPEED`]).
///
/// The box is laid on the ground by its shadow: each of its three half axes
/// is carried into the world and flattened onto XZ, and the stamp is the
/// rectangle that bounds them, along the longest one. A box turning about the
/// vertical keeps its size; one lying on its side covers its length.
pub(crate) fn box_stamp(
    transform: &GlobalTransform,
    footprint: &GrassInteractorFootprint,
    strength: f32,
    motion: Vec2,
) -> Stamp {
    let linear = Mat3::from(transform.affine().matrix3);
    let half = footprint.half_extents.abs();
    let ground = [0, 1, 2].map(|axis| {
        let reach = linear.col(axis) * finite(half[axis]);
        Vec2::new(reach.x, reach.z)
    });
    let longest = [1, 2].into_iter().fold(0, |longest, axis| {
        if ground[axis].length_squared() > ground[longest].length_squared() {
            axis
        } else {
            longest
        }
    });
    let along = ground[longest].try_normalize().unwrap_or(Vec2::X);
    let across = along.perp();
    let extent = ground
        .iter()
        .fold(Vec2::ZERO, |extent, axis| {
            extent + Vec2::new(axis.dot(along).abs(), axis.dot(across).abs())
        })
        .min(Vec2::splat(GrassSettings::MAX_INTERACTOR_RADIUS));
    let rounding = finite(footprint.rounding).clamp(0.0, extent.min_element());
    let core = extent - rounding;
    // Grass beside a box is pushed aside a little, more beside a bigger one.
    let margin = (0.5 * extent.min_element()).clamp(0.15, 0.6);
    let center = transform.transform_point(footprint.center);
    Stamp {
        disc: Vec4::new(
            center.x,
            center.z,
            core.length() + rounding + margin,
            strength,
        ),
        shape: Vec4::new(along.x, along.y, core.x, core.y),
        motion: Vec4::new(motion.x, motion.y, rounding, margin),
    }
}

/// A length that can go into a stamp: finite, or 0.
fn finite(value: f32) -> f32 {
    if value.is_finite() { value } else { 0.0 }
}

/// Where a box was on the ground and how it was moving, kept from frame to
/// frame to give its stamp a direction.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Tracked {
    pub(crate) at: Vec2,
    pub(crate) velocity: Vec2,
}

impl Tracked {
    /// This frame's position `at`, `elapsed` seconds after the last one. The
    /// velocity is smoothed over about a tenth of a second, so one uneven
    /// physics step does not turn the grass; a jump of more than 4 m in a
    /// frame is a teleport, not a motion.
    pub(crate) fn update(self, at: Vec2, elapsed: f32) -> Self {
        if elapsed <= 0.0 {
            return Self { at, ..self };
        }
        let step = at - self.at;
        if !step.is_finite() || step.length_squared() > 16.0 {
            return Self {
                at,
                velocity: Vec2::ZERO,
            };
        }
        let follow = 1.0 - (-elapsed / 0.1).exp();
        Self {
            at,
            velocity: self.velocity.lerp(step / elapsed, follow),
        }
    }

    /// The motion a stamp gets: the direction of travel, 0 at rest and 1 at
    /// [`MOVING_SPEED`] or faster.
    pub(crate) fn motion(&self) -> Vec2 {
        let speed = self.velocity.length();
        if speed <= 1e-4 {
            return Vec2::ZERO;
        }
        self.velocity / speed * (speed / MOVING_SPEED).min(1.0)
    }
}

/// The interactors a camera stamps: those that reach its window of
/// `cell`-metre cells, nearest to `center` first, at most `max`. A disc's
/// radius is clamped to `max_radius` (`GrassSettings::max_interactor_radius`);
/// a box is already clipped, in [`box_stamp`]. Strengths are clamped to 1.
pub(crate) fn select_interactors(
    all: &[Stamp],
    center: Vec2,
    max: u32,
    cell: f32,
    max_radius: f32,
) -> Vec<Stamp> {
    let half = window_side(cell) * 0.5;
    let mut near: Vec<(f32, Stamp)> = all
        .iter()
        .filter(|i| i.disc.z > 0.0 && i.disc.w > 0.0)
        .map(|&i| {
            let radius = if i.is_box() {
                i.disc.z
            } else {
                i.disc.z.min(max_radius)
            };
            let disc = Vec4::new(i.disc.x, i.disc.y, radius, i.disc.w.min(1.0));
            Stamp { disc, ..i }
        })
        .filter(|i| {
            let offset = (i.disc.xy() - center).abs();
            offset.x < half + i.disc.z && offset.y < half + i.disc.z
        })
        .map(|i| (i.disc.xy().distance_squared(center), i))
        .collect();
    let max = max as usize;
    if near.len() > max {
        near.select_nth_unstable_by(max, |a, b| a.0.total_cmp(&b.0));
        near.truncate(max);
    }
    near.into_iter().map(|(_, i)| i).collect()
}

#[cfg(test)]
mod tests;
