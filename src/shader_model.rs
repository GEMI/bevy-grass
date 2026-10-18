//! CPU model of the deterministic placement and static tables the GPU uses.
//!
//! Every function here has a WESL twin in `shaders/common.wesl` or
//! `shaders/generate.wesl`. The runtime CPU uses only this module: hashes,
//! the blue-noise tile, chunk layout, distance LOD, and the static clump,
//! tuft and gust tables. `mirror.rs` holds the test-only CPU oracle that
//! checks the same behaviour and the GPU readback test against it.

use std::sync::OnceLock;

use bevy::math::{IVec2, Vec2};

use crate::GrassSettings;

/// Side of the gust texture, in texels.
pub(crate) const GUST_SIZE: u32 = 256;

/// Points in the blue-noise tile.
pub(crate) const TILE_POINTS: u32 = 1024;

/// PCG hash (Jarzynski and Olano, 2020). The WESL twin is `hash`.
pub(crate) fn hash(x: u32) -> u32 {
    let state = x.wrapping_mul(747_796_405).wrapping_add(2_891_336_453);
    let word = ((state >> ((state >> 28) + 4)) ^ state).wrapping_mul(277_803_737);
    (word >> 22) ^ word
}

/// Hash of two values. The WESL twin is `hash2`.
pub(crate) fn hash2(a: u32, b: u32) -> u32 {
    hash(a ^ hash(b))
}

/// A hash mapped to `[0, 1)` with 24 bits of precision. The WESL twin is `unit`.
pub(crate) fn unit(h: u32) -> f32 {
    (h >> 8) as f32 / 16_777_216.0
}

/// The blue-noise tile: [`TILE_POINTS`] points in the unit square, wrapping at
/// its edges.
///
/// The points are in *progressive* order: every prefix of the list is itself
/// evenly spread. Thinning keeps the blades whose index is below a threshold,
/// so the grass stays evenly spread at every density instead of clumping as
/// it thins. Generated once with Mitchell's best-candidate algorithm and a
/// fixed seed, so it is identical on every machine and every run.
pub(crate) fn blue_noise_tile() -> &'static [Vec2] {
    static TILE: OnceLock<Vec<Vec2>> = OnceLock::new();
    TILE.get_or_init(|| best_candidate_tile(TILE_POINTS as usize, 12, 0x6b8a_3a00))
}

pub(crate) fn best_candidate_tile(count: usize, candidates: u32, seed: u32) -> Vec<Vec2> {
    let mut points: Vec<Vec2> = Vec::with_capacity(count);
    let mut rng = seed;
    let mut next = || {
        rng = hash(rng);
        unit(rng)
    };
    while points.len() < count {
        let mut best = Vec2::ZERO;
        let mut best_distance = -1.0;
        for _ in 0..candidates {
            let candidate = Vec2::new(next(), next());
            let nearest = points
                .iter()
                .map(|p| toroidal_distance_squared(*p, candidate))
                .fold(f32::INFINITY, f32::min);
            if nearest > best_distance {
                best_distance = nearest;
                best = candidate;
            }
        }
        points.push(best);
    }
    points
}

/// Squared distance between two points of the unit square, wrapping at the edges.
pub(crate) fn toroidal_distance_squared(a: Vec2, b: Vec2) -> f32 {
    let d = (a - b).abs();
    let d = d.min(Vec2::ONE - d);
    d.length_squared()
}

/// How a chunk is covered by copies of the blue-noise tile.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ChunkLayout {
    /// Tile copies along each side of the chunk.
    pub tiles_per_side: u32,
    /// Side of one tile copy, in metres.
    pub tile_size: f32,
    /// Blades per square metre when every candidate is kept.
    pub candidate_density: f32,
}

impl ChunkLayout {
    /// The coarsest layout with at least `density` candidates per square
    /// metre, never fewer than one tile per chunk. Rounding the tile count to
    /// the nearest instead silently capped some styles: 45 blades per m²
    /// drew 36. Spare candidates cost nothing, because a chunk only evaluates
    /// the prefix it can keep.
    pub(crate) fn for_density(chunk_size: f32, density: f32) -> Self {
        let ideal = chunk_size * (density.max(0.0) / TILE_POINTS as f32).sqrt();
        let tiles_per_side = (ideal.ceil() as u32).max(1);
        let tile_size = chunk_size / tiles_per_side as f32;
        Self {
            tiles_per_side,
            tile_size,
            candidate_density: TILE_POINTS as f32 / (tile_size * tile_size),
        }
    }
}

/// Distance-based thinning and level of detail for perspective views, from a
/// [`GrassSettings`]. The tier's own table is the architecture's: 100% to
/// the start of LOD 1, 50% where LOD 2 starts, 15% where the fade starts.
///
/// Orthographic views use none of it: every blade is the same size on screen,
/// so they keep full density and one fixed LOD per tier (T3 in the plan).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct DistanceLod {
    /// Beyond this no grass is drawn, in metres.
    pub radius: f32,
    /// Where LOD 1 and LOD 2 begin, in metres.
    pub lod_start: [f32; 2],
    /// Width of the dithered band around each LOD boundary, in metres.
    pub band: f32,
    /// Where the last fade begins, as a share of `radius`.
    pub fade_start: f32,
    /// The share of the blades kept where LOD 2 begins, and where the fade
    /// begins.
    pub density_at: [f32; 2],
    /// The share of blades kept from the start of LOD 1 on, on top of the
    /// thinning; coverage widening makes the survivors wider. Thin blades
    /// waste most of the fragment work on the 2×2 pixel quads GPUs shade in,
    /// so half as many blades at 1.4× the width cost 16% less in the
    /// perspective benchmark and look the same.
    pub mid_range_density: f32,
}

impl DistanceLod {
    /// The table `settings` ask for, scaled to its radius.
    pub(crate) fn from_settings(settings: &GrassSettings) -> Self {
        Self {
            radius: settings.radius,
            lod_start: [
                settings.radius * settings.lod1_start,
                settings.radius * settings.lod2_start,
            ],
            band: settings.lod_blend,
            fade_start: settings.fade_start,
            density_at: [settings.lod2_density, settings.fade_density],
            mid_range_density: settings.mid_range_density,
        }
    }
}

/// Clump table entries per metre along each axis.
pub(crate) const CLUMP_PER_METRE: i32 = 4;
/// Entries along each side of the clump table: it repeats every 64 m.
pub(crate) const CLUMP_SIDE: i32 = 256;

/// The clump of every 25 cm square of a 64 m tile, 0–63, one byte each,
/// four to a word. It is a Voronoi pattern with one jittered point per
/// square metre, wrapped so the tile repeats seamlessly. Blades in one clump
/// share a tint, which reads as natural patches rather than per-blade noise.
///
/// Built once and uploaded: searching the nine nearest points per blade on
/// the GPU made blade generation 40–130% slower; a table lookup costs one
/// load.
pub(crate) fn clump_table() -> &'static [u32] {
    static TABLE: OnceLock<Vec<u32>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let metres = CLUMP_SIDE / CLUMP_PER_METRE;
        let mut words = vec![0u32; (CLUMP_SIDE * CLUMP_SIDE / 4) as usize];
        for z in 0..CLUMP_SIDE {
            for x in 0..CLUMP_SIDE {
                let point = (Vec2::new(x as f32, z as f32) + 0.5) / CLUMP_PER_METRE as f32;
                let cell = point.floor();
                let mut nearest = f32::MAX;
                let mut id = 0;
                for dz in -1..=1 {
                    for dx in -1..=1 {
                        let neighbour = cell + Vec2::new(dx as f32, dz as f32);
                        let wrapped = neighbour.as_ivec2().rem_euclid(IVec2::splat(metres));
                        let key = hash2(wrapped.x as u32, wrapped.y as u32);
                        let feature = neighbour + Vec2::new(unit(key), unit(hash(key)));
                        let distance = feature.distance_squared(point);
                        if distance < nearest {
                            nearest = distance;
                            id = key;
                        }
                    }
                }
                let index = (z * CLUMP_SIDE + x) as usize;
                words[index / 4] |= (hash(id ^ 0xc1) >> 26) << ((index % 4) * 8);
            }
        }
        words
    })
}
/// Tuft table entries per tuft spacing, along each axis.
pub(crate) const TUFT_ENTRIES_PER_TUFT: i32 = 4;
/// Entries along each side of the tuft table: it repeats every 64 tufts.
pub(crate) const TUFT_SIDE: i32 = 256;

/// Where the tufts are: one centre per tuft spacing (a square of side 1 in
/// tuft units), jittered, as a Voronoi pattern like the clump table's. Each
/// entry covers a quarter of a tuft spacing and holds its nearest centre,
/// relative to the entry's own centre, in tuft spacings: x and z as signed
/// bytes over ±2 in the low two bytes, and a random byte per tuft in the
/// third. Wraps every 64 tufts.
///
/// A table for the same reason as the clump table: searching nine centres
/// per blade is what made generation 40–130% slower there.
pub(crate) fn tuft_table() -> &'static [u32] {
    static TABLE: OnceLock<Vec<u32>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let tufts = TUFT_SIDE / TUFT_ENTRIES_PER_TUFT;
        let byte = |offset: f32| ((offset / 4.0 + 0.5).clamp(0.0, 1.0) * 255.0).round() as u32;
        (0..TUFT_SIDE * TUFT_SIDE)
            .map(|index| {
                let entry = Vec2::new((index % TUFT_SIDE) as f32, (index / TUFT_SIDE) as f32);
                let point = (entry + 0.5) / TUFT_ENTRIES_PER_TUFT as f32;
                let cell = point.floor();
                let mut nearest = (f32::MAX, Vec2::ZERO, 0);
                for dz in -1..=1 {
                    for dx in -1..=1 {
                        let neighbour = cell + Vec2::new(dx as f32, dz as f32);
                        let wrapped = neighbour.as_ivec2().rem_euclid(IVec2::splat(tufts));
                        let key = hash2(wrapped.x as u32, wrapped.y as u32) ^ 0x7f4a_7c15;
                        let centre = neighbour + Vec2::new(unit(key), unit(hash(key)));
                        let distance = centre.distance_squared(point);
                        if distance < nearest.0 {
                            nearest = (distance, centre - point, key);
                        }
                    }
                }
                let (_, offset, key) = nearest;
                byte(offset.x) | (byte(offset.y) << 8) | ((hash(key ^ 0x3c) >> 24) << 16)
            })
            .collect()
    })
}

/// One bilinear sample of the gust noise (an R8 texture, wrapping), as the
/// GPU's sampler takes it.
pub(crate) fn sample_noise(noise: &[u8], uv: Vec2) -> f32 {
    let size = (noise.len() as f32).sqrt() as i32;
    let p = uv * size as f32 - 0.5;
    let base = p.floor();
    let t = p - base;
    let at = |x: i32, y: i32| {
        let x = (base.x as i32 + x).rem_euclid(size);
        let y = (base.y as i32 + y).rem_euclid(size);
        f32::from(noise[(y * size + x) as usize]) / 255.0
    };
    let top = at(0, 0) + (at(1, 0) - at(0, 0)) * t.x;
    let bottom = at(0, 1) + (at(1, 1) - at(0, 1)) * t.x;
    top + (bottom - top) * t.y
}
/// The value of `bare_noise` below which `share` of the ground lies.
#[expect(clippy::manual_midpoint, reason = "the same arithmetic as the GPU's")]
pub(crate) fn bare_threshold(share: f32) -> f32 {
    static SORTED: OnceLock<Vec<f32>> = OnceLock::new();
    let sorted = SORTED.get_or_init(|| {
        let noise = gust_noise(GUST_SIZE);
        let mut values: Vec<f32> = (0..32_768u32)
            .map(|i| {
                let uv = Vec2::new(unit(hash(i ^ 0xba5e)), unit(hash(i ^ 0xbead))) * 16.0;
                let turned =
                    Vec2::new(uv.x * 0.8 - uv.y * 0.6, uv.x * 0.6 + uv.y * 0.8) * 1.37 + 0.5;
                0.5 * (sample_noise(&noise, uv) + sample_noise(&noise, turned))
            })
            .collect();
        values.sort_by(f32::total_cmp);
        values
    });
    let index = (share.clamp(0.0, 1.0) * (sorted.len() - 1) as f32).round() as usize;
    sorted[index]
}

/// Index list for one level of detail, with vertex indices local to the level.
///
/// Rows run root to tip; row `r` is vertices `2r` (left) and `2r + 1` (right),
/// and the tip is vertex `2 * rows`.
pub(crate) fn lod_indices(rows: u32) -> Vec<u32> {
    let tip = 2 * rows;
    let mut indices = Vec::with_capacity((2 * rows as usize - 1) * 3);
    for r in 0..rows - 1 {
        let (l, rt) = (2 * r, 2 * r + 1);
        indices.extend_from_slice(&[l, rt, l + 2, rt, rt + 2, l + 2]);
    }
    indices.extend_from_slice(&[2 * (rows - 1), 2 * (rows - 1) + 1, tip]);
    indices
}

/// Tileable gust noise: three octaves of value noise on lattices that divide
/// the texture, so it wraps without a seam. Values span the full 0–255 range.
pub(crate) fn gust_noise(size: u32) -> Vec<u8> {
    const OCTAVES: [(u32, f32); 3] = [(4, 0.6), (8, 0.28), (16, 0.12)];
    let lattice = |period: u32, octave: u32, x: u32, y: u32| {
        unit(hash2(hash2(x % period, y % period), octave))
    };
    let smooth = |t: f32| t * t * (3.0 - 2.0 * t);
    let mut values = Vec::with_capacity((size * size) as usize);
    for y in 0..size {
        for x in 0..size {
            let mut value = 0.0;
            for (octave, (period, weight)) in OCTAVES.iter().enumerate() {
                let fx = x as f32 / size as f32 * *period as f32;
                let fy = y as f32 / size as f32 * *period as f32;
                let (ix, iy) = (fx.floor() as u32, fy.floor() as u32);
                let (tx, ty) = (smooth(fx.fract()), smooth(fy.fract()));
                let o = octave as u32;
                let top =
                    lattice(*period, o, ix, iy) * (1.0 - tx) + lattice(*period, o, ix + 1, iy) * tx;
                let bottom = lattice(*period, o, ix, iy + 1) * (1.0 - tx)
                    + lattice(*period, o, ix + 1, iy + 1) * tx;
                value += (top * (1.0 - ty) + bottom * ty) * weight;
            }
            values.push(value);
        }
    }
    let (low, high) = values
        .iter()
        .fold((f32::MAX, f32::MIN), |(lo, hi), v| (lo.min(*v), hi.max(*v)));
    let span = (high - low).max(1e-6);
    values
        .iter()
        .map(|v| ((v - low) / span * 255.0).round() as u8)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gust_noise_wraps_without_a_seam() {
        let size = 64;
        let noise = gust_noise(size);
        let at = |x: u32, y: u32| i32::from(noise[(y * size + x) as usize]);
        let inner = (0..size - 1)
            .flat_map(|y| (0..size - 1).map(move |x| (x, y)))
            .map(|(x, y)| {
                (at(x, y) - at(x + 1, y))
                    .abs()
                    .max((at(x, y) - at(x, y + 1)).abs())
            })
            .max()
            .unwrap();
        for i in 0..size {
            assert!((at(size - 1, i) - at(0, i)).abs() <= inner);
            assert!((at(i, size - 1) - at(i, 0)).abs() <= inner);
        }
    }

    #[test]
    fn gust_noise_uses_the_full_range() {
        let noise = gust_noise(64);
        assert_eq!(noise.iter().min(), Some(&0));
        assert_eq!(noise.iter().max(), Some(&255));
    }
}
