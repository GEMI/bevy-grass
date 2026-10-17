//! The CPU side of trampling: where each camera's trample window sits, and
//! which interactors it stamps. The GPU side (decay, stamp, sampling) is in
//! `shaders/generate.wesl`.
//!
//! Each camera owns a square of `TEXELS × TEXELS` cells, `TEXEL_SIZE` metres
//! each, centred near what the camera looks at. Cells are addressed
//! toroidally (a world cell's slot is its coordinates modulo `TEXELS`), so
//! when the window moves no data is copied: the decay pass clears only the
//! slots whose world cell has just come into the window, and trails stay in
//! world space.

use bevy::math::{IVec2, Vec2, Vec3, Vec3Swizzles, Vec4, Vec4Swizzles};

use crate::fields::{CHUNK_SIZE, FieldTables};

/// Cells along each side of the window. The WESL twin is `TRAMPLE_TEXELS`.
pub(crate) const TEXELS: i32 = 256;
/// Side of one cell, in metres: the window covers 64 m.
pub(crate) const TEXEL_SIZE: f32 = 0.25;
/// Seconds of game time a fully flattened cell takes to spring back.
pub(crate) const RECOVERY_SECONDS: f32 = 1.5;
/// Largest interactor radius stamped, in metres; the stamp pass covers a
/// fixed footprint per interactor. The WESL twin is `MAX_PUSH_RADIUS`.
pub(crate) const MAX_PUSH_RADIUS: f32 = 2.0;

/// The window's lowest cell for a window centred on `center` (world XZ).
pub(crate) fn window_origin(center: Vec2) -> IVec2 {
    (center / TEXEL_SIZE).floor().as_ivec2() - IVec2::splat(TEXELS / 2)
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

/// The interactors a camera stamps: those whose disc reaches its window,
/// nearest to `center` first, at most `max`. Each is `(x, z, radius,
/// strength)` with the radius clamped to [`MAX_PUSH_RADIUS`].
pub(crate) fn select_interactors(all: &[Vec4], center: Vec2, max: u32) -> Vec<Vec4> {
    let half = TEXELS as f32 * TEXEL_SIZE * 0.5;
    let mut near: Vec<(f32, Vec4)> = all
        .iter()
        .filter(|i| i.z > 0.0 && i.w > 0.0)
        .map(|&i| {
            let radius = i.z.min(MAX_PUSH_RADIUS);
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
mod tests {
    use super::*;

    #[test]
    fn an_interactor_fades_out_as_it_rises_above_the_grass() {
        // No limit, or no ground to measure from: always full.
        assert_eq!(reach(None, 50.0, Some(0.0)), 1.0);
        assert_eq!(reach(Some(1.0), 50.0, None), 1.0);
        // Within the lower half of its range: full. At the top: none.
        assert_eq!(reach(Some(1.0), 0.0, Some(0.0)), 1.0);
        assert_eq!(reach(Some(1.0), 0.5, Some(0.0)), 1.0);
        assert_eq!(reach(Some(1.0), 1.0, Some(0.0)), 0.0);
        assert_eq!(reach(Some(1.0), 9.0, Some(0.0)), 0.0);
        // Between, it falls smoothly, and it never rises with height.
        let mut last = 1.0;
        for step in 0..=100 {
            let now = reach(Some(1.0), step as f32 * 0.01, Some(0.0));
            assert!((0.0..=1.0).contains(&now) && now <= last + 1e-6);
            last = now;
        }
        assert!((reach(Some(1.0), 0.75, Some(0.0)) - 0.5).abs() < 1e-6);
        // Below the ground it is as at the ground; the ground may be raised.
        assert_eq!(reach(Some(1.0), -2.0, Some(0.0)), 1.0);
        assert_eq!(reach(Some(1.0), 3.0, Some(3.0)), 1.0);
    }

    #[test]
    fn the_window_contains_its_centre_with_room_on_every_side() {
        for center in [Vec2::ZERO, Vec2::new(-3.3, 1000.1), Vec2::new(12.6, -0.124)] {
            let origin = window_origin(center).as_vec2() * TEXEL_SIZE;
            let side = TEXELS as f32 * TEXEL_SIZE;
            let into = center - origin;
            assert!(
                into.cmpge(Vec2::splat(side * 0.5 - TEXEL_SIZE)).all(),
                "{center}"
            );
            assert!(
                into.cmple(Vec2::splat(side * 0.5 + TEXEL_SIZE)).all(),
                "{center}"
            );
        }
    }

    #[test]
    fn an_orthographic_camera_centres_the_window_on_the_ground_it_sees() {
        let forward = Vec3::new(1.0, -1.0, 0.0).normalize();
        let position = Vec3::new(-40.0, 40.0, 5.0);
        let center = focus(position, forward, true, &FieldTables::default());
        assert!(center.distance(Vec2::new(0.0, 5.0)) < 1e-3, "{center}");
        // A perspective camera stands where it is.
        assert_eq!(
            focus(position, forward, false, &FieldTables::default()),
            position.xz()
        );
    }

    #[test]
    fn the_cap_keeps_the_nearest_interactors() {
        let all: Vec<Vec4> = (0..100)
            .map(|i| Vec4::new(i as f32 * 0.3, 0.0, 0.5, 1.0))
            .collect();
        let kept = select_interactors(&all, Vec2::ZERO, 10);
        assert_eq!(kept.len(), 10);
        assert!(kept.iter().all(|i| i.x < 3.0), "{kept:?}");
    }

    #[test]
    fn interactors_outside_the_window_or_without_effect_are_skipped() {
        let all = [
            Vec4::new(0.0, 0.0, 0.5, 1.0),
            Vec4::new(500.0, 0.0, 0.5, 1.0),
            Vec4::new(1.0, 1.0, 0.0, 1.0),
            Vec4::new(1.0, 1.0, 0.5, 0.0),
            Vec4::new(2.0, 2.0, 9.0, 3.0),
        ];
        let kept = select_interactors(&all, Vec2::ZERO, 64);
        assert_eq!(kept.len(), 2);
        assert!(kept.contains(&Vec4::new(2.0, 2.0, MAX_PUSH_RADIUS, 1.0)));
    }
}
