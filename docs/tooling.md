# Stats, debugging and tooling

## See what grass is doing

"Why is there no grass here?" and "why did that frame spike?" can be answered
without a profiler.

**`GrassStats`** is a resource with the numbers: blades per level of detail,
visible chunk slices, buffer overflows, table rebuilds and how long the last
one took, and fields or species a limit left out. The GPU numbers are read
back every 120 frames (`GrassSettings::readback_interval`; 0 turns the
readback off), summed over cameras.

```rust
fn hud(stats: Res<GrassStats>) {
    if stats.gpu.overflows > 0 {
        warn!("grass buffers overflowed: raise GrassSettings::instance_capacity");
    }
    if stats.tables.fields_dropped > 0 {
        warn!("{} fields are not drawn: the tables hold 256 entries", stats.tables.fields_dropped);
    }
}
```

The same numbers are Bevy diagnostics, for `LogDiagnosticsPlugin` or an
overlay: `grass/blades/near`, `/mid`, `/far`, `grass/visible_slices`,
`grass/overflows`, `grass/table_rebuilds` and `grass/rebuild_time` (ms), as
the constants of `GrassDiagnostics`.

**`GrassDebugPlugin`** draws gizmos for what grass has: every field's
rectangle (grey while hidden, so scale, rotation and visibility problems
show), the 16 m chunks, each interactor's disc (red where a radius over the
limit is cut off), each footprint's rounded rectangle, and each camera's
trample window. All follow the settings. Toggle parts with the `GrassDebug`
resource; `enabled` is the master switch (the `meadow` example binds it to
F3). It needs the `debug` cargo feature, which turns on Bevy's `bevy_gizmos`,
so adding this crate never changes how Bevy is compiled:

```toml
bevy_grass = { version = "0.20.5", features = ["debug"] }
```

## Scenes, inspectors and save files

Every public type is reflected, with the type data tools look for: every
component has `ReflectComponent`, `GrassWind` and `GrassSettings` have
`ReflectResource`, and `GrassStyle` has `ReflectAsset` (the plugin calls
`register_asset_reflect`). So fields can be spawned from `.scn.ron` scenes,
BSN or Blender exporters such as Skein and Blenvy; `GrassWind`,
`GrassSettings` and styles show up in the resource and asset panels of
inspectors such as `bevy-inspector-egui`; and a designer can tune a meadow
live there instead of editing RON. A reflected `GrassField` takes its style
from a handle, so give it one in code after loading the scene.

## Order your systems against the grass

Fields, styles and images are turned into GPU tables only when they change.
Three public system sets in `PostUpdate`, `GrassSystems::DetectChanges`,
`GrassSystems::Edit` and `GrassSystems::Rebuild`, run after Bevy has
propagated transforms and visibility and sent asset events. Order your own
systems against them when a change must show this frame, for procedural
terrain or runtime painting:

```rust
// Moving or showing fields: before DetectChanges.
app.add_systems(PostUpdate, stream_in_the_level.before(GrassSystems::DetectChanges));
// Writing `GrassEdit`s: before Edit. Reading `GrassSampler`: after it, to
// see this frame's edits.
app.add_systems(PostUpdate, (clear_under_buildings.before(GrassSystems::Edit),
    footstep_sounds.after(GrassSystems::Edit)));
// Editing a density mask, a heightmap or a style: before Bevy sends the
// asset events.
app.add_systems(PostUpdate, paint_density_mask.before(AssetEventSystems));
```

Systems in `Update` need no ordering. Without any, a change shows the next
frame. The collider bridge runs in `Update` itself: a system that writes
`GrassColliderBounds` goes in `GrassColliderSystems::Bounds` (see
[Another engine, or none](trampling.md#another-engine-or-none)).
