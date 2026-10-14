# Performance notes

What has been tried to make grass cheaper, what it gained, and why. Read this before optimizing: several ideas that work in other engines were measured here and did not pay off. Add a section for every new pass.

## How to measure

Grass cost is GPU time, and on the development laptop it is mostly fragment shading. Measure it with the `bench` example in two scenes:

- **Perspective:** a deterministic 940 m flythrough at 1.8 m eye height over a 2 × 2 km heightmap, with `MEADOW` grass at the High tier. This is gate G2's scene.
- **DEMO ARPG GAME** (`--ortho`): DEMO ARPG GAME's orthographic camera (30° pitch, 45° yaw, 14.64 m frame) panning the same terrain, with DEMO ARPG GAME's blades and light. Blades are 5 cm wide, 0.4–0.9 m tall, 36 per m², with a 2-pixel minimum width; the light has one shadow cascade.

Both run at 1920 × 1080 with 4× MSAA and no vsync, measuring 1,380 frames after 120 warm-up frames.

`tools/bench_ab.py` runs variants against a reference in paired, interleaved rounds. A result is the median paired difference in frame time p50, as a share of the reference's grass cost: the reference minus a `--no-grass` run. Frame time is used rather than per-pass GPU spans because it also captures costs that move between passes. For example, drawing grass in its own pass made the main-pass span look much cheaper while the frame got slower.

Rules learned the hard way:

- **Reject runs that log errors.** A shader that fails to compile draws no grass, which looks like a large speed-up.
- **Use paired differences.** The `--no-grass` baseline drifts by up to 0.9 ms between runs, because without grass the frame is limited by the CPU, not the GPU. Use it only for the size of the grass cost, never as the per-run reference.
- **Expect noise of about ±2% of grass cost.** Four rounds resolve a 5% change.
- **Keep a change only if it cuts grass cost by at least 10%** in the scene it targets and makes no scene worse. Changes that alter the look go to the developer even when they pass.
- **Count the blades, not just the time.** `RUST_LOG=bevy_grass=debug` logs visible chunks and blades per bucket, and it exposed a density bug (below).

Development machine: Windows 11, NVIDIA T1200 laptop GPU (roughly a third of an RTX 3060), Bevy's default backend.

## Where the time goes

As of M2, on the T1200:

| | Perspective | DEMO ARPG GAME |
| --- | --- | --- |
| Frame p50 without grass | 2.2 ms | 1.9–2.1 ms |
| Frame p50 with grass | 6.96 ms | 5.14 ms |
| Grass cost | 4.68 ms | 3.17 ms |
| Main pass, terrain included (terrain alone 0.72 ms) | 4.36 ms | 3.11 ms |
| Blade generation (compute) | 0.92 ms | 0.34–0.68 ms (varies between sessions) |
| Chunk cull (compute) | 0.016 ms | 0.016 ms |
| Grass in the shadow pass | about 0.34–0.42 ms | about 0.44 ms |
| Blades drawn | 16k / 59k / 72k (LOD 0 / 1 / 2) | 26k–31k, all LOD 1 |

- **The main pass is limited by pixels, not vertices.** In M2, culling cut vertex work 4.2× and the main pass did not change. With a flat colour instead of PBR lighting the pass took 1.7 ms instead of 3.7 ms, so lighting is about half of it.
- **The main pass runs 4.3M fragment shaders for 2.07M screen pixels** (pipeline statistics, perspective). The terrain accounts for about 1.9M, so grass shades about 2.4M fragments.
- **Most of the waste is in the 2×2 pixel quads GPUs shade in.** A blade 1–4 pixels wide covers only part of each quad it touches, but every pixel of the quad is shaded. This is why fewer, wider blades are cheaper even at the same screen coverage (experiments 7 and 8).
- **Far grass is cheap.** Removing everything beyond 60 m saved only 9%, so the cost is in the first 50 m.

## Performance pass after M2 (October 13, 2026)

The ideas come from a survey of AAA grass rendering (Ghost of Tsushima, Horizon Forbidden West, The Witcher 3, Red Dead Redemption 2, Unreal Engine 5). They were tested one at a time against the baseline above. Results are the median of 4–6 paired rounds.

| # | Idea | What was built | Perspective | DEMO ARPG GAME | Verdict |
| --- | --- | --- | --- | --- | --- |
| 1 | Cheaper shadow filtering | Grass pipeline specialized with Bevy's `Hardware2x2` filter (4 samples) instead of the view's Gaussian (9 samples) | −0.45 ms, −9.6% | −0.30 ms, −9.5% | Rejected. Re-tested after experiment 7: −6.9%, −7.3%. |
| 2 | No shadow casting from grass | `GrassShadows::Off` at High; grass still receives shadows and keeps its fake root occlusion | −0.34 ms, −7.3% | −0.44 ms, **−13.9%** | DEMO ARPG GAME only. The setting already exists; whether DEMO ARPG GAME grass casts shadows is decision D5. Defaults unchanged. |
| 3 | Screen-size culling | Generation drops perspective blades projecting under 6 px tall | −0.08 ms, −1.7% | 0% | Rejected. Within a 100 m radius at a 45° field of view the smallest blade is about 4 px tall, so a 1–2 px cull removes nothing. |
| 4 | Ground impostor beyond the grass | Grass radius 60 m instead of 100 m, LOD distances unchanged; no ground tint was built | −0.44 ms, −9.4% | 0% | Rejected, and it would cost the far field's look. |
| 5 | Occlusion culling (Hi-Z), upper bound | Chunk cull ray-marches the heightmap to the chunk's top corners and skips chunks hidden from all five. The benchmark's only occluder is the terrain, so this is the best a Hi-Z pyramid could do here. | −0.08 ms, −1.8% | −0.03 ms, −1.0% | Rejected. It culled up to 25% of LOD 2 blades on some frames, but Bevy draws non-mesh phase items after meshes, so hidden grass already fails the depth test cheaply. A hillier scene would gain more; DEMO ARPG GAME's top-down camera gains nothing. |
| 6a | Draw-order check | LOD buckets drawn far-to-near instead of near-to-far | +0.30 ms, +6.5% | +0.4% | Not a change, a measurement. Near-first order (the current one) is worth about 6.5%, so a full front-to-back sort (T4) would gain less than that. Not built. |
| 6b | Better early depth rejection: grass first | Grass drawn in its own render pass before Bevy's main opaque pass, so the ground under it fails the depth test | +0.08 ms, +1.6% | +0.18 ms, +5.8% | Rejected (slower). Ground shows between the blades and MSAA tests depth per sample, so little ground shading is skipped. The extra pass also stores and reloads the 4× multisampled targets. |
| 7 | **Fewer, wider mid-range blades** ("cross-card clumps", kept opaque) | Density halved across LOD 1's dither band and beyond; coverage widening makes the survivors about 1.4× wider | **−0.77 ms, −16.5%** | 0% (no distance LOD) | **Kept.** `mirror::MID_RANGE_DENSITY = 0.5`. Screenshots show no visible change. |
| 8 | The same at DEMO ARPG GAME's camera | Style at half density, width × 1.41 (18 per m², 7.1 cm) | +1% (unchanged) | −1.33 ms, **−41.9%** | Passes, but the field is visibly sparser and shows more ground. Style is the developer's call (D3); not applied. A 70% density, 1.2× width variant gained only 7%. |

**Overall:** the perspective flythrough's grass cost fell from 4.68 ms to about 3.77 ms, measured as −0.91 ms (−19.5%) in a final paired run of the original baseline against the result. Grass at DEMO ARPG GAME's camera is unchanged at about 3.1 ms.

### Ideas that needed no test

- **GPU-driven culling and indirect draws.** Built in M2: frustum and distance culling per chunk and per blade, and three indirect draws per view.
- **Geometry instead of alpha cards.** Blades are opaque geometry, so there is no alpha overdraw.
- **Dithered LOD transitions.** Built in M2: each blade picks its LOD inside a 4 m band.
- **Wind from a scrolling world-space noise texture.** Built in M1. It is computed once per blade in the compute pass, cheaper than the per-vertex version described in the survey. Only the small tip flutter is per vertex.
- **Bent and rounded normals, baked root occlusion.** Built in M1, for the look. They do not change cost.
- **No shadow casting from mid and far grass.** The `Near` mode casts LOD 0 into the first cascade only. Drawing every blade into all four cascades cost 2.4 ms; `Near` costs about 0.4 ms.
- **Interaction through a top-down texture (render target or virtual texture).** This is the trample texture planned for M3.
- **Screen-space contact shadows instead of shadow maps.** Bevy 0.20 only computes them with a depth prepass. DEMO ARPG GAME has none, and adding one costs a full extra depth pass.
- **Depth-only grass pre-pass.** Measured in M2 and removed: it made the main pass slower, 4.3 ms against 3.7 ms.

### Bug found by the measurements

`ChunkLayout::for_density` rounded the number of blue-noise tiles per chunk to the nearest integer. When it rounded down, a chunk had fewer candidates than the style asked for, and the keep fraction clamped at 1:

- DEMO ARPG GAME's 45 blades per m² drew 36.
- `TALL_PRAIRIE`'s 40 drew 36.
- A style asking for 22.5 drew 16.

That made experiment 8's first run look far better than it was. The layout now rounds up, and the test `every_style_density_is_reached` checks every density from 0.1 to 200 per m². Spare candidates cost nothing: the cull hands generation only the prefix of candidates a chunk can keep.

## M3: trampling and shading (October 13, 2026)

These are features, not optimizations, so the 10% rule does not apply. Each was built, measured against the build before it, and made cheaper where the measurement showed a problem. Same benchmark and method as above.

### Trampling (step 3.1)

Each camera keeps 256 × 256 cells of 0.25 m (64 m) around what it looks at. Cells are addressed modulo 256, so trails stay in world space without copying when the window moves. The 512-interactor case uses `bench --interactors 512`, with interactors walking rings around the view.

| Version | Blade generation, perspective | Blade generation, DEMO ARPG GAME | Notes |
| --- | --- | --- | --- |
| Before trampling | 0.88 ms | 0.34 ms | |
| Bilinear sampling of 4 cells, atomic loads, angle-encoded direction | 1.21–1.24 ms | 0.59 ms | Sampling alone: +0.34 ms (checked by stubbing it out). |
| Plain loads (decay and stamp moved to their own shader module), direction as two 11-bit components | 1.16 ms | 0.54 ms | Small gain; the atomics and trigonometry were not the main cost. |
| Power-of-two masks instead of signed integer modulo, early out outside the window | 1.10 ms | 0.50 ms | |
| Plus a flag per 4 m block, so untrampled blocks skip sampling | 0.90 ms (1.18 ms with 512 interactors) | 0.35 ms (0.54 ms with 512) | Cheap when nothing is trampled, but a horde covers most blocks. |
| **Nearest cell instead of bilinear, flags removed** | **0.93 ms, with or without 512 interactors** | **0.37 ms, with or without 512 interactors** | **Kept.** A cell is about one blade wide; screenshots could not tell the two apart. With one load per blade, a flag check costs as much as the load it saves, so the flags went. |

The trample pass itself (decay of all 65,536 cells, plus stamping) costs 0.017 ms with no interactors and 0.022 ms with 512. Trampling in total costs about 0.05–0.07 ms on the T1200.

### Clump colour (step 3.2)

Blades in the same Voronoi cell of about 1 m share a brightness, and one clump in five is drier, toward straw.

| Version | Perspective | DEMO ARPG GAME | Notes |
| --- | --- | --- | --- |
| Voronoi search per blade (9 neighbouring points, hashed) | +0.38 ms | +0.13 ms | Generation +0.42 ms and +0.51 ms. Rejected. |
| **Table lookup: a 256 × 256 Voronoi table at 0.25 m, repeating every 64 m, built at startup** | **+0.025 ms** | **+0.037 ms** | **Kept.** One load per blade. The GPU readback test checks every blade's clump against the CPU table. |

### Translucency (step 3.2)

Blades seen against the sun were nearly black. Bevy's own diffuse transmission evaluates the light and its 9-sample shadow a second time, which would cost far more than the arithmetic below, so it was not built. Instead, a face turned away from the sun is lit as its sunward side, through Bevy's normal lighting with one shadow lookup. The direct part of that light is then dimmed and tinted by the blade (`GrassStyle::translucency`), and an estimate of the ambient part is kept whole.

| Version | Perspective | DEMO ARPG GAME | Notes |
| --- | --- | --- | --- |
| With branches, extra flat varying | +0.07 ms | +0.11 ms | |
| Translucency packed into the colour varying | +0.15 ms | +0.10 ms | Within noise of the first; the varying was not the cost. |
| **Branch-free** | **+0.14 ms** | **+0.11 ms** | **Kept.** About 3–4% of grass cost; the arithmetic itself, in an already heavy fragment shader. |

In the `meadow` screenshot, near-black grass pixels fell from 2.27% to 1.62% of the near field, with a camera not facing the sun.

### Total

Against the build before M3, in paired runs: **+0.27 ms in perspective and +0.18 ms at DEMO ARPG GAME's camera**, 7–8% of grass cost. Per term: trampling about 0.07 ms, clumps about 0.03 ms, translucency about 0.11–0.14 ms.

Lessons for the next feature in the generation pass:

- **Stub the feature out and measure before optimizing.** Generation's cost is very sensitive to per-blade work, because it runs for every kept blade, about 150k a frame in perspective.
- **Signed integer division and modulo are expensive on GPUs.** Keep sizes powers of two and use masks.
- **A precomputed table beats a per-blade search** whenever the pattern can tile.

## Levers not yet tried

In expected order of effect:

1. **A lighting path written for grass.** Lighting is about half the main pass. A grass-specific function could skip what Bevy's full PBR path does for every fragment: specular, clustered light lookups when a scene has no point lights, and cascade blending. Point lights would need a fallback.
2. **Lighting computed less often than per pixel.** For example per vertex or per blade, where normals are already smooth, as the survey's "volumetric shading" suggests. Bevy's shadow-map bindings may only be visible to the fragment stage, which would block the per-vertex version.
3. **Fewer near blades.** LOD 0 is 100% density to 18 m. Halving from 10 m would apply experiment 7's gain to the most expensive blades, at a visible cost close to the camera.
4. **DEMO ARPG GAME style.** See experiment 8, decision D3.
5. **Shadows off in DEMO ARPG GAME.** See experiment 2, decision D5.
