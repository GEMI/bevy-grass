//! Test-only CPU oracle for the GPU's placement and generation logic.
//!
//! This module mirrors the WESL shaders bit for bit where integers are
//! involved. It is compiled only for tests, so production code cannot
//! accidentally depend on it. Its WESL twins live in `shaders/common.wesl`
//! and `shaders/generate.wesl`.

use std::f32::consts::TAU;

use bevy::math::{IVec2, Vec2, Vec3};

use crate::GrassSettings;

pub(crate) use crate::shader_model::{
    CLUMP_PER_METRE, CLUMP_SIDE, ChunkLayout, DistanceLod, GUST_SIZE, TILE_POINTS,
    TUFT_ENTRIES_PER_TUFT, TUFT_SIDE, bare_threshold, best_candidate_tile, blue_noise_tile,
    clump_table, gust_noise, hash, hash2, lod_indices, sample_noise, toroidal_distance_squared,
    tuft_table, unit,
};

/// Vertical range an instance's root height can encode, in metres either side
/// of the view's instance origin.
pub(crate) const HEIGHT_RANGE: f32 = 256.0;

/// An instance's width encodes up to this many times its style's width, room
/// for the ±25% per-blade variation times the coverage widening, up to
/// `GrassSettings::MAX_WIDENING` (3.2×; 2.5× by default).
pub(crate) const WIDTH_RANGE: f32 = 4.0;

/// Vertex-index stride between levels of detail.
///
/// The draw for LOD `n` uses `base_vertex = n * LOD_VERTEX_STRIDE`, so the
/// vertex shader recovers the level from `vertex_index` alone. That avoids
/// `first_instance` in indirect draws, which needs an optional GPU feature.
pub(crate) const LOD_VERTEX_STRIDE: u32 = 64;
/// One candidate blade, before thinning.
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

impl ChunkLayout {
    /// Tile copies in one chunk.
    pub(crate) fn cells(&self) -> u32 {
        self.tiles_per_side * self.tiles_per_side
    }

    /// Candidates that must be evaluated so that every blade whose rank is
    /// below `keep` is among them. The WESL twin is in `cull`.
    pub(crate) fn candidates_needed(&self, keep: f32) -> u32 {
        let points = (keep.clamp(0.0, 1.0) * TILE_POINTS as f32).ceil() as u32;
        points * self.cells()
    }
}

/// Candidate `index` of chunk `chunk` in a field seeded with `field_seed`.
///
/// Candidates are numbered point-major: all tile copies' first point, then
/// all their second point, and so on. Rank grows with the index, so a chunk
/// that keeps only a fraction of its blades evaluates only a prefix of its
/// candidates. The WESL twin is `candidate`.
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
pub(crate) fn keep_fraction(density: f32, scale: f32, layout: &ChunkLayout) -> f32 {
    (density * scale / layout.candidate_density).clamp(0.0, 1.0)
}

impl DistanceLod {
    /// The High-tier table from the architecture, scaled to `radius`.
    pub(crate) fn for_radius(radius: f32) -> Self {
        Self::from_settings(&GrassSettings {
            radius,
            ..GrassSettings::HIGH
        })
    }

    /// Share of the full density drawn at `distance`. The architecture's
    /// table (100% to the start of LOD 1, 50% where LOD 2 starts, 15% at 90%
    /// of the radius, then fading to nothing at the radius so the edge never
    /// shows as a line), times `mid_range_density` from LOD 1's dither band
    /// on. The WESL twin is `density_falloff`.
    pub(crate) fn falloff(&self, distance: f32) -> f32 {
        let into_lod1 =
            ((distance - (self.lod_start[0] - self.band * 0.5)) / self.band).clamp(0.0, 1.0);
        self.distance_thinning(distance) * lerp(1.0, self.mid_range_density, into_lod1)
    }

    /// The architecture's table alone. The WESL twin is `distance_thinning`.
    fn distance_thinning(&self, distance: f32) -> f32 {
        let [lod1, lod2] = self.lod_start;
        let [at_lod2, at_edge] = self.density_at;
        let edge = self.radius * self.fade_start;
        if distance <= lod1 {
            1.0
        } else if distance <= lod2 {
            lerp(1.0, at_lod2, (distance - lod1) / (lod2 - lod1))
        } else if distance <= edge {
            lerp(at_lod2, at_edge, (distance - lod2) / (edge - lod2))
        } else {
            lerp(
                at_edge,
                0.0,
                ((distance - edge) / (self.radius - edge).max(1e-4)).min(1.0),
            )
        }
    }

    /// The LOD of a blade at `distance` whose dither value is `dither` in
    /// `[0, 1)`. Inside a band around each boundary the blade switches when
    /// its own dither value says so, so the boundary dissolves into noise
    /// instead of a ring. The WESL twin is `select_lod`.
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
/// widen by the inverse square root of the density drop, at most `max` times
/// (2.5 by default). The WESL twin is `coverage_width`.
pub(crate) fn coverage_width(falloff: f32, max: f32) -> f32 {
    (1.0 / falloff.max(1.0 / (max * max)).sqrt()).min(max)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

/// One blade, unpacked. Every field is in the range the packed form can hold.
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

fn quantize(value: f32, bits: u32) -> u32 {
    let max = ((1u64 << bits) - 1) as f32;
    (value.clamp(0.0, 1.0) * max).round() as u32
}

fn dequantize(value: u32, bits: u32) -> f32 {
    value as f32 / ((1u64 << bits) - 1) as f32
}

/// Steps of last frame's bend per unit of bend.
pub(crate) const BEND_DELTA_STEPS: f32 = 128.0;

/// One axis of a blade's bend in 16 bits: the bend in the low 12, and last
/// frame's bend as a 4-bit step from it. The WESL twin is `pack_bend`.
fn pack_bend(bend: f32, previous: f32) -> u32 {
    let now = quantize(bend * 0.5 + 0.5, 12);
    let decoded = dequantize(now, 12) * 2.0 - 1.0;
    let step = ((previous - decoded) * BEND_DELTA_STEPS)
        .round()
        .clamp(-8.0, 7.0);
    now | (((step as i32 + 8) as u32) << 12)
}

/// This frame's and last frame's bend from `pack_bend`'s 16 bits.
pub(crate) fn unpack_bend(bits: u32) -> (f32, f32) {
    let now = dequantize(bits & 0xfff, 12) * 2.0 - 1.0;
    let step = ((bits >> 12) & 0xf) as i32 - 8;
    (now, now + step as f32 / BEND_DELTA_STEPS)
}

/// Packs a blade into the 16-byte instance format. The WESL twins are
/// `pack` and `unpack`.
///
/// | Word | Bits |
/// | --- | --- |
/// | 0 | x (16), z (16), over ±`extent` around the view's instance origin |
/// | 1 | y (16) over ±`HEIGHT_RANGE`, facing (8), seed (8) |
/// | 2 | height (8), width (8), field (8), LOD (2), flags (6) |
/// | 3 | bend x and z (16 each: 12 for this frame, 4 for last frame's as a step from it) |
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
        pack_bend(blade.bend.x, blade.previous_bend.x)
            | (pack_bend(blade.bend.y, blade.previous_bend.y) << 16),
    ]
}

/// Unpacks the 16-byte instance format.
pub(crate) fn unpack(words: [u32; 4], extent: f32) -> Blade {
    let bend_x = unpack_bend(words[3] & 0xffff);
    let bend_z = unpack_bend(words[3] >> 16);
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
        bend: Vec2::new(bend_x.0, bend_z.0),
        previous_bend: Vec2::new(bend_x.1, bend_z.1),
    }
}
/// The clump a root at `point` (world XZ) belongs to, 0–63. The WESL twin
/// is `clump_of`.
pub(crate) fn clump_of(point: Vec2) -> u8 {
    let entry = (point * CLUMP_PER_METRE as f32).floor().as_ivec2() & (CLUMP_SIDE - 1);
    let index = (entry.y * CLUMP_SIDE + entry.x) as usize;
    ((clump_table()[index / 4] >> ((index % 4) * 8)) & 63) as u8
}

/// The nearest tuft centre to `q` (in tuft spacings) and the tuft's random
/// value in `[0, 1]`. The WESL twin is `tuft_at`.
pub(crate) fn tuft_at(q: Vec2) -> (Vec2, f32) {
    let entry = (q * TUFT_ENTRIES_PER_TUFT as f32).floor();
    let wrapped = entry.as_ivec2() & (TUFT_SIDE - 1);
    let word = tuft_table()[(wrapped.y * TUFT_SIDE + wrapped.x) as usize];
    let offset = (Vec2::new((word & 0xff) as f32, ((word >> 8) & 0xff) as f32) / 255.0 - 0.5) * 4.0;
    let centre = (entry + 0.5) / TUFT_ENTRIES_PER_TUFT as f32 + offset;
    (centre, ((word >> 16) & 0xff) as f32 / 255.0)
}
/// The noise bare patches are cut from, at world point `at`, for patches
/// about `size` metres across: two samples of the gust noise, the second
/// turned and scaled, so the pattern does not visibly repeat. The WESL twin
/// is `bare_noise`.
#[allow(clippy::manual_midpoint, reason = "the same arithmetic as the GPU's")]
pub(crate) fn bare_noise(noise: &[u8], at: Vec2, size: f32) -> f32 {
    let uv = at / (size * 8.0);
    let turned = Vec2::new(uv.x * 0.8 - uv.y * 0.6, uv.x * 0.6 + uv.y * 0.8) * 1.37 + 0.5;
    0.5 * (sample_noise(noise, uv) + sample_noise(noise, turned))
}
/// Which of a field's species a blade is: 0 for the style's own, `k` for
/// the `k`th of `species`, given as (chance, clustering) pairs. A species'
/// chance applies to the blades the species before it left. A clustered
/// species draws its random number from the blade's clump instead of the
/// blade: shuffled per species, and spread within the clump by the blade's
/// own number, so the shares stay exact. The WESL twin is in `evaluate`.
pub(crate) fn pick_species(seed: u32, clump: u32, species: &[(f32, f32)]) -> usize {
    for (index, (chance, clustering)) in species.iter().enumerate() {
        let k = index as u32 + 1;
        let salt = k.wrapping_mul(0x9e37_79b9);
        let scattered = unit(hash(seed ^ salt));
        let mixer = hash(salt ^ 0x5bd1_e995);
        let shuffled = (clump
            .wrapping_mul((mixer & 0x3e) | 1)
            .wrapping_add(mixer >> 8))
            & 63;
        let patched = (shuffled as f32 + unit(hash(seed ^ salt ^ 0x68e3_1da4))) / 64.0;
        let clustered = unit(hash(seed ^ salt ^ 0x1b87_3593)) < *clustering;
        if (if clustered { patched } else { scattered }) < *chance {
            return k as usize;
        }
    }
    0
}

/// The three Bézier control points of a blade of length `height`, leaning
/// `lean` (a fraction of the height, horizontal) along +Z, bent by `bend`
/// (a horizontal tip offset as a fraction of the height) and drooping by
/// `droop` (0 to 1). The WESL twin is `blade_curve`.
///
/// The tip is pushed horizontally, then pulled back down so the curve keeps
/// its length: wind bends a blade, never stretches it. The construction is
/// Jahrmann and Wimmer's (2017), with the length correction applied twice so
/// that strong bends stay within 1% of the rest length. A drooping blade
/// reaches further, its tip sags and its control point rises: it stands up
/// from the root and arches over.
pub(crate) fn blade_curve(height: f32, lean: f32, bend: Vec2, droop: f32) -> [Vec3; 3] {
    let wanted = (Vec2::new(0.0, lean) + bend) * height;
    // An upright blade has no side to droop toward.
    let droop = if wanted.length_squared() > 1e-12 {
        droop
    } else {
        0.0
    };
    let horizontal = (wanted.length() + droop * 0.7 * height).min(height * 0.95);
    let offset = wanted.normalize_or_zero() * horizontal;
    let p0 = Vec3::ZERO;
    let mut p2 = Vec3::new(
        offset.x,
        (height * height - horizontal * horizontal).max(0.0).sqrt() * (1.0 - 0.7 * droop),
        offset.y,
    );
    // The control point sits above the root, lower the further the tip leans,
    // which is what makes the blade arch instead of hinging at the root.
    let ratio = horizontal / height;
    let mut p1 = Vec3::new(0.0, height * (1.0 - ratio).max(0.05 * ratio.max(1.0)), 0.0);
    p1.y += (height * 0.9 - p1.y) * (droop * 2.0).min(1.0);
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
pub(crate) fn bezier(points: &[Vec3; 3], t: f32) -> Vec3 {
    let u = 1.0 - t;
    points[0] * (u * u) + points[1] * (2.0 * u * t) + points[2] * (t * t)
}
/// Cells along a trample window's side. Twin of `trample::TEXELS`.
const TRAMPLE_TEXELS: i32 = 256;

/// A push packed for `atomicMax`: strength in the top 10 bits, so the larger
/// word is always the stronger push, and the direction as two 11-bit
/// components below. The WESL twin is `trample_pack`.
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
pub(crate) fn trample_unpack(word: u32) -> Vec2 {
    let strength = (word >> 22) as f32 / 1023.0;
    let direction =
        Vec2::new(((word >> 11) & 0x7ff) as f32, (word & 0x7ff) as f32) / 2047.0 * 2.0 - 1.0;
    direction * strength
}

/// Index of a world cell's slot in the window buffer. The WESL twin is
/// `trample_slot`.
pub(crate) fn trample_slot(cell: IVec2) -> u32 {
    let wrapped = cell & IVec2::splat(TRAMPLE_TEXELS - 1);
    (wrapped.y * TRAMPLE_TEXELS + wrapped.x) as u32
}

/// The world cell a slot holds while the window's lowest cell is `origin`.
/// The WESL twin is `trample_cell`.
pub(crate) fn trample_cell(slot: IVec2, origin: IVec2) -> IVec2 {
    origin + ((slot - origin) & IVec2::splat(TRAMPLE_TEXELS - 1))
}

/// The WESL twin is `in_trample_window`.
pub(crate) fn in_trample_window(cell: IVec2, origin: IVec2) -> bool {
    cell.cmpge(origin).all() && cell.cmplt(origin + TRAMPLE_TEXELS).all()
}

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
        assert_eq!(coverage_width(1.0, 2.5), 1.0);
        assert!((coverage_width(0.5, 2.5) - 2f32.sqrt()).abs() < 1e-6);
        assert_eq!(coverage_width(0.01, 2.5), 2.5);
        // A game can allow more or less widening.
        assert_eq!(coverage_width(0.01, 1.0), 1.0);
        assert_eq!(coverage_width(0.01, 3.0), 3.0);
        assert!((coverage_width(0.25, 3.0) - 2.0).abs() < 1e-6);
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
            previous_bend: Vec2::new(-0.47, 0.21),
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
        assert!((back.bend - blade.bend).abs().max_element() <= 1.0 / 4095.0 + 1e-6);
        assert!(
            (back.previous_bend - blade.previous_bend)
                .abs()
                .max_element()
                <= 1.0 / 4095.0 + 0.5 / BEND_DELTA_STEPS + 1e-6
        );
    }

    #[test]
    fn a_large_change_in_bend_is_clamped_not_wrapped() {
        for (bend, previous) in [(0.0, 0.9), (0.0, -0.9), (1.0, -1.0), (-1.0, 1.0)] {
            let (now, back) = unpack_bend(pack_bend(bend, previous));
            // Half a 12-bit step, 1 / 4095, and a little for rounding.
            assert!((now - bend).abs() <= 1.0 / 4095.0 + 1e-6);
            // The furthest step toward last frame's bend.
            assert!((back - previous).abs() < (previous - bend).abs());
            assert!((back - now).abs() <= 8.0 / BEND_DELTA_STEPS);
        }
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
                    let curve = blade_curve(height, lean, Vec2::new(bend, 0.0), 0.0);
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
    fn a_drooping_blade_arches_over_and_keeps_its_length() {
        for height in [0.1, 0.5, 1.4] {
            for droop in [0.05, 0.25, 0.5, 1.0] {
                for (lean, bend) in [(0.02, 0.0), (0.1, 0.0), (0.3, 0.0), (0.1, 0.6), (0.0, -0.9)] {
                    let curve = blade_curve(height, lean, Vec2::new(bend, 0.0), droop);
                    // The length correction assumes a gentle arch; a
                    // drooping blade's tighter curve comes out within 6%,
                    // which blades varying ±50% in height hide.
                    let length = curve_length(&curve);
                    assert!(
                        (length - height).abs() <= height * 0.06,
                        "height {height}, droop {droop}: length {length}"
                    );
                    // From half on, the tip hangs below the blade's highest
                    // point.
                    let top = (0..=64)
                        .map(|i| bezier(&curve, i as f32 / 64.0).y)
                        .fold(0.0, f32::max);
                    assert!(droop < 0.5 || curve[2].y < top - height * 0.05 * droop);
                }
            }
        }
        // A blade with no lean or bend has no side to droop toward: it stays
        // upright.
        assert_eq!(
            blade_curve(1.0, 0.0, Vec2::ZERO, 0.6),
            blade_curve(1.0, 0.0, Vec2::ZERO, 0.0)
        );
    }

    #[test]
    fn tufts_are_the_nearest_jittered_centres() {
        // Tuft centres sit one per tuft spacing, jittered, and every point
        // belongs to its nearest centre, give or take the table's rounding.
        for i in 0..4000 {
            let q = Vec2::new(unit(hash(i)), unit(hash(i ^ 7))) * 200.0 - 100.0;
            let (centre, _) = tuft_at(q);
            assert!(q.distance(centre) < 1.5, "{q} -> {centre}");
            let entry = (q * TUFT_ENTRIES_PER_TUFT as f32).floor() + 0.5;
            let entry = entry / TUFT_ENTRIES_PER_TUFT as f32;
            let (entry_centre, _) = tuft_at(entry);
            assert!(entry_centre.distance(centre) < 1e-4);
        }
        // The table repeats every 64 tufts.
        let (a, ra) = tuft_at(Vec2::new(3.3, 7.9));
        let (b, rb) = tuft_at(Vec2::new(3.3 + 64.0, 7.9 - 128.0));
        assert!((b - a - Vec2::new(64.0, -128.0)).length() < 1e-3);
        assert_eq!(ra, rb);
    }

    #[test]
    fn bare_patches_cover_their_share() {
        let noise = gust_noise(GUST_SIZE);
        for share in [0.05, 0.2, 0.5] {
            let threshold = bare_threshold(share);
            let bare = (0..20_000)
                .filter(|&i| {
                    let at = Vec2::new(unit(hash(i ^ 0x51)), unit(hash(i ^ 0x52))) * 500.0;
                    bare_noise(&noise, at, 3.0) < threshold
                })
                .count() as f32
                / 20_000.0;
            assert!((bare - share).abs() < 0.03, "{share}: {bare}");
        }
        assert!(bare_threshold(0.0) <= bare_threshold(0.3));
    }

    #[test]
    fn species_take_their_share_scattered_or_clustered() {
        // Two species besides the style's own blades: 10% and 3%.
        for clustering in [0.0, 0.8, 1.0] {
            let chances = [(0.1, clustering), (0.03 / 0.9, clustering)];
            let mut counts = [0usize; 3];
            let blades = 200_000;
            for i in 0..blades {
                let clump = hash(i ^ 0x77) & 63;
                counts[pick_species(hash(i), clump, &chances)] += 1;
            }
            let share = |n: usize| n as f32 / blades as f32;
            assert!(
                (share(counts[1]) - 0.1).abs() < 0.01,
                "{clustering}: {counts:?}"
            );
            assert!(
                (share(counts[2]) - 0.03).abs() < 0.005,
                "{clustering}: {counts:?}"
            );
        }
        // Clustered, a species fills some clumps and leaves the rest.
        let chances = [(0.1, 1.0)];
        let mut per_clump = [0usize; 64];
        for i in 0..64_000 {
            let clump = i % 64;
            if pick_species(hash(i), clump, &chances) == 1 {
                per_clump[clump as usize] += 1;
            }
        }
        let empty = per_clump.iter().filter(|&&n| n == 0).count();
        assert!(empty >= 56, "{per_clump:?}");
    }

    #[test]
    fn lods_have_the_planned_vertex_and_triangle_counts() {
        let expected = [(15, 13), (7, 5), (3, 1)];
        for (rows, (vertices, triangles)) in GrassSettings::HIGH.blade_rows.iter().zip(expected) {
            let indices = lod_indices(*rows);
            assert_eq!(indices.len() / 3, triangles);
            assert_eq!(indices.iter().max().copied().unwrap() + 1, vertices);
            assert!(vertices < LOD_VERTEX_STRIDE);
        }
        // The most rows a game may ask for still fit the stride.
        let most = lod_indices(GrassSettings::MAX_BLADE_ROWS);
        assert!(most.iter().max().copied().unwrap() < LOD_VERTEX_STRIDE);
    }

    #[test]
    fn width_range_covers_variation_times_widening() {
        assert!(WIDTH_RANGE >= 1.25 * coverage_width(0.0, GrassSettings::MAX_WIDENING));
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
