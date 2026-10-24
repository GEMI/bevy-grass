# Ideas from other grass write-ups

Status: a backlog, not a plan (October 2026). Every technique found in three
articles about real-time grass, including the ones bevy_grass already does
as well or better, so each can be retried when the reason it was skipped
changes. Before retrying a performance idea, check
[Performance notes](../performance.md): several were measured there.

Sources:

1. [Procedural grass rendering with mesh shaders](https://gpuopen.com/learn/mesh_shaders/mesh_shaders-procedural_grass_rendering/), AMD GPUOpen.
2. [Stylized grass shader in Unity: how it works](https://yelzkizi.org/stylized-grass-shader-in-unity-how-it-works/), Yelzkizi. A general overview with no code or numbers.
3. A write-up of the grass in the Three.js game Revo (TSL compute shaders),
   read from pasted text; the URL was not kept.

## Pipeline and geometry

- **Mesh shaders, one thread group per patch** [1]. Blades are generated in
  the stage that feeds the rasteriser, with no instance buffer. Declined
  ([roadmap](../roadmap.md#non-goals)): WebGPU has no mesh-shader stage and
  our cost is pixels; retry if Bevy exposes mesh shaders and compute plus
  instance traffic grow into a real share of the frame.
- **A fixed 8-vertex, 3-segment blade** [1]. It fits the 256-vertex mesh
  shader limit at 32 blades per patch; we use 15, 7 and 3 vertices. Retry
  only if vertex work ever becomes the bottleneck.
- **Alpha cards or shells instead of blades** [2]. A few textured quads fake
  density with almost no geometry, at the price of overdraw and lost early
  depth rejection. Retry only for far grass on a tier where vertex count, not
  pixels, is the limit.
- **Camera-facing sprites instead of two-sided strips** [3]. One face always
  points at the camera, but blades swim as the camera orbits and shadows stop
  matching. Our strip is already drawn once per pixel (`cull_mode: None`) and
  turns toward the camera only when edge-on; retry as a far detail level if
  blade lighting ever goes there.
- **Instancing with CPU-placed instances** [2]. One draw call for many copies
  of a grass mesh. We place instances on the GPU each frame; nothing to retry.
- **One big patch, no tiles** [3]. One draw call and one set of buffers, but
  every blade is processed every frame. Our 16 m chunks cost no CPU time;
  nothing to retry unless chunk culling itself shows up in a profile.
- **Hide culled blades by moving their vertices out of the frustum** [3].
  The vertex shader still runs, but no triangle is rasterised. We compact
  with an atomic append and indirect draws instead; retry, collapsing the
  vertices to one point rather than to infinity, for any pass that cannot
  use indirect draws.
- **Generation every other frame** [3]. Halves compute work, but motion runs
  at half rate and motion vectors break. Compute is 3–5% of grass time;
  retry only if per-blade compute grows a lot.
- **Bit packing per-blade values** [3]. Quantised flags, angles and factors
  share one 32-bit lane. Our instances are already 16 bytes; retry if
  instance bandwidth ever shows, with WGSL's `pack4x8unorm`,
  `pack2x16float` and `bitcast` rather than float arithmetic.

## Overdraw and pixel cost

- **Never `discard` to hide grass** [3]. A discarded fragment has already
  been shaded, and a pipeline that may discard defers depth writes. Grass
  never discards; keep it that way.
- **Avoid transparency and alpha blending** [2]. Layers of transparent
  grass are each shaded, with no early depth rejection. Grass is opaque; keep
  it that way.
- **Deferred grass to shade lighting once per pixel** (from reading [3]).
  Hidden blades would cost only a G-buffer write, and lighting is about half
  the main pass. [Measured](../performance.md#deferred-against-forward-october-23-2026)
  against forward with MSAA off: −12% looking across a field, +39% looking
  down on one; dropped as not worth it. Retry if Bevy's G-buffer gets
  slimmer or a top-down game goes deferred; the bench's flags for it were
  removed, so they need re-adding.
- **Depth prepass, front-to-back sorting, grass before the ground** (the
  usual cures for the opaque overdraw [3] is about). Measured and
  rejected (P1, P2, 6b in [Performance notes](../performance.md)); retry the
  prepass if blade shading gets much heavier, since its gain grows with
  shading cost and its loss with geometry.
- **Move work from the fragment to the vertex shader** [2]. Done for the
  look; the per-vertex sun shadow (P5) passed the bar and was rejected as
  looking cheap. Retry P5 if a stylized tier wants speed over shadow
  quality.
- **Simpler shading for distant grass** [2]. Drop or cheapen terms such as
  the 9-tap shadow filter, sheen or translucency beyond some distance. Not
  tried by distance; a 4-tap filter everywhere was measured (G3, and L3 in
  [Performance notes](../performance.md#three-look-trading-levers-on-the-t1200-october-23-2026))
  and turned down (L3) or still waiting (G3). Retry by distance, where a
  cheaper filter would show least.
- **Unlit grass** [3]. A flat colour took our main pass from 3.7 to 1.7 ms.
  Retry as an opt-in shading mode if a stylized game wants that trade.

## Density and level of detail

- **Blade count per patch falls with distance, last blade fades by width**
  [1]. `frac(count)` scales the last blade's width so blades never pop. We
  thin per blade in rank order; retry the width fade if thinning ever pops
  visibly.
- **Survivors widen to keep coverage** [1]. Done (`mid_range_density`,
  −16.5%).
- **Stochastic thinning by distance** [3]. A hash of the instance keeps all
  blades inside R0 and a share `pMin` beyond R1. Done, as rank-order
  thinning with widening.
- **Distance fade-out** [2]. Done by removing blades (`fade_start`), not
  with alpha.
- **Density by slope** [2]. Grass thins on steep ground with no mask
  painting. Not built; our heightmaps make it possible.
- **Bake distant grass into the terrain texture** [2]. `FarGrassMaterial`
  does the same job procedurally, with gusts; retry a bake only for a tier
  that cannot afford the far material.
- **Billboards for sparse far grass** [1, future work]. Impostor cards are a
  non-goal ([roadmap](../roadmap.md#non-goals)); retry if the gap between
  the last blade and the far ground ever shows.
- **Biome blending** [2]. Soft transitions between grass types across one
  field. A non-goal (no splatmaps); overlapping fields with masks does it.

## Blade shape and wind

- **Quadratic Bézier blade** [1]. Done (`blade_curve`).
- **Keep the blade's length when bent** [1, after Jahrmann and Wimmer 2017].
  Done, with the correction applied twice to stay within 1%.
- **Width weights 1.0, 0.7, 0.3 on the control points** [1]. A blunt tip
  that reads as a strap; we taper to a point. Retry only for a style that
  wants blunt blades.
- **Wind from two sine waves with Perlin-shifted time** [1]. No texture
  lookups, but periodic and without travelling gusts. Retry if the gust
  texture lookup ever shows in a profile.
- **Wind as world-space displacement, not rotation** [3]. Done, as a bend
  that keeps length, so blades never stretch.
- **Layered wind frequencies for gusts** [2]. Done: gusts, sway and
  flutter.
- **A downward force on the tip for seasons** [1, future work]. Snow or rain
  weighing the blades down. Droop exists per style; retry as a weather input
  that changes over time.

## Shading

- **Blend the normal partly toward world up** [1]. Lights the field like
  the canopy it reads as from afar, brighter at noon and steadier in the
  distance, but darker at sunset and with less translucency. A look change;
  retry ramped by distance, not as a flat 25%.
- **Root darkening as fake self-shadow** [1, 3]. Done (`blade_occlusion`).
- **Keep root occlusion near the base and fade it with distance** [3]. Dark
  roots on thin far blades speckle. Ours covers the bottom 60% and does not
  fade; retry if far grass speckles.
- **Noise-driven brightness patches** [1, 2]. Done, as Voronoi clump
  colour and colour patches on noise turned 34°.
- **Two-sided lighting by flipping the back face's normal** [1]. Done.
- **Gradient or toon-ramp lighting** [2]. Done (`toon_bands`).
- **Fake subsurface scattering** [2]. Done, as translucency that reuses the
  same light and shadow lookup.
- **Rim lighting** [2]. Brightens blade silhouettes, popular in stylized
  games. Not built; retry as an optional style term, compiled in only when
  used.
- **Colour by slope** [2]. Drier or sparser-looking grass on steep ground.
  Not built.
- **Match grass colour to the terrain palette** [2]. Done (ground tint,
  `FarGrassMaterial`).

## Scenes and interaction

- **Bending around the player** [2, 3]. Done, with trails and shaped
  physics bodies.
- **Other vegetation: flowers, shrubs** [1, future work]. Flowers, seed
  heads, ferns and broad leaves are species; shrubs are not. Retry if a game
  asks for low bushes inside the grass.
