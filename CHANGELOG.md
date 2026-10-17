# Changelog

All notable changes to bevy_grass, newest first. Until 1.0 any release may
break the API; each breaking change below says what to write instead.

## Unreleased

### The API and settings pass (October 16, 2026)

Built from the review in [`review.md`](review.md), items P1-1 to P1-9.

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

**Internal**

- The `.grass.ron` format is declared once, in one macro, from the settings it
  holds; a new setting is one line there, and a test fails if one is missing.

## 0.1.0

Not released. The first version: GPU-driven grass with levels of detail,
wind, trampling, shadows, the prepass and deferred paths, flowers and other
species, 22 presets, and `FarGrassMaterial`. See
[`bevy-grass-architecture-plan.md`](bevy-grass-architecture-plan.md) for what
each milestone added.
