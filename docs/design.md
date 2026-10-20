# How it works

What makes bevy_grass fast, and what makes it look like grass. Each technique
below was measured on its own; ideas that did not pay for themselves were
dropped, and [Performance notes](performance.md) records both kinds with
numbers.

## How it is fast

1. **Blades are computed, never stored.** Each field is cut into 16 m chunks
   of 32 bytes each. A blade's position, height and colour come from a hash
   of its chunk and index, so the same blade appears every frame with no
   stored state. Memory grows with what is visible, not with the world.
2. **The GPU decides what to draw.** Per camera, a compute pass culls chunks
   against the view and the grass radius. A second pass, one workgroup per
   visible chunk, generates the blades. Each blade passes a density test, a
   frustum test (its shadow included) and a level-of-detail choice, then is
   appended as a 16-byte instance. The CPU only records commands, and uploads
   only what changed: a style edit rewrites that style's table entries, a
   colour-only edit uploads one small buffer, and masks and heightmaps are read
   from their images once.
3. **Three indirect draws per camera**, whatever the number of fields or
   chunks. There is no vertex buffer: the vertex shader builds each blade from
   its instance and a shared index buffer.
4. **Three levels of detail**: 7, 3 and 1 rows by default (15, 7 and 3
   vertices; `blade_rows`). Each blade picks its level by its own dither value
   inside a 4 m band (`lod_blend`), so the change never shows as a ring.
5. **Fewer, wider blades with distance.** Density thins with distance, in a
   fixed order per blade, and the survivors widen to keep the same coverage.
   GPUs shade pixels in 2 × 2 quads, so one wider blade is cheaper than two
   thin ones covering the same area. Halving mid-range density (the default of
   `mid_range_density`) cut grass cost by 16.5% with no visible change.
6. **Coarser blades for top-down cameras.** Orthographic cameras draw every
   blade at the same size, so they use 3-row blades. That was 9% faster and
   indistinguishable at 1:1.
7. **Wind once per blade, not per vertex.** The compute pass samples the gust
   texture once per blade and stores the bend. It also stores last frame's
   bend, which makes exact motion vectors free.
8. **Opaque geometry, near to far.** No alpha testing, so early depth
   rejection works. Drawing the near detail level first is worth about 6.5%.
9. **Cheap shadows.** `Near` mode casts only full-detail blades into the
   first cascade: about 0.4 ms, where every blade in every cascade cost
   2.4 ms.
10. **Tables instead of searches.** Clumps and tufts are Voronoi cells, looked
    up in tables built at startup (one load per blade) instead of searched per
    blade. The search cost 15 times as much.
11. **One trample texture per camera.** Interactors are stamped into 256 ×
    256 cells of 25 cm (`trample_cell_size`) around what the camera sees,
    addressed with power-of-two masks so trails stay in world space without
    copying. Each blade reads one cell. The whole pass costs about 0.02 ms,
    the same for 1 or 512 interactors.
12. **You pay only for what you use.** Optional terms (sheen, ground tint,
    toon bands, blade shape, colour variation) are compiled into the shaders
    only when some field's style uses them. Tufts, bare patches and species
    are uniform branches that a field without them skips.
13. **Only the candidates a chunk can keep.** The cull hands generation just
    the prefix of candidate blades the chunk's density can use, so spare
    candidates cost nothing.

## How it looks good

1. **Curved, tapered blades.** Each blade is a quadratic Bézier curve, the
   technique Ghost of Tsushima popularized, tapering to a point. When the wind
   bends it, the curve is corrected to keep its length (within 1%), so blades
   bend and never stretch.
2. **Rounded normals.** Normals tilt across the blade's width, so a flat strip
   shades like a curved leaf. Back faces are lit as well as front faces.
3. **No paper-thin blades.** A blade turning edge-on to the camera rotates
   slightly toward it (`GrassSettings::turn_to_camera`). Blades never get
   narrower than `GrassSettings::min_width_px` (1 pixel by default) on screen,
   which removes most shimmer.
4. **Even, natural spacing.** Positions come from a blue-noise tile, with no
   bald spots or accidental clumps, and the field reads equally dense at every
   distance.
5. **No popping.** Every blade has a fixed rank, and lower density removes
   blades in rank order instead of reshuffling them. Detail levels change
   blade by blade inside a dithered band.
6. **Three layers of wind.** Rolling gusts from a noise texture that scrolls
   across the world. Each blade also sways at its own phase and at a
   frequency set by its stiffness, so neighbours never move in lockstep. Near
   tips get a small flutter. `GrassWind::sway`, `sway_speed`, `flutter` and
   `gust_contrast` turn each layer up or down.
7. **Bevy's own lighting.** Grass goes through Bevy's PBR functions, so it
   gets every light type, cascaded shadows, environment maps and fog, and
   matches everything else in the scene.
8. **Root-to-tip gradient and fake occlusion.** Roots are darker and duller
   than tips, as they are under a canopy of blades.
9. **Clump colour.** Brightness varies by clumps about a metre across, not
   blade by blade, and one clump in five is drier. That reads as natural
   patches instead of noise.
10. **Translucency.** A blade between you and the sun glows instead of turning
    black.
11. **Sheen.** Rough specular on the upper blade makes wind waves visible as
    moving light.
12. **Ground tint.** Roots can blend into the terrain colour, so the field
    sits in the ground instead of on it.
13. **Real variation.** Other plants mixed in exact shares, scattered or in
    patches. Seed heads and flowers turn to face you. Blades gather in tufts
    and lean out from them, bare patches have soft edges, and blades twist,
    droop and fold, each by its own amount. Colour drifts in patches metres
    across, on noise turned 34° so it never lines up with the world's axes,
    with dry tips and dead blades among the living.
14. **Trampling with trails.** Grass parts around anything with a
    `GrassInteractor` and recovers over about 1.5 s (`trample_recovery`), in
    world space, so trails stay put while the camera moves. Large bodies
    flatten as one shape: the grass under a moving vehicle lies the way it
    drives, and only its sides push out.
15. **No seam at the horizon.** `FarGrassMaterial` turns the ground the grass
    colour as the blades thin out, with the same gusts rolling across it.
16. **Works with post-processing.** Exact motion vectors for TAA, depth and
    normals for SSAO, and a deferred path.
17. **Toon option.** `STYLIZED`, `CEL_FIELD`, `COZY_PASTEL` and `toon_bands`
    give flat bands of light for non-photoreal games.
