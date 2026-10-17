# bevy_grass: an API and developer-experience review

**Point of view:** a game developer adding bevy_grass to their own Bevy 0.20 game.

**Reviewed:** commit `4747d7d`. That covers:
- the public API (`src/lib.rs`, `src/api.rs`, `src/far.rs`, `src/style_file.rs`);
- the code behind it that decides how the API behaves (`src/fields.rs`, `src/render/view.rs`, `src/render/resources.rs`);
- the README and the examples.

**Compared with:**
- [Avian](https://github.com/avianphysics/avian) (physics, Bevy 0.19);
- [bevy_hanabi](https://github.com/djeedai/bevy_hanabi) (GPU particles, Bevy 0.19);
- Bevy's own first-party conventions (`Mesh3d`, `MeshMaterial3d`, `DistanceFog`, `Bloom`, `RenderLayers`, `Visibility`).

---

## Summary

**What already works well.**
- **Getting started is easy.** It takes one plugin, one component and one asset. No other Bevy grass crate I know of is as easy to start with.
- **Rendering fits into Bevy:** PBR, shadows, fog, prepass, deferred and TAA motion vectors.
- **The style system is good:** const presets, struct-update syntax, RON files with `base:` inheritance, hot reload, and error messages that say how to fix the problem.
- **Every setting documents its unit and range.**

**What keeps it from setting the standard.**
1. **Settings follow four different patterns**, and which one a knob uses is arbitrary:
   - plugin fields that hold `Option` overrides;
   - runtime resources;
   - asset fields;
   - fixed tier `const fn`s.
2. **Several standard Bevy contracts are ignored without a word:**
   - `Visibility`;
   - `RenderLayers`;
   - a field's rotation and scale;
   - reflection registration for components and resources.
3. **Failures are silent and hit everything at once.** One style or mask that is missing or failed to load makes *every* field in the world stop updating, with no log line.

All three can be fixed before a 0.1 release on crates.io. That is the cheapest moment to make breaking changes.

### Scorecard

| Area | Grade | One line |
| --- | --- | --- |
| First five minutes | A− | One plugin, one component. Loses marks because the quick start shows nothing on screen by itself (no camera or light). |
| API surface size | A | About 12 public types. Hard to get lost. |
| Documentation of individual settings | A | Units and ranges on every field, and "off at 0" everywhere. |
| Consistency of configuration | C | Plugin `Option`s, resources, assets and fixed tiers each work differently. See [Configuration](#2-configuration-is-not-uniform). |
| Bevy-idiomatic contracts | C+ | Ignores `Visibility`, `RenderLayers`, rotation and scale; no `#[reflect(Component)]`. |
| Failure behaviour | D | Global, silent stalls; validation differs between code and files. |
| Data types | B | Bevy `Color`, `Handle`, `Rect` and `ExtendedMaterial` are right. `Vec2` as a range, `Vec2` as a direction and a `Cow` slice are less so. |
| Runtime tunability | C+ | Wind and shadows can change at runtime; quality, radius and density cannot. |
| Debuggability | C | Logs only. No gizmos and no Bevy diagnostics. |
| Distribution | C | Git-only, `=0.20.0` pin, no docs.rs, no CHANGELOG. |

---

## Pros

1. **Small, obvious entry point.** `GrassPlugin::default()` plus `commands.spawn(GrassField { size, style, ..default() })`. It mirrors Hanabi's `HanabiPlugin` + `ParticleEffect::new(handle)` and Avian's `PhysicsPlugins::default()` + components.
2. **Look and placement are split the way Hanabi splits them.**
   - `GrassStyle` is an asset, like `EffectAsset`, so many fields share one look and a single edit retunes all of them.
   - `GrassField` is the per-instance component, like `ParticleEffect`.
3. **Presets are `const`s, and struct-update syntax composes them** (`..GrassStyle::TALL_PRAIRIE`).
   - It is the Bevy-native way to express "a preset with tweaks", the same as `Wind::STORM`.
   - It works in `const` contexts, including inside other presets' `species` lists.
4. **The `.grass.ron` format is designer-friendly.**
   - Every field is optional and `base:` inherits from a preset.
   - Colours are sRGB tuples.
   - `deny_unknown_fields` catches typos.
   - An unknown `base` lists the valid names.
   - Hot reload keeps the old grass when a file is broken.
5. **Every number states its unit and range** ("metres", "0 (soft) to 1 (stiff)", "in turns at the tip"). Few Bevy crates do this consistently.
6. **"0 is off, and off is free."** Every variation setting has the same neutral value. Shader features compile out when no field uses them. A consumer can reason about cost without reading shaders.
7. **It is a real part of Bevy's renderer:**
   - PBR lighting and fog;
   - cascaded shadows;
   - depth, normal and motion-vector prepass;
   - deferred G-buffer;
   - exact motion vectors.

   Most grass crates draw their own unlit or half-lit shading. Here, grass matches the rest of the scene with no extra work.
8. **Minimal dependency footprint.** It enables only Bevy features a `DefaultPlugins` game already has, and adds no new crates (bytemuck, ron and serde are already in Bevy's tree). Adding it never forces a Bevy rebuild.
9. **Quality tiers are named after hardware** ("GTX 1660 / RX 5600"). A settings menu can map to them directly.
10. **Logs suggest a fix.** The overflow warning names the knob to turn (`Raise GrassPlugin::instance_capacity or lower the style density`).
11. **Runtime resources are plain data.** `ResMut<Wind>` and `ResMut<GrassShadows>` are mutated like any Bevy resource, with no setter API to learn.
12. **Quality engineering is visible:**
    - `forbid(unsafe_code)`, pedantic clippy and `missing_docs`;
    - golden images on hardware and on WARP;
    - CPU twins of the GPU logic;
    - every perf claim backed by a measurement.

    That builds trust for a consumer deciding whether to depend on it.
13. **`#[non_exhaustive]`** on `GrassGround` and `GrassStyleFileError`, so new variants won't break consumers.

---

## Cons

### 1. The API contract does not follow Bevy's conventions

| Bevy convention | bevy_grass today | Consequence for a game |
| --- | --- | --- |
| Anything that renders honours `Visibility` and the hierarchy (`Mesh3d` requires `Visibility`) | `GrassField` requires only `Transform`; there is no `Visibility` anywhere in `src/` | `Visibility::Hidden` on a field, or on a level-streaming parent, does nothing. The only way to hide grass is to despawn the field, which rebuilds every table. |
| `RenderLayers` decide which camera sees what | Ignored. Only the `NoGrass` marker opts a camera out | Split-screen, picture-in-picture, portals and render-to-texture cameras all draw grass. Each also allocates its own blade buffers (about 10 MB at High). |
| `Transform` means translation, rotation and scale | Rotation and scale are "ignored" (`api.rs:20`) | A level designer rotates or scales a field in an editor, sees no change and gets no warning. Scaling a `Plane3d` mesh works, so the expectation is set. |
| Components and resources carry `#[reflect(Component)]` / `#[reflect(Resource)]` / `#[reflect(Default)]`, and asset types use `register_asset_reflect` | Only `derive(Reflect)` | Fields can't be spawned from `.scn.ron` scenes, BSN or Blender exporters (Skein, Blenvy), or inserted from bevy-inspector-egui. `Wind` and `GrassStyle` can't be edited in inspector resource or asset panels. Worth confirming against 0.20's auto-registration, but `ReflectComponent` still needs the attribute. |
| A handle sits in a newtype component (`Mesh3d(Handle)`, `MeshMaterial3d(Handle)`) or behind a constructor (`ParticleEffect::new(handle)`) | `GrassField { style: Handle<GrassStyle>, .. }` with `Default` | `..default()` silently gives `Handle::default()`, which points to no style. See failure modes below. |
| Ordering hooks are public `SystemSet`s (Avian's `PhysicsSystems`) | Systems are private and sit in `PostUpdate` after transform propagation | A game that writes a heightmap or mask procedurally, or moves fields, can't order itself relative to the rebuild. |

### 2. Configuration is not uniform

Today a consumer has to learn four configuration patterns:

| Knob | Where it lives | Runtime-changeable? | Pattern |
| --- | --- | --- | --- |
| Quality tier | `GrassPlugin::quality` | No | enum |
| Instance capacity, max interactors | `GrassPlugin` | No | `Option<u32>` override of the tier |
| Shadow mode | `GrassPlugin::shadows`, then the `GrassShadows` resource | Yes | `Option` at startup, resource after |
| Grass radius, density scale, ortho LOD | `GrassQuality` `const fn`s | **No, and not overridable at all** | Fixed per tier |
| Wind | `Wind` resource | Yes | const presets + struct update |
| Look | `GrassStyle` asset | Yes | const presets + struct update |
| Minimum blade width in pixels | `GrassStyle::min_width_px` (per field) | Yes | This is really a camera concern, filed under the style |
| Toon bands | `GrassStyle::toon_bands` (per field) | Yes | Silently ignored on deferred cameras |
| Far-grass fade distance | `FarGrass::new(&style, quality)` | Manual | The caller must pass the same tier the plugin got |

**Problems:**
- **A settings menu can't change grass quality.** Every game has a graphics menu; the radius and density are the first things a player turns down. Shadows can be changed but nothing else can.
- **The radius can't be tuned between tiers.** A game whose camera sits 30 m up wants High density at a 140 m radius. Today it must pick Ultra and inherit Ultra's shadows, capacity and ortho LOD too.
- **Two override styles side by side.**
  - The plugin uses `quality` + `Option<T>` overrides.
  - Styles and wind use `PRESET` + `..PRESET` struct update.
  - Same idea, two spellings. The style one is the better one.
- **`FarGrass::new(&style, quality)` duplicates the plugin's state.** Change the tier in one place and forget the other, and a seam appears at the grass radius.

### 3. `GrassStyle` mixes two kinds of settings in one flat struct

`GrassStyle` has 30 public fields:
- **13 describe a blade:** height, width, colours and shape.
- **About 13 describe the field:** density, patches, tufts, bare patches, ground tint, toon bands, `min_width_px` and species.

The difference matters only inside `GrassSpecies::style`, where the field settings are **silently ignored**. The type system lets you write `GrassSpecies::new(GrassStyle { tufts: 0.8, species: vec![...].into(), ..}, 0.1)`, and none of it does anything. The README has to mark rows "*Field.*" to explain this. That is documentation compensating for a type that is too wide.

### 4. Naming and data types

| Item | Issue | Suggestion |
| --- | --- | --- |
| `GrassField::density` vs `GrassStyle::density` vs `GrassQuality::density_scale` | Three different "densities". On the field it is an `Option<Handle<Image>>` mask; on the style it is blades per m². | `density_map` (or a separate `GrassDensityMap` component) |
| `GrassStyle::height: Vec2` | `x` = min, `y` = max is a convention, not a type. `width` has a hidden ±25% variation while height's range is explicit. | `height: FloatRange { min, max }`, or keep `Vec2` but name it `height_range` and document `width_variation` |
| `GrassStyle::head: f32` | Reads like a toggle or a struct. It is a share of the blade length. | `head_length` |
| `tuft_size` | Documented as "distance between tufts", which is a spacing, not a size | `tuft_spacing` |
| Intensity names | `patch_amount`, `tufts`, `dry_tips`, `droop`, `bare_patches` (a share), `dead_blades` (a share), `ground_tint` | Pick one pattern. A bare noun for 0–1 strength (`tufts`, `droop`) is fine. Then `patch_amount` → `patches`, and shares end in `_share` (`bare_share`, `dead_share`). |
| `Wind::direction: Vec2` | Bevy 0.14+ has `Dir2`, which can't be zero or unnormalized. The shader already falls back to +X for zero. | `Dir2` |
| `Wind` (in the prelude) | A very generic name. It will clash with a game's own weather type or another crate's. Avian gets away with `Gravity` because physics owns gravity; grass doesn't own wind. | `GrassWind`, or keep `Wind` out of the prelude |
| `species: Cow<'static, [GrassSpecies]>` | Necessary for `const` presets, but `Cow` in a user-facing field is unusual. Users must know `.into()` or `Cow::Borrowed`. | Keep the field. Push users to `with_species(vec![..])` and `GrassStyle::species()` in the docs. |
| `pub mod api` | Types sit in `bevy_grass::api::*` and `bevy_grass::*` (plugin, quality, far grass). A module named `api` says nothing. | Re-export everything at the root; make `api` private or split it into `field`, `style`, `wind`, `interact` |
| `FarGrassUniform` | Public, "for custom materials", but every field is private, so it can't be used | Make it `pub(crate)`, or make its fields public and documented |
| `twist` in turns | Bevy uses radians everywhere | Fine for designers, but be explicit in the RON docs. Optionally accept `twist_degrees`. |

### 5. Failures are silent and global

1. **One unloaded or failed asset freezes every field.** `fields::rebuild` (`src/fields.rs:241-258`) waits for every field's style, mask and heightmap, and `return`s if any is missing. The consequences:
   - A typo in one `assets.load("grass/meado.grass.ron")` path means no field in the world ever appears or updates.
   - `GrassField { size, ..default() }` with a forgotten `style` has the same effect.
   - Nothing is logged.
2. **A mask or heightmap whose CPU copy is gone** (`RenderAssetUsages::RENDER_WORLD`, which many asset pipelines set for terrain textures) never shows up in `Assets<Image>` → the same global stall.
3. **Compressed or unsupported formats** (KTX2/BCn): `get_color_at` fails → the mask reads as full density, silently.
4. **sRGB masks:** an sRGB-loaded mask (Bevy's default for PNG) is converted to linear, so 50% grey becomes about 21% density. The README documents the `is_srgb = false` dance, but every consumer has to get it right.
5. **Validation differs by path.** The RON loader *rejects* `head >= 0.9`, nested species, `share > 1` and zero sizes. The same values built in code are *silently clamped* (`head.clamp(0.0, 0.9)`), truncated (species beyond 15, with one warning) or ignored (nested species).
6. **Limits warn once in the log and then drop content:** 256 table entries (fields × species), 16 species, instance capacity and visible slices. There is no way to observe these from game code.

### 6. Hidden costs

- **Changing a style asset rebuilds everything.** Any `AssetEvent<GrassStyle>`, any field `Changed<GlobalTransform>`, or any watched image event rebuilds *all* tables and re-reads *every* mask and heightmap texel by texel with `get_color_at`.
  - A game that animates `tip_color` for a day/night or season blend, or moves one small field, pays a full rebuild every frame. With a 4096² mask that is a multi-millisecond CPU hitch.
  - The README advertises runtime style edits without this caveat.
- **Every active `Camera3d` without `NoGrass` allocates per-camera buffers** sized for the full tier capacity, and runs the full compute pipeline. Editor cameras, picking cameras, reflection-probe or portal cameras and UI-in-3D cameras all pay this unless the user knows about `NoGrass`.

### 7. Integration friction

- **The quick start renders a black screen.** It has no camera, no light and no ground ("Your scene still needs a camera, a light and something to stand on"). Avian's quick start is a complete, visible scene.
- **The examples depend on `examples/common`** (argument parsing, the screenshot harness and `spawn_person`). A developer copying `meadow.rs` into their game has to untangle it. There is no standalone `minimal.rs`.
- **Shadow quality depends on the game's `CascadeShadowConfig`, and nothing says so.** This session's blinking-shadow investigation is exactly the trap a consumer will fall into:
  - with a first cascade over 60 m, a shadow texel is wider than a blade, and blade shadows blink as grass sways;
  - the library neither documents a recommended cascade setup nor detects a bad one.
- **Shadows come from the "first" shadow-casting directional light** (`resources.rs:112`, query order). A sun-and-moon setup picks one arbitrarily, and there is no marker to choose.
- **The shipped `.grass.ron` presets live in the repository's `assets/`.** A crates.io consumer can't `load()` them without copying files. `base: "MEADOW"` softens this.
- **Bevy is pinned at `=0.20.0`** (`Cargo.toml:15`). For a published library this is a hard blocker: a game on Bevy 0.20.1 can't resolve both. The pin exists for DEMO ARPG GAME's build; DEMO ARPG GAME can pin itself.

### 8. Behaviour the docs don't warn about

- **Interactors push at most 2 m** (`MAX_PUSH_RADIUS`); a larger `radius` is clipped silently.
- **Interaction is horizontal only.** A bird, a jumping player or a flying camera drone flattens the grass below it.
- **Trampling exists only in a 64 m window** (256 × 25 cm cells) around each camera's focus. Interactors outside it leave no trail.
- **Time wrap (to verify).** Wind, sway, flutter and the far material all use `Time::elapsed_secs_wrapped()` / `globals.time`, which wrap every hour by default. When they wrap, the gust UV and every blade's sway phase jump, so the whole field likely pops once per hour of play. Worth a test with a short wrap period.
- **`toon_bands` and deferred cameras.** `toon_bands` is silently ignored on deferred cameras. This is documented on the field but not reported at runtime.

---

## Common pitfalls (what a new user will hit, in rough order)

| # | What they do | What happens | Today's mitigation |
| --- | --- | --- | --- |
| 1 | Copy the quick start into an empty project | Black window | A sentence in the README |
| 2 | `GrassField { size, ..default() }` and forget `style`, or mistype a `.grass.ron` path | **All** grass disappears, no log | None |
| 3 | Load a mask PNG with default settings | Densities and heights are wrong (sRGB curve) | A README snippet |
| 4 | Use terrain textures loaded with `RENDER_WORLD` only, or compressed ones | All grass stalls, or masks are ignored | One README sentence ("keep their CPU copy") |
| 5 | Rotate or scale a field in an editor or code | Nothing changes | A doc comment |
| 6 | Hide a field with `Visibility::Hidden` | Still drawn | None |
| 7 | Add a minimap, mirror or editor camera | Grass is drawn (and paid for) there too | `NoGrass`, if they know |
| 8 | Use the default 4-cascade light, or one long cascade | Blinking or missing blade shadows | None (example code only) |
| 9 | Animate a style's colour every frame | CPU spikes from full table rebuilds | None |
| 10 | Build hundreds of small fields from level data | "Some fields are not drawn" after 256 entries | A log warning |
| 11 | Add a sun and a moon | Grass shadows follow an arbitrary one | None |
| 12 | Change the tier in the settings menu | Impossible without restarting the app | None |
| 13 | `GrassInteractor { radius: 4.0, .. }` for a vehicle | Clipped to 2 m | A doc comment |
| 14 | Depend on Bevy 0.20.1 | Cargo resolution error | None |

---

## How it compares

| Concern | Avian | bevy_hanabi | bevy_grass |
| --- | --- | --- | --- |
| Entry point | `PhysicsPlugins` (a plugin group; parts can be disabled or replaced) | `HanabiPlugin` | `GrassPlugin`, monolithic (far-grass material and RON loader always on) |
| Authoring unit | Many small components (`RigidBody`, `Collider`, `Friction`, `Mass`…) with required-component defaults | `EffectAsset` built from modifiers and expressions | One flat `GrassStyle` asset with 30 fields |
| Instance component | Components on any entity | `ParticleEffect::new(handle)` | `GrassField { size, style, ground, density }` |
| Per-instance overrides without a new asset | Every property is its own component | `EffectProperties` | None: a field-level tint means a new style asset |
| Runtime configuration | Resources (`Gravity`, `Time<Physics>`), configurable schedule | Properties, spawner activation and reset | `Wind`, `GrassShadows`; the rest is startup-only |
| Public system sets | Yes | Yes | No |
| Debug view | Debug render plugin (colliders, AABBs, contacts, joints) | Inspector-friendly assets | None |
| Diagnostics | Physics timing diagnostics | Particle counts | `debug!`/`warn!` lines only |
| Visibility / layers | n/a | Respects `Visibility` / `RenderLayers` | Neither |
| Serialization | `serialize` feature | serde on assets | Hand-written RON mirror of `GrassStyle` |
| Web | WASM | WebGPU WASM, with a hosted demo | Not stated (compute-only, so WebGPU should work) |
| Docs | docs.rs with guide-level module docs, migration guides per release | docs.rs, examples gallery | Excellent README; not on docs.rs yet |
| Version table | Yes | Yes | Yes (one row) |

**Takeaways:**
- Hanabi validates the asset + component split that bevy_grass already has.
- Avian shows the next level of polish:
  - modular plugin groups;
  - public system sets;
  - a debug-render plugin;
  - diagnostics;
  - migration guides.

---

## Improvement ideas

Ordered by impact on a consumer.
- **P0:** fix before the first crates.io release.
- **P1:** sets the standard.
- **P2:** polish.

### P0: correctness of the contract

**P0-1. Make a field independent of other fields' assets, and report failures.**
- **Change:**
  - Skip, and keep the previous data for, any field whose style, mask or heightmap isn't loaded yet, instead of `return`ing from the whole rebuild.
  - On `LoadState::Failed` or a dangling handle, `warn!` once per field, naming the entity and the path.
- **Why:**
  - This is the one pitfall that turns a small typo into "the crate is broken".
  - Bevy's own meshes and materials degrade per entity, never globally.
- **Cost:** small, local to `fields::rebuild`.

**P0-2. Make the default style handle valid.**
- **Change:** at plugin build, insert `GrassStyle::MEADOW` at `Handle::<GrassStyle>::default()`'s id (`Assets::insert(AssetId::default(), ..)`). Also, or instead, add `GrassField::new(size, style)`.
- **Why:**
  - `..default()` then gives visible meadow grass instead of nothing.
  - Bevy does the same for `StandardMaterial`: its default handle renders.
- **Cost:** a few lines.

**P0-3. Treat masks and heightmaps as data whatever the image's colour space.**
- **Change:**
  - For `*Srgb` formats, read the raw stored byte instead of the linearised colour.
  - For unsupported formats, warn once instead of defaulting to full density.
  - Optionally accept `RENDER_WORLD`-only images by reading them back on the GPU, or `warn!` clearly.
- **Why:**
  - Removes the most error-prone step of the README.
  - Nobody wants gamma-curved density.
- **Cost:** small.

**P0-4. Relax the Bevy pin to `0.20`.**
- **Why:**
  - `=0.20.0` makes the crate unusable for anyone on 0.20.x.
  - DEMO ARPG GAME can keep its own `=` pin, and Cargo will pick 0.20.0 for both.
- **Cost:** none.

**P0-5. One validation for both paths.**
- **Change:**
  - Add `GrassStyle::validate(&self) -> Result<(), GrassStyleError>`, used by the RON loader.
  - Check it with a `warn!` (or `debug_assert!`) when a style asset is added or modified from code.
  - Turn silent clamps into "clamped `head` from 0.95 to 0.9" warnings.
- **Why:** the same style must not be an error from a file and silently different from code.

### P1: consistency and Bevy idioms

**P1-1. Honour `Visibility`, `RenderLayers`, and the field's scale.**
- **Change:**
  - `GrassField` requires `Visibility`. Hidden fields are left out of the chunk table, or better, a per-field enabled bit avoids a rebuild.
  - Cameras and fields both read `RenderLayers` (default layer 0, as for meshes).
  - Field size is `size * scale.xz`.
  - Warn once if a field has a non-identity rotation, until yaw is supported.
- **Why:**
  - These are the contracts every Bevy user already knows.
  - Level streaming, split-screen and editors depend on them.
  - `NoGrass` can stay as a shorthand.

**P1-2. One settings type, built the way styles are.** Replace `GrassQuality` + `Option` overrides with a `GrassSettings` struct that has tier presets as `const`s:

```rust
#[derive(Resource, Clone, Reflect)]
#[reflect(Resource, Default)]
pub struct GrassSettings {
    pub radius: f32,
    pub density_scale: f32,
    pub shadows: GrassShadows,
    pub orthographic_lod: u32,
    pub instance_capacity: u32,   // buffers are reallocated when it changes
    pub max_interactors: u32,
}
impl GrassSettings {
    pub const LOW: Self = ...;  pub const MEDIUM: Self = ...;
    pub const HIGH: Self = ...; pub const ULTRA: Self = ...;
}

app.add_plugins(GrassPlugin { settings: GrassSettings { radius: 140.0, ..GrassSettings::HIGH } });
// and later, from a settings menu:
fn apply(mut grass: ResMut<GrassSettings>) { *grass = GrassSettings::LOW; }
```

- **Why:**
  - **One pattern:** a const preset plus struct update, the same as `GrassStyle` and `Wind`.
  - **Every knob is overridable.**
  - **Everything is runtime-changeable.** Radius, density and LOD are just uniforms; capacity needs a buffer reallocation, which per-view lazy creation already handles.
  - **`GrassShadows` becomes a field,** not a second resource.
  - **`GrassQuality` can stay** as a convenience (`GrassSettings::from(GrassQuality::High)`) for settings menus.

**P1-3. Per-camera overrides as a camera component.**
- **Change:** an optional `GrassCamera { radius, density_scale, min_width_px, shadows }` (each `Option`, falling back to `GrassSettings`).
- **Why:**
  - Per-view rendering settings live on the camera in Bevy (`Msaa`, `Tonemapping`, `DistanceFog`, `Bloom`).
  - A minimap at Low next to a main camera at High becomes one component.
  - `min_width_px` moves off the style, where it never belonged: a pixel width depends on the camera, not on the grass.

**P1-4. Split `GrassStyle` into blade and field parts.**

```rust
pub struct GrassBlade { height, width, lean, stiffness, root_color, tip_color, root_occlusion,
                        translucency, specular, twist, droop, fold, head_length, head_width, head_color }
pub struct GrassStyle {
    pub blade: GrassBlade,
    pub density: f32,
    pub variation: GrassVariation, // patches, dry tips, dead blades, tufts, bare patches
    pub ground_tint: GroundTint,   // color + amount
    pub toon_bands: u32,
    pub species: Cow<'static, [GrassSpecies]>,
}
pub struct GrassSpecies { pub share: f32, pub clustering: f32, pub blade: GrassBlade }
```

- **Why:**
  - The type system then says what the README's "*Field.*" column says.
  - Nested species and ignored field settings become unrepresentable.
- **Trade-off:**
  - Struct updates get one level deeper (`blade: GrassBlade { tip_color, ..GrassStyle::MEADOW.blade }`).
  - The RON format can stay flat via serde `flatten`.
  - Do it before 0.1, or not at all.

**P1-5. Public system sets.**
- **Change:** `GrassSystems::{DetectChanges, Rebuild}` in `PostUpdate`, documented.
- **Why:** procedural terrain and runtime painting (on the roadmap) need to order against the rebuild. Avian's `PhysicsSystems` is the model.

**P1-6. Reflection, done fully.**
- **Change:**
  - `#[reflect(Component, Default, Debug)]` on `GrassField`, `GrassInteractor` and `NoGrass`.
  - `#[reflect(Resource, Default)]` on `Wind`, `GrassShadows` and the new `GrassSettings`.
  - `app.register_asset_reflect::<GrassStyle>()`.
- **Why:**
  - Scenes, BSN, Blender workflows (Skein) and bevy-inspector-egui all work.
  - A designer can tune a meadow live in an inspector, which beats editing RON.

**P1-7. Choose the shadow light explicitly, and help with cascades.**
- **Change:**
  - A `GrassSun` marker on a `DirectionalLight` (falling back to today's "first shadow caster").
  - A "Shadows" README section with a recommended `CascadeShadowConfigBuilder` (3 cascades, a 12 m first cascade, from this session's findings).
  - Optionally a startup `warn!` when the first cascade's texel is wider than the typical blade.
- **Why:** blinking blade shadows were the first visual bug found in this crate's own examples. A consumer won't know why it happens.

**P1-8. Diagnostics and a debug plugin.**
- **Change:**
  - Register Bevy `Diagnostic`s: visible blades per bucket, visible slices, overflow count, table rebuilds, and rebuild CPU time.
  - Add a `GrassDebugPlugin` with gizmos for field rectangles, chunks, interactor discs and each camera's trample window.
- **Why:**
  - Avian's debug render plugin is one of its most-loved features.
  - "Why is there no grass here?" should be answerable by looking.

**P1-9. Incremental rebuilds.**
- **Change:**
  - Cache packed masks and heights per `AssetId`, and re-read only the image that changed.
  - Rewrite only the field entries whose style changed, without re-reading masks.
  - Treat a colour-only style change as a uniform update.
- **Why:** it makes "change a style at runtime", which the README advertises, safe to do every frame for seasons, day/night or wet grass.

### P2: polish

1. **A quick start that renders.** Add a complete `minimal.rs` example, about 40 lines with camera, light, ground and field, that needs no `common` module, and make it the README quick start.
2. **Make `GrassPlugin` a plugin group** (`GrassPlugins`): core, `FarGrassPlugin`, `GrassStyleFilePlugin` and `GrassDebugPlugin`, so a game can drop the far material or the RON loader, as Avian allows.
3. **Rename for one meaning per word:**
   - `GrassField::density` → `density_map`;
   - `head` → `head_length`;
   - `tuft_size` → `tuft_spacing`;
   - shares end in `_share`.
   Keep serde aliases for the old RON names during 0.x.
4. **`Wind::direction: Dir2`,** and consider `GrassWind`, or keep it out of the prelude.
5. **Flatten the module tree.** Every public type at the crate root (plus the prelude), with no `api` module, as Hanabi and Avian do.
6. **Read the far-grass fade from the settings resource.**
   - `FarGrass::new(&style)`, with the fade following `GrassSettings::radius` through the same sync system that already copies `Wind`.
   - An explicit `fade` overrides it.
7. **Per-field overrides without a new asset.**
   - An optional `GrassFieldOverrides { tint: Option<Color>, density_scale: f32, height_scale: f32 }` component, like Hanabi's `EffectProperties`.
   - Many games want "this meadow, a bit drier" for one area without cloning a style asset.
8. **Embed the presets as assets.** For example `embedded://bevy_grass/styles/meadow.grass.ron`, so a crates.io user can load them by path, not only as `base:`.
9. **Interactor contract:**
   - document the 2 m cap and warn when it is exceeded;
   - add an optional `height` (the interactor affects grass only when its feet are within the blade height);
   - document the 64 m trample window in the type's docs, not only in the module docs.
10. **Fix or document the one-hour time wrap.** Pass time modulo a period that keeps the gust UV and sway phases continuous, or use `Time<Real>` deltas accumulated per field.
11. **Hex colours in RON** (`tip_color: "#5c8f24"`), the format artists copy from their tools. Keep tuple support.
12. **Derive the style file from the type.** `GrassStyleFile` is a hand-written mirror of 30 fields, edited in four places per new setting. Generate it with a small macro, or with reflection-based "apply over base", so the format can't drift.
13. **Distribution:**
    - crates.io and docs.rs;
    - a `CHANGELOG.md` and per-release migration notes (Avian's are exemplary);
    - a WebGPU WASM demo page (Hanabi's live demo sells the crate).
14. **A `PartialEq` derive** on `GrassStyle`, `GrassSpecies` and `GrassField`, so games and tests can compare styles. It is also needed for change-detection-friendly `set_if_neq`.

---

## Keep as they are

- The asset + component split, the const presets and struct-update composition.
- Units and ranges on every field, and "off at 0, free when off".
- The RON format's `base:` inheritance, `deny_unknown_fields` and helpful errors.
- Rendering through Bevy's PBR, prepass and deferred paths. This is the crate's strongest differentiator.
- Hardware-named quality tiers, kept as a convenience on top of `GrassSettings`.
- Actionable log messages. Extend the same voice to every new warning above.

---

## Suggested order of work

1. **P0-1 to P0-5**, about a day. They remove every way to get "no grass and no message", and unblock crates.io.
2. **P1-1, P1-2, P1-6**, the breaking API shape: visibility and layers, the settings type and reflection. Do them together in one breaking change before 0.1.
3. **P1-4 (blade/field split).** Decide yes or no before 0.1. After release it is a painful migration.
4. **P1-5, P1-7, P1-8, P1-9**, which add features without breaking anything.
5. **P2 items**, as time allows. P2-1 (a quick start that renders) costs little and is worth doing first.
