# Wind

The built-in wind has three layers: rolling gusts from a noise texture that
scrolls across the world, a sway of each blade at its own phase, and a small
flutter of near tips. The gusts sample the texture twice, the second sample
turned and larger, so they do not repeat in a grid even from a strategy
camera high above the field. `GrassWind` drives it, and the far-grass material
follows the same gusts.

## Pick and tune a wind

`GrassWind` has the presets `CALM`, `BREEZE` (the default), `GUSTY` and
`STORM`. Its fields are `direction` (XZ; any length, normalized on use;
`.toward(Dir2)` sets it from a `Dir2`), `strength` (0–1), `gust_scale`
(metres) and `gust_speed` (m/s), and four multipliers that are 1 in every
preset: `sway` (how far each blade sways on its own), `sway_speed` (how fast),
`flutter` (the shiver of the tips of near blades) and `gust_contrast` (how
much stronger the gusts are than the wind between them; 0 is a steady wind).

```rust
app.insert_resource(GrassWind {
    sway: 0.5,          // calmer individual blades
    gust_contrast: 2.0, // but deeper waves across the field
    ..GrassWind::BREEZE
});
```

There is one wind for the whole world, as there is one weather: every field
sways in the same gusts, so two fields that meet never disagree at the seam.
A field that should move less, in a courtyard, a canyon or a greenhouse,
gets a `GrassFieldOverrides { wind_scale: 0.3, ..default() }`: the same gusts
roll across it, weaker, and 0 holds it still. A field on a windswept ridge
can go above 1. A style can also be stiffer: `GrassBlade::stiffness` runs
from 0 (soft) to 1, and a blade at 1 bends a quarter as far as one at 0, and
sways faster.

It is a resource, so weather can change it at any time:

```rust
fn storm(mut wind: ResMut<GrassWind>) {
    *wind = GrassWind { direction: Vec2::new(1.0, 0.3), ..GrassWind::STORM };
}
```

## Bring your own wind

The built-in model is a plugin of its own, and the grass actually reads one
`GrassWindFrame` a frame. Turn the built-in model off and any wind source can
write that frame:

```rust
app.add_plugins(GrassPlugins.set(GrassPlugin {
    wind: false,
    ..default()
}));
// ...and in `Update`, in `GrassWindSystems::Sample`:
fn drive_wind(time: Res<Time>, weather: Res<Weather>, mut wind: ResMut<GrassWindFrame>) {
    wind.direction = weather.direction;
    wind.strength = weather.strength;
    // Roll the gusts along the wind at the weather's speed, in m/s.
    wind.advance(time.delta_secs(), weather.speed);
}
```

`GrassWindFrame` holds the values the shaders move blades by: direction,
strength, gust size and scroll (`gust_offset` and `previous_gust_offset`, for
motion vectors), sway, flutter and gust contrast. It is plain data, not the
GPU layout: the renderer copies it into its own uniform each frame, so a
change to the shaders does not change the type your source writes. They
are the grass shader's parameters, not a general description of wind: the
gusts are a noise texture scrolled along the wind, and
`GrassWindFrame::advance(delta, gust_speed)` scrolls it the way the built-in
model does, continuously when the speed or direction changes, so your source
does not have to. A source that scrolls the offsets itself wraps them at
256 repeats, not 1: the turned sample moves by whole repeats only at
multiples of 256. Put the
system in
`GrassWindSystems::Sample` so the blades and the far-grass material read the
same frame. The built-in model remains available by itself as
`GrassWindPlugin`; [`examples/custom_wind.rs`](../examples/custom_wind.rs) is
a complete scene driven this way.
