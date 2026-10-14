//! CPU mirrors of the functions the shaders run.
//!
//! Every function here has a WESL twin in `shaders/common.wesl` or
//! `shaders/generate.wesl` that must give the same answer, bit for bit where
//! integers are involved. At runtime the CPU only needs the hash, the tile and
//! the chunk layout (to build the field table); the rest exists so that the
//! placement properties the design depends on (determinism, stable thinning,
//! smooth LOD, constant blade length) are tested here, and so that the GPU
//! readback test has a reference to compare against.

use std::sync::OnceLock;

use bevy::math::IVec2;
use bevy::math::Vec2;
#[cfg(test)]
use bevy::math::Vec3;
#[cfg(test)]
use std::f32::consts::TAU;

/// Points in the blue-noise tile.
pub(crate) const TILE_POINTS: u32 = 1024;

/// Vertical range an instance's root height can encode, in metres either side
/// of the view's instance origin.
#[cfg(test)]
pub(crate) const HEIGHT_RANGE: f32 = 256.0;

/// An instance's width encodes up to this many times its style's width, room
/// for the ±25% per-blade variation times the 2.5× coverage widening.
#[cfg(test)]
pub(crate) const WIDTH_RANGE: f32 = 4.0;

/// Rows per level of detail. A row is two vertices; every blade also has a
/// single tip vertex, so LOD 0 is 15 vertices and 13 triangles.
pub(crate) const LOD_ROWS: [u32; 3] = [7, 3, 1];

/// Vertex-index stride between levels of detail.
///
/// The draw for LOD `n` uses `base_vertex = n * LOD_VERTEX_STRIDE`, so the
/// vertex shader recovers the level from `vertex_index` alone. That avoids
/// `first_instance` in indirect draws, which needs an optional GPU feature.
#[cfg(test)]
pub(crate) const LOD_VERTEX_STRIDE: u32 = 64;

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

fn best_candidate_tile(count: usize, candidates: u32, seed: u32) -> Vec<Vec2> {
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

    /// Tile copies in one chunk.
    #[cfg(test)]
    pub(crate) fn cells(&self) -> u32 {
        self.tiles_per_side * self.tiles_per_side
    }

    /// Candidates that must be evaluated so that every blade whose rank is
    /// below `keep` is among them. The WESL twin is in `cull`.
    #[cfg(test)]
    pub(crate) fn candidates_needed(&self, keep: f32) -> u32 {
        let points = (keep.clamp(0.0, 1.0) * TILE_POINTS as f32).ceil() as u32;
        points * self.cells()
    }
}

/// One candidate blade, before thinning.
#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Candidate {
    /// Position inside the chunk, in metres from its minimum corner.
    pub local: Vec2,
    /// Thinning rank in `[0, 1)`: the blade is kept while its rank is below
    /// the local keep fraction. Lower ranks survive longer.
    pub rank: f32,
    /// Per-blade random seed.
    pub seed: u32,
}

/// Candidate `index` of chunk `chunk` in a field seeded with `field_seed`.
///
/// Candidates are numbered point-major: all tile copies' first point, then
/// all their second point, and so on. Rank grows with the index, so a chunk
/// that keeps only a fraction of its blades evaluates only a prefix of its
/// candidates. The WESL twin is `candidate`.
#[cfg(test)]
pub(crate) fn candidate(
    chunk: [u32; 2],
    field_seed: u32,
    layout: &ChunkLayout,
    index: u32,
) -> Candidate {
    let cells = layout.cells();
    let point = index / cells;
    let cell = index % cells;
    let cell_x = cell % layout.tiles_per_side;
    let cell_z = cell / layout.tiles_per_side;
    let cell_hash = hash2(hash2(chunk[0], chunk[1]), hash2(cell, field_seed));
    // Each tile copy is shifted by its own random offset (wrapping), so the
    // repetition of one tile across the field is not visible.
    let offset = Vec2::new(unit(cell_hash), unit(hash(cell_hash)));
    let p = (blue_noise_tile()[point as usize] + offset).fract();
    Candidate {
        local: (Vec2::new(cell_x as f32, cell_z as f32) + p) * layout.tile_size,
        rank: (point as f32 + 0.5) / TILE_POINTS as f32,
        seed: hash2(cell_hash, point),
    }
}

/// The share of candidates to keep where the requested density is
/// `density × scale` blades per square metre (`scale` folds in the mask,
/// the distance falloff and the quality tier).
#[cfg(test)]
pub(crate) fn keep_fraction(density: f32, scale: f32, layout: &ChunkLayout) -> f32 {
    (density * scale / layout.candidate_density).clamp(0.0, 1.0)
}

/// The share of blades kept from the start of LOD 1 on, on top of the
/// architecture's distance thinning; coverage widening makes the survivors
/// wider. Thin blades waste most of the fragment work on the 2×2 pixel quads
/// GPUs shade in, so half as many blades at 1.4× the width cost 16% less in
/// the perspective benchmark and look the same. The WESL twin is
/// `MID_RANGE_DENSITY`.
#[cfg(test)]
pub(crate) const MID_RANGE_DENSITY: f32 = 0.5;

/// Distance-based thinning and level of detail for perspective views.
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
}

impl DistanceLod {
    /// The High-tier table from the architecture, scaled to `radius`.
    pub(crate) fn for_radius(radius: f32) -> Self {
        Self {
            radius,
            lod_start: [radius * 0.2, radius * 0.5],
            band: 4.0,
        }
    }

    /// Share of the full density drawn at `distance`. The architecture's
    /// table (100% to the start of LOD 1, 50% where LOD 2 starts, 15% at 90%
    /// of the radius, then fading to nothing at the radius so the edge never
    /// shows as a line), times [`MID_RANGE_DENSITY`] from LOD 1's dither band
    /// on. The WESL twin is `density_falloff`.
    #[cfg(test)]
    pub(crate) fn falloff(&self, distance: f32) -> f32 {
        let into_lod1 =
            ((distance - (self.lod_start[0] - self.band * 0.5)) / self.band).clamp(0.0, 1.0);
        self.distance_thinning(distance) * lerp(1.0, MID_RANGE_DENSITY, into_lod1)
    }

    /// The architecture's table alone. The WESL twin is `distance_thinning`.
    #[cfg(test)]
    fn distance_thinning(&self, distance: f32) -> f32 {
        let [lod1, lod2] = self.lod_start;
        let edge = self.radius * 0.9;
        if distance <= lod1 {
            1.0
        } else if distance <= lod2 {
            lerp(1.0, 0.5, (distance - lod1) / (lod2 - lod1))
        } else if distance <= edge {
            lerp(0.5, 0.15, (distance - lod2) / (edge - lod2))
        } else {
            lerp(
                0.15,
                0.0,
                ((distance - edge) / (self.radius - edge)).min(1.0),
            )
        }
    }

    /// The LOD of a blade at `distance` whose dither value is `dither` in
    /// `[0, 1)`. Inside a band around each boundary the blade switches when
    /// its own dither value says so, so the boundary dissolves into noise
    /// instead of a ring. The WESL twin is `select_lod`.
    #[cfg(test)]
    pub(crate) fn lod(&self, distance: f32, dither: f32) -> u32 {
        self.lod_start
            .iter()
            .filter(|&&start| {
                let progress = (distance - (start - self.band * 0.5)) / self.band;
                dither < progress
            })
            .count() as u32
    }
}

/// Width multiplier that keeps coverage constant as density thins: blades
/// widen by the inverse square root of the density drop, at most 2.5×. The
/// WESL twin is `coverage_width`.
#[cfg(test)]
pub(crate) fn coverage_width(falloff: f32) -> f32 {
    (1.0 / falloff.max(0.16).sqrt()).min(2.5)
}

#[cfg(test)]
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

/// One blade, unpacked. Every field is in the range the packed form can hold.
#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Blade {
    /// Root position relative to the view's instance origin, in metres. `x`
    /// and `z` are in `[-extent, extent]`, `y` in `[-HEIGHT_RANGE, HEIGHT_RANGE]`.
    pub position: Vec3,
    /// Yaw as a fraction of a full turn, `[0, 1)`.
    pub facing: f32,
    /// Height as a fraction of the style's tallest blade, `[0, 1]`.
    pub height: f32,
    /// Width as a fraction of `WIDTH_RANGE` times the style's width, `[0, 1]`.
    pub width: f32,
    /// Low bits of the blade's seed: colour variation and sway phase.
    pub seed: u8,
    /// Level of detail, 0–2.
    pub lod: u8,
    /// Index into the field table.
    pub field: u8,
    /// The blade's clump, 0–63: blades in the same 1 m clump share a tint.
    pub clump: u8,
    /// Tip offset from wind and interaction this frame, as a fraction of the
    /// blade's height in each horizontal axis, `[-1, 1]`.
    pub bend: Vec2,
    /// The same offset last frame, for motion vectors.
    pub previous_bend: Vec2,
}

#[cfg(test)]
fn quantize(value: f32, bits: u32) -> u32 {
    let max = ((1u64 << bits) - 1) as f32;
    (value.clamp(0.0, 1.0) * max).round() as u32
}

#[cfg(test)]
fn dequantize(value: u32, bits: u32) -> f32 {
    value as f32 / ((1u64 << bits) - 1) as f32
}

#[cfg(test)]
fn snorm8(value: f32) -> u32 {
    quantize(value * 0.5 + 0.5, 8)
}

#[cfg(test)]
fn unsnorm8(value: u32) -> f32 {
    dequantize(value, 8) * 2.0 - 1.0
}

/// Packs a blade into the 16-byte instance format. The WESL twins are
/// `pack` and `unpack`.
///
/// | Word | Bits |
/// | --- | --- |
/// | 0 | x (16), z (16), over ±`extent` around the view's instance origin |
/// | 1 | y (16) over ±`HEIGHT_RANGE`, facing (8), seed (8) |
/// | 2 | height (8), width (8), field (8), LOD (2), flags (6) |
/// | 3 | bend x, bend z, previous bend x, previous bend z (8 each) |
#[cfg(test)]
pub(crate) fn pack(blade: &Blade, extent: f32) -> [u32; 4] {
    let x = quantize(blade.position.x / (2.0 * extent) + 0.5, 16);
    let z = quantize(blade.position.z / (2.0 * extent) + 0.5, 16);
    let y = quantize(blade.position.y / (2.0 * HEIGHT_RANGE) + 0.5, 16);
    let facing = (blade.facing.rem_euclid(1.0) * 256.0) as u32 & 0xff;
    [
        x | (z << 16),
        y | (facing << 16) | (u32::from(blade.seed) << 24),
        quantize(blade.height, 8)
            | (quantize(blade.width, 8) << 8)
            | (u32::from(blade.field) << 16)
            | ((u32::from(blade.lod) & 3) << 24)
            | ((u32::from(blade.clump) & 63) << 26),
        snorm8(blade.bend.x)
            | (snorm8(blade.bend.y) << 8)
            | (snorm8(blade.previous_bend.x) << 16)
            | (snorm8(blade.previous_bend.y) << 24),
    ]
}

/// Unpacks the 16-byte instance format.
#[cfg(test)]
pub(crate) fn unpack(words: [u32; 4], extent: f32) -> Blade {
    Blade {
        position: Vec3::new(
            (dequantize(words[0] & 0xffff, 16) - 0.5) * 2.0 * extent,
            (dequantize(words[1] & 0xffff, 16) - 0.5) * 2.0 * HEIGHT_RANGE,
            (dequantize(words[0] >> 16, 16) - 0.5) * 2.0 * extent,
        ),
        facing: ((words[1] >> 16) & 0xff) as f32 / 256.0,
        height: dequantize(words[2] & 0xff, 8),
        width: dequantize((words[2] >> 8) & 0xff, 8),
        seed: (words[1] >> 24) as u8,
        lod: ((words[2] >> 24) & 3) as u8,
        field: ((words[2] >> 16) & 0xff) as u8,
        clump: (words[2] >> 26) as u8,
        bend: Vec2::new(unsnorm8(words[3] & 0xff), unsnorm8((words[3] >> 8) & 0xff)),
        previous_bend: Vec2::new(unsnorm8((words[3] >> 16) & 0xff), unsnorm8(words[3] >> 24)),
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

/// The clump a root at `point` (world XZ) belongs to, 0–63. The WESL twin
/// is `clump_of`.
#[cfg(test)]
pub(crate) fn clump_of(point: Vec2) -> u8 {
    let entry = (point * CLUMP_PER_METRE as f32).floor().as_ivec2() & (CLUMP_SIDE - 1);
    let index = (entry.y * CLUMP_SIDE + entry.x) as usize;
    ((clump_table()[index / 4] >> ((index % 4) * 8)) & 63) as u8
}

/// The three Bézier control points of a blade of length `height`, leaning
/// `lean` (a fraction of the height, horizontal) along +Z and bent by `bend`
/// (a horizontal tip offset as a fraction of the height). The WESL twin is
/// `blade_curve`.
///
/// The tip is pushed horizontally, then pulled back down so the curve keeps
/// its length: wind bends a blade, never stretches it. The construction is
/// Jahrmann and Wimmer's (2017), with the length correction applied twice so
/// that strong bends stay within 1% of the rest length.
#[cfg(test)]
pub(crate) fn blade_curve(height: f32, lean: f32, bend: Vec2) -> [Vec3; 3] {
    let offset = (Vec2::new(0.0, lean) + bend) * height;
    let horizontal = offset.length().min(height * 0.95);
    let offset = offset.normalize_or_zero() * horizontal;
    let p0 = Vec3::ZERO;
    let mut p2 = Vec3::new(
        offset.x,
        (height * height - horizontal * horizontal).sqrt(),
        offset.y,
    );
    // The control point sits above the root, lower the further the tip leans,
    // which is what makes the blade arch instead of hinging at the root.
    let ratio = horizontal / height;
    let mut p1 = Vec3::new(0.0, height * (1.0 - ratio).max(0.05 * ratio.max(1.0)), 0.0);
    for _ in 0..2 {
        let chord = p2.distance(p0);
        let polygon = p1.distance(p0) + p2.distance(p1);
        let length = (2.0 * chord + polygon) / 3.0;
        let scale = height / length;
        p1 = p0 + (p1 - p0) * scale;
        p2 = p1 + (p2 - p1) * scale;
    }
    [p0, p1, p2]
}

/// A point on a quadratic Bézier curve.
#[cfg(test)]
pub(crate) fn bezier(points: &[Vec3; 3], t: f32) -> Vec3 {
    let u = 1.0 - t;
    points[0] * (u * u) + points[1] * (2.0 * u * t) + points[2] * (t * t)
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

/// Cells along a trample window's side. Twin of `trample::TEXELS`.
#[cfg(test)]
const TRAMPLE_TEXELS: i32 = 256;

/// A push packed for `atomicMax`: strength in the top 10 bits, so the larger
/// word is always the stronger push, and the direction as two 11-bit
/// components below. The WESL twin is `trample_pack`.
#[cfg(test)]
pub(crate) fn trample_pack(push: Vec2) -> u32 {
    let strength = push.length().clamp(0.0, 1.0);
    let direction = if strength > 1e-6 {
        push / push.length()
    } else {
        Vec2::X
    };
    let d = ((direction * 0.5 + 0.5) * 2047.0).round().as_uvec2();
    (((strength * 1023.0).round() as u32) << 22) | (d.x << 11) | d.y
}

/// The WESL twin is `trample_unpack`.
#[cfg(test)]
pub(crate) fn trample_unpack(word: u32) -> Vec2 {
    let strength = (word >> 22) as f32 / 1023.0;
    let direction =
        Vec2::new(((word >> 11) & 0x7ff) as f32, (word & 0x7ff) as f32) / 2047.0 * 2.0 - 1.0;
    direction * strength
}

/// Index of a world cell's slot in the window buffer. The WESL twin is
/// `trample_slot`.
#[cfg(test)]
pub(crate) fn trample_slot(cell: IVec2) -> u32 {
    let wrapped = cell & IVec2::splat(TRAMPLE_TEXELS - 1);
    (wrapped.y * TRAMPLE_TEXELS + wrapped.x) as u32
}

/// The world cell a slot holds while the window's lowest cell is `origin`.
/// The WESL twin is `trample_cell`.
#[cfg(test)]
pub(crate) fn trample_cell(slot: IVec2, origin: IVec2) -> IVec2 {
    origin + ((slot - origin) & IVec2::splat(TRAMPLE_TEXELS - 1))
}

/// The WESL twin is `in_trample_window`.
#[cfg(test)]
pub(crate) fn in_trample_window(cell: IVec2, origin: IVec2) -> bool {
    cell.cmpge(origin).all() && cell.cmplt(origin + TRAMPLE_TEXELS).all()
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHUNK: f32 = 16.0;

    #[test]
    fn hash_values_are_pinned() {
        // Pinned so that any change to the hash, which would move every blade,
        // is deliberate; the WESL `hash` must produce the same numbers.
        assert_eq!(hash(0), 129_708_002);
        assert_eq!(hash(1), 2_831_084_092);
    }

    #[test]
    fn the_tile_is_identical_on_every_call() {
        let a = best_candidate_tile(256, 12, 7);
        let b = best_candidate_tile(256, 12, 7);
        assert_eq!(a, b);
    }

    #[test]
    fn every_prefix_of_the_tile_is_evenly_spread() {
        let tile = blue_noise_tile();
        for prefix in [64usize, 256, 1024] {
            let points = &tile[..prefix];
            // For n points evenly spread over a unit torus the nearest-neighbour
            // distance is about 0.9 / sqrt(n); white noise has many pairs far
            // closer. Require every point to keep at least 40% of that spacing.
            let floor = 0.4 * 0.9 / (prefix as f32).sqrt();
            for (i, a) in points.iter().enumerate() {
                for b in &points[i + 1..] {
                    assert!(
                        toroidal_distance_squared(*a, *b).sqrt() >= floor,
                        "two of the first {prefix} points are closer than {floor}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_same_inputs_give_the_same_blade() {
        let layout = ChunkLayout::for_density(CHUNK, 60.0);
        let all = layout.cells() * TILE_POINTS;
        for index in [0, 1, 999, all - 1] {
            assert_eq!(
                candidate([3, 9], 42, &layout, index),
                candidate([3, 9], 42, &layout, index)
            );
        }
    }

    #[test]
    fn candidates_stay_inside_their_chunk() {
        let layout = ChunkLayout::for_density(CHUNK, 60.0);
        for index in 0..layout.cells() * TILE_POINTS {
            let c = candidate([1, 2], 5, &layout, index);
            assert!(c.local.cmpge(Vec2::ZERO).all() && c.local.cmplt(Vec2::splat(CHUNK)).all());
        }
    }

    #[test]
    fn rank_never_decreases_with_the_candidate_index() {
        let layout = ChunkLayout::for_density(CHUNK, 60.0);
        let ranks: Vec<f32> = (0..layout.cells() * TILE_POINTS)
            .map(|i| candidate([0, 0], 0, &layout, i).rank)
            .collect();
        assert!(ranks.windows(2).all(|w| w[0] <= w[1]));
    }

    #[test]
    fn the_needed_prefix_holds_every_kept_blade() {
        let layout = ChunkLayout::for_density(CHUNK, 80.0);
        for keep in [0.0, 0.01, 0.137, 0.5, 0.999, 1.0] {
            let needed = layout.candidates_needed(keep);
            for index in needed..layout.cells() * TILE_POINTS {
                assert!(
                    candidate([4, 4], 9, &layout, index).rank >= keep,
                    "keep {keep}, index {index}"
                );
            }
        }
    }

    #[test]
    fn lowering_density_only_removes_blades_in_rank_order() {
        let layout = ChunkLayout::for_density(CHUNK, 80.0);
        let kept = |density: f32| -> Vec<u32> {
            let keep = keep_fraction(density, 1.0, &layout);
            (0..layout.cells() * TILE_POINTS)
                .filter(|&i| candidate([0, 0], 1, &layout, i).rank < keep)
                .collect()
        };
        let mut previous = kept(80.0);
        for density in [60.0, 40.0, 20.0, 5.0, 0.0] {
            let now = kept(density);
            assert!(
                now.iter().all(|i| previous.contains(i)),
                "a blade appeared at {density}"
            );
            previous = now;
        }
        assert!(previous.is_empty());
    }

    #[test]
    fn every_style_density_is_reached() {
        for tenth in 1..2000 {
            let density = tenth as f32 / 10.0;
            let layout = ChunkLayout::for_density(CHUNK, density);
            assert!(
                layout.candidate_density >= density * (1.0 - 1e-5),
                "{density}: {layout:?}"
            );
            let drawn = keep_fraction(density, 1.0, &layout) * layout.candidate_density;
            assert!(
                (drawn - density).abs() < 1e-3 * density,
                "{density}: draws {drawn}"
            );
        }
    }

    #[test]
    fn falloff_is_continuous_and_never_increases() {
        let lod = DistanceLod::for_radius(100.0);
        let mut previous = lod.falloff(0.0);
        assert_eq!(previous, 1.0);
        let mut d = 0.0;
        while d <= 110.0 {
            let now = lod.falloff(d);
            assert!(now <= previous + 1e-6, "falloff rose at {d}");
            assert!((previous - now).abs() < 0.02, "falloff jumps at {d}");
            previous = now;
            d += 0.1;
        }
        assert_eq!(lod.falloff(100.0), 0.0);
        // The architecture's table (50–100% across LOD 1, 15–50% across
        // LOD 2), halved from LOD 1's band on.
        assert!((lod.falloff(17.0) - 1.0).abs() < 1e-6);
        assert!((lod.falloff(50.0) - 0.25).abs() < 1e-6);
        assert!((lod.falloff(90.0) - 0.075).abs() < 1e-6);
    }

    #[test]
    fn lod_switches_only_inside_the_dither_band() {
        let lod = DistanceLod::for_radius(100.0);
        for dither in [0.0, 0.3, 0.7, 0.999] {
            assert_eq!(lod.lod(17.9, dither), 0);
            assert_eq!(lod.lod(22.1, dither), 1);
            assert_eq!(lod.lod(47.9, dither), 1);
            assert_eq!(lod.lod(52.1, dither), 2);
        }
        // Inside the band, a blade's LOD never goes back down as it recedes.
        for dither in [0.1, 0.5, 0.9] {
            let lods: Vec<u32> = (0..100)
                .map(|i| lod.lod(18.0 + i as f32 * 0.04, dither))
                .collect();
            assert!(lods.windows(2).all(|w| w[0] <= w[1]));
        }
    }

    #[test]
    fn coverage_widening_matches_the_architecture() {
        assert_eq!(coverage_width(1.0), 1.0);
        assert!((coverage_width(0.5) - 2f32.sqrt()).abs() < 1e-6);
        assert_eq!(coverage_width(0.01), 2.5);
    }

    #[test]
    fn clumps_are_patches_about_a_metre_across() {
        // Neighbouring roots mostly share a clump; roots metres apart rarely do.
        let same = |gap: f32| {
            (0..2000)
                .filter(|&i| {
                    let p = Vec2::new(unit(hash(i)) * 200.0, unit(hash(i ^ 9)) * 200.0);
                    clump_of(p) == clump_of(p + Vec2::new(gap, 0.0))
                })
                .count() as f32
                / 2000.0
        };
        assert!(same(0.1) > 0.8, "{}", same(0.1));
        assert!(same(3.0) < 0.1, "{}", same(3.0));
        // Every tint is used.
        let mut seen = [false; 64];
        for i in 0..20_000 {
            seen[clump_of(Vec2::new(unit(hash(i)) * 300.0, unit(hash(i ^ 3)) * 300.0)) as usize] =
                true;
        }
        assert!(seen.iter().all(|&s| s));
        // The table repeats seamlessly every 64 m.
        for i in 0..2000 {
            let p = Vec2::new(unit(hash(i)) * 64.0, unit(hash(i ^ 5)) * 64.0);
            assert_eq!(clump_of(p), clump_of(p + Vec2::new(64.0, -128.0)));
        }
    }

    #[test]
    fn packing_round_trips_within_quantization() {
        let blade = Blade {
            position: Vec3::new(12.345, -3.21, -101.5),
            facing: 0.3,
            height: 0.75,
            width: 0.2,
            seed: 201,
            lod: 2,
            field: 17,
            clump: 45,
            bend: Vec2::new(-0.5, 0.25),
            previous_bend: Vec2::new(0.9, -1.0),
        };
        let extent = 128.0;
        let back = unpack(pack(&blade, extent), extent);
        let horizontal = |v: Vec3| Vec2::new(v.x, v.z);
        assert!(
            (horizontal(back.position) - horizontal(blade.position))
                .abs()
                .max_element()
                < 2.0 * extent / 65_535.0
        );
        assert!((back.position.y - blade.position.y).abs() < 2.0 * HEIGHT_RANGE / 65_535.0);
        assert!((back.facing - blade.facing).abs() <= 1.0 / 256.0);
        assert!((back.height - blade.height).abs() <= 0.5 / 255.0);
        assert!((back.width - blade.width).abs() <= 0.5 / 255.0);
        assert_eq!(
            (back.seed, back.lod, back.field, back.clump),
            (blade.seed, blade.lod, blade.field, blade.clump)
        );
        assert!((back.bend - blade.bend).abs().max_element() <= 1.0 / 255.0);
        assert!(
            (back.previous_bend - blade.previous_bend)
                .abs()
                .max_element()
                <= 1.0 / 255.0
        );
    }

    fn curve_length(points: &[Vec3; 3]) -> f32 {
        let steps = 256;
        (0..steps)
            .map(|i| {
                let a = bezier(points, i as f32 / steps as f32);
                let b = bezier(points, (i + 1) as f32 / steps as f32);
                a.distance(b)
            })
            .sum()
    }

    #[test]
    fn a_bent_blade_keeps_its_length() {
        for height in [0.1, 0.5, 1.4] {
            for bend in [0.0, 0.3, 0.6, 0.9, 1.5] {
                for lean in [0.0, 0.2] {
                    let curve = blade_curve(height, lean, Vec2::new(bend, 0.0));
                    let length = curve_length(&curve);
                    assert!(
                        (length - height).abs() <= height * 0.01,
                        "height {height}, bend {bend}, lean {lean}: length {length}"
                    );
                }
            }
        }
    }

    #[test]
    fn lods_have_the_planned_vertex_and_triangle_counts() {
        let expected = [(15, 13), (7, 5), (3, 1)];
        for (rows, (vertices, triangles)) in LOD_ROWS.iter().zip(expected) {
            let indices = lod_indices(*rows);
            assert_eq!(indices.len() / 3, triangles);
            assert_eq!(indices.iter().max().copied().unwrap() + 1, vertices);
            assert!(vertices < LOD_VERTEX_STRIDE);
        }
    }

    #[test]
    fn width_range_covers_variation_times_widening() {
        assert!(WIDTH_RANGE >= 1.25 * coverage_width(0.0));
    }
    #[test]
    fn trample_pushes_round_trip_within_quantization() {
        for i in 0..1000 {
            let angle = unit(hash(i)) * TAU - std::f32::consts::PI;
            let push = Vec2::from_angle(angle) * unit(hash(i ^ 0x55));
            let back = trample_unpack(trample_pack(push));
            assert!(back.distance(push) < 2e-3, "{push} came back as {back}");
        }
    }

    #[test]
    fn the_larger_packed_word_is_the_stronger_push() {
        for i in 0..1000 {
            let a = Vec2::from_angle(unit(hash(i)) * TAU) * unit(hash(i ^ 1));
            let b = Vec2::from_angle(unit(hash(i ^ 2)) * TAU) * unit(hash(i ^ 3));
            let (pa, pb) = (trample_pack(a), trample_pack(b));
            if (a.length() - b.length()).abs() > 1.0 / 1023.0 {
                assert_eq!(pa > pb, a.length() > b.length(), "{a} vs {b}");
            }
        }
    }

    #[test]
    fn every_cell_of_a_window_has_its_own_slot() {
        for origin in [IVec2::ZERO, IVec2::new(-300, 17), IVec2::new(1000, -1)] {
            let mut seen = vec![false; (TRAMPLE_TEXELS * TRAMPLE_TEXELS) as usize];
            for z in 0..TRAMPLE_TEXELS {
                for x in 0..TRAMPLE_TEXELS {
                    let cell = origin + IVec2::new(x, z);
                    let slot = trample_slot(cell);
                    assert!(!seen[slot as usize], "two cells share slot {slot}");
                    seen[slot as usize] = true;
                    let slot_xy =
                        IVec2::new(slot as i32 % TRAMPLE_TEXELS, slot as i32 / TRAMPLE_TEXELS);
                    assert_eq!(trample_cell(slot_xy, origin), cell);
                    assert!(in_trample_window(cell, origin));
                }
            }
        }
        assert!(!in_trample_window(IVec2::new(256, 0), IVec2::ZERO));
        assert!(!in_trample_window(IVec2::new(0, -1), IVec2::ZERO));
    }
}
