# Performance notes

What has been tried to make grass cheaper, what it gained, and why. Read this before optimizing: several ideas that work in other engines were measured here and did not pay off. Add a section for every new pass.

## How to measure

Grass cost is GPU time, and on the development laptop it is mostly fragment shading. Measure it with the `bench` example in two scenes:

- **Perspective:** a deterministic 940 m flythrough at 1.8 m eye height over a 2 × 2 km heightmap, with `MEADOW` grass at the High tier. This is gate G2's scene.
- **DEMO ARPG GAME** (`--ortho`): DEMO ARPG GAME's orthographic camera (30° pitch, 45° yaw, 14.64 m frame) panning the same terrain, with DEMO ARPG GAME's blades and light. Blades are 5 cm wide, 0.4–0.9 m tall, 36 per m², with a 2-pixel minimum width; the light has one shadow cascade.

Both run at 1920 × 1080 with 4× MSAA and no vsync, measuring 1,380 frames after 120 warm-up frames.

`tools/bench_ab.py` runs variants against a reference in paired, interleaved rounds. A result is the median paired difference in frame time p50 or p95 (`report REFERENCE p95`), as a share of the reference's grass cost: the reference minus a `--no-grass` run. A run takes about 10 seconds, so five rounds of four variants, the reference and `--no-grass` in both scenes take about 10 minutes. Frame time is used rather than per-pass GPU spans because it also captures costs that move between passes. For example, drawing grass in its own pass made the main-pass span look much cheaper while the frame got slower.

Rules learned the hard way:

- **Reject runs that log errors.** A shader that fails to compile draws no grass, which looks like a large speed-up.
- **Use paired differences.** The `--no-grass` baseline drifts by up to 0.9 ms between runs, because without grass the frame is limited by the CPU, not the GPU. Use it only for the size of the grass cost, never as the per-run reference.
- **Expect noise of about ±2% of grass cost.** Four or five rounds resolve a 5% change.
- **Keep a change if it cuts grass cost by at least 5% at p95** (the developer's ruling of October 15, 2026; it was 10% at p50 before) in the scene it targets, makes no scene worse, and does not visibly degrade the image. Changes that alter the look go to the developer even when they pass.
- **Keep the machine quiet.** A video playing in the background invalidated one run; restart rather than average it in.
- **Count the blades, not just the time.** `RUST_LOG=bevy_grass=debug` logs visible chunks and blades per bucket, and it exposed a density bug (below).

Development machine: Windows 11, NVIDIA T1200 laptop GPU (roughly a third of an RTX 3060), Bevy's default backend.

## The 240 Hz gate

The review that prompted this work asks for grass usable in 240 FPS games.
A 240 Hz frame budget is `1000 / 240 = 4.167 ms` for the whole frame, not
just for grass, so the project now has an explicit acceptance gate instead of
only relative A/B measurements.

```text
python tools/bench_240.py --machine t1200
python tools/bench_240.py --machine rtx3060 --scene perspective
python tools/bench_240.py --machine rtx4070 --profile release-lto --no-gpu-timing
```

The script builds the bench (or uses `--no-build`), runs three configurations
in both scenes (or one with `--scene`), three rounds by default, and writes
JSONL, CSV, logs and a Markdown table under `target/bench-240/`:

| Configuration | What it separates |
| --- | --- |
| `no-grass` | The Bevy + terrain + camera floor. If this misses 4.167 ms, grass cannot fix the frame. |
| `high` | `GrassQuality::High`, the default. |
| `competitive` | `GrassQuality::Competitive`, the 240 Hz-oriented tier: 40 m reach, half density, no grass shadow casting. |

Each run reports frame p50 and p95 against the budget, grass cost p50 and p95
(paired against the no-grass CSV), the main pass and shadow pass, and a
PASS/FAIL verdict based on **p95 within each run**, then the median of those
p95s across rounds, not the frame median. A failed gate exits non-zero. `--report-only` is for bringing a target up without turning a
nightly red; `--no-gpu-timing` removes Bevy's timestamp queries for the
strictest frame check, at the cost of the GPU pass columns.

The bench itself also accepts the gate flags directly:

```text
cargo run --profile release-lto --example bench -- --tier competitive --target-hz 240
cargo run --profile release-lto --example bench -- --tier competitive --target-hz 240 --baseline-csv target/no-grass.csv
```

`--budget-ms 4.167` is equivalent to `--target-hz 240`. Full output includes
the machine name from `--machine`.

### Reference machines

GPU frame times cannot be enforced on GitHub-hosted runners. The gate runs
manually (or on a nightly schedule once the machines exist) through
`.github/workflows/perf-240.yml` on self-hosted runners labelled with the
names in `tools/reference-machines.json`:

| Runner label | Machine | Why it is a reference |
| --- | --- | --- |
| `t1200` | NVIDIA T1200 laptop | The development machine used for most numbers on this page. GPU-bound at 1080p. |
| `rtx3060` | NVIDIA RTX 3060 desktop | The class the default `High` tier targets. The 240 Hz gate must pass here before a build is called 240 Hz ready. |
| `rtx4070` | NVIDIA RTX 4070 or faster | The `Ultra` target. Also useful for exposing CPU-bound floors on older CPUs. |

The reference machine must run the same build profile and display mode as
the run it is compared with: 1920 x 1080 physical, 4x MSAA, no vsync,
plugged in, no other GPU work. The bench forces the resolution and no-vsync
mode itself.

### Recording runs

`tools/bench_240.py` writes the Markdown table to
`target/bench-240/latest.md`. Append the current run to the table below (or
replace the row for the same machine and date) so the gate's history lives
with the measurements. Do not record a run whose log contains a shader error.

| Date | Machine | Profile, rounds | Scene | No-grass p50 / p95 | High p50 / p95 | Competitive p50 / p95 | High | Competitive |
| --- | --- | --- | --- | ---: | ---: | ---: | :---: | :---: |
| 2026-10-17 | T1200 laptop | dev, 3, no GPU timing | perspective | 2.384 / 2.840 ms | 5.622 / 6.394 ms | 2.960 / 3.611 ms | FAIL | **PASS** |
| 2026-10-17 | T1200 laptop | dev, 3, no GPU timing | ARPG | 2.177 / 2.539 ms | 4.990 / 5.709 ms | 3.254 / 4.328 ms | FAIL | FAIL (4.328 vs 4.167) |
| 2026-10-17 | RTX 4070 Ti + i5-4570S (Linux) | release-lto, 3, no GPU timing | perspective | 2.611 / 2.956 ms | 2.651 / 3.000 ms | 2.649 / 2.994 ms | **PASS** | **PASS** |
| 2026-10-17 | RTX 4070 Ti + i5-4570S (Linux) | release-lto, 3, no GPU timing | ARPG | 2.428 / 2.755 ms | 2.519 / 2.823 ms | 2.495 / 2.801 ms | **PASS** | **PASS** |

The first real gate run says two useful things. `High` is still far from the
240 Hz budget in both scenes. `Competitive` **passes in perspective** on the
T1200 with 0.56 ms of headroom, and misses the ARPG scene by 0.16 ms, with the
no-grass floor already at 2.54 ms. One timing-instrumented run had a single 17 ms
p95 spike in its no-grass baseline, which is why the gate takes the median of
three rounds instead of trusting one run.

The instrumented diagnostic run (GPU timing on) put the Competitive main pass
at about 1.9 ms p95 in perspective and 2.7 ms at the ARPG camera; the shadow
pass was under 0.04 ms because the tier does not cast grass shadows. So the
remaining ARPG gap is not the grass caster or the shadow pass. It is the
main-pass pixel cost plus the 2.5 ms non-grass floor.

That is the number the optimization work has to move, and the gate makes
success or failure unambiguous. The `High` row is the default-quality
baseline; the Competitive row is the 240 Hz target that the clump/meshlet
work is meant to improve.

**RTX 4070 Ti note.** The 4070 Ti rows pass because the scene is empty, not because the frame is roomy. Their `no-grass` floor is 2.4–2.6 ms p50 (Bevy on a 2013 i5), about 60–65% of the 4.167 ms budget, and grass adds only 0.04–0.1 ms at p50 (about 0.3 ms of GPU time, mostly overlapped with the CPU). A real game's own frame has to fit in the remaining 1.2 ms at p95. So the gate shows that grass is not what stops a game reaching 240 FPS on this card; it does not show that a game reaches it. A faster CPU should lower the floor, but that has not been measured. `High` and `Competitive` are indistinguishable in frame time here (the run had no GPU timing), so the tier difference needs a run with GPU timing on.

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

Each camera keeps 256 × 256 cells of 0.25 m (64 m; `GrassSettings::trample_cell_size`) around what it looks at. Cells are addressed modulo 256, so trails stay in world space without copying when the window moves. The 512-interactor case uses `bench --interactors 512`, with interactors walking rings around the view.

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

## M4: the rest of the look, and Bevy's other passes (October 14, 2026)

Paired runs against the M3 build, four rounds each, T1200. The bench's styles keep the M3 look; `--sheen`, `--tint` and `--toon` add one optional term each.

| Change | Perspective | DEMO ARPG GAME | Notes |
| --- | --- | --- | --- |
| First version: shared look module, new terms always compiled in, bench styles inheriting sheen and tint from `MEADOW` and `TALL_PRAIRIE` | +0.29 ms | +0.22 ms | 8–9% of grass cost. |
| **Optional terms compiled in only when a style uses them** (shader definitions, part of the pipeline key) | **+0.06 ms** | **+0.02 ms** | **Kept.** Noise level: a game using none of the terms, DEMO ARPG GAME included, pays what it paid at M3. |
| Ground tint on | +0.05 ms | +0.02 ms | Free: a few vertex instructions. |
| Toon bands on | +0.19 ms | +0.06 ms | |
| Sheen on, per fragment (height and style passed down) | +0.31 ms | +0.17 ms | |
| Sheen on, roughness left constant | about the same | about the same | Not kept: losing constant roughness was not the cost. |
| **Sheen on, amount computed per vertex** (one varying) | **+0.19 ms** | **+0.05 ms** | **Kept:** 20–30% less than per fragment in a paired run, and simpler. |

The prepass and deferred paths cost nothing unless a camera has a prepass or is deferred; DEMO ARPG GAME's has neither. With a prepass, grass is drawn twice (prepass and lit pass), as every opaque mesh is; with motion vectors its vertex shader builds each blade twice.

Lesson: **put optional look terms behind shader definitions from the start.** Even a term set to zero costs fragment time when it is compiled in, because the compiler cannot fold it.

## Variation (October 15, 2026)

Paired runs, four rounds each, T1200. Each bench flag takes `SUMMER_WILD`'s settings for one kind of variation; "grass cost" is the frame time minus a `--no-grass` run's.

| Change | Perspective | DEMO ARPG GAME | Notes |
| --- | --- | --- | --- |
| Everything off, against M4 | +0.07 ms | −0.09 ms | Noise level (+2% / −3% of grass cost). The first version cost +0.10 ms in generation; skipping the species pick, and the second read of the field's entry, when a field has one species brought it back. Images are bit-identical to M4's. |
| `--tufts`: tufts and bare patches | +0.04 ms | −0.03 ms | Free. Generation costs more (the tuft lookup and the bare-patch noise), the main pass less (bare ground). |
| `--shape`: twist, droop, fold | −0.05 ms | −0.10 ms | Free. Drooping blades are shorter on screen. |
| `--variation`: colour patches, dry tips, dead blades | +0.14 ms | +0.08 ms | 3–4% of grass cost: a noise sample per vertex. |
| `--species`: four plants, 21% of blades | +0.28 ms | +0.16 ms | 6.5% / 4.7% of grass cost (seven rounds; the first four alone gave 10% / 6%). Most of it is generation, and some is the main pass: heads, wider leaves, and the shape code the heads turn on. |
| Species pick reading only the chance and clustering of each species, and only the shape and stiffness of the chosen one | +0.02 ms | −0.00 ms | Not kept: the compiler already left out the loads that were not used. |
| Species pick with one hash per species instead of three | +0.04 ms | +0.02 ms | Not kept: the pick's arithmetic is not the cost. |

Lesson: as with M4's look terms, **every kind of variation is behind a shader definition or a uniform branch**, so a game pays only for what its styles use.

## Re-test on the most complex grass (October 15, 2026)

The ideas from the M2 pass that gained a little were tried again with variation on, since the work per blade has grown: the bench's styles with every kind of variation and both optional terms (`--species --tufts --shape --variation --sheen --tint`). The question was whether any now passes 5% at p95 without hurting the image. Five paired rounds, T1200.

Grass cost with all of that on: about 4.3 ms in perspective (4.9 ms at p95) and 3.3 ms at DEMO ARPG GAME's camera (3.8 ms at p95). Generation is now 1.63 ms and 1.14 ms of it, against 0.92 ms and about 0.4 ms for plain grass.

| # | Idea | What was built | p95, perspective | p95, DEMO ARPG GAME | p50, perspective | p50, DEMO ARPG GAME | Verdict |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | Cheaper shadow filtering | The grass pipeline specialized with Bevy's `Hardware2x2` filter instead of the view's | **−10.1%** | −4.1% | −8.7% | −10.1% | **Rejected: degrades the image.** At the same frame, 8.6% of pixels change by more than 16 of 255 in both views. Self-shadowing on the blades turns harsh, dark and speckled (`target/retest/compare_*.png` when re-run). |
| 3 | Screen-size culling | Generation drops perspective blades under 4 px tall | +1.9% | +0.1% | +1.5% | −1.2% | Rejected. It removes about 1% of far blades: even the smallest plant in the scene, a 0.2 m broad leaf, is 2.6 px tall only at the very edge of the 100 m radius. The test costs about what it saves. |
| 5 | Occlusion culling by the terrain | The chunk cull marches 16 steps from the eye to the four top corners and the centre of each chunk's box through the field's own heightmap, and skips chunks hidden from all five (1 m margin) | +1.5% | +1.0% | +0.5% | −1.0% | Rejected. The flythrough has almost no chunk fully behind a hill: it removed 11 of 33,800 far blades. Worth trying again only in a hillier scene. |
| T | Translucency with a branch | The backlit term computed only for backlit faces (identical image), the M3 variant that once measured cheaper | 0.0% | −0.8% | −1.5% | −1.3% | Rejected: within noise. |

Not tried again, and why:

- **2, no grass shadows; 4, a shorter grass radius; 8, fewer, wider blades at DEMO ARPG GAME's camera.** These change the look, and each is already a setting: `GrassShadows::Off`, the `Medium` or `Low` tier (70 m and 40 m), and the style's density and width.
- **6b, grass in a pass of its own, and a depth-only grass pre-pass.** Both made frames slower. The reasons (MSAA testing depth per sample, storing and reloading the multisampled targets) do not depend on the blades.

Lesson: with variation on, the cost has moved further into the generation pass (+0.7 ms in perspective) and into pixels, not into work that culling can remove.

### Two levers, tried the same day

Same scene, five paired rounds.

| Idea | What was built | p95, perspective | p95, DEMO ARPG GAME | p50, perspective | p50, DEMO ARPG GAME | Verdict |
| --- | --- | --- | --- | --- | --- | --- |
| A lighting path written for grass | Bevy's `apply_pbr_lighting` cut down to a matte leaf: the same Burley diffuse, shadows, clustered point and spot lights, environment map and ambient, but `F_AB` from Bevy's polynomial instead of three lookups in its DFG table per fragment, and the sun's specular only where there is sheen. Images: a mean change of 1–3 of 255 (slightly less grey gloss), 0.1% of pixels by more than 16; no visible difference side by side | −0.3% | +1.5% | −0.1% | +0.2% | Rejected: within noise. The arithmetic and table lookups Bevy adds for grass are not where the main pass's time goes. |
| Generation without last frame's wind | The previous bend set to the current one, as it could be for views without motion vectors (the bench's have none). Generation 1.62 → 1.54 ms in perspective | −1.2% | +1.2% | −1.1% | −0.6% | Rejected: the 0.08 ms saved in generation does not show in the frame. |
| Bare-patch noise after thinning | The two noise samples taken only for blades that survive thinning without them; identical images | +0.7% | +0.2% | +0.7% | +0.8% | Rejected: within noise. |

So the main pass's cost is neither the lighting maths nor its table lookups, and the generation pass is not limited by its texture samples. What is left in the main pass is the shadow lookup (9 samples), rasterizing and shading 2×2 quads of thin blades at 4× MSAA, and the clustered-light lookup. The next useful step is a diagnostic rather than another optimization: stub each of those out once to find which one dominates.

## The frame-time floor (October 15, 2026)

On a second machine, an RTX 4070 Ti with an Intel i5-4570S (a 4-core Haswell from 2013, no hyper-threading), the grass costs about 0.62 ms of GPU time in perspective and 0.45 ms at DEMO ARPG GAME's camera, against 3.8 and 3.1 ms on the T1200. The frame did not follow: about 2.9 ms p50 and 3.5 ms p95, with or without grass. This section finds out why. Same bench, 1080p, 4× MSAA, no vsync, Linux X11 and Vulkan; paired, interleaved rounds as above, with the frame time's p50 and p95 rather than grass cost.

### What sets the floor

The GPU was busy 47% of the time at full clocks and no CPU thread was saturated, so the frame waited on the CPU. Taking things away, four rounds each:

| Run | Frame p50 | Frame p95 |
| --- | --- | --- |
| Bench, as before | 2.91 ms | 3.55 ms |
| `--no-grass` (plugin loaded, no field) | 2.89 ms | 3.60 ms |
| `--no-plugin` (no `GrassPlugin` at all) | 2.71 ms | 3.34 ms |
| `--no-plugin --no-gpu-timing` | 2.56 ms | 3.15 ms |
| ... and `--no-terrain --no-shadows`: an empty scene | 2.13 ms | 2.69 ms |

**An empty Bevy app takes 2.1 ms on this CPU.** The whole grass library, drawing included, is about 0.2 ms of the frame (7%), and the GPU timing queries the bench adds are another 0.15–0.2 ms. On the T1200 the GPU was the limit, so the floor never showed.

A trace (Bevy's `trace_chrome`, which doubles frame time, so proportions only) shows why. A frame runs about 620 systems; grass is 10 of them. The render thread is the critical path, and most of its `Render` schedule is spent waiting while its multi-threaded executor hands systems to the compute pool. `nsys` counts about 340 `futex` calls per frame. Presenting costs 0.07 ms a frame, so the swapchain is not the cause.

### Hypotheses tested

Kept if the frame gets at least 10% faster (the developer's rule for this pass).

| # | Hypothesis | What was built | Perspective p50 / p95 | DEMO ARPG GAME p50 / p95 | Verdict |
| --- | --- | --- | --- | --- | --- |
| L1 | The orthographic trample focus scans every chunk, three times a frame (15,625 chunks, 0.5 MB, more than Haswell's L2) | `chunk_height_at` indexes each field's chunk block directly | +0.0% / +0.8% | −2.9% / −3.3% | Rejected: under 10%, and the retest below found less. |
| L2 | Grass rebuilds five bind groups every frame | Rebuilt only when a buffer they bind is reallocated (`BufferId`) | +2.4% / +1.5% | — | Rejected: noise. |
| L3 | `GrassPlugin` always adds `FarGrassMaterial`'s material plugin and systems | Left out | −0.7% / −1.6% | — | Rejected: under 10%. |
| H1 | Unused Bevy plugins add systems | Picking, animation, text, gamepads and scenes disabled (gizmos cannot be: `LightPlugin` needs them) | −2.0% / +0.3% | — | Rejected: noise. |
| H2 | The multi-threaded executor costs more than the systems on 4 cores | `--single-threaded`: one thread per app; pipelined rendering kept | −9.7% / −12.6% | −4.3% / −5.9% | Under 10% alone; see H2 + H3. |
| H3 | Default release builds leave CPU time on the table | `--profile release-lto`: fat LTO, one codegen unit, no `target-cpu=native` (`native` gained nothing more) | −12.0% / −8.3% | −10.8% / −10.5% | **Kept** as an opt-in profile. |
| | **H2 + H3** | | **−18.7% / −17.7%** | **−15.8% / −16.5%** | **Kept**: 2.44 / 2.68 ms and 2.34 / 2.53 ms. |
| | Fewer worker threads (3) | `TaskPoolOptions::with_num_threads` | +17.5% / +7.3% | — | Rejected. Six threads: no change. |
| | No pipelined rendering | `PipelinedRenderingPlugin` disabled | +26% / +15% | — | Rejected: pipelining overlaps the two apps. |

### Retest: the library's own CPU work, at a 4–6% bar

Slower CPUs pay for the library's CPU work too, so three changes were retested together, kept if the frame gets 4–6% faster combined. Measured as a game runs, without the bench's GPU timing (`--no-gpu-timing`), six paired rounds:

| Change | Perspective p50 / p95 | DEMO ARPG GAME p50 / p95 |
| --- | --- | --- |
| L1, chunk lookup by index | +0.2% / +1.0% | −1.8% / −2.0% |
| L3, far grass in its own `FarGrassPlugin`, added only by games that use it | −1.1% / +0.2% | −1.0% / +0.2% |
| L4, trample, cull and generate in one compute pass, timed inside it (`pass_span`); the GPU readback tests pass | −0.7% / −1.1% | −0.2% / +1.2% |
| **All three** | **+1.7% / +1.3%** | **−0.6% / +2.6%** |

Rejected: together they are within noise. The estimate of 4–6% was wrong. These three were the CPU work that the trace attributes to grass, but they are not where the library's 0.2 ms goes. What remains unexplained is the plugin's fixed cost: `--no-grass` (plugin loaded, no field) is still about 6% slower than `--no-plugin`. With no field, every frame still runs the trample decay over all 65,536 cells, records the compute encoder and the grass render-graph system, and schedules ten systems. Stub those out one at a time before building another fix.

Lessons:

- **On a CPU-bound machine, a faster GPU does not show in the frame.** Before reading frame times on a new machine, run the empty scene (`--no-plugin --no-gpu-timing --no-terrain --no-shadows`) and compare.
- **Grass cannot reach a 10% frame change here.** Leaving the library out entirely saves 7%. Its CPU work is a few uniform writes, bind groups and one compute encoder; at this point it gets cheaper only by the GPU numbers above.
- **The levers are the game's, not the library's.** The release profile and the executor belong to DEMO ARPG GAME's `Cargo.toml` and app setup. `--single-threaded` was measured only on 4 cores; on a CPU with many cores and heavier game systems it may lose, so measure it there before adopting it.

## GPU pass on a starved GPU (October 15, 2026)

### How it was measured

The RTX 4070 Ti was locked to 300 MHz core and 5001 MHz memory (`sudo nvidia-smi -lgc 300,300` and `-lmc 5001,5001`; undo with `-rgc` and `-rmc`). That is about twice a T1200's shading rate with a similar ratio of arithmetic to memory bandwidth, and it makes the bench GPU-bound again: grass costs 6.5 ms in perspective and 4.1 ms at DEMO ARPG GAME's camera (p50), more than on the T1200. With frame time limited by the GPU, the usual method applies unchanged: paired rounds, frame time p50 and p95, grass cost against `--no-grass`. The card is a newer architecture (Ada) than the T1200 (Turing), so what is kept here should be confirmed on the T1200 with `tools/bench_ab.py`.

### Where the time went

Each part stubbed out for a run, four rounds, perspective p50 (DEMO ARPG GAME's camera in brackets):

| Part | Share of grass cost |
| --- | --- |
| Compute (trample, cull, generate): every draw skipped | 45% (28%) |
| Shadow lookup in grass fragments | 12% (11%) |
| The rest of the lighting | 1% (2%) |
| Grass in the shadow map | 13% (14%) |
| Rasterizing and shading the blades, without lighting | the rest, about 29% (45%) |

The lighting maths is nearly free: of the 13% that a flat colour saves, the shadow lookup is 12%. That is why the cheaper lighting function (above) gained nothing.

The surprise was generation: 3.9 ms at the locked clock (0.35 ms unlocked), almost exactly the 9.45× clock ratio. Work that scales with the core clock like that is a serial chain, not bandwidth. `cull` handed each visible chunk to one 64-thread workgroup, and `generate` walked all of a chunk's candidates in a loop: about 9,200 candidates, 144 rounds of four barriers, for a full-density chunk. The few dense near chunks ran on a few multiprocessors while the rest of the GPU sat idle. At DEMO ARPG GAME's camera only a few dozen chunks are visible at all.

### G6: chunks split into slices (kept)

`cull` now writes one work item per slice of 512 candidates (`VisibleChunk` holds the chunk, the slice's first candidate and its count), and `generate` runs one workgroup per slice. Nothing else changes: the 22 presets render pixel-identical (golden check, mean difference 0.000), the bench draws the same blades per bucket frame for frame, and the GPU readback tests pass. Five paired rounds:

| Slice | Perspective p50 / p95, share of grass cost | DEMO ARPG GAME p50 / p95 |
| --- | --- | --- |
| 256 | −48.9% / −43.6% | −44.1% / −37.0% |
| **512** | **−48.2% / −43.4%** (−3.1 ms, a third of the frame) | **−43.2% / −38.4%** |
| 1024 | −46.8% / −41.3% | −39.0% / −39.0% |

512 is kept: as fast as 256 within noise, with half the work items. Generation fell from 3.9 to 0.18 ms. The work list holds 8,192 slices (the default of `GrassSettings::max_chunk_slices`); the bench peaks at 420 at High, 739 at Ultra and 270 at DEMO ARPG GAME's camera. The overflow check now also warns when the list overflows.

At stock clocks the frame stays CPU-bound, so the frame barely moves, but nothing gets worse. Five paired rounds against the pre-G6 build: generation fell from 0.36 to 0.034 ms in perspective and from 0.23 to 0.031 ms at DEMO ARPG GAME's camera. The frame went −3.6% / −4.0% (p50 / p95) in perspective and −4.2% / −3.7% at DEMO ARPG GAME, which is most of what grass cost there. The 22 presets again rendered pixel-identical.

### After G6

Grass cost fell to 3.4 ms in perspective and 2.4 ms at DEMO ARPG GAME's camera (p50). Stubbed out again:

| Part | Perspective | DEMO ARPG GAME |
| --- | --- | --- |
| Compute | 3% | 5% |
| Vertex shading (every vertex moved off-screen, so nothing is rasterized; the shadow pass still draws, so this is an upper bound) | about 4% | under 6% |
| Grass in the shadow map | 14% | 22% |
| Rasterizing and shading the blades | 78% | 91% |
| … of which the shadow lookup | 20% | 18% |
| … of which the rest of the lighting | 3% | 3% |
| 4× MSAA (grass cost with MSAA off) | −27% | −32% |

### Other hypotheses

| # | Hypothesis | Verdict |
| --- | --- | --- |
| G1 | `let field = fields[i]` copies the 272-byte `Field` per vertex and per candidate | Not testable on NVIDIA, whose compiler drops unused loads. Worth one run on an AMD or Intel GPU. |
| G2 | Per-blade work (curve, facing, minimum width) repeated for every vertex | Rejected: all vertex shading is about 4% of grass cost. |
| G3 | Bevy's 4-sample hardware shadow filter for grass instead of the view's 9-sample Gaussian | **−17.1% / −18.2%** in perspective, **−12.7% / −12.2%** at DEMO ARPG GAME's camera. Passes, but it changes the look: blades shadowing each other turn crisper and grainier, 0.3–11% of pixels off by more than 16 of 255 across the 22 presets. **Developer's decision.** |
| G4 | Generation's arithmetic: storage loads instead of filtered texture reads, integer division, barriers | Rejected: after G6 all compute is 3%. |
| G5 | Fewer interpolated values (13 floats per vertex) | Not built: interpolation sits inside the raster bucket, and on its own it cannot be large. |

Lessons:

- **Lock the clocks to find what a slow GPU pays for.** At full clock on a fast GPU, generation looked like 0.35 ms; on a starved one it was half the grass.
- **Spread work across workgroups by its size, not by its container.** One workgroup per chunk made the frame wait for the densest chunk.
- **Stub out to attribute, then optimize.** The vertex shader, the lighting maths and generation's arithmetic all looked like candidates and together cost under 10%.

## Five more hypotheses on the T1200 (October 15, 2026)

Back on the development laptop (T1200, GPU-bound), after G6. Same method: `tools/bench_ab.py`, 6–10 interleaved paired rounds against the G6 build, shares of grass cost (reference minus `--no-grass`). Kept if both p50 and p95 improve by at least 3% in both scenes (the developer's bar for this pass).

| # | Hypothesis | What was built | Perspective p50 / p95 | DEMO ARPG GAME p50 / p95 | Verdict |
| --- | --- | --- | --- | --- | --- |
| P1 | A depth prepass shades each pixel once | Bevy's `DepthPrepass` on the camera (bench `--prepass`), which grass already draws into | +11.6% / +17.8% | +29.9% / +26.5% | Rejected, as in M2: the main pass fell from 3.43 to 2.51 ms, but drawing every blade twice cost more. |
| P2 | Blades drawn front to back hide the ones behind before those are shaded | `cull` counts slices per distance band (32 bands over the grass radius, view depth at an orthographic camera) and a second pass, `place`, writes them nearest band first, so generation and drawing follow | +0.2% / +0.6% | +1.3% / +1.2% | Rejected: the main pass did not move (3.69 against 3.68 ms). The 0.3 ms main-pass rise seen after G6 was noise between sessions, not draw order. Images identical. |
| P3 | Grass work nobody needs: compute with no field, trample decay with nothing trampled | Compute skipped with no chunks; the decay pass skipped once the CPU's mirror of its recovery step shows every cell back | +1.4% / +2.4% | +1.9% / −0.2% | Rejected. With no field, the frame was 0.04–0.08 ms (2–3%) faster: real, but under the bar, and only where there is no grass. |
| P4 | Shadow maps need less detail than the eye | Shadow draws at LOD 1 or coarser (separate draw arguments) | −2.3% / −0.6% | +1.8% / +0.5% | Rejected. DEMO ARPG GAME's camera already draws LOD 1 at High, so it is the control. The shadow map's cost is its pixels, not its vertices. |
| P5 | The sun's shadow per vertex instead of per pixel | See below; Bevy's 9-tap filter per vertex | −3.3% / −7.0% | −7.3% / −4.1% | Rejected by the developer: passes the bar, but the shadows look cheap. Reverted. |

The ceiling for P5, grass with no shadow lookup at all, was −31.5% / −24.7% in perspective and −25.7% / −3.6% at DEMO ARPG GAME's camera.

### P5: the sun's shadow per vertex (rejected)

Bevy binds its shadow maps for the fragment stage only, so grass bound them again (group 3 of the main pass: Bevy's lights uniform, the directional shadow-map array and its comparison sampler, visible to the vertex stage). The vertex shader repeated Bevy's `fetch_directional_shadow` for the first directional light (cascade choice and blend, normal and depth bias, on the side of the blade facing the sun) and passed the result to the fragment, which lit the blade with Bevy's per-pixel lookup switched off and took away the sun's direct light, from Bevy's own `directional_light`, where the vertices found shadow.

With one hardware-filtered tap per vertex it measured −5.6% / −20.3% in perspective and −12.8% / −4.5% at DEMO ARPG GAME's camera, but shadows blinked as the grass moved: one tap is close to on or off, so a whole segment between two vertices flipped at once. On 60 recorded frames of swaying grass, the mean on-off-on change between frames was 13.8 per pixel with per-pixel shadows, 15.9 with one tap per vertex and 14.3 with Bevy's 9-tap filter per vertex, which cost most of the gain (above). Even filtered, shadows resolved at 7 rows along a blade looked cheap next to per-pixel ones, and grass would also have lost point- and spot-light shadows (Bevy's shadow switch is one flag per surface, for every light). Per-vertex or per-blade shadow lookups are not worth trying again.

## Smoother motion and steadier shadows (October 15, 2026)

Comparing P5 with the build before it, the developer noticed that both blinked, and that blades vibrated rather than swayed. Two causes, neither a performance change:

- **The bend was stored in 8 bits per axis**, since M0. A blade's tip could only move in steps of 1/127 of its height, about 1.2 cm on a 1.5 m blade: blades held still, then hopped, out of step with each other. It is now 12 bits, with last frame's bend (for motion vectors) as a 4-bit step from it, in the same 16-byte instance: steps of 0.7 mm. Paired rounds against the 8-bit build: −0.7% / −0.4% at DEMO ARPG GAME's camera and +0.3% / +0.5% in perspective (p50 / p95 of grass cost), noise. The goldens changed (blade edges moved up to a pixel) and were re-blessed.
- **The `variation` example's shadow map was too coarse.** One cascade over 60 m in a 2048 map is about 4–5 cm per texel; a blade is 1–2 cm wide, so blade shadows popped on and off as blades crossed texel centres. It now uses three cascades, the first 12 m deep, as the bench's perspective scene already did with Bevy's defaults (first cascade 10 m), so its cost is in the numbers above. A 4096 map with `GrassShadows::Full` looked sharper still but cost +27% to +47% of grass cost and was not needed.

Lessons:

- **Judge motion, not stills.** The golden images could not show either problem.
- **Quantize per frame only what the eye cannot see step.** Positions were 16-bit and fine. The bend changes every frame, so its steps showed as motion.
- **A shadow texel must be smaller than a blade near the camera**, or grass shadows blink. Games should keep the first cascade short (10–15 m in perspective). DEMO ARPG GAME's single map, about 30 m deep, has texels of roughly 2 cm against blades 4–8 cm wide.

## After the API and polish passes (October 17, 2026)

The API work (settings, per-camera overrides, the split style, incremental tables, a plugin group, field overrides, interactor height, the wind clock) was measured against the build from before it, to check that nothing got slower.

**Method.** `tools/bench_ab.py`, four interleaved rounds in both scenes (dev profile with optimised dependencies, T1200, 1920 × 1080, 4× MSAA, High tier, 1,380 frames a run), three builds: `4747d7d` (before), `5b61aba` (the P1 pass: settings, split style, tables) and the working tree after the P2 pass. Medians of the four rounds:

| Scene | Before | After P1 | After P2 | No grass |
| --- | --- | --- | --- | --- |
| Perspective, frame p50 | 5.780 ms | 5.804 ms | 5.799 ms | 2.702 ms |
| DEMO ARPG GAME's camera, frame p50 | 5.008 ms | 5.024 ms | 5.024 ms | 2.724 ms |
| Perspective, main pass | 3.825 ms | 3.851 ms | 3.838 ms | 0.716 ms |
| DEMO ARPG GAME's camera, main pass | 3.450 ms | 3.448 ms | 3.462 ms | 0.698 ms |
| Perspective, blade generation | 0.108 ms | 0.110 ms | 0.109 ms | |
| DEMO ARPG GAME's camera, blade generation | 0.071 ms | 0.071 ms | 0.071 ms | |

Paired differences against the build before, as a share of grass cost: P1 +0.0% (perspective) and +0.6% (DEMO ARPG GAME's camera) at p50; P2 +0.6% and +0.8% at p50, and +1.8% and −1.5% at p95. All are inside the ±2% noise of this machine, so **nothing got slower**, which is what the design predicts: the changes are table work on the CPU when something changes, and in the shaders one `round` per blade (sway) and per full-detail vertex (flutter).

The no-grass baseline drifted from 2.3 to 3.0 ms over the session (the laptop warming up); the builds were interleaved in each round, so the paired differences are not affected. The raw rows are in `target/bench-ab/results.jsonl` (not committed).

The CPU side, per frame, is unchanged in kind: the main-world systems compare change flags, and tables are rebuilt only when something changed. P1-9's tests also pin that a style change every frame rewrites only the entries of the fields using it, and reads no image.

## On an RTX 4070 Ti at stock clocks (October 17, 2026)

The README's numbers came from the T1200 laptop only. This section measures the current build (`675d149`, after the API pass and the chunk slices) on the faster machine, at the clocks a user would have.

**Machine.** RTX 4070 Ti, stock clocks (not locked), Intel i5-4570S (4 cores, no hyper-threading, 2013), Linux X11, Vulkan. Quiet: browser closed, nothing else running.

**Method.** `cargo build --profile release-lto --example bench`, then five interleaved rounds of the bench with and without `--no-grass`, in three scenes: the perspective flythrough, the ARPG camera (`--ortho`) and the ARPG camera with `--interactors 512`, at the High tier and again at Ultra (`--tier ultra`). 1920 × 1080, 4× MSAA, no vsync, 1,380 frames a run after warm-up. Medians of five paired differences (grass minus no-grass in the same round).

**Grass cost is read from GPU timestamps, not the frame.** The frame is limited by the CPU (see "The frame-time floor"), so frame time barely moves with grass: it stays at about 2.7–2.9 ms either way. The grass's GPU time is the difference in the main pass (terrain included), culling and blade generation. The trampling pass is left out of the sum: it runs whether or not grass is drawn (0.007 ms with no interactors, 0.010 ms with 512).

| Scene | Tier | Main pass | Cull | Generate | **Grass GPU time** | Frame p50, grass / none |
| --- | --- | --- | --- | --- | --- | --- |
| Perspective | High | +0.291 ms | +0.006 ms | +0.022 ms | **0.32 ms** | 2.945 / 2.840 ms |
| ARPG camera | High | +0.237 ms | +0.005 ms | +0.020 ms | **0.26 ms** | 2.781 / 2.657 ms |
| ARPG, 512 interactors | High | +0.188 ms | +0.005 ms | +0.020 ms | **0.21 ms** | 2.784 / 2.679 ms |
| Perspective | Ultra | +0.436 ms | +0.005 ms | +0.025 ms | **0.47 ms** | 2.932 / 2.877 ms |
| ARPG camera | Ultra | +0.279 ms | +0.005 ms | +0.020 ms | **0.30 ms** | 2.755 / 2.650 ms |
| ARPG, 512 interactors | Ultra | +0.234 ms | +0.005 ms | +0.020 ms | **0.26 ms** | 2.770 / 2.674 ms |

The GPU differences are steady to within 0.01 ms across the five rounds (main pass, High, perspective: +0.285 to +0.296 ms). The frame differences are not: paired frame p50 runs from −0.05 to +0.16 ms in perspective and the median is +0.12 ms, which is inside this CPU's noise. Read the GPU columns, not the frame columns.

**What it says.**

- Against the T1200 (3.8 and about 3 ms, measured before the chunk slices), grass is about 10× cheaper on this card at the High tier. The T1200 after G6 was 3.4 and 2.4 ms, so the like-for-like ratio is nearer 9–11×.
- Ultra (150 m radius, up to 1.2 million blades, shadows into the second cascade) costs 0.15 ms more than High in perspective and 0.04–0.05 ms more at the ARPG camera.
- The design target, 1 ms at the High tier on RTX 3060 class hardware, is still not measured. This card uses a third of it.
- 512 interactors lower the main pass (by 0.05 ms at High): probably because trampled blades lie lower and cover fewer pixels (not checked). This is not a saving to count on; it depends on the interactors' radius and where they walk.
- On a CPU of this age the frame stays at about 340–360 fps with or without grass. A faster CPU would let the frame follow the GPU figures; that has not been tested here.

Raw rows were kept in the scratchpad only; rerun with `tools/bench_ab.py` or the commands above.

## Leaving plugins out of `DefaultPlugins` (October 17, 2026)

A suggestion: `DefaultPlugins` is heavy and may include plugins the game does not need, so drop them. The earlier H1 test left out picking, animation, text, gamepads and scenes and found noise. This pass tests all of the rest that can be left out, one group at a time and all together.

**What can be left out.** `DefaultPlugins` follows Cargo features, so with this crate's examples (Bevy's `3d` profile plus `ui`) it is a few dozen plugins. The bench gained `--without a,b,c`, which disables named members at runtime (`PluginGroupBuilder::disable`); `--without lean` is every name below. Groups: `picking` (3 plugins), `animation`, `scene` (scene and world serialization), `gilrs`, `clipboard`, `gltf`, `states`, `a11y`, `input-focus` (2), `anti-alias`, `post-process`, `ui` (UI, its renderer and text). Not removable: the window, winit, render, mesh, image, camera, light, core pipeline, PBR and gizmo plugins that rendering needs (`LightPlugin` needs the gizmos; see H1).

**Method.** `--release` (not LTO, so the build is quick), RTX 4070 Ti and i5-4570S, 1920 × 1080, 4× MSAA, no vsync, no GPU timing queries, 1,380 frames a run. Four interleaved rounds of 14 variants in two scenes: the empty scene (`--no-plugin --no-terrain --no-shadows`, the floor of "The frame-time floor") and the real one (perspective, High tier). Paired difference of frame time against the unmodified run of the same round.

| Left out | Empty scene, p50 | Empty scene, p95 | Perspective with grass, p50 |
| --- | --- | --- | --- |
| Nothing (reference) | 2.408 ms | 2.746 ms | 3.103 ms |
| **Everything above (`lean`)** | **−0.005 ms (−0.2%)** | −0.022 ms (−0.8%) | **±0.000 ms (0.0%)** |
| `ui` | −0.012 ms | −0.047 ms | +0.005 ms |
| `scene` | −0.011 ms | −0.056 ms | +0.009 ms |
| `gilrs` | −0.002 ms | −0.069 ms | −0.041 ms |
| `post-process` | −0.006 ms | −0.050 ms | −0.012 ms |
| Each other group | within ±0.006 ms | within −0.041 ms | within ±0.016 ms |

**Verdict: rejected, no effect.** Taking out every plugin that can be taken out changes the frame by 0.2% or less at p50, against the 5–10% bars used elsewhere on this page. No single group moves it either; the largest p50 gain is 0.012 ms (0.5%), and the rounds of one variant spread wider than that. That fits plugins whose systems have nothing to do costing close to nothing (not traced). Plugins that stay in `DefaultPlugins` but add real work (like the UI) were exactly the ones tested and still showed nothing.

**What it does not cover.** Leaving a plugin out at runtime does not remove its code from the binary: building with fewer Cargo features would shrink the binary and compile time, but not the frame, since the plugins here were already idle. The ones the renderer needs cannot be left out. And `MinimalPlugins` plus a hand-picked list would add back those same renderer plugins, ending at the same set.

**Where the floor is.** The same conclusion as "The frame-time floor": the 2.1–2.4 ms is the cost of running Bevy's renderer at all (hundreds of small systems, the render thread as the critical path, the multi-threaded executor's wake-ups on 4 cores), not of unused plugins. The levers that moved it remain `--single-threaded` (−10%) and the LTO profile (−12%); they belong to the game's `Cargo.toml` and app setup.

## The CPU floor on an Apple M2 Pro (macOS) (October 17, 2026)

`docs/cpu-floor-test.md` asks whether the ~2.1 ms empty-scene floor on the RTX 4070 Ti / i5-4570S is the 2013 CPU or a cost every Bevy app pays. This is that test on a much faster CPU, run for a colleague.

**Machine.** Apple M2 Pro, 10 cores (6 performance and 4 efficiency, no SMT), 16-core integrated GPU, 32 GB memory, macOS 26.6.2 (Tahoe), Metal 4. 1920 × 1080 window, 4× MSAA, no vsync. Commit `9dd146e` (`exp/lean-plugins`, based on `6577d5f`), `release-lto` (`cargo build --profile release-lto --example bench`, 9m52s).

**Method.** The plan's ladder, five interleaved rounds of steps 1–4 in order, each run 1,379 measured frames after the 120-frame warm-up, `--no-gpu-timing` throughout. The GPU timestamp columns are all `NaN` on this Metal backend, so no GPU split is available.

| Step | Flags | p50 (median, range) | p95 (median, range) |
| --- | --- | --- | --- |
| 1 empty | `--no-plugin --no-gpu-timing --no-terrain --no-shadows` | **2.012 ms** [1.999–2.055] | 7.633 ms [6.781–7.746] |
| 2 + terrain, shadows | `--no-plugin --no-gpu-timing` | 2.637 ms [2.621–2.717] | 8.736 ms [8.277–13.293] |
| 3 + grass plugin, no field | `--no-gpu-timing --no-grass` | 2.725 ms [2.704–2.759] | 8.690 ms [8.224–9.227] |
| 4 + grass, High tier | `--no-gpu-timing` | 4.367 ms [4.344–4.408] | 16.193 ms [15.914–16.286] |

Against the reference numbers from the i5-4570S:

| Step | i5-4570S p50 | M2 Pro p50 | Change | Step's addition on i5 / M2 Pro |
| --- | --- | --- | --- | --- |
| 1 empty | 2.075 ms | 2.012 ms | −3% | — |
| 2 + terrain, shadows | 2.548 ms | 2.637 ms | +3% | +0.473 / +0.625 ms |
| 3 + grass plugin | 2.656 ms | 2.725 ms | +3% | +0.108 / +0.088 ms |
| 4 + grass, High | 2.669 ms | 4.367 ms | +64% | +0.013 / +1.642 ms |

**The floor did not move.** A CPU with roughly 2× the i5-4570S's single-thread speed (published benchmarks, not measured here) produces the same empty-scene p50 (2.01 vs 2.08 ms). By the plan's own reading, that is the "≥ 1.5 ms, little change" row: the ~2 ms is not the old CPU's speed. It is a fixed per-frame cost of running Bevy's renderer on this platform. This run cannot separate portable Bevy cost from macOS/Metal's present and compositor path, so it does not prove the same 2 ms would appear on a fast Windows CPU; it does show the floor does not follow single-core speed.

**Steps 2–4 are GPU-bound on this machine, not CPU conclusions.** The M2 Pro's 16-core integrated GPU is far weaker than the 4070 Ti. The grass field adds 1.64 ms here against 0.013 ms on the 4070 Ti, and the frame scales with the grass tier (single runs, not part of the five-round ladder):

| Tier | p50 |
| --- | --- |
| low | 2.933 ms |
| medium | 3.384 ms |
| high | 4.382 ms |
| ultra | 6.903 ms |
| competitive | 2.899 ms |

The cost rising with blade count is a GPU signature. Terrain and shadows (+0.63 ms) are likely GPU work for the same reason. So the ladder's later steps say nothing about CPU headroom here; only step 1 does.

**The p95 is not usable from this run.** It is 7.6 ms even for the empty scene, with periodic stalls roughly every 80 ms, against 2.6 ms on the quiet i5 machine. The machine was **not quiet**: a Rancher Desktop VM was using ~49% CPU, iTerm ~23%, and Teams, Outlook, Chrome, Brave and VS Code were open (load average 4–8). The p50 was stable to ±0.06 ms across rounds, but the tail is contaminated by that background load. A clean p95 needs a quiet desktop; redo this run there.

**Follow-ups from the plan that macOS cannot do.**

- **Four cores.** macOS has no `taskset` or hard CPU affinity, so the 4-core variant could not be run as written.
- **`--single-threaded`.** It panics on macOS in `raw-window-metal` ("can only access NSView on the main thread") at surface access on the render thread, so no number was obtained. This is a Bevy/Metal limitation, not a grass one.

**What it says for a 240 Hz macOS game.** The ~2 ms fixed floor is still half the 4.167 ms budget before the game exists, exactly as on the i5. Grass at the High tier is the other limit here, because this GPU is slow; the Competitive tier (2.90 ms) is the one that would fit. The plan's next step still applies: trace what the frame does, and repeat on a quiet machine for a usable p95.

## The CPU floor on an i7-11850H (October 17, 2026)

This runs the plan in [`docs/cpu-floor-test.md`](cpu-floor-test.md) on a newer laptop CPU, to separate the old PC's CPU cost from Bevy's own per-frame cost. Commit `9dd146e` (`exp/lean-plugins`, after `6577d5f`).

**Machine.** Intel Core i7-11850H (8 cores, 16 threads, 2.50 GHz), NVIDIA T1200 Laptop GPU (driver 32.0.15.9647) with Intel UHD Graphics, Windows 11 Pro 10.0.26300. `cargo build --profile release-lto --example bench` took 15m53s for the dependency tree and 10m24s for the branch rebuild. The only available Windows power scheme was **Balanced**, not the high-performance plan the test plan asks for, so these numbers may understate the CPU's full-speed result. No runs were rejected: the logs had no errors or panics.

**Method.** The four-step ladder from the plan, 1920 × 1080, 4× MSAA, no vsync, 1,380 measured frames after warm-up, five rounds in order. Medians of the five rounds, with the p50 range in brackets.

| Step | p50 | p95 |
| --- | --- | --- |
| 1 empty scene | **1.359 ms** [1.356–1.411] | 1.650 ms |
| 2 + terrain, shadows | 2.097 ms [1.974–2.182] | 2.707 ms |
| 3 + grass plugin, no field | 2.101 ms [2.062–2.206] | 2.731 ms |
| 4 + grass, High tier | 5.711 ms [5.661–5.760] | 6.851 ms |

| Follow-up, five rounds | p50 | p95 |
| --- | --- | --- |
| Step 1, four-core affinity | 1.554 ms [1.517–1.619] | 1.776 ms |
| Step 1, `--single-threaded` | **1.012 ms** [1.002–1.018] | 1.255 ms |
| Step 4, four-core affinity | 5.700 ms [5.681–5.717] | 6.837 ms |
| Step 4, `--single-threaded` | 5.651 ms [5.632–5.687] | 6.670 ms |

**What it says.** The empty-scene floor is 1.359 ms, down 35% from the i5-4570S's 2.075 ms. That is the test plan's 1.0–1.5 ms band: the old CPU was part of the cost, but a faster CPU did not remove the floor, which is still about a third of a 240 FPS frame. Terrain and shadows add 0.74 ms here (paired median; 0.47 ms on the old PC), so that addition did not shrink with the floor and is not simply old-CPU compute. The grass plugin adds about 0.02 ms (paired median), against 0.11 ms on the old PC.

The follow-ups show two different limits. Four-core affinity leaves the empty frame 14% slower (1.554 vs 1.359 ms), so extra cores help a little, but it leaves the High-tier frame unchanged (5.700 vs 5.711 ms). `--single-threaded` cuts the empty frame by 26% (1.012 ms) and the High-tier frame by only 1.1% (5.651 ms). The grass scene is therefore GPU-bound on the T1200: at 5.7 ms p50 it misses the 4.167 ms, 240 FPS budget before the CPU matters. On the CPU side, the 1.0 ms single-threaded empty frame is the useful number for a game that controls its own schedules; Bevy's multi-threaded executor costs more than its systems save on this empty scene, as it did on the old PC.

Raw logs are in `target/cpu-floor-ladder.log` and `target/cpu-floor-followups.log` (not committed).

## Which executor owns the CPU floor? (October 17, 2026)

The i7 result above left one question: `--single-threaded` helps the empty frame, but it changes both apps. The example harness now has `--single-threaded-main` and `--single-threaded-render` to attribute it. Five interleaved rounds on the same i7 machine, empty scene:

| Executor | p50 | Paired change |
| --- | --- | --- |
| Default multi-threaded | 1.351 ms | — |
| Main app single-threaded | 1.349 ms | −0.1% |
| **Render app single-threaded** | **1.029 ms** | **−23.8%** |
| Both single-threaded | 0.983 ms | −27.3% |

**The render app owns it.** Single-threading the main app changes nothing; single-threading the render app recovers about 87% of the total saving. Pipelined rendering stays on, so this is the render app's multi-threaded schedule executor coordinating small systems, not the two apps' threading. The High-tier control moved only −0.8% (render only) and −1.4% (both), because that frame is GPU-bound on the T1200. Full method, caveats and reproduction are in [`docs/bevy-baseline-executor.md`](bevy-baseline-executor.md), with a draft upstream issue in [`docs/bevy-issue-baseline-executor.md`](bevy-issue-baseline-executor.md).

## Levers not yet tried

In expected order of effect. The main pass is the cost (see "After G6"): rasterizing and shading thin blades at 4× MSAA, with the shadow lookup about a fifth of it. The no-look-change levers are exhausted on the T1200; what is left trades the look or is a game setting.

1. **MSAA**, a game setting: 27–32% of grass cost, with more shimmer on thin blades.
2. **Shadows off in DEMO ARPG GAME.** See experiment 2, decision D5: grass in the shadow map is 22% of grass cost at DEMO ARPG GAME's camera.
3. **Fewer near blades.** LOD 0 is 100% density to 18 m. Halving from 10 m would apply experiment 7's gain to the most expensive blades, at a visible cost close to the camera.
4. **DEMO ARPG GAME style.** See experiment 8, decision D3.
5. **The game's build and executor** on CPU-bound PCs: fat LTO and single-threaded schedules (above), and untried, PGO and a faster allocator such as mimalloc.
6. **G3, Bevy's 4-tap shadow filter for grass**: −12 to −18% measured on the 4070 Ti, waiting on the developer's decision about the look.
