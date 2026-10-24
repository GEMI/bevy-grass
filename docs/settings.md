# Quality and settings

`GrassSettings` sets how far grass reaches, how dense it is, which blades cast
shadows, and how many blades and interactors each camera's buffers hold. It
has a preset per hardware tier, every setting can change at runtime, and
`GrassCamera` overrides some of them for one camera.

## Pick a quality tier

Start from a tier and change what you like with struct update syntax, the
same as for styles and wind:

```rust
app.add_plugins(GrassPlugins.set(GrassPlugin {
    settings: GrassSettings {
        shadows: GrassShadows::Off,
        max_interactors: 512,
        ..GrassSettings::MEDIUM
    },
    ..default()
}));
```

| Tier | Meant for | Grass radius | Blade capacity per camera | Density | Shadows |
| --- | --- | --- | --- | --- | --- |
| `Competitive` | 240 Hz and competitive games | 40 m | 100,000 | 50% | Off |
| `Low` | Steam Deck, integrated GPUs | 40 m | 80,000 | 60% | Off |
| `Medium` | GTX 1660 / RX 5600 | 70 m | 250,000 | 80% | Near |
| `High` (default) | RTX 3060 / RX 6700 | 100 m | 600,000 | 100% | Near |
| `Ultra` | RTX 4070 and up | 150 m | 1,200,000 | 100% | Full |

These are the `GrassSettings` constants `COMPETITIVE`, `LOW`, `MEDIUM`,
`HIGH` and `ULTRA`. `orthographic_lod` (the detail level orthographic cameras
use: 3-row blades, or 7-row at Ultra) and `max_interactors` are not shown, and
the [other settings](#tune-the-distances-the-blades-and-the-trampling) are
the same in every tier. For a graphics menu, `GrassQuality` names the tiers
(`GrassSettings::from(GrassQuality::High)`) and has a `const fn` for each
column: `settings()`, `radius()`, `instance_capacity()`, `density_scale()`,
`shadows()` and `orthographic_lod()`.

## Change settings at runtime

The settings are also a resource, so a graphics menu can change any of them
while the game runs:

```rust
fn apply_graphics_menu(mut grass: ResMut<GrassSettings>) {
    *grass = GrassSettings::LOW;
    grass.radius = 60.0; // Low, but reaching a little further
}

fn toggle_shadows(keys: Res<ButtonInput<KeyCode>>, mut grass: ResMut<GrassSettings>) {
    if keys.just_pressed(KeyCode::KeyG) {
        grass.shadows = match grass.shadows {
            GrassShadows::Off => GrassShadows::Near,
            _ => GrassShadows::Off,
        };
    }
}
```

Raising `instance_capacity`, `max_interactors` or `max_chunk_slices` makes each
camera allocate its buffers again, once. Values out of range are clamped, with
a warning that names the setting.

## Tune the distances, the blades and the trampling

Nothing about how far grass reaches, how it thins or how it is trampled is
fixed in the code. The tier presets set the first seven settings below; every
other setting is the same in all of them, and **its default is the number the
crate was designed and measured with**, so you only set one when your game is
not that game. A small-grass game seen from a few metres, a strategy map seen
from a kilometre, and a stylized low-poly meadow want different numbers:

```rust
app.insert_resource(GrassSettings {
    // A third-person camera that rarely looks far: full detail twice as far,
    // and no fade-out until nearly the edge.
    lod1_start: 0.4,
    lod2_start: 0.7,
    fade_start: 0.95,
    // Low-poly blades: 3, 2 and 1 rows (7, 5 and 3 vertices).
    blade_rows: [3, 2, 1],
    // Deep snow: trails take 6 s to fill in.
    trample_recovery: 6.0,
    ..GrassSettings::HIGH
});
```

The first seven are set by the tier:

| Setting | Default | Meaning |
| --- | --- | --- |
| `radius` | 100 m | How far grass reaches from a perspective camera (1 and up). Orthographic cameras ignore it |
| `density_scale` | 1 | Multiplies every style's density (0 and up) |
| `min_width_px` | 1 | Narrowest a blade is drawn, in pixels |
| `shadows` | `Near` | `Off`, `Near` (full-detail blades into the first shadow cascade) or `Full` (also mid-range blades into the second) |
| `orthographic_lod` | 1 | The level of detail orthographic cameras draw at, 0 to 2 |
| `instance_capacity` | 600,000 | Visible blades each camera's buffers hold (32 bytes each), up to 4,000,000 |
| `max_interactors` | 64 | Interactors a camera stamps a frame, up to 65,536 |
| `blade_rows` | `[7, 3, 1]` | Rows in a blade of LOD 0, 1 and 2, 1 to 31 each. A row is two vertices, plus one for the tip |
| `lod1_start`, `lod2_start` | 0.2, 0.5 | Where LOD 1 and LOD 2 begin, as shares of `radius` (0 to 1, in order) |
| `fade_start` | 0.9 | Where the last fade to nothing begins, as a share of `radius`; 1 ends abruptly |
| `lod_blend` | 4 m | Width of the dithered band around each LOD boundary |
| `lod2_density`, `fade_density` | 0.5, 0.15 | Share of blades kept where LOD 2 begins and where the fade begins (density falls in a line from 1 at `lod1_start`) |
| `mid_range_start` | 0.1 | Where the mid-range thinning begins, as a share of `radius` (0 to 1), independent of the levels of detail. Push it out toward `lod1_start` for denser grass close to the camera, at a price |
| `mid_range_density` | 0.5 | A further share kept from `mid_range_start` on; the survivors widen to cover. Raise it for lusher mid-distance at a price |
| `max_widening` | 2.5 | The most thinned blades widen, as a multiple of their width (1 to 3.2) |
| `turn_to_camera` | 0.4 | How far an edge-on blade turns toward the camera: 0 thins to lines, 1 always faces it |
| `orthographic_extent` | 256 m | Half the side of the square around an orthographic camera in which blades are stored (8 to 4,096). Blades further away are misplaced, so a map camera that sees a kilometre raises it; positions take 16 bits, so a bigger extent is coarser (8 mm at 256 m, 12 cm at 4,096 m) |
| `max_chunk_slices` | 8,192 | Slices of visible chunks a camera generates a frame (1 to 65,535; 16 bytes each). A very dense style or a long radius needs more; `GrassStats::gpu.slices` shows what is used |
| `readback_interval` | 120 | Frames between the GPU counter readbacks behind `GrassStats` and the overflow warnings; 0 never reads them |
| `trample_recovery` | 1.5 s | Time a fully flattened blade takes to spring back (0.01 and up) |
| `trample_cell_size` | 0.25 m | Cell of the trample window, 0.05 to 2. The window is 256 cells across: 64 m by default, 512 m at 2 |
| `max_interactor_radius` | 2 m | Largest interactor disc radius stamped, 0.1 to 16; the cost grows with its square. Footprints are not limited by it (they are clipped at 16 m per side) |

The wind has its own tunables; see [Wind](wind.md).

## Draw differently on one camera

Settings that are about a view live on the camera, as Bevy's own `Msaa` and
`Bloom` do. `GrassCamera` overrides any of `radius`, `density_scale`,
`min_width_px` and `shadows` for that camera; what it leaves at `None` follows
`GrassSettings`, including when that changes:

```rust
// A minimap at low detail beside a main camera at the settings' High.
commands.spawn((
    Camera3d::default(),
    GrassCamera {
        radius: Some(30.0),
        density_scale: Some(0.4),
        shadows: Some(GrassShadows::Off),
        ..default()
    },
));
```

A camera far above the ground, like an ARPG's, wants thicker blades: its
blades are thin on screen, so it asks for `min_width_px: Some(2.0)`. That is a
property of the camera, not of the grass, which is why it is not in
`GrassStyle`.

A perspective camera draws grass only within `radius` of the camera itself,
in 3D: a camera 150 m above the ground reaches none at High's 100 m, however
wide its blades. `min_width_px` helps blades that are drawn but thin, not
blades beyond the radius; a camera that zooms out that far wants
`radius_from_ground` ([Strategy cameras](#strategy-cameras)). When a camera
has had fields in view for two seconds and every one was out of reach, the
log says so, once, with the distance and the radius.

## Strategy cameras

A strategy or city-builder camera looks down on the grass from tens or
hundreds of metres, and zooms. Four settings make grass work there:

```rust
app.add_plugins(GrassPlugins.set(GrassPlugin {
    settings: GrassSettings {
        // The trample window follows the ground in view; 1 m cells make it
        // 256 m across instead of 64.
        trample_cell_size: 1.0,
        ..GrassSettings::HIGH
    },
    ..default()
}));

commands.spawn((
    Camera3d::default(),
    GrassCamera {
        // Grass reaches `radius` across the ground, at any height.
        radius_from_ground: true,
        // Blades at least 2 pixels wide, however far.
        min_width_px: Some(2.0),
        ..default()
    },
    // ...your camera controller, such as bevy_rts_camera's `RtsCamera`.
));
```

- **`radius_from_ground`** measures the radius from the ground below the
  camera: a camera `h` metres up draws grass out to `h + radius` metres, and
  thins it and lowers its detail over that distance as a camera standing on
  the ground would over `radius`. The ground in view grows with the square
  of the height, so the blades also thin by `(radius / (radius + h))²` and
  widen to cover: the number of blades stays about the same at any zoom,
  and so does the cost. `GrassCamera::density_scale` buys density back where
  the GPU allows.
- **`min_width_px`**: from 100 m a 1 cm blade is a tenth of a pixel. Two
  pixels keeps it visible.
- **`trample_cell_size`**: the trample window moves onto the ground the
  camera shows (see [Trampling](trampling.md)), but at 0.25 m cells it is
  64 m across, less than such a camera sees.
- **The ground.** Past the grass, `FarGrassMaterial` carries its colour and
  gusts ([Terrain](terrain.md#keep-the-grass-colour-past-the-last-blade)).
  Seen from above, the soil shows between the blades more than from the
  side: a soil close to the roots' colour keeps the field green.

[`examples/rts_camera.rs`](../examples/rts_camera.rs) is an 800 m map with
tanks under such a camera; F switches `radius_from_ground` off to compare.
Its costs on an NVIDIA T1200 laptop GPU at 1920 × 1080, High, `MEADOW` at
0.6 height, pitched 35° to 55° below the horizon:

| Camera height | Blades | Grass in the main pass |
| --- | --- | --- |
| 15 m | 63,000 | 1.9 ms |
| 40 m | 92,000 | 1.5 ms |
| 120 m | 85,000 | 0.5 ms |
| 200 m | 91,000 | 0.2 ms |
| 40 m, without `radius_from_ground` | 49,000, ending halfway up the screen | 0.8 ms |

From about 100 m a blade is a pixel or two tall and the field reads as
texture; that is what the far-grass material is for, and the blades fade
into it.

Every other 3D camera draws its own grass, with its own culling and level of
detail. To leave a camera out, or draw some fields on some cameras only, see
[Hide a field, or leave a camera out](terrain.md#hide-a-field-or-leave-a-camera-out).
