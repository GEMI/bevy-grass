# API overview

Everything a game needs is in `bevy_grass::prelude`. Every item also has
rustdoc (`cargo doc --open --features debug,rapier,avian`); this page is the
map. The style settings are in [Styles](styles.md#reference), the far-grass
material in [Fields, terrain and visibility](terrain.md#keep-the-grass-colour-past-the-last-blade).

## Plugins and settings

| Item | Kind | What it does |
| --- | --- | --- |
| `GrassPlugins` | Plugin group | `GrassPlugin`, `FarGrassPlugin` and `GrassStyleFilePlugin`. Add it after `DefaultPlugins`; `.set(GrassPlugin { settings, ..default() })` starts from other settings, `.disable::<FarGrassPlugin>()` leaves one out |
| `GrassPlugin` | Plugin | The grass itself (fields, styles, interaction, rendering). `settings: GrassSettings` (`High` by default), and `wind: bool` (the built-in `GrassWindPlugin`, on by default) |
| `GrassWindPlugin` | Plugin | The built-in wind model. Part of `GrassPlugin`; leave it out with `wind: false` and write `GrassWindFrame` yourself |
| `GrassWindFrame` | Resource | The wind the shaders read this frame: direction, strength, gust size and scroll, sway, flutter and gust contrast. The built-in model or your own source writes it in `GrassWindSystems::Sample` |
| `GrassSettings` | Resource | Reach, density, shadows, buffer sizes, levels of detail and trampling; every setting is in the [settings table](settings.md#tune-the-distances-the-blades-and-the-trampling). Presets `COMPETITIVE`, `LOW`, `MEDIUM`, `HIGH` (default), `ULTRA`. Change it at any time |
| `GrassQuality` | Enum | `Competitive`, `Low`, `Medium`, `High` (default), `Ultra`: a hardware tier for a graphics menu. `GrassSettings::from(GrassQuality::High)` |
| `GrassShadows` | Enum | `Off`, `Near` (full-detail blades into the first shadow cascade; default), `Full` (also mid-range blades into the second). `GrassSettings::shadows` |

## Components, resources and system sets

| Item | Kind | What it does |
| --- | --- | --- |
| `GrassField` | Component (requires `Transform` and `Visibility`; honours `RenderLayers`) | One rectangle of grass, turned with the transform's rotation about Y. `size: Vec2` (metres, X and Z, times the transform's scale), `style: Handle<GrassStyle>`, `ground: GrassGround`, `density_map: Option<Handle<Image>>` (mask: red density, green height multiplier). `GrassField::new(size, style, &mut styles)` adds the style for you |
| `GrassGround` | Enum | `Flat` (default; at the field's height), `Heightmap { image, height_scale }`, or `Mesh { mesh, cell_size }`: the top of a mesh, drawn with the field's transform, baked once into a heightmap. `GrassGround::mesh(handle)` bakes at 0.5 m |
| `GrassFieldOverrides` | Component | On a field: `tint: Option<Color>` (multiplies blade colours), `density_scale`, `height_scale`, for one field without a style of its own |
| `GrassInteractor` | Component (requires `Transform`) | Flattens grass around the entity. `radius` (metres, default 0.6, up to `GrassSettings::max_interactor_radius`, 2 by default), `strength` (0–1, default 1), `max_height: Option<f32>` (only while within this height of the ground). One disc; add a `GrassInteractorFootprint` for one large object |
| `GrassInteractorSpread` | Component (requires `GrassInteractor`) | Extra disc centres in the interactor's local space, for a few separate contacts (feet, wheels); each disc pushes away from its own centre. `covering_aabb(center, half_extents, max_radius)` covers a local AABB with at most 32 discs |
| `GrassInteractorFootprint` | Component (requires `GrassInteractor`) | A box flattened as one shape instead of a disc: `center`, `half_extents` (local space), `rounding` (metres; a ball's or capsule's radius). Under a moving box the grass lies the way it moves and only its sides push sideways; at rest it pushes out from the nearest edge. Follows the transform's rotation and scale; clipped to 16 m from its centre per side. The interactor's `radius` and spread are not used |
| `GrassInteractorFromCollider` | Component (requires `GrassInteractor` and `GrassInteractorFootprint`) | Derives a `GrassInteractorFootprint` from this entity's physics collider (features `avian`, `rapier`), following its size, roundness and scale. `scale` (default 1), `strength` (default 1), `max_height: Option<f32>` (default `None`). Disabled colliders/bodies and sensors flatten nothing; warns once if no collider size arrives |
| `GrassColliderBounds` | Component | The collider `GrassInteractorFromCollider` flattens under: `center`, `half_extents` (metres along the entity's axes, scale applied), `rounding`, `active`. The physics features write it; write it yourself for another collider type |
| `GrassColliderSystems` | System set (`Update`) | `Bounds` (backends write `GrassColliderBounds`), then `Fit` (the adapters are fitted) |
| `GrassSystems` | System set | `DetectChanges`, `Edit`, then `Rebuild`, in `PostUpdate`: order your systems against them |
| `GrassWind` | Resource | `direction: Vec2` (XZ; any length, normalized on use; `.toward(Dir2)` sets it from a `Dir2`), `strength` (0–1), `gust_scale` (metres), `gust_speed` (m/s), and the multipliers `sway`, `sway_speed`, `flutter`, `gust_contrast` (1 in every preset). Presets `CALM`, `BREEZE` (default), `GUSTY`, `STORM` |
| `GrassStats` | Resource | What grass is doing: `tables` (fields, chunks, rebuilds and their time, what limits dropped), `gpu` (blades per level of detail, slices, overflows) and `edits` (`EditStats`: edits applied, texels changed, bytes uploaded, edits waiting, this frame's time) |
| `GrassDiagnostics` | Diagnostic paths | `grass/blades/near`, `mid`, `far`, `grass/visible_slices`, `grass/overflows`, `grass/table_rebuilds`, `grass/rebuild_time` |
| `GrassDebugPlugin`, `GrassDebug` | Plugin, resource (feature `debug`) | Gizmos for fields, chunks, interactors, footprints and trample windows |
| `GrassSun` | Component | On a directional light: the one grass casts shadows for, and a deferred camera treats as the sun. Without it, the first shadow-casting one |
| `GrassCamera` | Component | On a camera: overrides `radius`, `density_scale`, `min_width_px` and `shadows` of `GrassSettings` for that camera (each an `Option`; `None` follows the resource) |
| `NoGrass` | Component | On a camera: draw no grass there (a shorthand for a `RenderLayers` that shares none with any field) |

## Editing and queries

See [Editing and reading the grass](editing.md).

| Item | Kind | What it does |
| --- | --- | --- |
| `GrassEdit` | Message | A change to the grass: `shape`, `op`, `strength` (1: set, idempotent; below 1: a step that adds up), `softness` (0.3), `field: Option<Entity>` (default: every field under the shape). `clear`, `paint`, `mow`, `restore`, `with_strength`, `with_softness`, `on_field` |
| `GrassBrushShape` | Enum | `Circle { center, radius }`, `Stroke { from, to, radius }`, `Rect { center, half_size, yaw }`, in world XZ |
| `GrassEditOp` | Enum | `Density(d)`, `Height(h)`, `Mow(h)` (lowers only), `Restore` (back to the field's own mask) |
| `GrassBrush` | Component (requires `Transform`) | Edits wherever the entity goes: `op`, `radius`, `softness`, `rate: Option<f32>` (`None`: at once, swept along the path; a rate: per second while it stays) |
| `GrassBrushFootprint` | Component (requires `GrassBrush`) | The rectangle a brush edits instead of a disc: `center` (local space), `half_size` along the entity's X and Z; turned and scaled with the transform, swept between frames. The brush's `radius` is not used |
| `GrassRegrowth` | Component | On a field: `density_rate`, `height_rate` per second, back toward the field's own mask; only raises grass |
| `GrassSampler` | System parameter | `sample(point)`, `samples(point)`: a `GrassSample` per field; `mask_image(field)`: the mask with edits, as an image |
| `GrassSample` | Struct | `field`, `density` (0–1), `blades_per_m2`, `height_scale`, `height` (metres, a typical blade), `tallest` (metres) |

## Styles and style files

| Item | Kind | What it does |
| --- | --- | --- |
| `GrassStyle` | Asset | What a field looks like: a `GrassBlade` and the field's settings; see [Styles](styles.md#reference) |
| `GrassBlade` | Struct (`Copy`) | The blade's shape and colours; also what a species grows |
| `GrassVariation`, `GroundTint` | Structs | The field's variation and ground tint, inside `GrassStyle` |
| `GrassSpecies` | Struct | Another plant among the blades: a `GrassBlade`, a `share` and `clustering` |
| `GrassStyleFilePlugin` | Plugin | Registers the loader and the presets as `embedded://bevy_grass/styles/<name>.grass.ron` assets. Part of `GrassPlugins` |
| `GrassStyleLoader` | Asset loader | Loads `.grass.ron` |
| `GrassStyleFileError` | Enum | `Io`, `Parse { line, column, message }`, `UnknownBase`, `Invalid`, `SpeciesFieldSettings`: why a file could not be read |
