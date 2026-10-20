# Proposal: combined blade sets, meshlet culling, and more caching

Status: declined (October 2026), kept as a record of why. Grass time is
dominated by shading the blades' pixels, which meshlets do not reduce; see
[Roadmap and scope](../roadmap.md#non-goals). This document translates the review
comment

> "Look into meshlets + meshlet culling, and you'll want some larger combined
> sets of blades not all individual."

into concrete work for this crate, states what the ideas can and cannot fix,
and proposes an order of experiments.

## 1. Short version

The comment contains three separate ideas:

1. **Larger combined sets of blades.** Instead of one 16-byte instance per
   blade, store a group of blades (a *clump*, *cluster* or *meshlet*) in static
   buffers and make the group the unit of instancing, culling, LOD and drawing.
2. **Meshlets and meshlet culling.** Give each combined set a bounding volume
   and cull it on the GPU as a unit. Draw only the visible sets, normally with
   indirect / multi-draw commands, instead of visiting every blade every frame.
3. **More caching, less per-frame work.** Bake the deterministic parts of the
   field (blade placement, shape, species, static geometry) once, and keep only
   wind, trample, visibility and LOD selection per frame.

The current renderer is already GPU-driven: `src/shaders/generate.wesl` culls
chunks, generates blade instances and writes three indirect draw arguments.
The proposed change is therefore not "make it GPU-driven"; it is "make the
instance and culling granularity coarser and cacheable".

**Important caveat.** The measurements in `docs/performance.md` say the frame
is now dominated by fragment work, not by generation or culling:

| Part of grass cost, after G6 | Perspective | DEMO ARPG GAME camera |
| --- | ---: | ---: |
| Compute (trample, cull, generate) | 3% | 5% |
| Vertex shading (upper bound) | ~4% | <6% |
| Grass in the shadow map | 14% | 22% |
| Rasterizing + shading blades | 78% | 91% |
| … of which the shadow lookup | 20% | 18% |
| … of which the rest of lighting | 3% | 3% |

So meshlets and baking will remove a few percent of per-frame work, but they
will **not on their own** take a 3.8 ms grass cost to a 240 Hz budget. They
become valuable when they enable a cheaper *representation* of the same
coverage: fewer, wider blades per clump, per-clump LOD, and coarser culling.
Those changes can move the 78–91% raster/shading bucket. They must be paired
with pixel-cost work: MSAA setting, shadow filtering, mid-range density, and
possibly a lower internal resolution.

Recommendation:

- Prototype combined clumps first, with no new culling. Measure whether the
  main pass changes at all before building a meshlet pipeline.
- If it does, productize clumps and then add meshlet-level culling.
- Treat caching as part of the clump design, not as a separate rewrite.
- Keep the existing per-blade path as a fallback and as the reference for
  image comparison.

## 2. What the current renderer does

Relevant code:

- `src/render/compute.rs` records four compute steps per view: trample decay /
  stamping, chunk cull, blade generation, and draw-argument writing.
- `src/shaders/generate.wesl`:
  - `cull` tests 16 m chunks and writes slices of candidate ranges.
  - `generate` evaluates every candidate, applies mask/height/tuft/bare-patch
    sampling, picks species and LOD, computes wind and trample, and packs a
    16-byte instance (`BladeData` / `pack`).
- `src/shaders/blade.wesl`: `blade_ref` decodes the instance and the vertex
  index, then `blade_vertex` rebuilds the curved blade, normals and minimum
  screen width procedurally.
- `src/render/resources.rs`: one shared index buffer with three LOD
  topologies; `blade_indices` builds it from `GrassSettings::blade_rows`.
- `src/render/bevy_glue.rs`: one `NonMesh` phase item per camera and three
  `draw_indexed_indirect` calls, one per LOD bucket.

In other words, the pipeline is:

```
chunk cull -> candidate generation -> packed per-blade instance -> one draw per LOD
```

A chunk is large (16 m) and a blade is small (centimetres). The existing
per-blade frustum test inside `generate` is what keeps the coarse chunk cull
from overdrawing. There is no intermediate granularity.

## 3. What "meshlet" means for grass

In this crate, a useful working definition is:

> A meshlet is a small, spatially compact, precomputed set of blade geometry
> with a bounding volume, a fixed LOD, and an index range, cullable and
> drawable as one unit.

For grass, a practical meshlet is 64–128 vertices and 32–96 triangles, roughly
4–16 blades depending on LOD. That is much smaller than a 16 m chunk and much
larger than one blade. It is the missing middle level of the hierarchy:

```
field -> chunk (16 m) -> clump/meshlet (1-2 m) -> blade
```

This is also what the comment means by "larger combined sets of blades not all
individual": blades inside one clump share an instance, a cull test and a draw,
instead of each being its own instance and its own cull result.

A "true" meshlet pipeline (mesh shaders, work graphs, cluster culling with
cone tests) is not a natural fit for a portable Bevy 0.20 library: Bevy's
public pipeline API does not expose mesh shaders as a portable feature, and
WebGPU has no mesh-shader stage. The same data flow can be built with compute
culling plus indirect / multi-draw. That is the proposal here. Native-only
mesh shaders can be a later experiment, not the base design.

## 4. Proposal A: baked combined clumps

### 4.1 Concept

Replace the per-blade instance buffer with two levels:

- **`GrassClump`**: one instance per 4–16 blades. Holds a world transform or
  root, a bounding sphere/box, field index, LOD, and an offset into a static
  clump vertex/index table.
- **`ClumpBlade`**: static per-blade data inside the clump: local root, height,
  width, facing, seed, species/field index, and per-blade random values.
- **Static geometry**: an indexed mesh for the clump at each LOD. The vertex
  shader still builds the blade curve, but it reads `ClumpBlade` instead of
  decoding a packed per-frame instance.

The per-frame work then becomes:

```
cull clumps -> select clump LOD -> update only wind/trample state for visible clumps
            -> draw visible clumps
```

instead of:

```
cull chunks -> evaluate every blade candidate -> pack every blade instance
            -> draw every blade
```

### 4.2 Two ways to populate the clumps

**A1. Per-chunk baked clumps (higher fidelity, more machinery).**

Build clumps from the same deterministic candidates the current generator uses,
once per `(field, chunk, style version, LOD)`. Cache the resulting static
geometry in a GPU pool. When a chunk is visible for the first time, a compute
pass builds its clumps into a free slot; the slot stays valid until the field,
style, mask or heightmap changes. For huge fields, keep an LRU budget and
rebuild only what is near the camera.

This preserves the existing placement, species selection, tuft/bare-patch
behaviour and density. It is the recommended long-term design.

**A2. Archetype clumps (much simpler, lower fidelity).**

Build a small library of clump archetypes per style (`8` blades, `16` blades,
several random variants and three LOD topologies) at startup. At runtime, place
an archetype at each clump centre and rotate/scale it from a hash. The field's
mask, density and bare patches decide where clumps exist; per-instance scale
can carry mask height.

This makes the cache tiny and avoids a GPU builder, but loses exact per-blade
placement and some of the field's current variation. It is a good first
prototype because it can be implemented without a persistent chunk cache, but
it should not be assumed to be the final look.

### 4.3 Data layout sketch

```wgsl
struct GrassClump {
    // xyz: world-space centre; w: bounding radius.
    bounds: vec4<f32>,
    // x: first index; y: index count; z: base vertex; w: LOD / bucket.
    indices: vec4<u32>,
    // x: first blade in clump_blades; y: blade count; z: field index; w: flags.
    blades: vec4<u32>,
};

struct ClumpBlade {
    // xyz: local root relative to the clump; w: height.
    root_height: vec4<f32>,
    // x: width; y: facing; z: seed; w: field / species index.
    shape: vec4<f32>,
    // x, y: reserved for baked per-blade colour or variation.
    extras: vec4<f32>,
};
```

The exact packing should stay 16 bytes per blade where possible. A per-vertex
attribute is only needed for `blade_id` and `(row, side)`; the current vertex
index already encodes row and side, so a small `blade_id` attribute or a
computed `vertex / stride` may be enough.

An alternative that keeps the existing packed instance format is to store the
same `vec4<u32>` words in the clump's static blade table and replace
`instances[bucket_base[bucket] + instance_index]` with
`clump_blades[clump.blades.x + blade_id]`. This reuses `unpack` and
`blade_vertex` and keeps the change small.

### 4.4 Wind and trampling

This is the part that decides whether the clump is a real win.

- **Look-preserving option:** keep a small dynamic per-blade buffer containing
  only the current and previous bend (and trample result). A compute pass
  updates it for visible clumps, as `generate` does today. The clump vertex
  shader reads the bend by blade id. Placement, mask, species and static shape
  are cached; only the 4–8 bytes that actually change per frame are written.
- **Cheapest option:** compute one gust value per clump and a per-blade sway
  phase in the vertex shader from baked seeds. No per-blade dynamic buffer, but
  blades in a clump share more motion and trampling becomes approximate.
- **Middle option:** per-clump gust from compute, per-blade sway in the vertex
  shader, and per-clump trample bending. Probably enough for small clumps, but
  needs visual comparison against the existing trample trails.

The first should be prototyped because trampling and wind coherence are
user-visible features of this crate. The second is the performance ceiling.

### 4.5 LOD

Bake each clump in at least the existing three `blade_rows` topologies. Pick
one LOD per clump, not per blade:

- Use the clump's bounding-sphere distance to the camera.
- Dither the transition per clump, as `select_lod` does per blade today.
- Keep the transition band wide enough that a whole clump does not visibly pop.
- Because clumps are small (1–2 m), the switch should cover a small screen area.

If per-clump LOD popping is visible, split large clumps into smaller ones, or
keep a per-blade LOD *within* the clump at the cost of a less regular topology.
The latter makes the meshlet less useful and should be a last resort.

The important interaction is density. Today distance thinning drops individual
blades and widens the survivors. A clump LOD should do the same: bake a sparser
variant that uses fewer, wider blades at a larger footprint. This is exactly
the mechanism behind the measured "fewer, wider mid-range blades" win, but
applied to a cullable unit. A clump variant that keeps the same footprint with
half the blades and ~1.4× width is the natural far-LOD target.

### 4.6 What to measure

Before building the full cache:

1. Build one combined clump from 8 blades and draw it with the existing draw
   path, but decode `ClumpBlade` instead of the per-blade instance.
2. Compare against the current build with `tools/bench_ab.py`, both scenes, at
   least four paired rounds.
3. Record instance count, generation time, vertex time and main-pass time
   separately, not just total frame time.
4. Check the golden images and a moving-camera capture, because clump LOD and
   shared wind can look fine in a still and wrong in motion.

Kill the idea at this point if the main pass does not move. A 16-byte instance
encode/decode per blade is not where 3.8 ms comes from.

## 5. Proposal B: software meshlet culling

Once clumps exist, make them cullable units.

### 5.1 Cluster cull

A new compute pass, one thread per clump:

```
for each clump:
    if field is hidden on this view's layers: skip
    if bounding sphere outside view frustum: skip
    if distance beyond grass radius: skip
    choose LOD from distance (or reuse the chunk's LOD)
    if screen-space size is below the minimum: skip
    optionally test a cone / backface bound
    append the clump to this LOD's visible list
    append a DrawIndexedIndirectArgs for it
```

This replaces the current `cull` + `size_dispatch` + `generate` combination for
the static parts. A separate `write_draw_args` becomes a count / prefix-sum or
an indirect-count argument, depending on the draw API used.

### 5.2 Drawing

The current `DrawGrassBuckets` in `src/render/bevy_glue.rs` records three
`draw_indexed_indirect` calls. A clump renderer wants one of:

- `multi_draw_indexed_indirect` per LOD, with one argument per visible clump;
- `draw_indexed_indirect_count` with a compact list of arguments;
- if neither is available portably, a CPU-side loop over visible clump
  arguments, which is still fewer items than blades but removes much of the
  benefit.

The first is preferred on native GPUs. The fallback should be chosen from
`RenderDevice` features, not assumed. Keep the existing three-bucket path as
the fallback for devices without multi-draw.

### 5.3 What meshlet culling can and cannot buy here

Measured in this project so far:

- Terrain occlusion culling removed only ~1% of grass cost in the benchmark,
  because hidden grass already fails the depth test cheaply. A hillier scene
  would gain more; the current scenes would not.
- Front-to-back order did not help; non-mesh phase items are already drawn
  after meshes.
- Chunk culling is 0.016 ms. Blade generation is 3–5% of grass cost after G6.
- Vertex shading is about 4–6% at most.

So do **not** expect meshlet frustum culling to deliver a large win in the open
field benchmark. The wins come from:

- **Culling granularity for shadow cascades.** Shadow cost is 14–22% of grass
  cost. Smaller cullable units can skip clumps outside each cascade more
  tightly than a 16 m chunk.
- **Per-clump LOD.** A clump can switch to a cheaper topology/density as a
  whole, which a per-blade pipeline cannot express without generating a new
  blade set.
- **Occlusion / Hi-Z in scenes with real occluders**, not the current flat
  flythrough.
- **Less per-frame state**: one cull result per clump instead of one per blade.

Cone/backface culling is a poor fit for grass because blades are two-sided and
their normals point in many directions. A cluster cone test may cull almost
nothing. It should be measured, not assumed.

### 5.4 Bounding volumes

Use a sphere or an AABB enclosing the clump's *animated* blades, not just the
static roots. Add the maximum lean/bend and tuft spread to the bounds, as
`cull` already does for chunks. If the bound is too large in the near field,
culling loses its point; keep clumps small. A 1–2 m clump with blades up to
1.5 m tall is a reasonable starting point.

## 6. Proposal C: more caching, less per-frame work

These are orthogonal to meshlets and can be evaluated separately.

### 6.1 Bake deterministic blade data

`evaluate` in `generate.wesl` recomputes, for every blade every frame:

- tile / candidate position and hash,
- mask sample and ground height,
- species pick,
- height, width, facing, clump,
- wind and trample.

Only wind, trample, visibility and LOD actually change. The rest could be
baked into the clump or into a per-chunk candidate table. This is the main
"more caching" win. It removes most of the 3–5% compute slice and makes clumps
possible.

### 6.2 Persist clump data

Key clump data by `(field, chunk, style version, mask version, heightmap
version, LOD)`. Rebuild only when one of those changes. A GPU buffer pool with
an LRU budget avoids unbounded memory on a 4 km field. While a chunk is being
built, either skip it for a frame or draw the existing per-blade path.

### 6.3 Cache cull results

The current cull is already tiny, but with clumps it becomes even smaller.
Possible reuse:

- One cull result per `(view, LOD)` shared by the main pass, prepass and shadow
  cascades where the view direction is the same.
- Reuse last frame's visible list when the camera has moved less than a margin.
  This needs conservative bounds and is easy to get wrong; only worth it if
  profiling shows the cull list write is measurable.
- Keep draw arguments persistent and only update counts when the visible set
  changes. This is a small CPU win, not a GPU one.

Do not let caching add a frame of latency: the main pass and shadow pass must
agree on instance/bend data.

### 6.4 Cache static style tables

The crate already caches clump colour, tuft and mask tables and rebuilds them
only on change. Extend that principle to any new per-style data: archetype
clumps, per-species variation tables, baked LOD meshes. Building them on the
CPU at startup or on style change is preferable to per-frame GPU work.

## 7. How this interacts with the real bottleneck

The proposal should not be judged only by "did generation get faster". The
target is frame time. Use the existing measurement method:

- `tools/bench_ab.py` with paired, interleaved rounds.
- Perspective and DEMO ARPG GAME scenes.
- At least 4 rounds; reject runs that log shader errors.
- Report p50 and p95 as a share of grass cost.
- Keep a change only if it cuts grass cost by at least 5% at p95 without a
  visible image change, or if the developer accepts the look change.

The measured levers that address the 78–91% raster/shading share should be
tested alongside the clump work:

| Lever | Measured effect | Cost |
| --- | ---: | --- |
| MSAA off / 2× | 27–32% of grass cost | shimmer on thin blades |
| Cheaper shadow filter (4-tap) | 12–18% | crisper/grainier self-shadowing |
| Mid-range density 0.5 + wider blades | 16.5% (perspective) | sparser/different look |
| Mid-range density at ARPG camera | 41.9% in one experiment | visibly sparser |
| Fewer near blades / no shadows | 14–22% in some scenes | look / gameplay |

A clump renderer should make the "fewer, wider blades" lever easier to apply
because a whole clump can switch to a sparse variant. It should also reduce
the number of thin quads for the same coverage if the sparse variant is used at
the same footprint. That is the realistic path to a 240 Hz grass budget, not
culling alone.

## 8. Recommended implementation order

### Phase 0: prove the premise (1–2 days)

- Add a tiny prototype that draws 8 blades from one clump's static table.
- Keep the current shader path as a reference.
- Measure main pass, vertex and generation separately.
- **Gate:** if the main pass does not improve, stop before the full meshlet
  pipeline. Try a variant with half the blades and 1.4× width in the same
  footprint; that is the variant with a measured reason to be faster.

### Phase 1: baked clumps, no new culling

- Introduce `GrassClump` and `ClumpBlade` tables.
- Build per-chunk clumps on the CPU for small fields first, or GPU-build into a
  cached pool.
- Reuse the existing three indirect draws by making each draw's instances be
  clumps; the vertex shader expands a clump's blades.
- Keep the per-blade wind/trample buffer so the look does not change.
- **Gate:** golden images match or the image difference is accepted; grass cost
  improves by ≥5% at p95.

### Phase 2: per-clump LOD and density variants

- Bake 3 topologies and at least 2 density variants per clump.
- Select LOD per clump with dithering and hysteresis.
- Add a sparse/widened variant for the mid and far range.
- **Gate:** ≥10% main-pass improvement in the targeted scene; no visible popping
  in a moving capture.

### Phase 3: meshlet culling and multi-draw

- Add `cull_clumps` and compact visible lists.
- Use `multi_draw_indexed_indirect` where supported; fall back to the existing
  draw path otherwise.
- Reuse the cull for main/prepass/shadow views where possible.
- **Gate:** measurable improvement in a scene with occluders or many shadow
  cascades. Do not force it if the open-field benchmark shows noise.

### Phase 4: caching and streaming

- Persist clump tables per chunk with versioning and an LRU budget.
- Build on demand; avoid a frame of stale data.
- Add telemetry for cache size, rebuilds and hit rate.
- **Gate:** no regression in frame time; memory under the documented budget.

### Phase 5: quality tiers and documentation

- Expose clump size, LOD distances, density and meshlet mode as settings with
  measured defaults.
- Document that combined clumps do not remove fragment cost and that the 240 Hz
  target requires the pixel-cost settings too.
- Update `docs/performance.md` with every measurement and rejected variant.

## 9. Risks and open questions

- **Memory.** A 4 km field cannot have every clump baked. Need a visible-set
  cache with an LRU budget and a fallback path.
- **Build hitch.** A GPU build pass for a newly visible chunk can cause a frame
  spike. Spread builds over frames or build at field load for small fields.
- **Look.** Archetype clumps repeat; per-chunk clumps preserve placement but
  cost more machinery. Per-clump LOD can pop; needs dithering and motion tests.
- **Wind coherence.** Per-clump wind is cheaper but may look like blocks of
  grass moving together. The per-blade bend buffer avoids this at some cost.
- **Trampling.** Per-clump trample is approximate. The per-blade dynamic bend
  buffer is the look-preserving answer.
- **Draw API support.** Multi-draw availability varies by backend and feature
  set. Keep the current path as a fallback.
- **Shadow correctness.** The main, prepass and shadow passes must use the same
  clump visibility and bend data, or motion vectors and shadows will disagree.
- **Mesh shaders.** Do not require them for the library; use compute +
  indirect as the portable implementation.
- **Culling value.** The existing measurements say culling is not the current
  bottleneck. Validate in a hillier/occluded scene before investing in Hi-Z or
  cone culling.

## 10. Bottom line

The comment is directionally right about architecture: per-blade instancing is
a fine granularity for generation but a poor granularity for caching and
culling. A clump/meshlet layer would make the renderer more cacheable and would
enable per-clump LOD and sparse variants.

It is not a silver bullet for the reported 3.8 ms. The measured frame is
fragment-bound, and the current culling/generation is already only a few percent
of grass cost. The proposal that has the best chance of reaching a 240 Hz budget
is therefore:

1. combined clumps with baked static geometry and per-blade dynamic bend,
2. per-clump LOD and sparse/widened far variants,
3. software meshlet culling where it can be shown to help,
4. persistent caching of chunk/clump data,
5. and, in parallel, the pixel-cost levers (MSAA, shadow filtering, density)
   that address the 78–91% of grass cost that clumps do not touch.

The first experiment is cheap and decisive: draw 8 blades from one clump,
measure the main pass, and only then decide whether to build the full meshlet
pipeline.
