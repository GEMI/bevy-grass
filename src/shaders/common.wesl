//! bevy_grass::shaders::common — data layouts and pure functions shared by the
//! compute passes and the draws. No bindings live here.
//!
//! Functions marked "twin" must stay identical to their counterparts in
//! `src/mirror.rs`, which is where their properties are tested.

const TAU: f32 = 6.283185307;
/// Seconds after which `Frame::time` starts again. Twin of
/// `clock::TIME_PERIOD`.
const TIME_PERIOD: f32 = 3600.0;
const TILE_POINTS: u32 = 1024u;
const WIDTH_RANGE: f32 = 4.0;
const LOD_VERTEX_STRIDE: u32 = 64u;
/// Twin of `gpu_layout::NO_MAP`.
const NO_MAP: u32 = 0xffffffffu;
/// Twin of `trample::TEXELS`.
const TRAMPLE_TEXELS: i32 = 256;

/// Per frame, shared by every view. The wind lives in its own `Wind`
/// uniform (`wind.wesl`), so another wind model can be dropped in.
struct Frame {
    time: f32,
    previous_time: f32,
    chunk_size: f32,
    chunk_count: u32,
    /// Rows in a blade of each LOD (`GrassSettings::blade_rows`), x for LOD 0.
    lod_rows: vec4<u32>,
}

/// `frequency` (radians per second) rounded to a whole number of cycles per
/// `TIME_PERIOD`, so that `sin(time * periodic(f))` is continuous where
/// `time` wraps. The rounding is below 0.1% for any frequency above 1.
fn periodic(frequency: f32) -> f32 {
    let step = TAU / TIME_PERIOD;
    return round(frequency / step) * step;
}

/// Per view.
struct View {
    /// Frustum planes, normals pointing inward, normalized.
    planes: array<vec4<f32>, 6>,
    /// xyz: camera position; w: 1 for orthographic views.
    camera_position: vec4<f32>,
    /// xyz: camera forward; w: metres per pixel per unit of clip-space w.
    camera_forward: vec4<f32>,
    /// xyz: where a blade's shadow lands, per metre of blade height; w: 1
    /// when a directional light casts shadows.
    shadow_offset: vec4<f32>,
    /// xyz: the origin instance positions are relative to; w: their extent.
    instance_origin: vec4<f32>,
    radius: f32,
    lod1_start: f32,
    lod2_start: f32,
    lod_band: f32,
    /// Where the mid-range thinning begins, in metres.
    mid_range_start: f32,
    density_scale: f32,
    /// LOD for every blade, or 3 for distance-based LOD.
    fixed_lod: u32,
    max_visible_chunks: u32,
    /// Interactors stamped this frame.
    interactor_count: u32,
    /// The render layers the view draws, one bit each.
    layers: u32,
    /// Narrowest a blade is drawn, in pixels.
    min_width_px: f32,
    /// Metres taken off every distance to the camera before the radius, the
    /// thinning and the levels of detail apply (`radius_from_ground`).
    distance_offset: f32,
    bucket_base: vec4<u32>,
    bucket_capacity: vec4<u32>,
    /// The LOD each bucket's blades are drawn with.
    bucket_lod: vec4<u32>,
    /// xy: the trample window's lowest cell this frame; zw: last frame's.
    trample_origin: vec4<i32>,
    /// x: cell size in metres; y: flattening recovered this frame (0–1).
    trample_step: vec4<f32>,
    /// xyz: direction to the first directional light; w: 1 when there is one.
    sun: vec4<f32>,
    /// Density along the distance table (`GrassSettings`): x: the share kept
    /// where LOD 2 starts; y: the share kept where the last fade starts; z:
    /// the extra share kept from LOD 1 on; w: where the last fade starts, as
    /// a share of the radius.
    lod_density: vec4<f32>,
    /// x: the most a thinned blade widens; y: the largest interactor radius;
    /// z: how far an edge-on blade turns toward the camera; w: the vertical
    /// range instance positions are stored over, in metres either side of
    /// the origin.
    limits: vec4<f32>,
}

/// One field. Mirrors `FieldGpu` in `gpu_layout.rs`.
struct Field {
    min_corner: vec4<f32>,
    /// x, y: size; z: heightmap scale; w: tile size.
    extent: vec4<f32>,
    /// x: shortest, y: tallest, z: width, w: density.
    shape: vec4<f32>,
    /// x: lean, y: stiffness, w: root occlusion.
    motion: vec4<f32>,
    root_color: vec4<f32>,
    tip_color: vec4<f32>,
    /// x: seed, y: tiles per chunk side, z: mask offset, w: height offset.
    placement: vec4<u32>,
    map_sizes: vec4<u32>,
    /// rgb: the ground's colour; w: how far roots blend toward it.
    ground: vec4<f32>,
    /// x: specular, y: toon bands (0: physically based).
    look: vec4<f32>,
    /// x: this species' chance of taking a blade the species before it left;
    /// y: its clustering; z: species in the field and w: the tallest blade
    /// of any of them (both on the field's first entry).
    mix: vec4<f32>,
    /// x: twist, in turns; y: droop; z: fold; w: head, a share of the length.
    form: vec4<f32>,
    /// rgb: head colour; w: head width.
    head: vec4<f32>,
    /// rgb: patch colour over tip colour; w: patch amount.
    patches: vec4<f32>,
    /// x: patch size; y: dry tips; z: dead blades.
    variation: vec4<f32>,
    /// x: tuft pull; y: tuft size; z: bare-patch noise threshold (-1: none);
    /// w: bare patch size.
    tufts: vec4<f32>,
    /// x: render layers the field is drawn on, one bit each; 0 while hidden.
    access: vec4<u32>,
    /// x, y: the field's own X axis in world XZ; its Z axis is (-y, x).
    axis: vec4<f32>,
}

/// One chunk. Mirrors `ChunkGpu` in `gpu_layout.rs`.
struct Chunk {
    bounds: vec4<f32>,
    ids: vec4<u32>,
}

/// Index ranges of the three LODs in the shared index buffer.
struct LodTable {
    index_count: vec4<u32>,
    first_index: vec4<u32>,
}

/// Twin of `mirror::hash`.
fn hash(x: u32) -> u32 {
    let state = x * 747796405u + 2891336453u;
    let word = ((state >> ((state >> 28u) + 4u)) ^ state) * 277803737u;
    return (word >> 22u) ^ word;
}

/// Twin of `mirror::hash2`.
fn hash2(a: u32, b: u32) -> u32 {
    return hash(a ^ hash(b));
}

/// Twin of `mirror::unit`.
fn unit(h: u32) -> f32 {
    return f32(h >> 8u) / 16777216.0;
}

/// Twin of `DistanceLod::falloff`.
fn density_falloff(view: View, distance: f32) -> f32 {
    let into_mid = clamp((distance - (view.mid_range_start - view.lod_band * 0.5)) / view.lod_band, 0.0, 1.0);
    return distance_thinning(view, distance) * mix(1.0, view.lod_density.z, into_mid);
}

/// The distance table, before the mid-range thinning: 1 to the start of LOD 1,
/// `lod_density.x` where LOD 2 starts, `lod_density.y` where the fade starts,
/// 0 at the radius. A boundary that coincides with the next (an empty span)
/// is never divided by.
fn distance_thinning(view: View, distance: f32) -> f32 {
    let edge = view.radius * view.lod_density.w;
    if distance <= view.lod1_start {
        return 1.0;
    }
    if distance <= view.lod2_start {
        return mix(1.0, view.lod_density.x, clamp((distance - view.lod1_start) / (view.lod2_start - view.lod1_start), 0.0, 1.0));
    }
    if distance <= edge {
        return mix(view.lod_density.x, view.lod_density.y, clamp((distance - view.lod2_start) / (edge - view.lod2_start), 0.0, 1.0));
    }
    return mix(view.lod_density.y, 0.0, clamp((distance - edge) / max(view.radius - edge, 1e-4), 0.0, 1.0));
}

/// The share of blades a view raised by `distance_offset` keeps
/// (`radius_from_ground`): the ground it sees grows with the square of its
/// height, so the blades thin by as much and widen to cover, and their
/// number stays about the same at any height. 1 when the offset is 0. Twin
/// of `mirror::height_thinning`.
fn height_thinning(view: View) -> f32 {
    let share = view.radius / (view.radius + view.distance_offset);
    return share * share;
}

/// Twin of `DistanceLod::lod`.
fn select_lod(view: View, distance: f32, dither: f32) -> u32 {
    var lod = 0u;
    if dither < (distance - (view.lod1_start - view.lod_band * 0.5)) / view.lod_band {
        lod += 1u;
    }
    if dither < (distance - (view.lod2_start - view.lod_band * 0.5)) / view.lod_band {
        lod += 1u;
    }
    return lod;
}

/// Twin of `mirror::coverage_width`; `widest` is `View::limits.x`.
fn coverage_width(falloff: f32, widest: f32) -> f32 {
    return min(1.0 / sqrt(max(falloff, 1.0 / (widest * widest))), widest);
}

fn quantize(value: f32, max_value: f32) -> u32 {
    return u32(round(clamp(value, 0.0, 1.0) * max_value));
}

/// Steps of last frame's bend per unit of bend. Twin of
/// `mirror::BEND_DELTA_STEPS`.
const BEND_DELTA_STEPS: f32 = 128.0;

/// One axis of a blade's bend in 16 bits: the bend in the low 12 (8 bits
/// made the blades step visibly as the wind moved them), and last frame's
/// bend, for motion vectors, as a 4-bit step from it. Twin of
/// `mirror::pack_bend`.
fn pack_bend(bend: f32, previous: f32) -> u32 {
    let now = quantize(bend * 0.5 + 0.5, 4095.0);
    let decoded = f32(now) / 4095.0 * 2.0 - 1.0;
    let step = clamp(round((previous - decoded) * BEND_DELTA_STEPS), -8.0, 7.0);
    return now | (u32(i32(step) + 8) << 12u);
}

/// This frame's and last frame's bend from `pack_bend`'s 16 bits.
fn unpack_bend(bits: u32) -> vec2<f32> {
    let now = f32(bits & 0xfffu) / 4095.0 * 2.0 - 1.0;
    let step = f32(i32((bits >> 12u) & 0xfu) - 8);
    return vec2<f32>(now, now + step / BEND_DELTA_STEPS);
}

/// A blade as the generation pass produces it.
struct BladeData {
    /// World-space root.
    root: vec3<f32>,
    facing: f32,
    /// Fractions of the style's tallest blade and of WIDTH_RANGE × its width.
    height: f32,
    width: f32,
    seed: u32,
    lod: u32,
    field: u32,
    /// 0–63; see `clump_of`.
    clump: u32,
    bend: vec2<f32>,
    previous_bend: vec2<f32>,
}

/// Twin of `mirror::pack`, with positions made relative to the view's origin.
/// `height_range` is `View::limits.w`.
fn pack(blade: BladeData, origin: vec4<f32>, height_range: f32) -> vec4<u32> {
    let relative = blade.root - origin.xyz;
    let x = quantize(relative.x / (2.0 * origin.w) + 0.5, 65535.0);
    let z = quantize(relative.z / (2.0 * origin.w) + 0.5, 65535.0);
    let y = quantize(relative.y / (2.0 * height_range) + 0.5, 65535.0);
    let facing = u32(fract(blade.facing) * 256.0) & 0xffu;
    return vec4<u32>(
        x | (z << 16u),
        y | (facing << 16u) | ((blade.seed & 0xffu) << 24u),
        quantize(blade.height, 255.0) | (quantize(blade.width, 255.0) << 8u) | ((blade.field & 0xffu) << 16u)
            | ((blade.lod & 3u) << 24u) | ((blade.clump & 63u) << 26u),
        pack_bend(blade.bend.x, blade.previous_bend.x) | (pack_bend(blade.bend.y, blade.previous_bend.y) << 16u),
    );
}

/// An instance, unpacked into world units.
struct Instance {
    root: vec3<f32>,
    facing: f32,
    height: f32,
    width: f32,
    /// In [0, 1].
    seed: f32,
    /// The blade's clump tint, in [0, 1].
    clump: f32,
    field: u32,
    bend: vec2<f32>,
    previous_bend: vec2<f32>,
}

/// Twin of `mirror::unpack`, scaled by the field's style. `height_range` is
/// `View::limits.w`.
fn unpack(words: vec4<u32>, origin: vec4<f32>, height_range: f32) -> Instance {
    var out: Instance;
    out.field = (words.z >> 16u) & 0xffu;
    out.root = origin.xyz + vec3<f32>(
        (f32(words.x & 0xffffu) / 65535.0 - 0.5) * 2.0 * origin.w,
        (f32(words.y & 0xffffu) / 65535.0 - 0.5) * 2.0 * height_range,
        (f32(words.x >> 16u) / 65535.0 - 0.5) * 2.0 * origin.w,
    );
    out.facing = f32((words.y >> 16u) & 0xffu) / 256.0;
    out.seed = f32(words.y >> 24u) / 255.0;
    out.clump = f32(words.z >> 26u) / 63.0;
    out.height = f32(words.z & 0xffu) / 255.0;
    out.width = f32((words.z >> 8u) & 0xffu) / 255.0;
    let bend_x = unpack_bend(words.w & 0xffffu);
    let bend_z = unpack_bend(words.w >> 16u);
    out.bend = vec2<f32>(bend_x.x, bend_z.x);
    out.previous_bend = vec2<f32>(bend_x.y, bend_z.y);
    return out;
}

/// The three control points of a blade, in world space relative to its root.
/// Twin of `mirror::blade_curve`, with the lean and bend already in world XZ.
/// `droop` is 0 without `GRASS_SHAPE`, and the curve is then exactly the
/// plain blade's.
fn blade_curve(height: f32, offset_fraction: vec2<f32>, droop_wanted: f32) -> array<vec3<f32>, 3> {
    let wanted = offset_fraction * height;
    // An upright blade has no side to droop toward.
    let droop = select(0.0, droop_wanted, dot(wanted, wanted) > 1e-12);
    let horizontal = min(length(wanted) + droop * 0.7 * height, height * 0.95);
    let offset = select(vec2<f32>(0.0), normalize(wanted) * horizontal, horizontal > 1e-6 && dot(wanted, wanted) > 1e-12);
    let p0 = vec3<f32>(0.0);
    var p2 = vec3<f32>(offset.x, sqrt(max(height * height - horizontal * horizontal, 0.0)) * (1.0 - 0.7 * droop), offset.y);
    let ratio = horizontal / max(height, 1e-6);
    var p1 = vec3<f32>(0.0, height * max(1.0 - ratio, 0.05 * max(ratio, 1.0)), 0.0);
    p1.y += (height * 0.9 - p1.y) * min(droop * 2.0, 1.0);
    for (var i = 0; i < 2; i++) {
        let chord = distance(p2, p0);
        let polygon = distance(p1, p0) + distance(p2, p1);
        let scale = height / max((2.0 * chord + polygon) / 3.0, 1e-6);
        p1 = p0 + (p1 - p0) * scale;
        p2 = p1 + (p2 - p1) * scale;
    }
    return array<vec3<f32>, 3>(p0, p1, p2);
}

/// A random number in [0, 1) for a blade and `salt`, from the seed, facing
/// and clump its instance carries.
fn instance_random(instance: Instance, salt: u32) -> f32 {
    let bits = u32(round(instance.seed * 255.0)) | (u32(instance.facing * 256.0) << 8u)
        | (u32(round(instance.clump * 63.0)) << 16u);
    return unit(hash(bits ^ salt));
}

/// The colour of dried grass of `color`'s brightness.
fn straw_of(color: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(dot(color, vec3<f32>(0.3, 0.59, 0.11))) * vec3<f32>(1.25, 1.1, 0.6);
}

/// Rows in a blade of level of detail `lod`.
fn lod_rows(frame: Frame, lod: u32) -> u32 {
    return frame.lod_rows[lod];
}

/// Twin of `mirror::trample_pack`. A push is packed with its strength in the
/// top 10 bits, so `atomicMax` on the packed word keeps the strongest push,
/// and its direction as two 11-bit components below, so unpacking needs no
/// trigonometry.
fn trample_pack(push: vec2<f32>) -> u32 {
    let strength = clamp(length(push), 0.0, 1.0);
    let direction = select(vec2<f32>(1.0, 0.0), push / max(length(push), 1e-6), strength > 1e-6);
    let d = vec2<u32>(round((direction * 0.5 + 0.5) * 2047.0));
    return (u32(round(strength * 1023.0)) << 22u) | (d.x << 11u) | d.y;
}

/// Twin of `mirror::trample_unpack`.
fn trample_unpack(word: u32) -> vec2<f32> {
    let strength = f32(word >> 22u) / 1023.0;
    let direction = vec2<f32>(f32((word >> 11u) & 0x7ffu), f32(word & 0x7ffu)) / 2047.0 * 2.0 - 1.0;
    return direction * strength;
}

/// Twin of `mirror::trample_slot`: where a world cell lives in the window.
/// The side is a power of two, so a mask is the positive modulo, negative
/// cells included; integer division made blade generation 30% slower.
fn trample_slot(cell: vec2<i32>) -> u32 {
    let wrapped = cell & vec2<i32>(TRAMPLE_TEXELS - 1);
    return u32(wrapped.y * TRAMPLE_TEXELS + wrapped.x);
}

/// Twin of `mirror::trample_cell`: the world cell slot `slot` holds while
/// the window's lowest cell is `origin`.
fn trample_cell(slot: vec2<i32>, origin: vec2<i32>) -> vec2<i32> {
    return origin + ((slot - origin) & vec2<i32>(TRAMPLE_TEXELS - 1));
}

/// Twin of `mirror::in_trample_window`.
fn in_trample_window(cell: vec2<i32>, origin: vec2<i32>) -> bool {
    return all(cell >= origin) && all(cell < origin + TRAMPLE_TEXELS);
}
