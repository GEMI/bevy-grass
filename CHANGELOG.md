# Changelog

All notable changes to bevy_grass, newest first. Until 1.0 any release may
break the API; each breaking change below says what to write instead.

## 0.2.0 (not released yet)

The API and settings pass and the polish pass, below, as one version.

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

- Every preset definition and the one preset list now live in `src/presets.rs`.
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
