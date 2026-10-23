# Production cleanup: separating grass from Bevy

Status: in progress (October 2026); each step below says when it was done. An architecture review of where
bevy_grass is tied to Bevy's render internals, to Bevy's rendering settings
and to features it does not own (wind, lights, physics), and what to change
before the Bevy 0.21 port so that port is a matter of fixing one known list of
files. Changes that touch shaders or public code still go through the
performance bar and the golden check before they are kept
([Performance notes](../performance.md), [Testing](../testing.md)).

**Summary.** The crate does not need a large refactor. The game-side logic
(fields, tables, editing, trampling maths, styles, presets, the physics
bridge) is mostly free of Bevy's render code. The risk is in three places:

1. **The render boundary is wider than its own docs say.** It spans about
   3,100 lines of Rust across 7 files in `src/render/`, plus 5 shaders.
2. **Two public types depend on Bevy render code.** `GrassWindFrame` derives
   a render-only trait, and `GrassCamera` and `NoGrass` use a render-only
   macro.
3. **The most fragile render paths have no automated check.** These are
   deferred rendering, the prepass and motion vectors.

The third matters most: when Bevy 0.21 lands, those paths are the likeliest
to break without anyone noticing.

## The layers as they are

```text
 Game code
   │  GrassField · GrassStyle · GrassSettings · GrassCamera · GrassWind(Frame) · GrassSun · GrassInteractor
   ▼
┌────────────────────────── main world (game-side logic) ──────────────────────────┐
│ style / style_file / presets   fields (tables)   edit   ground   trample   physics │
│ shader_model (CPU twin)        wind (built-in model)   sun (light choice)          │
└──────────────────────────────────┬───────────────────────────────────────────────┘
                                   │  ExtractedGrass  ← the real hand-off point
┌──────────────────────────────────▼──── render world (changes with Bevy) ──────────┐
│ render/extract · resources · view · compute · platform · bevy_glue · mod          │
│ shaders: grass / look / grass_prepass / grass_shadow (import bevy_pbr)            │
│ far.rs + far_ground.wesl (ExtendedMaterial<StandardMaterial>)                     │
└───────────────────────────────────────────────────────────────────────────────────┘
```

## What is already well separated

Keep these as they are.

- **The physics bridge** (`src/physics.rs`) is the best example in the crate.
  A backend only reports `GrassColliderBounds`; every rule about what a
  collider does lives once, in the core. Rapier and Avian are about 100 lines
  each. Wind and lights should follow this pattern.
- **The style file format** (`src/style_file.rs`) is its own struct
  (`GrassStyleFile`) with its own colour handling and field aliases. It does
  not depend on Bevy's `Color` serialisation, so a Bevy serde change cannot
  break players' or designers' style files.
- **Bevy's render settings are inherited, not copied.** MSAA, HDR,
  tonemapping, fog and exposure reach grass through Bevy's per-view pipeline
  keys (`ViewKeyCache`, `ViewKeyPrepassCache`) and
  `main_pass_post_lighting_processing`. Grass has no settings of its own for
  any of them, which is right: they are Bevy's settings, and grass follows
  them.
- **`ExtractedGrass` is the hand-off.** The main world gives the renderer one
  resource of plain data, so the logic does not know how it is drawn.
- **The GPU support check** (`src/render/platform.rs`) counts storage buffers
  from the bind group layouts themselves, so a Bevy or wgpu limit change
  becomes one warning instead of a crash.
- **The wind hand-off exists.** `GrassWindFrame`, `GrassWindSystems::Sample`
  and `GrassPlugin { wind: false, .. }` let a game replace the wind model.
  C6 below covers its weak spots.

## The tight couplings

Most likely to break first.

### C1. Bevy's binned-phase and pipeline internals

Highest risk: these change in nearly every Bevy release. All in
`src/render/bevy_glue.rs` and `src/render/mod.rs`.

- **Phase keys.** Grass builds Bevy's phase keys by hand in three places
  (`add_prepass_item`, `queue_opaque`'s `Opaque3dBatchSetKey`,
  `queue_shadows`' `ShadowBatchSetKey`). Fields such as `lightmap_slab`,
  `slabs: MeshSlabs` and `material_bind_group_index` have been added, renamed
  or removed in most recent releases. These break at compile time, which is
  the good kind of break.
- **Borrowing the mesh pipeline.** `GrassMainPipeline::specialize` calls
  `MeshPipeline::specialize` with a template vertex layout, then
  `layout.truncate(2)` and pushes the grass group. This assumes Bevy keeps the
  view at group 0 and the binding array at group 1. If Bevy reorders its
  groups, it still compiles and fails only when the pipeline is validated at
  runtime.
- **The prepass shader defs are copied by hand.**
  `GrassPrepassPipeline::specialize` lists `MATERIAL_BIND_GROUP = 3`,
  `DEPTH_PREPASS`, `NORMAL_PREPASS_OR_DEFERRED_PREPASS` and the rest itself,
  which copies part of Bevy's `PrepassPipeline::specialize`. When Bevy adds or
  renames a def, grass falls out of step with no compile error.
- **A private Bevy field read through reflection.** If Bevy changes the shape
  of `DefaultOpaqueRendererMethod`, `deferred_by_default` quietly returns
  `false`, and deferred cameras stop drawing grass into the G-buffer with no
  error.
- **The render schedule.** `GrassRenderPlugin` orders its systems against
  `RenderGraph`, `camera_driver`, `init_prepass_pipeline` and
  `MeshPipelineSystems`. Bevy reworked the render graph into schedules
  recently, and that work may not be finished.

**Fix.** Keep the code where it is, but make it fail loudly:

- a `debug_assert!` on the layout count before `truncate(2)`, and a unit test
  of it;
- a unit test that `deferred_by_default(&DefaultOpaqueRendererMethod::deferred())`
  is `true`, so a Bevy change breaks the test, not players' games;
- the prepass def list in one function, with a comment naming the Bevy
  function it copies (`PrepassPipeline::specialize` in bevy_pbr 0.20), so the
  port checklist can diff the two.

### C2. Shaders import Bevy's shader modules, and fail only at runtime

`look.wesl`, `grass.wesl`, `grass_prepass.wesl`, `grass_shadow.wesl` and
`far_ground.wesl` rely on `PbrInput`'s field names, `pbr_input_new`,
`apply_pbr_lighting`, `main_pass_post_lighting_processing`,
`deferred_gbuffer_from_pbr_input`, `STANDARD_MATERIAL_FLAGS_FOG_ENABLED_BIT`,
`MESH_FLAGS_SHADOW_RECEIVER_BIT` and `previous_view_uniforms`. A broken import
draws no grass, logs an `ERROR` and otherwise looks like a large speed-up.
The imports are spread across five files, mixed in with grass maths.

**Fix.** One `bevy_bridge.wesl`, the only file allowed to import `bevy_pbr`
or `bevy_render`. It offers a few small functions with stable signatures
(`grass_lit`, `grass_gbuffer`, `grass_post`, the view), and `blade_pbr_input`
moves into it. `look.wesl` becomes grass logic only (colour, sheen,
variation), and a Bevy shader change means editing one file.

### C3. Render-only code in public, game-facing types

Medium risk, and fixing it is a breaking change.

- **`GrassWindFrame`** (`src/wind/mod.rs`) derives `ShaderType` from
  `bevy::render`. That makes the public wind hand-off the same thing as the
  GPU uniform layout: adding a wind term to the shader is a breaking change
  for every custom wind source, and the public type follows Bevy's render
  crate paths.
- **`GrassCamera` and `NoGrass`** (`src/api.rs`) derive `ExtractComponent`
  with `#[extract_app(RenderApp)]`. `api.rs` is the game-facing file, yet it
  imports `bevy::render`, and that macro attribute is new enough to change.

**Fix.**

- `GrassWindFrame` becomes a plain struct, and a private
  `WindUniform: ShaderType` in `src/render/` is built from it in
  `extract_grass`: a 48-byte copy a frame.
- The two camera components are extracted from `src/render/`, by manual
  `ExtractComponent` impls or by `extract_grass` reading them.

Afterwards the only files outside `src/render/` that import `bevy::render`
are `far.rs`, which is a material by nature, and the texture types `edit.rs`
and `ground.rs` use to make `Image`s, which are stable wgpu re-exports.

### C4. Main-world tables are stored in GPU layout

Low to medium risk, and deliberate. `FieldGpu` and `ChunkGpu`
(`src/fields.rs`) derive `ShaderType` and are built directly by the field
logic. It is fast (no conversion, the data is shared as `Arc`), and the twin
rules keep it correct; the cost is that the field logic depends on the GPU
layout.

**Fix.** Do not convert them: this is a hot path. Move the two structs into a
`gpu_layout.rs` module whose docs say it is the contract with `common.wesl`.
The dependency stays, but as one named seam instead of being spread through
`fields.rs`. *Done:* `src/gpu_layout.rs`, with `NO_MAP`.

### C5. Lights are read from Bevy's components, in two places

`src/sun.rs` and `src/render/extract.rs` each query `DirectionalLight` and
`GlobalTransform` and build `SunCandidate`s themselves. Shadows also depend
on Bevy's cascade model (`LightEntity::Directional { cascade_index }`,
`ViewLightEntities`), and `GrassShadows` is defined in cascade indices.

In practice:

- a game whose sun is not a `DirectionalLight` (a custom sky or atmosphere, a
  day/night cycle driving another component) cannot tell grass where the sun
  is;
- if Bevy reworks lights again (it split them into `bevy_light` in 0.20),
  both copies need editing.

**Fix.** One adapter system turns Bevy's lights into a
`GrassLightFrame { shadow_dir, sun_dir }` resource, the way the wind frame
works: the built-in adapter can be turned off, and a game can write the
resource itself. `extract_grass` reads the resource instead of querying
lights. The cascade logic stays in `bevy_glue.rs`; it is render-side by
nature.

### C6. The wind hand-off is built around the built-in model

The design is right; two weak spots.

- `GrassWindFrame`'s fields (`sway`, `flutter`, `gust_contrast`, and
  `gust_offset` in repeats of the gust texture) are the built-in model's
  parameters, not a neutral description of wind. A weather crate has to
  imitate the gust texture's scrolling.
- `GrassPlugin` always initialises `GrassWind`, even with `wind: false`.
  Harmless, but the built-in model's input is always there.

**Fix.** Document the frame as "the grass shader's wind parameters", and add
a helper such as `GrassWindFrame::advance(dt, direction, speed)` so a custom
source does not have to re-derive the scrolling.

### C7. The grass design API: stable logic, breaking growth

`GrassStyle`, `GrassBlade`, `GrassSettings` and `GrassShadows` are
public-field structs and enums without `#[non_exhaustive]`. Adding a field
breaks any game that writes a literal without `..`. `#[non_exhaustive]` is
not the fix: it would also forbid `..GrassSettings::HIGH` outside the crate,
which the docs teach.

Every bevy_grass minor already tracks a Bevy minor and is breaking anyway, so
the policy is:

- new style and setting fields arrive in a minor release only;
- the CHANGELOG says to add `..default()`;
- `.grass.ron` files keep working through the field aliases `style_file.rs`
  already supports.

The real cost of a design change is internal: a new look term touches about
six places (`style.rs`, the `style_file.rs` file struct, `FieldGpu`, `Field`
in `common.wesl`, `look.wesl`, `mirror.rs`). That is the price of the twin
rules and is acceptable; a checklist in the house rules helps more than a
refactor.

## The four concerns

| Concern | State | Main gap |
|---|---|---|
| Bevy engine internals | All in `src/render/`, but across 7 files; `bevy_glue.rs`'s claim that "a Bevy upgrade should need changes here and little else" is too optimistic | C1, C2 |
| Grass design API (for users) | Plain data, `const` presets, its own file format | C7 (policy, not code) |
| Bevy's render settings (AA, MSAA, HDR, fog, deferred, prepass) | Inherited from Bevy's view keys, the right design | C1: the deferred check and prepass defs fail silently, with no automated test |
| Features grass does not own (wind, lights, physics) | Physics excellent, wind good, lights coupled | C3, C5, C6 |

## Catching the next port's breakage

CI renders the goldens on lavapipe, but they cover the presets through one
forward camera. Deferred, the depth, normal and motion-vector prepasses, TAA,
MSAA off, HDR, orthographic cameras and a broken shader are checked by
nobody, or by hand with `cargo run --example passes`. Those are the paths in
C1 and C2.

**Proposal.** A small render-path set in `examples/golden.rs`: one preset,
about eight cameras, and two checks per camera, no `ERROR` in the log (which
catches shader and pipeline failures) and a matching image.

- forward
- forward without MSAA
- `DepthPrepass`
- `DepthPrepass` + `NormalPrepass` + `MotionVectorPrepass`, with TAA
- deferred
- HDR with bloom
- orthographic
- shadows off

It runs on the existing lavapipe job. The port then becomes "fix until green"
instead of hoping someone opens `passes -- --deferred`.

## Plan

In order:

1. **Render-path goldens with a log-error check** (above). Catches the
   silent failures in C1 and C2. *Done:* `path_*.png` in `examples/golden.rs`,
   blessed on the T1200; WARP and lavapipe need theirs from CI.
2. **Tripwire tests in `bevy_glue.rs`** (C1): `deferred_by_default`, the
   layout count before `truncate(2)`, and the prepass def list named against
   Bevy's source. About an hour. *Done:* `main_pass_layout` builds groups 0
   and 1 from Bevy's view layout and asserts that `MeshPipeline::specialize`
   built them in that order; `deferred_by_default` asserts the resource's
   shape; `prepass_shader_defs` names its Bevy source; unit tests in
   `src/render/bevy_glue/tests/`.
3. **Render code out of the public types** (C3): `GrassWindFrame`,
   `GrassCamera`, `NoGrass`. Breaking, so it goes in the 0.21 release.
   *Done:* `WindUniform` in `render/resources.rs` is built from the frame in
   `extract_grass` and for each far-grass material; `extract_cameras` in
   `render/extract.rs` copies the two camera components.
4. **`bevy_bridge.wesl`** (C2), so the shaders import Bevy in one file.
   *Done,* for the grass draws; `far_ground.wesl` keeps its own imports, as
   a `StandardMaterial` extension whose entry point takes Bevy's
   `VertexOutput`. The shadow pass binds the light's view through the bridge
   too, under `GRASS_SHADOW_PASS`. All 44 goldens unchanged.
5. **One light adapter and `GrassLightFrame`** (C5). *Done:* `GrassLightFrame
   { shadow_direction, sun_direction }`, `GrassLightPlugin` (off with
   `GrassPlugin { lights: false, .. }`) and `GrassLightSystems::Sample` in
   `PostUpdate` after transform propagation; `extract_grass` reads the frame.
   `check_shadow_setup` still reads Bevy's cascades, which are Bevy's by
   nature, through the same `SunCandidate::of`.
6. **A Bevy port checklist** in `docs/`, listing every Bevy touch point.
   After steps 2 to 5 it is short: `bevy_glue.rs`, the scheduling in
   `render/mod.rs`, `bevy_bridge.wesl`, `far.rs` and `gpu_layout.rs`.

Steps 3 and 4 change shader or public code, so each is measured against the
current performance bar and checked against the goldens before it is kept.

## Not proposed

Each of these adds a layer that would itself need porting every release:

- **Wrapping `Vec2`, `Color` or `Handle`** in the public API. It would fight
  the engine and break reflection, scenes and inspectors.
- **A trait-based render backend.** There is one renderer, Bevy's; an
  abstraction over it is a second thing to port.
- **Converting the field tables away from GPU layout** (C4). It costs frame
  time for no gain at port time.
