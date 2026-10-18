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

use bevy::math::{IVec2, Vec2, Vec3, Vec3Swizzles, Vec4, Vec4Swizzles};

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

/// The interactors a camera stamps: those whose disc reaches its window of
/// `cell`-metre cells, nearest to `center` first, at most `max`. Each is
/// `(x, z, radius, strength)` with the radius clamped to `max_radius`
/// (`GrassSettings::max_interactor_radius`).
pub(crate) fn select_interactors(
    all: &[Vec4],
    center: Vec2,
    max: u32,
    cell: f32,
    max_radius: f32,
) -> Vec<Vec4> {
    let half = window_side(cell) * 0.5;
    let mut near: Vec<(f32, Vec4)> = all
        .iter()
        .filter(|i| i.z > 0.0 && i.w > 0.0)
        .map(|&i| {
            let radius = i.z.min(max_radius);
            (Vec4::new(i.x, i.y, radius, i.w.min(1.0)), radius)
        })
        .filter(|(i, radius)| {
            let offset = (Vec2::new(i.x, i.y) - center).abs();
            offset.x < half + radius && offset.y < half + radius
        })
        .map(|(i, _)| (Vec2::new(i.x, i.y).distance_squared(center), i))
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
