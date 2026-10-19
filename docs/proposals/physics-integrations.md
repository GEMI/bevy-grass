# Proposal: optional Avian and Rapier integrations

Status: Rapier part implemented in 0.2; Avian waits for an avian3d release on
Bevy 0.20. This document records the investigation behind the suggestion

> an optional feature (avian / rapier) that syncs collider transforms or
> spawns interactors [...] Also consider a capsule/box shape or a helper that
> spawns several interactors along a large object.

and proposes a small, ECS-shaped bridge from physics colliders to the
interaction system that already exists.

Implementation note: the Rapier half is implemented (`GrassInteractorSpread`,
the `rapier` feature, `GrassInteractorFromCollider`, `examples/rapier.rs`,
tests and CI). The adapter also keeps its last fitted AABB in a private
component so a moving body does not rebuild its spread every frame, and it
sets the interactor's strength to 0 while the collider is absent.

## 1. Short version

- **The transform plumbing already works.** Both Avian and Rapier write
  `Transform`/`GlobalTransform`; `bevy_grass` already reads
  `GlobalTransform` when it extracts interactors. Adding
  `GrassInteractor` to a physics body works today. No collider-transform
  sync is needed.
- **The real gap is shape and lifecycle.** Users have to derive a useful
  `radius`/`max_height` from a collider by hand, and a single disc is wrong
  for vehicles, walls, compound colliders and large monsters. The existing
  docs say "use several interactors", which is easy to get wrong.
- **Rapier is available now.** `bevy_rapier3d 0.37.0` depends on Bevy 0.20
  and a probe compiles against this crate.
- **Avian is blocked upstream.** `avian3d 0.7.0` (the current release) *and*
  the current `main` branch (`0.8.0-dev`) depend on Bevy 0.19. A crate built
  on Bevy 0.20 cannot query Avian's components: the probe fails with
  "multiple different versions of crate `bevy_ecs` in the dependency graph".
  Do not add an `avian` feature until Avian has a Bevy 0.20 release; the
  adapter code itself will then be nearly identical to the Rapier one.
- **Recommended shape of the work:** add a `rapier` optional feature now,
  plus a backend-neutral `GrassInteractorFromCollider` adapter component and
  a core `GrassInteractorSpread` footprint component for large objects. Add
  `avian` when it can compile. Keep physics types out of the core crate and
  keep the existing per-disc trample pipeline unchanged.
- **Do not auto-spawn proxy entities** as the default design. A data
  component that lists local disc centres is cheaper, easier to debug, and
  has no hierarchy lifecycle to manage.

Estimated effort:

| Piece | Effort |
| --- | ---: |
| `GrassInteractorSpread` in the core (component, extraction, debug, tests) | 2–3 days |
| Rapier optional feature and adapter system | 0.5–1 day |
| Rapier example, README/docs, reflection | 1 day |
| CI (feature build, tests, docs) | 0.5 day |
| Avian adapter, once Avian supports Bevy 0.20 | 0.5–1 day + review |
| Total for the Rapier release | roughly 4–6 focused days |

The single-disc adapter alone (without `GrassInteractorSpread`) is about a
day; the spread component is what makes large objects behave.

## 2. What exists today

The interaction model is deliberately simple:

- `GrassInteractor { radius, strength, max_height }` is a disc.
- `src/render/extract.rs` reads `(&GrassInteractor, &GlobalTransform)` and
  produces `(x, z, radius, strength)` stamps once per frame.
- `src/trample.rs` selects the nearest stamps per camera and
  `src/shaders/trample.wesl` stamps them into the per-camera trample texture.
  The GPU side is one workgroup per disc; there is no per-shape code.
- `max_interactor_radius` (2 m by default) clips a disc, and
  `max_interactors` (64 by default) caps how many a camera stamps.

Because the main-world side reads `GlobalTransform`, this already works:

```rust
commands.spawn((
    RigidBody::Dynamic,               // Avian or Rapier
    Collider::capsule_y(0.5, 0.4),
    GrassInteractor { radius: 0.4, ..default() },
));
```

The physics engine moves the entity, Bevy propagates the transform, and the
grass sees it. The integration proposal is therefore about **DX and
correctness**, not about a new interaction system.

## 3. What the physics engines already provide

Both engines keep the ECS transform in sync:

- **Avian** runs physics in `FixedPostUpdate` by default, then
  `TransformSystems::Propagate` in `PostUpdate` updates `GlobalTransform`.
  Its `ColliderAabb` component is a *world-space* AABB, updated after the
  simulation. A collider's local shape is available through
  `Collider::shape_scaled()` / `Collider::aabb(...)`.
- **Rapier** runs physics in `PostUpdate` before
  `TransformSystems::Propagate` by default, and writes back to `Transform`.
  `Collider::local_aabb()` returns the collider's local AABB *including the
  scale it currently has* (Rapier applies `GlobalTransform` scale to the
  collider in `apply_scale`). `Collider::aabb(translation, rotation)` gives
  a world AABB.

This means an adapter should **never read `Position`/`Rotation` directly**.
Doing so would break interpolation, parenting and users who run physics in
a custom schedule. Read `GlobalTransform` for placement and, if needed, a
collider's *local* AABB for size. That keeps the bridge engine-agnostic and
observes the normal ECS data flow.

Both engines also expose lifecycle components that are relevant to grass:

- `ColliderDisabled`, `RigidBodyDisabled` (Avian and Rapier; Rapier's
  `RigidBodyDisabled` is in `dynamics`)
- `Sensor` (Avian and Rapier)

A disabled collider should stop trampling. A sensor probably should not
trample by default (a trigger volume around a door is not a physical body),
but that is a policy decision; see §10.

## 4. How other Bevy libraries do it

The ecosystem uses three patterns.

### 4.1 Optional feature in the main crate

`bevy_feronia` has `avian = ["dep:avian3d"]` and conditionally adds Avian
components (`RigidBody`, `Collider`) to the entities it scatters. It keeps
the dependency optional and the feature off by default. This is the
simplest distribution model for a user: one crate, one feature flag.

### 4.2 Shared layer plus backend crates

`bevy-tnua` is the most relevant precedent:

- `bevy-tnua` (the core) knows nothing about a physics engine.
- `bevy-tnua-physics-integration-layer` defines backend-facing components.
- `bevy-tnua-avian3d` and `bevy-tnua-rapier3d` are separate crates, each
  with a plugin and an adapter component such as
  `TnuaAvian3dSensorShape(pub Collider)`.
- The backend systems read `Position`/`Rotation`/`Collider` and write the
  generic tnua components.

This decouples release schedules: the core can ship for a new Bevy before
the physics backend does. The cost is two packages for users and more
release/documentation work.

### 4.3 Marker component plus a deriving system

Both engines already use this idea for colliders:

- `bevy_rapier3d`'s `AsyncCollider` is a marker; a system watches `Mesh3d`
  and inserts/updates the real `Collider`.
- Avian's `ColliderConstructor` is a component; an observer/system turns it
  into a `Collider`.

This is the pattern to copy for grass: a small adapter component that says
"derive my interactor from this collider", and a system that owns the
derived `GrassInteractor`. Users explicitly opt in; nothing overrides a
hand-written `GrassInteractor` by accident.

## 5. Compatibility findings

These were checked with `cargo info`, the crates' published `Cargo.toml`
files, and small probe crates:

| Crate | Version checked | Depends on Bevy | Result with this crate (Bevy 0.20) |
| --- | --- | --- | --- |
| `bevy_rapier3d` | 0.37.0 | 0.20 | Compiles; an adapter system over `Collider` works |
| `avian3d` | 0.7.0 | 0.19.1 | Not usable; queries fail across two `bevy_ecs` versions |
| `avian3d` | `main` (`0.8.0-dev`) | 0.19.0 | Still not usable |

The Avian failure is not an API mismatch in the adapter; it is a hard ECS
type mismatch. `Collider`, `ColliderAabb` and the entity they belong to are
from `bevy_ecs 0.19`, while `GrassInteractor` and the `App` are from
`bevy_ecs 0.20`. Rust reports:

```text
error[E0277]: the trait bound `&ColliderAabb: QueryData` is not satisfied
...
note: there are multiple different versions of crate `bevy_ecs` in the
dependency graph
```

Consequences:

- An `avian` feature cannot be enabled in CI's `--all-features` builds
  today.
- A separate `bevy_grass_avian` crate would hit the same wall: the mismatch
  is in Avian, not in how the grass bridge is packaged.
- The correct time to add Avian is when `avian3d` depends on Bevy 0.20.
  Watch `avian3d`'s `Cargo.toml` and add the feature in the same release
  window. The adapter code can be prepared in a branch, but not published.

A separate integration crate is still a reasonable *future* option for
release cadence, but it does not solve the current Bevy-version blocker.

## 6. Approaches

### A. Documentation and examples only

Add an Avian/Rapier example that attaches a hand-written `GrassInteractor`
to a capsule, and a "physics" README section. No dependency, no feature.

- **Pros:** zero risk, zero dependency, no version coupling.
- **Cons:** users still have to choose a radius and remember `max_height`;
  large objects remain error-prone. Does not meet the "trivial to attach"
  goal.
- **Effort:** half a day.

### B. Optional feature plus a single-disc adapter

Add `rapier` (and later `avian`) features. Add one adapter component and a
system that reads the collider's local AABB, writes
`GrassInteractor { radius, strength, max_height }`, and disables itself for
disabled colliders.

- **Pros:** small, additive, one component for users; no render changes; no
  physics types in the core; easy to test without a GPU.
- **Cons:** one disc is wrong for long boxes/capsules, compound colliders
  and large creatures. The 2 m cap still bites. It does not address the
  friend's large-object concern.
- **Effort:** ~1 day for Rapier, ~1 day for Avian once compatible.

### C. Optional feature plus a footprint component (recommended)

Add `GrassInteractorSpread` to the core: a component holding local-space
disc centres. `GrassInteractor` keeps its radius/strength/max_height; the
spread says "stamp this disc here, and here, and here". Extraction emits
several stamps per entity. The shader, buffers and selection are unchanged:
the spread is just more entries in the existing `Vec<Vec4>`.

The physics adapter then computes the collider's local AABB and covers it
with a small grid of discs no larger than `max_interactor_radius`. A
capsule character gets one disc; a 10 × 4 m truck gets a handful; a wall
gets a row. A core helper makes the same footprint usable without physics.

- **Pros:** solves large objects without a new interaction system; keeps
  one adapter component; the core remains physics-free; easy to debug and
  test.
- **Cons:** adds a public component and extraction/debug work; needs a
  disc-count cap so one huge object cannot use every interactor slot.
- **Effort:** ~3–4 days including tests.

### D. Adapter spawns proxy interactor entities

The adapter spawns child entities with `Transform` + `GrassInteractor` for
each part of a large collider, and despawns/rebuilds them when the collider
changes.

- **Pros:** no core changes at all; existing debug/inspector see the
  proxies; child transforms follow the parent automatically.
- **Cons:** entity churn, hierarchy side effects (physics engines walk
  children for collider transforms), ownership markers, cleanup on
  despawn/removal, and one frame of stale proxies. It is more state to get
  wrong than a `Vec<Vec3>` and offers no advantage over C.
- **Effort:** ~2–3 days plus edge-case tests.

### E. Separate integration crates

Publish `bevy_grass_rapier` / `bevy_grass_avian` in the tnua style.

- **Pros:** each can release on its own schedule; `--all-features` on the
  core stays clean; users who do not use physics never see the dependency.
- **Cons:** two packages to document and version; users need to add a
  second crate; overkill while the adapter is one system. It also does not
  fix the Avian/Bevy mismatch.
- **Effort:** ~1–2 days of packaging/CI, plus the adapter work.

### F. Fit from Bevy's `Aabb` component (no physics dependency)

Bevy inserts a local-space `Aabb` on renderable entities (for frustum
culling). A core system could derive an interactor from `Aabb` instead of a
physics collider.

- **Pros:** works for any mesh object, physics or not; no optional deps.
- **Cons:** the mesh AABB is not the collider (invisible colliders,
  compound/trimesh colliders, character controllers), it is not updated
  when a mesh changes, and it may not exist on collider-only entities. It
  is a fallback, not a physics integration.
- **Effort:** ~1 day, but it does not replace B/C.

### Comparison

| | A docs | B single disc | C spread | D proxies | E separate crates | F `Aabb` |
| --- | --- | --- | --- | --- | --- | --- |
| One component for users | no | yes | yes | yes | yes | yes |
| Large objects correct | no | no | yes | yes | yes | no |
| Core changes | none | small | medium | none | none | small |
| Physics dependency optional | n/a | yes | yes | yes | yes | no |
| Works with Avian today | no (Bevy) | no (Bevy) | no (Bevy) | no (Bevy) | no (Bevy) | yes |
| Maintenance burden | low | low | medium | medium | high | low |
| Recommended | as docs | v1 | v1 target | no | later option | fallback |

## 7. Recommended design

### 7.1 Feature flags

```toml
[features]
# `GrassDebugPlugin` as today.
debug = ["bevy/bevy_gizmos"]

# Physics bridges. Off by default; enabling one must not change how Bevy is
# compiled for the game.
rapier = ["dep:bevy_rapier3d"]
# avian = ["dep:avian3d"]          # only once avian3d supports Bevy 0.20

[dependencies]
bevy_rapier3d = { version = "0.37", optional = true, default-features = false, features = ["dim3"] }
```

`default-features = false` is important. Rapier's default features turn on
`debug-render-3d`, `picking-backend` and `async-collider`, which enable
`bevy_gizmos`, `bevy_picking` and other Bevy features. The grass bridge does
not need them, and the crate promises that adding it never changes the
game's Bevy feature set. `dim3` alone adds no Bevy feature.

For Avian the equivalent will be `default-features = false, features =
["3d", "parry-f32"]` (or the version-appropriate feature names).

Docs.rs and CI:

- `[package.metadata.docs.rs] features = ["debug", "rapier"]`
- Add a CI job that runs `cargo test --features rapier`,
  `cargo clippy --all-targets --features debug,rapier`, and the MSRV check
  with `--features rapier`.
- Do not add `avian` to `--all-features` until it compiles. Either wait for
  the Avian release or change CI to an explicit feature list. A feature
  that cannot be enabled is worse than no feature.

### 7.2 Adapter component

Keep the core free of physics types. Put the adapter behind the feature:

```rust
// src/api.rs (gated) or src/physics/mod.rs
#[cfg(any(feature = "avian", feature = "rapier"))]
#[derive(Component, Debug, Clone, Copy, PartialEq, Reflect)]
#[reflect(Component, Default, Debug, PartialEq)]
#[require(GrassInteractor, GrassInteractorSpread)]
pub struct GrassInteractorFromCollider {
    /// Multiplies the footprint derived from the collider. 1.0 uses the
    /// collider as it is; 1.2 makes a character's trail a little wider.
    pub radius_scale: f32,
    /// Passed to `GrassInteractor::strength`. 0 disables trampling.
    pub strength: f32,
    /// Passed to `GrassInteractor::max_height`. `None` means any height,
    /// as for a hand-written interactor.
    pub max_height: Option<f32>,
}
```

Default: `radius_scale: 1.0`, `strength: 1.0`, `max_height: None`.

Usage:

```rust
commands.spawn((
    RigidBody::Dynamic,
    Collider::capsule_y(0.5, 0.4),
    GrassInteractorFromCollider::default(),
));
```

The required `GrassInteractor` is inserted automatically. The adapter is a
**source**, not a second interaction system: it only writes the three fields
of the existing component.

Naming: `GrassInteractorFromCollider` is deliberately engine-neutral. With
both features enabled, both systems can write it; an entity normally has a
collider from one engine only. If that ambiguity is uncomfortable, use
`AvianGrassInteractor` / `RapierGrassInteractor` instead; the trade-off is
two names for users who switch engines.

### 7.3 Footprint component

Add to the core, independent of physics:

```rust
#[derive(Component, Debug, Clone, Default, PartialEq, Reflect)]
#[reflect(Component, Default, Debug, PartialEq)]
#[require(GrassInteractor)]
pub struct GrassInteractorSpread {
    /// Disc centres in the interactor's local space. An empty list means
    /// "one disc at the entity's origin", the current behaviour.
    pub offsets: Vec<Vec3>,
}
```

Extraction becomes:

```rust
let discs: &[Vec3] = spread
    .filter(|s| !s.offsets.is_empty())
    .map_or(&[Vec3::ZERO][..], |s| s.offsets.as_slice());
for &offset in discs {
    let at = transform.transform_point(offset);
    let ground = interactor
        .max_height
        .and_then(|_| tables.highest_ground_at(at.xz()));
    let reach = trample::reach(interactor.max_height, at.y, ground);
    extracted.interactors.push(Vec4::new(
        at.x,
        at.z,
        interactor.radius,
        interactor.strength * reach,
    ));
}
```

This is the only render-side change. The shader, bind groups, selection and
debug budget are untouched. The spread's offsets are local, so a rotating or
scaled physics body moves them correctly.

A pure helper covers an AABB without physics:

```rust
impl GrassInteractorSpread {
    /// Disc centres covering a local AABB, and the disc radius to use.
    /// `max_radius` is `GrassSettings::max_interactor_radius`.
    pub fn covering_aabb(
        center: Vec3,
        half_extents: Vec3,
        max_radius: f32,
    ) -> (f32, Self);
}
```

A simple implementation uses one disc when the AABB's XZ diagonal fits in
`max_radius`. Otherwise it lays a grid over the AABB with cell sides of about
`max_radius * sqrt(2)` and one disc of `max_radius` per cell, which keeps the
discs at the cap and the count low. The adapter caps the number of discs (for
example 32) and warns once if a collider is so large that the cover becomes
coarse; a single object must not consume all `max_interactors` slots and
starve nearby characters.

Use `Vec3` offsets, not `Vec2`: a lying capsule has a real 3D segment, and
`GlobalTransform::transform_point` handles rotation and scale for free.

### 7.4 Rapier adapter

The adapter only needs a local AABB, so it avoids per-engine shape APIs and
their unit traps (Rapier's `cuboid` takes half extents; Avian's takes full
extents; Avian's `capsule` takes a length excluding hemispheres; Rapier's
`capsule_y` takes a half height).

```rust
fn sync_rapier_interactors(
    settings: Res<GrassSettings>,
    mut query: Query<(
        &Collider,
        &GrassInteractorFromCollider,
        &mut GrassInteractor,
        &mut GrassInteractorSpread,
        Has<ColliderDisabled>,
        Has<Sensor>,
    ), Or<(Changed<Collider>, Changed<GrassInteractorFromCollider>)>>,
) {
    let max_radius = settings.sanitized().max_interactor_radius;
    for (collider, source, mut interactor, mut spread, disabled, sensor) in &mut query {
        let local = collider.local_aabb();
        let center = Vec3::from((local.min + local.max) * 0.5);
        let half = Vec3::from((local.max - local.min) * 0.5);
        let (radius, cover) = GrassInteractorSpread::covering_aabb(center, half, max_radius);
        let radius = (radius * source.radius_scale).min(max_radius);
        let strength = if disabled || sensor { 0.0 } else { source.strength };
        // Write only on a real change, so a moving body costs three
        // comparisons and no allocation.
        if interactor.radius != radius
            || interactor.strength != strength
            || interactor.max_height != source.max_height
        {
            interactor.radius = radius;
            interactor.strength = strength;
            interactor.max_height = source.max_height;
        }
        if spread.offsets != cover.offsets {
            spread.offsets = cover.offsets;
        }
    }
}
```

Notes:

- `Collider::local_aabb()` already includes Rapier's collider scale, so do
  not multiply by `GlobalTransform::scale` again.
- `Changed<Collider>` can be set every frame for a moving body because
  Rapier's `apply_scale` mutably touches the collider when the transform
  changes. Comparing the derived values before writing keeps the steady
  state cheap and avoids `Vec` allocation.
- React to `GrassSettings` changes (especially `max_interactor_radius`) with
  a second system that runs when the resource changes, or with a dynamic
  filter. Otherwise a settings-menu change does not resize existing
  adapters.
- Add the system in `GrassPlugin::build` under `#[cfg(feature = "rapier")]`,
  or expose a `GrassRapierPlugin` and add it to `GrassPlugins`. Auto-adding
  is the better DX; the system is a cheap no-op when no entity has the
  adapter.
- Disabled colliders and sensors set `strength = 0`, which the existing
  selection already filters out. Do not remove `GrassInteractor`; a user may
  have added it by hand.
- Removing only the `Collider` (rather than the whole entity) leaves the
  last radius in place. This is an edge case; either document it or add a
  `RemovedComponents<Collider>` cleanup later. Removing the adapter should
  probably leave the last `GrassInteractor` alone, so users can switch from
  auto to manual without losing their tuning.

### 7.5 Avian adapter (once compatible)

The same shape, with Avian's types:

```rust
fn sync_avian_interactors(
    settings: Res<GrassSettings>,
    mut query: Query<(
        &Collider,
        &GrassInteractorFromCollider,
        &mut GrassInteractor,
        &mut GrassInteractorSpread,
        Has<ColliderDisabled>,
        Has<Sensor>,
    )>, /* changed filter, same as Rapier */,
) {
    let local = collider.shape_scaled().compute_local_aabb();
    // Avian's ColliderAabb is world-space; the shape's local AABB is what
    // the spread needs. Convert parry's mins/maxs to Vec3.
    // The rest is identical to the Rapier adapter.
}
```

Do not use Avian's world-space `ColliderAabb` for the spread: it changes
every frame as the body moves, loses the local orientation, and makes the
cover larger for rotated objects. Use the local shape AABB. If only a single
disc is needed, the world AABB center/radius is also a valid fallback.

### 7.6 Debug plugin

`src/debug.rs` currently draws one circle per `GrassInteractor`. It should
draw each spread disc, using the same `(transform, interactor, spread) ->
discs` helper as extraction. The debug feature must keep working with and
without the physics features; `GrassInteractorSpread` is in the core, so it
does.

### 7.7 Reflection and scenes

Register `GrassInteractorFromCollider` (when compiled) and
`GrassInteractorSpread` with `ReflectComponent` and `Default`. The adapter
component is a normal component, so a scene can spawn it next to a collider
and the system derives the rest on the next frame. `GrassInteractorSpread`'s
`Vec<Vec3>` reflects with `bevy_reflect`.

### 7.8 Ordering

With a local AABB, the adapter only reads collider data and writes
`GrassInteractor`; it does not need `GlobalTransform`. Run it in `Update`.
That is before `PostUpdate` extraction and is independent of whether physics
runs in `FixedUpdate`, `FixedPostUpdate`, `PostUpdate`, or a custom schedule.
A collider whose shape/scale changes during physics is therefore reflected
one frame later, which is fine for a footprint and avoids coupling the grass
plugin to another plugin's schedule.

If a future version uses world-space data, run it in `PostUpdate` after
`TransformSystems::Propagate` (and after Rapier's `PhysicsSet::Writeback` or
Avian's `PhysicsSystems::Writeback` when those plugins use their default
schedules). Document that ordering. Do not add a hard dependency on another
plugin's schedule for the local-AABB design.

## 8. Work breakdown

1. **Core footprint**
   - `GrassInteractorSpread` component, `covering_aabb` helper, extraction,
     debug, reflection, docs.
   - Tests for one-disc, grid, degenerate zero-size, scale/rotation, and
     disc-count cap.
2. **Rapier feature**
   - `Cargo.toml` optional dependency and feature.
   - `GrassInteractorFromCollider` component and Rapier sync system.
   - App-level test (spawn a `Collider` + adapter, run the system, assert the
     derived `GrassInteractor`).
3. **Examples and docs**
   - `examples/rapier.rs` behind `required-features = ["rapier"]`: a dynamic
     capsule on a plane, a static box/truck showing the spread, a field and
     a camera.
   - README section "Physics engines" with the two snippets and the
     "physics already writes `GlobalTransform`" explanation.
   - API table rows for the new components.
4. **CI**
   - Feature test/clippy/docs-docs.rs job.
   - MSRV check with `--features rapier`.
   - Keep `--all-features` only while every feature compiles; otherwise use
     an explicit `debug,rapier` list until Avian joins.
5. **Avian**
   - Wait for `avian3d` to declare Bevy 0.20. Add the same adapter, example
     and CI job. Do not ship a feature that cannot compile.

## 9. Testing

- Pure tests of `covering_aabb`: a small character AABB becomes one disc; a
  long box becomes a row/grid; a huge box hits the cap; zero extents do not
  panic or produce NaN.
- Extraction test: an entity with a spread produces multiple stamps at the
  transformed offsets; an entity without a spread produces one as today.
- Rapier adapter test with a minimal `App` (no renderer, no physics plugin):
  spawn `Collider` + `GrassInteractorFromCollider`, run the sync system,
  assert `radius`, `strength`, `max_height`, and spread offsets. Toggle
  `ColliderDisabled`/`Sensor` and assert `strength == 0`. Change
  `GrassSettings::max_interactor_radius` and assert the spread is rebuilt.
- A headless example/bench is not necessary; the interaction path is CPU
  data plus the existing trample shader. The existing golden images should
  not change because no shader or field path changes.
- Optional: a `--interactors` bench case with a spread-heavy object to show
  the cost is still one workgroup per disc.

## 10. Open questions

1. **Sensors.** Should a `Sensor` collider trample by default? Recommendation:
   no; `GrassInteractorFromCollider` sets strength to 0 for sensors. A user
   who wants a trigger volume to part grass can add `GrassInteractor`
   manually. Add an opt-in field only if someone asks.
2. **Automatic `max_height`.** A collider's height is a good default for
   "only trample while near the ground", but `GrassInteractor`'s default is
   `None` (any height). Keep `None` for consistency and document how to set
   `Some(height)`; consider an opt-in `Auto` enum later.
3. **One component or two per backend.** The engine-neutral
   `GrassInteractorFromCollider` is the best DX. If both engines are enabled
   and an entity has both colliders, one system wins; that is pathological.
   Backend-specific marker components are the safer but noisier alternative.
4. **Spread cap.** Pick a default (16 or 32 discs per entity) and warn once
   when a collider is too large. The alternative is to let
   `max_interactors` sort it out, which lets one wall starve a crowd.
5. **`GrassInteractorSpread` vs a shape enum.** A list of local disc centres
   keeps the renderer unchanged and is easy to test. A `Capsule`/`Box` enum
   in `GrassInteractor` would be more "declarative" but would need sampling
   in the hot extraction path and would change the existing public struct.
   The list is the smaller, more data-oriented step.
6. **Separate crates.** If Bevy/Rapier/Avian release schedules diverge
   enough that optional features hold back the core release, split the
   backend adapters into `bevy_grass_rapier` / `bevy_grass_avian` later.
   That is an additive change and does not have to be decided now.
7. **2D.** `avian2d`/`bevy_rapier2d` are out of scope. A 2.5D game can add
   `GrassInteractor` manually. If there is demand, a 2D adapter can map the
   XZ footprint from the 2D shapes.

## 11. What not to do

- Do not add a new interaction system, a vector displacement field, or a
  second trample texture. The per-disc stamp is the right model; the spread
  only feeds it more discs.
- Do not read physics `Position`/`Rotation` directly. `GlobalTransform` is
  the ECS boundary and already handles interpolation and custom schedules.
- Do not make the core crate depend on a physics engine. Optional features
  and cfg-gated modules, or separate crates, keep the core focused.
- Do not enable Rapier's default features. They pull in Bevy features the
  crate otherwise does not require.
- Do not auto-spawn proxy entities by default. A `Vec<Vec3>` in a component
  is simpler to reason about, inspect, test and serialize.
- Do not add an Avian feature before Avian can compile with Bevy 0.20. A
  broken feature is worse than waiting.
