//! Test-only CPU oracle for the GPU's placement and generation logic.
//!
//! This module mirrors the WESL shaders bit for bit where integers are
//! involved. It is compiled only for tests, so production code cannot
//! accidentally depend on it. Its WESL twins live in `shaders/common.wesl`
//! and `shaders/generate.wesl`.

use std::f32::consts::TAU;

use bevy::math::{IVec2, Vec2, Vec3};

use crate::GrassSettings;

use crate::shader_model::{
    CLUMP_PER_METRE, CLUMP_SIDE, ChunkLayout, DistanceLod, GUST_SIZE, TILE_POINTS,
    TUFT_ENTRIES_PER_TUFT, TUFT_SIDE, bare_noise, bare_threshold, best_candidate_tile,
    blue_noise_tile, clump_table, gust_noise, hash, hash2, lod_indices, toroidal_distance_squared,
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
    /// shows as a line), times `mid_range_density` from a dither band around
    /// `mid_range_start` on. The WESL twin is `density_falloff`.
    pub(crate) fn falloff(&self, distance: f32) -> f32 {
        let into_mid =
            ((distance - (self.mid_range_start - self.band * 0.5)) / self.band).clamp(0.0, 1.0);
        self.distance_thinning(distance) * lerp(1.0, self.mid_range_density, into_mid)
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

mod tests;
