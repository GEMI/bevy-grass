# Editing and reading the grass

Grass can be cleared, mowed, grown and put back while the game runs: paths,
lawnmowers, burnt ground, a building's plot. The game can also read it back
on the CPU: how much grass grows at a point and how tall, for stealth,
footstep sounds and AI.

Every edit changes the field's density mask, the same mask a level designer
paints (red is density, green a height multiplier; see
[Fields, terrain and visibility](terrain.md)). Nothing new is drawn, so an
edited field costs the GPU what an unedited one does, and a game that edits
nothing pays nothing.

The `editing` example shows all of it:

```text
cargo run --release --example editing
```

## Clear, paint, mow and restore

Write a `GrassEdit` message from any system:

```rust
fn place_building(mut edits: MessageWriter<GrassEdit>, placed: Query<&Transform, Added<Building>>) {
    for transform in &placed {
        let yaw = transform.rotation.to_euler(EulerRot::YXZ).0;
        edits.write(GrassEdit::clear(GrassBrushShape::rect(
            transform.translation.xz(),
            Vec2::new(6.0, 4.0),
            yaw,
        )));
    }
}
```

| Constructor | Op | What it does |
| --- | --- | --- |
| `GrassEdit::clear(shape)` | `Density(0.0)` | Removes the grass: paths, plots, burnt ground |
| `GrassEdit::paint(shape, density)` | `Density(d)` | Grows grass, or thins it, to `d` (0 to 1 of the style's density) |
| `GrassEdit::mow(shape, height)` | `Mow(h)` | Cuts blades taller than `h` (a share of full height) down to it; shorter ones are left alone |
| `GrassEdit::new(shape, GrassEditOp::Height(h))` | `Height(h)` | Moves the height to `h`, up or down |
| `GrassEdit::restore(shape)` | `Restore` | Puts back what the field had before any edit: its own mask, or full grass |

The shapes are in world X and Z (`translation.xz()`):

- `GrassBrushShape::circle(center, radius)`
- `GrassBrushShape::stroke(from, to, radius)`: a path or a track, every point
  within `radius` of the segment.
- `GrassBrushShape::rect(center, half_size, yaw)`: turned as
  `Quat::from_rotation_y(yaw)` turns things, so a building's own yaw fits.

Each edit has three more settings:

- **`softness`**, 0.3 by default: the outer share of the shape that fades
  out. 0 is a hard edge. Thinning removes blades in a fixed order, so a soft
  edge thins evenly and never flickers.
- **`strength`**, 1 by default. At 1 the edit sets the grass to its target,
  fading to the grass as it was across the soft edge, and doing it again
  changes nothing: an edit sent every frame while the mouse drags paints a
  stroke the way a paint program does. Below 1, each edit moves the grass
  that share of the way, and repeated edits add up, for effects built up over
  time. Steps smaller than the mask's 1/255 still add up.
- **`field`**: one field only (`.on_field(entity)`). By default an edit
  changes every field under it, so overlapping fields are cleared together.

Edits are applied in `GrassSystems::Edit`, in the order they were written,
and drawn the same frame. A system in `Update` needs no ordering.

## Brushes: things that edit as they move

A `GrassBrush` on an entity edits wherever the entity goes:

```rust
// A mower: cuts a 1.2 m swath to a fifth of the grass's height.
commands.spawn((Transform::default(), GrassBrush::new(GrassEditOp::Mow(0.2), 0.6)));

// Villagers wear paths: each passing takes a little grass.
commands.spawn((
    Transform::default(),
    GrassBrush::new(GrassEditOp::Density(0.0), 0.4).with_rate(0.3),
));
```

Without a `rate` the brush works at once along the whole way it moved since
the last frame, so a fast vehicle leaves no gaps, and a brush that stands
still does nothing more. A brush moving further than 16 m in a frame was
teleported, and edits only where it lands.

With a `rate` (per second, 0 to 1 of the full range) it works on the disc
under it for as long as it stays there: 0.25 clears full grass in four
seconds. That is a path worn in by feet, or a fire burning in place.

### Box brushes: decks, tracks and blades

A `GrassBrushFootprint` makes the brush a rectangle instead of a disc, under
the entity and turned and scaled with it:

```rust
// A ride-on mower whose deck is 1.4 m wide (its X) and 0.8 m long.
commands.spawn((
    Transform::default(),
    GrassBrush::new(GrassEditOp::Mow(0.2), 0.0),
    GrassBrushFootprint::new(Vec2::new(0.7, 0.4)),
));
```

Working at once, it covers everything the rectangle slid over since the last
frame, so a fast vehicle leaves a clean swath as wide as the rectangle across
its path; with a rate, it works on the rectangle where it stands. The brush's
`radius` is not used, and `softness` is a share of the smaller half size. A
vehicle that should leave two tracks rather than one wide strip gets a child
entity per track, each with its own brush and footprint.

## Regrowth

`GrassRegrowth` on a field undoes its edits over time, back to the field's
own mask:

```rust
commands.spawn((
    GrassField::new(Vec2::splat(64.0), GrassStyle::LAWN, &mut styles),
    // Mowed lawn is back to full height in about a minute; paths never close.
    GrassRegrowth { density_rate: 0.0, height_rate: 0.015 },
));
```

Rates are per second, 0 to 1 of the full range. Regrowth only raises grass:
grass painted above the field's mask stays. It runs on Bevy's virtual clock,
so it pauses with the game.

## Read the grass

`GrassSampler` is a system parameter that reads the grass at a point, with
every edit:

```rust
fn hide_in_tall_grass(
    grass: GrassSampler,
    player: Single<(Entity, &Transform), With<Player>>,
    mut commands: Commands,
) {
    let (entity, transform) = *player;
    let cover = grass.sample(transform.translation.xz());
    if cover.is_some_and(|grass| grass.density > 0.5 && grass.height > 1.0) {
        commands.entity(entity).insert(Hidden);
    } else {
        commands.entity(entity).remove::<Hidden>();
    }
}
```

A `GrassSample` has:

| Field | Meaning |
| --- | --- |
| `field` | The field's entity: read its `GrassField` for the style, to pick a footstep sound |
| `density` | How much of the field's grass grows here, 0 to 1: the mask, edits and the style's bare patches; 0 where the ground has a hole (past a mesh's edge, or a height function's `NaN`) |
| `blades_per_m2` | The style's density (with `GrassFieldOverrides::density_scale`) times `density` |
| `height_scale` | The mask's height with edits: below 1 where it was mowed |
| `height` | A typical blade's height here, in metres |
| `tallest` | The tallest blade of any of the field's species here, in metres |

`sample` answers for the field with the tallest grass if several overlap;
`samples` gives every field there. A hidden field has no grass. The numbers
are what grows, not what a camera draws: they do not thin with distance or
quality, and they do not know about trampling. A sample costs about a
microsecond per field, whatever its size.

## Burning

There is no burn op: burning is a clear plus what the game shows. A fire is a
`GrassBrush` with a rate (or edits of strength below 1, spreading as the
game's fire spreads), and the flames, smoke and embers are the game's
particles. For charred stubble, lay a second field of
`GrassStyle::BURNT_REGROWTH` over the first, cleared everywhere at the start,
and paint it in where the fire clears the living grass:

```rust
edits.write(GrassEdit::clear(burning).on_field(meadow));
edits.write(GrassEdit::paint(burning, 1.0).on_field(stubble));
```

## Saving

`GrassSampler::mask_image(field)` returns a field's mask as it is now, edits
and all, as an RGBA image. Save it with the game; on loading, make it the
field's `density_map`. Keep the original mask's handle too if the field
regrows: regrowth returns to whatever mask the field has, so with the saved
mask as its `density_map` the grass would not grow back. The same image is
how a level designer keeps what was painted in the running game.

## Limits

- **Resolution.** An edit is as fine as the mask. A field without a mask gets
  one on its first edit, of 0.25 m texels (at most 2048 per side, 8 MB, so a
  field over 512 m gets coarser ones). That first edit rebuilds the field
  tables once; give the field a mask of your own, even a blank one, to choose
  the resolution and skip the rebuild.
- **A new mask starts again.** Changing a field's `density_map`, or modifying
  or reloading that image, drops the field's edits and starts from the new
  mask. Despawning the field drops them too.
- **Edits wait for their mask.** An edit for a field whose `density_map` has
  not loaded waits for it, in order, so clearing ground under buildings at
  level load works. `GrassStats::edits.waiting` counts them.
- **`FarGrassMaterial`** keeps the grass colour on ground beyond the blades;
  it does not follow edits.

## Cost

Measured in release on the development laptop (i7-11850H), on a 1 km field
with a 2048 × 2048 mask:

| Work | CPU per frame | Uploaded per frame |
| --- | --- | --- |
| Two 4 m wide strokes a frame | 0.004 ms | about 100 bytes |
| Two 16 m wide strokes a frame | 0.03 ms | about 570 bytes |
| Twenty 4 m wide strokes a frame | 0.03 ms | about 900 bytes |
| Regrowing a cleared disc 200 m across | 0.09 ms | the rows it changed |

Only the rows an edit changed are written to the GPU, into the buffer the
shaders already read; nothing is read back. Regrowth visits at most 8192
texels a frame over all fields, so a large area regrows in coarser steps
rather than costing more. `GrassStats::edits` has the running totals and
this frame's time.
