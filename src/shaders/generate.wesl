//! bevy_grass::shaders::generate — the per-view compute passes that decide
//! which blades exist this frame. Run in this order for every view:
//!
//! 1. `cull`: one thread per chunk. Frustum, radius and density test; the
//!    survivors go into the visible list with how many candidates they need.
//! 2. `size_dispatch`: one thread. Turns the visible count into the indirect
//!    dispatch arguments for pass 3.
//! 3. `generate`: one workgroup per visible chunk. Evaluates candidates,
//!    thins, culls, picks a LOD, computes the wind, and appends survivors to
//!    their bucket.
//! 4. `write_draw_args`: one thread. Turns bucket counts into indirect draw
//!    arguments, clamped to capacity.
//!
//! Before them, the passes in `trample.wesl` update the view's trample cells,
//! and `generate` samples them at every blade's root.

import package::shaders::common::{
    Frame, View, Field, Chunk, LodTable, BladeData, TILE_POINTS, NO_MAP, WIDTH_RANGE, TAU,
    LOD_VERTEX_STRIDE, hash, hash2, unit, density_falloff, select_lod, coverage_width, pack,
    trample_unpack, trample_slot, in_trample_window,
};

// Shared by every view.
@group(0) @binding(0) var<uniform> frame: Frame;
@group(0) @binding(1) var<storage, read> fields: array<Field>;
@group(0) @binding(2) var<storage, read> chunks: array<Chunk>;
@group(0) @binding(3) var<storage, read> masks: array<u32>;
@group(0) @binding(4) var<storage, read> heights: array<f32>;
@group(0) @binding(5) var<storage, read> tile: array<vec2<f32>>;
@group(0) @binding(6) var gust_texture: texture_2d<f32>;
@group(0) @binding(7) var gust_sampler: sampler;
@group(0) @binding(8) var<uniform> lods: LodTable;
/// `mirror::clump_table`: a clump per 25 cm square of a 64 m tile, four bytes
/// to a word.
@group(0) @binding(9) var<storage, read> clumps: array<u32>;
/// `mirror::tuft_table`: the nearest tuft centre for every quarter of a
/// tuft spacing of a 64-tuft tile.
@group(0) @binding(10) var<storage, read> tuft_table: array<u32>;

/// One slice of a visible chunk's candidates: the chunk, the slice's first
/// candidate and how many it holds. A dense chunk is several slices, so
/// several workgroups generate it at once.
struct VisibleChunk {
    chunk: u32,
    first: u32,
    count: u32,
    _pad: u32,
}

struct DispatchArgs {
    x: u32,
    y: u32,
    z: u32,
}

struct DrawArgs {
    index_count: u32,
    instance_count: u32,
    first_index: u32,
    base_vertex: i32,
    first_instance: u32,
}

// This view.
@group(1) @binding(0) var<uniform> view: View;
@group(1) @binding(1) var<storage, read_write> visible: array<VisibleChunk>;
/// 0–2: blades per bucket (unclamped); 3: visible chunk slices (unclamped).
@group(1) @binding(2) var<storage, read_write> counters: array<atomic<u32>, 4>;
@group(1) @binding(3) var<storage, read_write> instances: array<vec4<u32>>;
@group(1) @binding(4) var<storage, read_write> draw_args: array<DrawArgs, 3>;
/// One packed push per trample cell, addressed by `trample_slot`. Read with
/// plain loads: `trample.wesl` writes it in earlier dispatches.
@group(1) @binding(5) var<storage, read_write> trample: array<u32>;
// Only `size_dispatch` writes the dispatch arguments, in a group of their own:
// `generate` reads them as its indirect arguments, and one dispatch may not
// use a buffer both ways.
@group(2) @binding(0) var<storage, read_write> dispatch: DispatchArgs;

const WORKGROUP: u32 = 64u;
/// Candidates per slice: one workgroup's share of a chunk.
const SLICE: u32 = 512u;
const FIXED_LOD_NONE: u32 = 3u;

fn is_orthographic() -> bool {
    return view.camera_position.w > 0.5;
}

fn sphere_visible(center: vec3<f32>, radius: f32) -> bool {
    for (var i = 0; i < 6; i++) {
        if dot(view.planes[i].xyz, center) + view.planes[i].w < -radius {
            return false;
        }
    }
    return true;
}

fn box_visible(lo: vec3<f32>, hi: vec3<f32>) -> bool {
    for (var i = 0; i < 6; i++) {
        let n = view.planes[i].xyz;
        // The corner furthest along the plane's normal.
        let corner = select(lo, hi, n > vec3<f32>(0.0));
        if dot(n, corner) + view.planes[i].w < 0.0 {
            return false;
        }
    }
    return true;
}

fn chunk_origin(chunk: Chunk, field: Field) -> vec2<f32> {
    return field.min_corner.xz + chunk.bounds.xy;
}

@compute @workgroup_size(64)
fn cull(@builtin(global_invocation_id) id: vec3<u32>) {
    let index = id.x;
    if index >= frame.chunk_count {
        return;
    }
    let chunk = chunks[index];
    let field = fields[chunk.ids.x];
    // Hidden fields, and fields on layers this view does not draw.
    if (field.access.x & view.layers) == 0u {
        return;
    }
    let tallest = field.mix.w;
    let origin = chunk_origin(chunk, field);
    // Tufts pull blades up to one and a half tuft spacings from where they
    // were placed.
    let spread = tallest + select(0.0, field.tufts.y * 1.5, field.tufts.x > 0.0);
    // The chunk's ground box, grown by the tallest blade: up for its height,
    // sideways for how far it can lean or bend.
    var lo = vec3<f32>(origin.x - spread, chunk.bounds.z, origin.y - spread);
    var hi = vec3<f32>(origin.x + frame.chunk_size + spread, chunk.bounds.w + tallest, origin.y + frame.chunk_size + spread);
    if view.shadow_offset.w > 0.5 {
        // Blades just outside the view can still shadow ground inside it.
        let reach = view.shadow_offset.xyz * tallest;
        lo = min(lo, lo + reach);
        hi = max(hi, hi + reach);
    }
    if !box_visible(lo, hi) {
        return;
    }

    var falloff = 1.0;
    if !is_orthographic() {
        let nearest = clamp(view.camera_position.xyz, lo, hi);
        let d = distance(nearest, view.camera_position.xyz);
        if d >= view.radius {
            return;
        }
        falloff = density_falloff(view, d);
    }
    let tile_size = field.extent.w;
    let candidate_density = f32(TILE_POINTS) / (tile_size * tile_size);
    let keep = clamp(field.shape.w * view.density_scale * falloff / candidate_density, 0.0, 1.0);
    let cells = field.placement.y * field.placement.y;
    // Twin of `ChunkLayout::candidates_needed`.
    let needed = u32(ceil(keep * f32(TILE_POINTS))) * cells;
    if needed == 0u {
        return;
    }
    let slices = (needed + SLICE - 1u) / SLICE;
    let slot = atomicAdd(&counters[3], slices);
    for (var s = 0u; s < slices; s++) {
        if slot + s < view.max_visible_chunks {
            visible[slot + s] = VisibleChunk(index, s * SLICE, min(SLICE, needed - s * SLICE), 0u);
        }
    }
}

/// The push of the cell `point` is in. One cell rather than a bilinear
/// blend of four: at 0.25 m a cell is about one blade wide, screenshots
/// could not tell the two apart, and four loads made generation under a
/// 512-interactor horde 0.2 ms slower on the development GPU.
fn trample_push(point: vec2<f32>) -> vec2<f32> {
    let cell = vec2<i32>(floor(point / view.trample_step.x));
    if !in_trample_window(cell, view.trample_origin.xy) {
        return vec2<f32>(0.0);
    }
    return trample_unpack(trample[trample_slot(cell)]);
}

@compute @workgroup_size(1)
fn size_dispatch() {
    dispatch = DispatchArgs(min(atomicLoad(&counters[3]), view.max_visible_chunks), 1u, 1u);
}

/// Twin of `mirror::clump_of`.
fn clump_of(point: vec2<f32>) -> u32 {
    let entry = vec2<i32>(floor(point * 4.0)) & vec2<i32>(255);
    let index = u32(entry.y * 256 + entry.x);
    return (clumps[index / 4u] >> ((index % 4u) * 8u)) & 63u;
}

/// The nearest tuft centre to `q`, in tuft spacings, and the tuft's random
/// value in [0, 1]. Twin of `mirror::tuft_at`.
fn tuft_at(q: vec2<f32>) -> vec3<f32> {
    let entry = floor(q * 4.0);
    let wrapped = vec2<i32>(entry) & vec2<i32>(255);
    let word = tuft_table[u32(wrapped.y * 256 + wrapped.x)];
    let offset = (vec2<f32>(f32(word & 0xffu), f32((word >> 8u) & 0xffu)) / 255.0 - 0.5) * 4.0;
    return vec3<f32>((entry + 0.5) / 4.0 + offset, f32((word >> 16u) & 0xffu) / 255.0);
}

/// The noise bare patches are cut from. Twin of `mirror::bare_noise`.
fn bare_noise(at: vec2<f32>, size: f32) -> f32 {
    let uv = at / (size * 8.0);
    let turned = vec2<f32>(uv.x * 0.8 - uv.y * 0.6, uv.x * 0.6 + uv.y * 0.8) * 1.37 + 0.5;
    return 0.5 * (textureSampleLevel(gust_texture, gust_sampler, uv, 0.0).r
        + textureSampleLevel(gust_texture, gust_sampler, turned, 0.0).r);
}

/// Which of the field's species a blade is: the index of its entry. Twin
/// of `mirror::pick_species`.
fn pick_species(first: u32, field: Field, seed: u32, clump: u32) -> u32 {
    let count = u32(field.mix.z);
    for (var k = 1u; k < count; k++) {
        let species = fields[first + k];
        let salt = k * 0x9e3779b9u;
        let scattered = unit(hash(seed ^ salt));
        let mixer = hash(salt ^ 0x5bd1e995u);
        let shuffled = (clump * ((mixer & 0x3eu) | 1u) + (mixer >> 8u)) & 63u;
        let patched = (f32(shuffled) + unit(hash(seed ^ salt ^ 0x68e31da4u))) / 64.0;
        let clustered = unit(hash(seed ^ salt ^ 0x1b873593u)) < species.mix.y;
        if select(scattered, patched, clustered) < species.mix.x {
            return first + k;
        }
    }
    return first;
}

/// Bilinear R8G8 mask sample: density and height multiplier.
fn sample_mask(field: Field, uv: vec2<f32>) -> vec2<f32> {
    if field.placement.z == NO_MAP {
        return vec2<f32>(1.0);
    }
    let size = field.map_sizes.xy;
    let p = clamp(uv * vec2<f32>(size) - 0.5, vec2<f32>(0.0), vec2<f32>(size) - 1.0);
    let base = vec2<u32>(floor(p));
    let t = fract(p);
    let next = min(base + 1u, size - 1u);
    let a = mask_texel(field, base.x, base.y);
    let b = mask_texel(field, next.x, base.y);
    let c = mask_texel(field, base.x, next.y);
    let d = mask_texel(field, next.x, next.y);
    return mix(mix(a, b, t.x), mix(c, d, t.x), t.y);
}

fn mask_texel(field: Field, x: u32, y: u32) -> vec2<f32> {
    let texel = field.placement.z + y * field.map_sizes.x + x;
    let half = (masks[texel / 2u] >> ((texel % 2u) * 16u)) & 0xffffu;
    return vec2<f32>(f32(half & 0xffu), f32(half >> 8u)) / 255.0;
}

/// Ground height in world units at a point of the field (bilinear).
fn ground_height(field: Field, local: vec2<f32>) -> f32 {
    if field.placement.w == NO_MAP {
        return field.min_corner.y;
    }
    let size = field.map_sizes.zw;
    let p = clamp(local / field.extent.xy * vec2<f32>(size) - 0.5, vec2<f32>(0.0), vec2<f32>(size) - 1.0);
    let base = vec2<u32>(floor(p));
    let t = fract(p);
    let next = min(base + 1u, size - 1u);
    let row = field.placement.w;
    let a = heights[row + base.y * size.x + base.x];
    let b = heights[row + base.y * size.x + next.x];
    let c = heights[row + next.y * size.x + base.x];
    let d = heights[row + next.y * size.x + next.x];
    return field.min_corner.y + mix(mix(a, b, t.x), mix(c, d, t.x), t.y) * field.extent.z;
}

/// Tip offset from wind, as a fraction of blade height, in world XZ.
///
/// Gusts, a scrolling noise texture sampled at the root, give the waves that
/// roll across a field; sway, a per-blade sine, keeps neighbours from moving
/// in lockstep. Flutter is added per vertex by the draw.
fn wind_bend(root: vec2<f32>, seed: f32, stiffness: f32, time: f32) -> vec2<f32> {
    let raw = frame.wind_direction;
    let direction = select(vec2<f32>(1.0, 0.0), normalize(raw), dot(raw, raw) > 1e-8);
    let strength = frame.wind_strength;
    let uv = (root - direction * time * frame.gust_speed) / max(frame.gust_scale, 0.01);
    let gust = textureSampleLevel(gust_texture, gust_sampler, uv, 0.0).r;
    let softness = 1.0 - 0.75 * stiffness;
    let frequency = mix(1.1, 2.4, stiffness) * (0.85 + 0.3 * seed);
    let sway = sin(time * frequency + seed * TAU);
    let across = vec2<f32>(-direction.y, direction.x);
    let push = strength * (0.15 + 1.4 * gust * gust);
    let wobble = (direction * 0.5 + across * 0.5) * sway * (0.04 + 0.14 * strength);
    return clamp((direction * push + wobble) * softness, vec2<f32>(-1.0), vec2<f32>(1.0));
}

struct Generated {
    keep: bool,
    bucket: u32,
    words: vec4<u32>,
}

/// Candidate `index` of a visible chunk: twin of `mirror::candidate` followed
/// by the tests a blade must pass to be drawn.
fn evaluate(chunk: Chunk, field: Field, index: u32) -> Generated {
    var out: Generated;
    out.keep = false;

    let tiles = field.placement.y;
    let cells = tiles * tiles;
    let point = index / cells;
    let cell = index % cells;
    let cell_hash = hash2(hash2(chunk.ids.y, chunk.ids.z), hash2(cell, field.placement.x));
    let offset = vec2<f32>(unit(cell_hash), unit(hash(cell_hash)));
    let p = fract(tile[point] + offset);
    var local = chunk.bounds.xy + (vec2<f32>(f32(cell % tiles), f32(cell / tiles)) + p) * field.extent.w;
    // Tufts: the blade moves part of the way to its tuft's centre, the tuft's
    // own share of the pull, so tufts differ in size.
    var outward = vec2<f32>(0.0);
    if field.tufts.x > 0.0 {
        let size = field.tufts.y;
        let q = (field.min_corner.xz + local) / size;
        let tuft = tuft_at(q);
        outward = q - tuft.xy;
        local -= outward * min(field.tufts.x * (0.6 + 0.8 * tuft.z), 0.95) * size;
    }
    if any(local >= field.extent.xy) || any(local < vec2<f32>(0.0)) {
        return out;
    }
    let rank = (f32(point) + 0.5) / f32(TILE_POINTS);
    let seed = hash2(cell_hash, point);

    let mask = sample_mask(field, local / field.extent.xy);
    let root = vec3<f32>(field.min_corner.x + local.x, ground_height(field, local), field.min_corner.z + local.y);

    var falloff = 1.0;
    var distance_to_eye = 0.0;
    if !is_orthographic() {
        distance_to_eye = distance(root, view.camera_position.xyz);
        falloff = density_falloff(view, distance_to_eye);
    }
    var bare = 1.0;
    if field.tufts.z >= 0.0 {
        let threshold = field.tufts.z;
        bare = smoothstep(threshold - 0.05, threshold + 0.05, bare_noise(root.xz, field.tufts.w));
    }
    let tile_size = field.extent.w;
    let candidate_density = f32(TILE_POINTS) / (tile_size * tile_size);
    let keep = clamp(field.shape.w * mask.x * bare * view.density_scale * falloff / candidate_density, 0.0, 1.0);
    if rank >= keep {
        return out;
    }

    // A field of one species skips the pick, and reads its entry once.
    var entry = chunk.ids.x;
    var look = field;
    if field.mix.z > 1.5 {
        entry = pick_species(chunk.ids.x, field, seed, clump_of(root.xz));
        look = fields[entry];
    }
    let h = unit(hash(seed ^ 0x68u));
    let height = (look.shape.x + h * (look.shape.y - look.shape.x)) * mask.y;
    if height < 0.01 {
        return out;
    }
    let visible_blade = sphere_visible(root + vec3<f32>(0.0, height * 0.5, 0.0), height);
    var visible_shadow = false;
    if view.shadow_offset.w > 0.5 {
        let reach = view.shadow_offset.xyz * height;
        visible_shadow = sphere_visible(root + reach * 0.5, length(reach) * 0.5 + height * 0.25);
    }
    if !(visible_blade || visible_shadow) {
        return out;
    }

    var lod = view.fixed_lod;
    if lod == 3u {
        lod = select_lod(view, distance_to_eye, unit(hash(seed ^ 0x10du)));
    }
    let width = look.shape.z * (0.75 + 0.5 * unit(hash(seed ^ 0x77u))) * coverage_width(falloff);
    let seed_unit = f32(seed >> 24u) / 255.0;

    var blade: BladeData;
    blade.root = root;
    blade.facing = unit(hash(seed ^ 0x66u));
    if dot(outward, outward) > 1e-8 {
        // Tufted blades lean out from the tuft's centre: the tighter the
        // tufts, the more of them.
        let random = vec2<f32>(sin(blade.facing * TAU), cos(blade.facing * TAU));
        let away = mix(random, normalize(outward), min(field.tufts.x * 1.5, 1.0));
        if dot(away, away) > 1e-6 {
            blade.facing = fract(atan2(away.x, away.y) / TAU);
        }
    }
    blade.height = height / max(look.shape.y, 1e-3);
    blade.width = width / (WIDTH_RANGE * max(look.shape.z, 1e-5));
    blade.seed = seed >> 24u;
    blade.lod = lod;
    blade.field = entry;
    blade.clump = clump_of(root.xz);
    // A trampled blade lies down away from what pushed it, and the wind moves
    // it less the flatter it lies.
    let push = trample_push(root.xz);
    let calm = 1.0 - min(length(push), 1.0);
    blade.bend = wind_bend(root.xz, seed_unit, look.motion.y, frame.time) * calm + push;
    blade.previous_bend = wind_bend(root.xz, seed_unit, look.motion.y, frame.previous_time) * calm + push;

    out.keep = true;
    out.bucket = select(lod, 0u, view.fixed_lod != 3u);
    out.words = pack(blade, view.instance_origin);
    return out;
}

var<workgroup> group_total: u32;
var<workgroup> group_counts: array<atomic<u32>, 3>;
var<workgroup> group_base: array<u32, 3>;

/// One workgroup per slice of a visible chunk. Survivors are counted per bucket inside
/// the workgroup first, so the global counters see one atomic per bucket per
/// 64 candidates instead of one per blade.
@compute @workgroup_size(64)
fn generate(@builtin(workgroup_id) group: vec3<u32>, @builtin(local_invocation_index) lane: u32) {
    let entry = visible[group.x];
    if lane == 0u {
        group_total = entry.count;
    }
    let total = workgroupUniformLoad(&group_total);
    let chunk = chunks[entry.chunk];
    let field = fields[chunk.ids.x];

    let rounds = (total + WORKGROUP - 1u) / WORKGROUP;
    for (var round = 0u; round < rounds; round++) {
        if lane < 3u {
            atomicStore(&group_counts[lane], 0u);
        }
        workgroupBarrier();

        let index = round * WORKGROUP + lane;
        var result: Generated;
        result.keep = false;
        if index < total {
            result = evaluate(chunk, field, entry.first + index);
        }
        var slot = 0u;
        if result.keep {
            slot = atomicAdd(&group_counts[result.bucket], 1u);
        }
        workgroupBarrier();

        if lane < 3u {
            group_base[lane] = atomicAdd(&counters[lane], atomicLoad(&group_counts[lane]));
        }
        workgroupBarrier();

        if result.keep {
            let at = group_base[result.bucket] + slot;
            if at < view.bucket_capacity[result.bucket] {
                instances[view.bucket_base[result.bucket] + at] = result.words;
            }
        }
        workgroupBarrier();
    }
}

@compute @workgroup_size(1)
fn write_draw_args() {
    for (var bucket = 0u; bucket < 3u; bucket++) {
        let lod = view.bucket_lod[bucket];
        draw_args[bucket] = DrawArgs(
            lods.index_count[lod],
            min(atomicLoad(&counters[bucket]), view.bucket_capacity[bucket]),
            lods.first_index[lod],
            // The vertex shader decodes bucket and LOD from the vertex index.
            i32((bucket * 3u + lod) * LOD_VERTEX_STRIDE),
            0u,
        );
    }
}
