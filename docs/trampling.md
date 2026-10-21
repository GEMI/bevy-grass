# Trampling

Anything with a `GrassInteractor` flattens the grass around it, and the grass
springs back over about 1.5 s after it leaves, so moving things leave trails.
Trails are kept in world space, so they stay put while the camera moves. The
cost is the same for one interactor or 512.

## Let things trample the grass

```rust
commands.spawn((
    Transform::from_xyz(0.0, 0.0, 0.0),
    GrassInteractor { radius: 0.5, strength: 1.0, ..default() },
));
```

Grass within `radius` (0.6 m by default) lies down away from the entity.
`strength` (0–1) is how far it goes down.

The limits, which are also in the type's documentation. The numbers are the
defaults of `GrassSettings`, and each can be changed (see
[Quality and settings](settings.md#tune-the-distances-the-blades-and-the-trampling)):

- **Radius, 2 m** (`max_interactor_radius`). A bigger disc is cut to it, with
  one warning naming the entity. Use a `GrassInteractorFootprint` for one large
  object (below), which this limit does not apply to, or raise the setting (to
  16 m at most): the cost grows with the square of the radius.
- **Count** (`max_interactors`). A camera stamps that many, the ones nearest
  to it first.
- **Distance, 64 m** (`trample_cell_size`, 256 cells across). Each camera
  tracks a 64 × 64 m square of ground around what it looks at; interactors
  outside it do nothing. Cells of 0.5 m make the window 128 m.
- **Recovery, 1.5 s** (`trample_recovery`). How long flattened grass takes to
  stand again.
- **Height.** Interaction is horizontal, so a bird flying over the meadow
  flattens it like a person walking in it. Give the bird
  `max_height: Some(1.0)`: it flattens grass only within a metre of the
  ground, fading out over the upper half. (The ground is the highest ground in
  the 16 m chunk of the field below it, so on a steep heightmap a flier is
  judged against the top of the slope.)

## Large objects

A disc pushes grass away from its centre, which suits a character's feet. A
vehicle, a crate or a fallen log is one solid shape: give it a
`GrassInteractorFootprint`, a box in the entity's local space, and the grass
is flattened under the whole box at once.

```rust
commands.spawn((
    Transform::from_xyz(0.0, 1.0, 0.0),
    // A 6 × 3 m cart; `rounding` makes a ball or a capsule round from above.
    GrassInteractorFootprint::new(Vec3::ZERO, Vec3::new(3.0, 1.0, 1.5)),
));
```

The grass under a moving box lies down the way it moves, and only the grass
along its sides is pushed out sideways, the way a roller or a sledge lays it;
the trail behind keeps lying the way the box went. A box at rest pushes the
grass out from its nearest edge. The box follows the transform's rotation and
scale: a crate tipped onto its side, or a log that falls over, flattens where
it lies. It is one stamp on the GPU however large, clipped to 16 m from its
centre per side.

`GrassInteractorSpread` is there for a few separate discs (feet, wheels):
extra disc centres in the interactor's local space, each pushing away from its
own centre. `GrassInteractorSpread::covering_aabb` covers a local box with at
most 32 discs.

## Physics colliders: Rapier and Avian

Avian and Rapier both write `Transform`/`GlobalTransform`, and grass reads
`GlobalTransform`, so a hand-written `GrassInteractor` on a physics body works
with no bridge at all. `GrassInteractorFromCollider` goes further and sizes
the footprint from the collider. Turn on the feature for your engine:

| Engine | Feature | Built against |
| --- | --- | --- |
| Avian | `avian` | `avian3d` 0.8 |
| Rapier | `rapier` | `bevy_rapier3d` 0.37 |

```toml
bevy_grass = { version = "0.20.3", features = ["avian"] } # or "rapier"
```

Then put the component next to a collider:

```rust
// Avian
commands.spawn((
    RigidBody::Dynamic,
    Collider::capsule(0.4, 1.0),
    GrassInteractorFromCollider::default(),
));

// Rapier
commands.spawn((
    RigidBody::Dynamic,
    Collider::capsule_y(0.5, 0.4),
    GrassInteractorFromCollider::default(),
));
```

There is no plugin to add and nothing to order: `GrassPlugins` adds the bridge
for each feature that is on, and it runs in `Update`, independent of the
physics schedule and of interpolation. The footprint follows the collider:

- **Shape.** The collider becomes a `GrassInteractorFootprint`: a capsule
  character a disc, a vehicle, a wall or a big creature one box (see
  [Large objects](#large-objects)). Balls and capsules stay round from above.
- **Scale.** Both engines scale a collider by its transform, and so does the
  footprint: a 1 m crate scaled to 2.5 m flattens 2.5 m of grass.
- **How the body lies.** A log standing on end is a small disc; when it falls
  over, it flattens a capsule-shaped strip along its length. Moving, turning
  and tumbling cost nothing on the CPU: the footprint is in the body's local
  space and is laid on the ground every frame; it is refitted only when the
  collider's shape or the body's scale changes.
- **Whether it takes part.** A disabled collider, a disabled rigid body (for
  a child collider, its parent body) or a sensor flattens nothing. Removing
  the collider stops the interaction.

For a flying creature, set `max_height: Some(height)`; `scale` widens or
narrows the trail.

Put the component on the entity that has the `Collider`. For a body built
from several child colliders, put it on each child that should touch the
grass. If no collider size arrives within a few frames, bevy_grass warns once,
naming the entity. That is what a missing feature looks like, and also a
`bevy_rapier3d` or `avian3d` version other than the one in the table: Cargo
then builds two copies of the engine, and bevy_grass cannot see the game's
colliders.

Both features bring their engine in with `default-features = false`, so they
do not turn on `bevy_gizmos`, `bevy_picking` or other Bevy features, and both
can be on at once. They are 3D only; grass is 3D.

The [`avian`](../examples/avian.rs) and [`rapier`](../examples/rapier.rs)
examples show a character, a vehicle, a scaled crate and a thrown log;
[`avian_minimal`](../examples/avian_minimal.rs) and
[`rapier_minimal`](../examples/rapier_minimal.rs) are the shortest setup.

### Another engine, or none

The engine backends only write a `GrassColliderBounds`: the collider's box
along the entity's own axes, in metres with the scale applied, how rounded it
is, and whether it is active. Everything else is engine-neutral, so a collider
type of your own needs only a system that writes it, in
`GrassColliderSystems::Bounds` so it is fitted the same frame:

```rust
app.add_systems(Update, my_collider_bounds.in_set(GrassColliderSystems::Bounds));

fn my_collider_bounds(
    mut commands: Commands,
    added: Query<(Entity, &MyCollider), Added<MyCollider>>,
) {
    for (entity, collider) in &added {
        commands
            .entity(entity)
            .insert(GrassColliderBounds::new(Vec3::ZERO, collider.half_extents));
    }
}
```

Without any collider at all, a `GrassInteractorFootprint` does the same job by
hand.
