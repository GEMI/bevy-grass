# Changelog

All notable changes to bevy_grass, newest first. Until 1.0 any release may
break the API; each breaking change below says what to write instead.

## Unreleased

**Breaking, and what to write instead**

`GrassStyleFileError::Parse` no longer holds `ron`'s `SpannedError`, so a
`ron` upgrade is no longer a breaking change of this crate. It is now
`Parse { line, column, message }`: where the parser stopped, counted from 1,
and why. Match `Parse { message, .. }` instead of `Parse(error)`, and read
`message` or the error's `Display` (unchanged apart from showing only the
start of the span) instead of the `ron` error.

**Added**

- The examples build for the browser (`wasm32-unknown-unknown`, WebGPU):
  `python tools/web_example.py` builds and serves them, and CI checks the
  wasm build. Browser support itself is not finished; see
  [docs/testing.md](docs/testing.md).
- `GrassSupport`, a resource: whether this GPU can draw grass, and if not,
  why (no compute shaders, as in WebGL2; no indirect draws; no storage
  buffers in the vertex shader; too few storage buffers per shader stage; too
  small a storage binding). Decided once when the renderer starts.
- The examples' `--no-grass` flag: the same scene without grass.
- The examples' `--max-storage-buffers N` flag: ask the GPU for at most N
  storage buffers per shader stage, as a weaker device or browser offers.

**Fixed**

- On a device that cannot draw grass, the grass pipelines were still created,
  and the wgpu validation error stopped Bevy's renderer. Now grass logs one
  warning saying why and draws nothing, and the rest of the scene renders.
  Grass binds 10 storage buffers in its busiest shader stage, so a device
  that allows fewer gets the warning.

**Changed**

- Grass fits in 10 storage buffers per shader stage, down from 14, so it
  draws on devices that offer 10 to 13: most browsers on macOS, Samsung
  Internet and some of Firefox. No picture changes. Each family of compute
  passes has its own layout, the masks and heightmaps share one buffer, and
  the draws bind storage buffers in the vertex stage only (see
  [docs/rendering.md](docs/rendering.md)).

## 0.20.3 (October 21, 2026)

Docs only: no API or behaviour changed. The tagline on crates.io and the
README's opening now describe the crate as grass and foliage for 3D games,
instead of "GPU-driven, wind-animated tall grass", which undersold what it
grows and led with how it is built.

## 0.20.2 (October 21, 2026)

Runtime editing, grass on mesh terrain, and fields that turn.

**Added**

- Runtime editing ([docs/editing.md](docs/editing.md)). `GrassEdit` messages
  clear, paint, mow, change the height of, and restore grass in a
  `GrassBrushShape` (circle, stroke or turned rectangle), with a soft edge
  and a strength: at 1 an edit sets the grass and repeating it changes
  nothing, below 1 edits add up. Edits change the field's density mask,
  which a field without one gets on its first edit (0.25 m texels), and
  only the rows they touch are uploaded; the shaders are unchanged, so a
  game that edits nothing pays nothing.
- `GrassBrush`: a component that edits wherever its entity goes, at once and
  swept along its path (a mower) or at a rate while it stays (worn paths, a
  fire). `GrassBrushFootprint` makes it a rectangle under the entity, turned
  and scaled with it and swept between frames: a mower's deck, a vehicle's
  tracks, a bulldozer's blade.
- `GrassRegrowth`: a field's edits grow back toward its own mask, at a rate
  for density and one for height.
- `GrassSampler`: reads density, blades per square metre and blade height at
  any point on the CPU, with every edit and the style's bare patches, for
  stealth, footsteps and AI; `mask_image` returns a field's mask with its
  edits, to save.
- `GrassSystems::Edit`, between `DetectChanges` and `Rebuild`, and
  `GrassStats::edits` (`EditStats`).
- The `editing` example.

- `GrassGround::Mesh { mesh, cell_size }` and `GrassGround::mesh(handle)`:
  grass on the top of a mesh, where it is drawn with the field's transform.
  The mesh is baked once on the CPU into a heightmap (0.5 m cells by
  default, at most 4096 per side) and drawn like one, with no cost per
  frame. Blades grow on the highest surface and nowhere the mesh does not
  reach. It is baked again when the mesh, the field's size or the cell size
  changes, not when the field moves. A mesh that cannot be baked (not
  triangles, or no CPU copy) draws nothing and logs one warning.
- The `mesh_terrain` example: a meadow on rolling hills built as a mesh.

**Changed**

- A field turns with its transform's rotation about Y, mask and heightmap
  included. It used to be drawn unturned with a warning; now only a tilt,
  which grass ignores because it grows upright, logs one.
- The debug gizmos draw fields and chunks turned, and a heightmap field's
  upper rectangle at its highest ground rather than at `height_scale`.

## 0.20.1 (October 20, 2026)

The first tagged release. Built as 0.2.0, it is numbered after the Bevy
version it targets: bevy_grass 0.20 is for Bevy 0.20. It holds everything
since 0.1.0, below: Avian and Rapier support, the API and settings pass, and
the polish pass.

### Avian support, footprints, and a physics bridge not tied to one engine (October 20, 2026)

`GrassInteractorFromCollider` now works with Avian as well as Rapier, and a
large body flattens grass as one shape: the grass under a moving vehicle lies
the way it drives, and only the grass along its sides is pushed out.

**Added**

- `GrassInteractorFootprint`: a box (in the entity's local space, with a
  rounding for balls and capsules) that an interactor flattens instead of a
  disc. Under a moving box the grass lies down the way it moves and only its
  sides push sideways, so the trail behind keeps lying the way it went; a box
  at rest pushes out from its nearest edge. The box is laid on the ground by
  its shadow every frame, so turning and tumbling bodies follow, and a log
  that falls over flattens a strip along its length. It is one stamp on the
  GPU, however large (clipped to 16 m per side).
- The `avian` cargo feature: `GrassInteractorFromCollider` for `avian3d` 0.8
  colliders. It brings `avian3d` in with `default-features = false` (`3d`
  and `parry-f32` only), so it does not change how Bevy is compiled. `avian`
  and `rapier` can both be on.
- `GrassColliderBounds`, the engine-neutral input: a collider's box along
  the entity's axes, in metres with the scale applied, how rounded it is, and
  whether it is active. The Avian and Rapier backends write it; a game writes
  it for a collider type of its own, or with no physics engine at all.
- `GrassColliderSystems::{Bounds, Fit}`, in `Update`, to order such a system
  before the fit.
- A warning, once per entity, when a `GrassInteractorFromCollider` gets no
  collider size within three frames. It names the likely causes: no physics
  feature, the adapter on an entity without the `Collider`, or a
  `bevy_rapier3d`/`avian3d` version other than the one bevy_grass is built
  against (Cargo then builds two copies and the bridge cannot see the game's
  colliders; before, the interactor silently did nothing).
- The debug gizmos draw each footprint's outline as it lies on the ground.
- `examples/avian.rs` and `examples/avian_minimal.rs`, the Rapier scenes with
  Avian.

**Changed**

- `GrassInteractorFromCollider` fills in a `GrassInteractorFootprint` instead
  of a `GrassInteractorSpread`, and requires it. A spread drew one radial fan
  per disc, which made a large body look like several small ones side by
  side, with rows of grass bent at odd angles. `radius_scale` is now `scale`:
  it scales the whole footprint. A character capsule is a disc of its own
  radius, fully flat to its edge, where it used to be a disc of its bounding
  square's half diagonal fading from 60 %.
- `GrassInteractorFromCollider` is always compiled, and is in the prelude
  without a feature. The engine features only add the backend that reads
  their `Collider`; the fit is in the core, shared by both.
- The interactor buffer holds three `vec4`s per stamp (48 bytes) instead of
  one; a disc is stamped exactly as before.
- The `rapier` and `avian` examples drive the vehicle lengthwise, add a crate
  scaled to 2.5 m driving a circle, and throw in a log every six seconds that
  tumbles and lies down across the grass.

**Fixed**

- A scaled collider was covered at its scale squared: the collider's box
  already had the scale applied, and extraction applied it again. A 6 m box
  at scale 2 put discs 3.6 m past its ends. The footprint now matches the
  simulated collider, for uniform and non-uniform scale.
- A body lying on its side got one small disc: the cover only looked at the
  collider's local X and Z. It is now covered where its shadow lies.
- A child collider whose parent rigid body has `RigidBodyDisabled` kept
  flattening grass. Rapier's backend checks the nearest body above the
  collider, Avian's the body in `ColliderOf`.

### A new walk film for the README (October 20, 2026)

The film at the top of the README now walks through three presets, three
seconds each: `WILDFLOWER_MEADOW`, `RYE_AND_CORNFLOWERS` and `REED_MARSH`,
at twice their density, with Ultra shadows in a 4096 map. The meadow and
the rye wave in a stronger wind than the breeze; the reeds stand in almost
still air, for the contrast.

**Added**

- The `walk` example, which films it: a field per preset, each with its own
  walker and following camera, and cuts between them. `--styles` and
  `--shot` pick the presets and how long each is shown.
- `tools/record_walk.py`, which records `walk` and encodes
  `docs/images/walk.webp`.

**Fixed**

- Blade shadows under the examples' water (`REED_MARSH`, `KELP_BED`) blinked
  as the camera moved. The water box's floor lay on the ground, and the two
  z-fought: in some frames the floor hid the shadows on the ground under a
  second layer of water. The box now reaches 5 cm below the ground.

### Sixteen distinct presets, and fewer near-duplicates (October 19, 2026)

Fourteen of the 26 presets were the same blade in another colour or height.
Six of them are gone, and sixteen new ones each push a setting the others
leave alone: tufts and droop to their limits, stiffness at 0 or 1, twist of
two turns, fold into facets, toon shading on mixed fields, heads as the main
feature, and colours outside green and brown.

**Added**

- `GrassStyle::BIOMES`: `COTTON_BOG`, `REED_MARSH`, `DUNE_MARRAM`,
  `RYE_AND_CORNFLOWERS`, `PAMPAS`, `SPINIFEX`, `ELEPHANT_GRASS`,
  `FROSTED_MEADOW`, `BURNT_REGROWTH` and `LAVENDER`.
- `GrassStyle::FANTASY`: `CRYSTAL_SPIRES`, `CORKSCREW_ALIEN`, `KELP_BED` and
  `BLIGHTED`.
- `GrassStyle::TOON`: `CEL_FIELD` and `COZY_PASTEL`.
- `GrassBlade::{COTTON_GRASS, CATTAIL, CORNFLOWER, POPPY}`, to mix into any
  style as species.
- All of them as `.grass.ron` files and embedded assets, like every preset.
- `examples/gallery.rs`: the sixteen new presets on 18 m plots of their own
  ground, to fly around, with a walker trampling each plot and a wind per
  plot (a slow current over the kelp).
- `showcase` and `styles` show every preset on its own ground (sand, snow,
  peat, a pond, the seabed, rows for lavender), from
  `examples/common/setting.rs`; `showcase` has a fifth page.

**Breaking, and what to write instead**

`GrassStyle::SUMMER` and `GrassStyle::AUTUMN` are now one group,
`GrassStyle::SEASONS` (`SUMMER_TALL`, `AUTUMN_TALL`). The variants that only
changed a colour or a height are removed; a file that names one as its
`base` now fails with `UnknownBase`. Each is its main look with a few
settings, in a file:

| Removed | Now |
| --- | --- |
| `SUMMER_LUSH` | `(base: "SUMMER_TALL", height: (0.9, 1.5), lean: 0.15, root_color: (0.03, 0.08, 0.02), tip_color: (0.22, 0.44, 0.10), root_occlusion: 0.7, density: 50.0)` |
| `SUMMER_MEADOW` | `(base: "SUMMER_TALL", height: (0.5, 1.0), lean: 0.18, stiffness: 0.3, root_color: (0.07, 0.13, 0.03), tip_color: (0.48, 0.62, 0.18))` |
| `SUMMER_SEEDING` | `(base: "SUMMER_TALL", height: (0.8, 1.4), lean: 0.28, stiffness: 0.18, root_color: (0.06, 0.12, 0.03), tip_color: (0.66, 0.64, 0.38))` |
| `AUTUMN_RUST` | `(base: "AUTUMN_TALL", root_color: (0.16, 0.07, 0.03), tip_color: (0.54, 0.29, 0.13))` |
| `AUTUMN_STRAW` | `(base: "AUTUMN_TALL", height: (0.6, 1.1), stiffness: 0.45, root_color: (0.24, 0.19, 0.10), tip_color: (0.72, 0.62, 0.38), translucency: 0.5)` |
| `AUTUMN_FADED` | `(base: "AUTUMN_TALL", height: (0.5, 1.1), lean: 0.4, stiffness: 0.15, root_color: (0.11, 0.10, 0.07), tip_color: (0.44, 0.39, 0.30), translucency: 0.3)` |

For a colour change on one field only, `GrassFieldOverrides::tint` needs no
style at all.

**Internal**

- The golden scene gives each camera room for 24,000 blades instead of
  60,000 (the densest patch has 16,000). With 60,000, WARP crashed once there
  were 34 cameras. Every golden was re-blessed: removing six presets moved
  the later patches along the row, onto other wind and noise. Rendered at
  their old places, all twenty kept presets matched their old goldens
  exactly, so none of them changed.

### Optional Rapier interaction bridge (October 19, 2026)

**Added**

- The `rapier` cargo feature and `GrassInteractorFromCollider`: put it next to
  a `bevy_rapier3d` `Collider` and the radius, strength and height of the
  grass interactor follow the collider. A capsule gets one disc; a vehicle or
  wall gets a spread of discs. Disabled colliders/bodies and sensors flatten
  nothing.
- `GrassInteractorSpread`, a core component listing local disc centres for a
  large object. `GrassInteractorSpread::covering_aabb` covers a local AABB
  with at most 32 discs, and works without a physics engine.
- `examples/rapier.rs` (a kinematic capsule and a large vehicle driving
  through a meadow) and `examples/rapier_minimal.rs` (the shortest
  integration example).

The adapter reads the collider's local AABB, not a physics transform: Rapier
already writes `Transform`/`GlobalTransform`, and grass already reads
`GlobalTransform`, so the bridge only decides the footprint. It runs in
`Update` and is independent of the physics schedule. `bevy_rapier3d` is
brought in with `default-features = false` and only `dim3`, so enabling the
feature does not change how Bevy is compiled.

Avian was not included at first: `avian3d` 0.7 depended on Bevy 0.19. It
arrived with `avian3d` 0.8, above.

### Missing-style feedback and a constructor (October 18, 2026)

**Added**

- `GrassField::new(size, style, &mut styles)` adds a style (usually a preset)
  and returns the field in one call. `GrassField::from_preset` is the same
  constructor under a preset-flavoured name.
- A field with the empty default style handle now logs one warning after a few
  frames, naming the entity and how to give it a style, instead of waiting
  forever with no visible feedback.

**Changed**

- The README quick start and `examples/minimal.rs` use `GrassField::new`.

### Wind as a plugin (October 18, 2026)

The wind is now a self-contained model behind one per-frame resource, so it
can be replaced without touching the grass renderer.

**Breaking, and what to write instead**

| Before | Now |
| --- | --- |
| `GrassPlugin { settings }` | `GrassPlugin { settings, ..default() }`; the new `wind: bool` is `true` |

**Added**

- `GrassWindFrame`, the exact per-frame values the shaders read. The built-in
  wind writes it; another wind model writes it instead.
- `GrassWindPlugin`, the built-in wind model (direction, strength, scrolling
  gusts, sway and flutter), now separate from `GrassPlugin`. Leave it out with
  `GrassPlugin { wind: false, ..default() }`.
- `GrassWindSystems::Sample`, where a custom wind source writes its frame, so
  the blades and `FarGrassMaterial` read the same wind.
- `examples/custom_wind.rs`, a complete scene driven by a `Weather` resource
  instead of the built-in model.
- `src/wind/`, containing the settings, the gust clock and the tests; the
  shader side lives in `shaders/wind.wesl`.

**Internal**

- The wind has its own GPU uniform and bindings instead of sharing the frame
  uniform, and `FarGrassUniform` embeds the same `Wind` struct. The built-in
  model can therefore be extracted or replaced without changing the blades'
  layout.

### Forest-floor presets (October 18, 2026)

**Added**

- Four forest-floor `GrassStyle` presets, also shipped as `.grass.ron` files:
  `FOREST_MOSS` (a short, wide moss carpet), `FOREST_FERNS` (arching fern
  clumps), `FOREST_FLOOR` (moss with ferns, fallen leaves, clover and broad
  leaves mixed in) and `FOREST_BRACKEN` (rusty autumn ferns over leaf litter).
- `GrassStyle::FOREST`, the group holding the four.
- `GrassBlade::{MOSS, FERN, BRACKEN, LEAF_LITTER}`, for building a forest floor
  of your own or mixing one of its plants into another style.
- `examples/forest_floor.rs`, the four presets side by side.
- `showcase` and `styles` now show the forest-floor presets too.

**Internal**

- Every preset definition and the one preset list now live in `src/presets/`, one file per group.
  One entry generates the constant, the group array, `PRESETS` and the embedded
  asset, so adding a preset no longer means editing a second list in
  `style_file.rs`. Presets ship as `.grass.ron` files generated from the same
  constants by `BLESS_STYLES=1 cargo test`.

### The 240 Hz gate (October 17, 2026)

The measurements that make the 240 FPS target testable instead of a comment.

**Added**

- `GrassQuality::Competitive` and `GrassSettings::COMPETITIVE`: a 240 Hz /
  competitive-game tier with 40 m of half-density grass and no grass shadow
  casting. It is the cheapest tier and the candidate the gate measures.
- `bench --target-hz N` and `--budget-ms MS`: fail with a non-zero exit code
  when frame p95 exceeds the budget.
- `bench --baseline-csv PATH`: paired grass-cost p50/p95 against a previous
  `--no-grass` run.
- `bench --machine NAME`, and shadow-pass time in the summary and CSV.
- `tools/bench_240.py`: runs High, Competitive and no-grass on a named
  reference machine, in both scenes, and writes JSONL, CSV, logs and a
  Markdown table under `target/bench-240/`.
- `.github/workflows/perf-240.yml`: a manual gate on the self-hosted reference
  runner labels `t1200`, `rtx3060` and `rtx4070`.
- `docs/performance.md` now documents the 240 Hz budget, the three
  configurations, the reference machines and where to record runs.

### The API and settings pass (October 16, 2026)

Built from a review of the crate's API and developer experience, items P1-1 to P1-9.

**Breaking, and what to write instead**

| Before | Now |
| --- | --- |
| `GrassPlugin { quality, shadows, instance_capacity, max_interactors }` | `GrassPlugin { settings: GrassSettings }`; `GrassSettings::{LOW, MEDIUM, HIGH, ULTRA}`, or `GrassSettings::from(GrassQuality::High)` |
| `Res<GrassShadows>` | `GrassSettings::shadows` |
| `GrassStyle { height, tip_color, density, tufts, .. }` | `GrassStyle { blade: GrassBlade { height, tip_color, .. }, density, variation: GrassVariation { tufts, .. }, ground_tint: GroundTint { .. }, .. }` |
| `GrassSpecies { style: GrassStyle, .. }`, `GrassSpecies::new(GrassStyle::SEED_HEADS, 0.1)` | `GrassSpecies { blade: GrassBlade, .. }`, `GrassSpecies::new(GrassBlade::SEED_HEADS, 0.1)` |
| `GrassStyle::min_width_px` | `GrassSettings::min_width_px`, or `GrassCamera::min_width_px` on one camera. Old `.grass.ron` files that set it still load, with a warning |
| a species in a `.grass.ron` file with `density`, `tufts` and other field settings | an error that names them: move them to the style that lists the species |

**Added**

- `GrassSettings`, a resource with a preset per hardware tier; every setting
  changes at runtime.
- `GrassCamera`: per-camera overrides of `radius`, `density_scale`,
  `min_width_px` and `shadows`.
- Fields honour `Visibility` (their own and their parent's), `RenderLayers`
  0 to 31 and their transform's scale. A rotated field logs a warning.
- `GrassSystems::{DetectChanges, Rebuild}`, to order your systems against.
- Full reflection: components, resources and the style asset, so fields work
  in scene files and inspectors.
- `GrassSun`, which marks the light grass follows, and warnings about
  cascades too deep for blade shadows.
- `GrassStats`, seven Bevy diagnostics, and `GrassDebugPlugin` (cargo feature
  `debug`).
- Incremental tables: a style edit, even every frame, rewrites only the
  entries that use it.

### The polish pass (October 17, 2026)

Built from the review's P2 items.

**Breaking, and what to write instead**

| Before | Now |
| --- | --- |
| `app.add_plugins(GrassPlugin::default())` | `app.add_plugins(GrassPlugins)`. `GrassPlugin` alone no longer registers the far-grass material or the `.grass.ron` loader (`FarGrassPlugin`, `GrassStyleFilePlugin`) |
| `GrassPlugin { settings }` | `GrassPlugins.set(GrassPlugin { settings })` |
| `Wind`, `Wind::STORM` | `GrassWind`, `GrassWind::STORM` |
| `bevy_grass::api::GrassField` | `bevy_grass::GrassField`: there is no `api` module; every public type is at the crate root and in the prelude |
| `GrassField::density` | `GrassField::density_map` (the style's `density` is blades per m²; the field's was a mask) |
| `GrassBlade::head` | `GrassBlade::head_length` |
| `GrassVariation::tuft_size` | `GrassVariation::tuft_spacing` |
| `GrassVariation::dead_blades` | `GrassVariation::dead_share` |
| `GrassVariation::bare_patches` | `GrassVariation::bare_share` |
| `FarGrass::new(&style, quality)`, `FarGrass::fade: Vec2` | `FarGrass::new(&style)`; the fade follows `GrassSettings::radius`. `FarGrass::fade` is an `Option<Vec2>`, and `.with_fade(Vec2)` sets one |
| `grass_far_color(.., far, time, gusts, sampler)` in WESL | `grass_far_color(.., far, gusts, sampler)`: the gusts' scroll is `far.strength.yz`, which `FarGrassPlugin` keeps equal to the blades' |
| `GrassInteractor { radius, strength }` | the same, plus `max_height: Option<f32>`; add `..default()` to a literal |

The `.grass.ron` files keep loading under the old names: `head`,
`tuft_size`, `dead_blades` and `bare_patches` are accepted as aliases, and
files are written with the new ones.

**Added**

- `GrassPlugins`, a plugin group, with `FarGrassPlugin` and
  `GrassStyleFilePlugin` to leave out what a game does not use.
- The presets as assets: `assets.load("embedded://bevy_grass/styles/meadow.grass.ron")`.
- Hex colours in `.grass.ron` files: `tip_color: "#5c8f24"`. Tuples still work.
- `GrassFieldOverrides` (`tint`, `density_scale`, `height_scale`) on a field,
  to change one field without a style asset of its own.
- `GrassInteractor::max_height`, so something flying over the grass does not
  flatten it, a warning for a radius over the 2 m limit, and the interactor
  limits (2 m radius, the count, the 64 m window) in its documentation.
- `GrassWind::toward(Dir2)`, and `PartialEq` for `GrassField` and
  `GrassGround`.
- `examples/minimal.rs`, which is the README's quick start (a test keeps the
  two equal).
- This changelog.

**Fixed**

- The wind jumped once an hour, when Bevy's clock wraps: sway and flutter
  were sines of a wrapped time, and the gusts scrolled by time times speed.
  Sway and flutter frequencies are now whole cycles per period, and the gust
  scroll is accumulated in `f64` on the CPU, so neither jumps, and a change
  of the wind's speed or direction no longer jerks the gusts either.

**Changed**

- Bevy is `0.20`, not `=0.20.0`: any 0.20.x works, and a game that pins `=0.20.0` itself still gets exactly that. Only 0.20.0 has been tested, since no later patch exists yet.

**Internal**

- The `.grass.ron` format is declared once, in one macro, from the settings it
  holds; a new setting is one line there, and a test fails if one is missing.

### The tunables pass (October 17, 2026)

Numbers that were fixed in the code and shaders, and that another game might
want different, are settings now. **Every default is the number it replaces**,
so nothing changes unless you set one: the rendered image is unchanged to
within one 8-bit level (llvmpipe rounds the new uniform reads differently, so
its goldens were re-blessed), and a GPU test checks the shaders against the CPU
mirror at the defaults and at other values.

**Added to `GrassSettings`** (the first group are its distances and densities;
see the README's table for each range and default):

- The blade mesh: `blade_rows` (rows per level of detail, `[7, 3, 1]`).
- Distance and thinning: `lod1_start`, `lod2_start` and `fade_start` (shares
  of `radius`), `lod_blend` (width of the dithered band between levels, in
  metres), `lod2_density`, `fade_density` and `mid_range_density` (how many
  blades are kept at each distance), `max_widening` (how far thinned blades
  widen), `turn_to_camera` (how far a blade seen edge-on turns toward the
  camera).
- Limits of a view: `orthographic_extent` (the area around an orthographic
  camera in which blades are stored: it was a fixed 256 m, and a blade
  further away was misplaced), `max_chunk_slices` (the work list of blade
  generation, a fixed 8,192) and `readback_interval` (frames between the
  GPU readbacks behind `GrassStats` and the overflow warnings; 0 turns them
  off).
- Trampling: `trample_recovery` (seconds, was 1.5), `trample_cell_size`
  (metres, was 0.25: the window is 256 cells across, so it was 64 m) and
  `max_interactor_radius` (metres, was 2).

**Added to `GrassWind`**: `sway`, `sway_speed`, `flutter` and `gust_contrast`,
multipliers of the wind's movement that are 1 in every preset.

**Breaking**

| Before | Now |
| --- | --- |
| `GrassSettings { radius, density_scale, min_width_px, shadows, orthographic_lod, instance_capacity, max_interactors }` written out in full | add `..GrassSettings::HIGH` (or any tier): the struct has more settings, all with defaults. Code that starts from a tier is unaffected |
| `GrassWind { direction, strength, gust_scale, gust_speed }` written out in full | add `..GrassWind::BREEZE` (or any preset) |

**Internal**

- The CPU mirror of the distance table, `DistanceLod`, is built from the
  settings; a test runs the GPU against it with other distances.
- `GrassDebugPlugin` draws the trample window and the interactor discs the
  settings give, and needs `GrassSettings` (which `GrassPlugin` inserts).
- Lavapipe golden folders are named after the Mesa and LLVM builds, and CI
  pins the Linux golden runner to Ubuntu 24.04. A `golden-bless` job writes
  and uploads them on that same runner.

## 0.1.0

Never released; the version the first milestones were built as. The first
version: GPU-driven grass with levels of detail, wind, trampling, shadows, the
prepass and deferred paths, flowers and other species, 22 presets, and
`FarGrassMaterial`.
