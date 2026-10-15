# bevy_grass

GPU-driven, wind-animated tall grass for [Bevy](https://bevy.org) 0.20.

Status: **milestone M4 (complete look and Bevy passes)**. Blades are placed,
culled, given a level of detail and blown by the wind on the GPU every frame,
per camera; the CPU only describes the fields when they change. Grass is lit by
Bevy's own PBR functions, fogged, casts directional shadows, shows light
through blades seen against the sun, varies in colour by clump, and lies down
under anything with a `GrassInteractor`, leaving trails that recover.

It also takes part in the rest of Bevy's renderer: it is drawn into the depth,
normal and motion-vector prepass (so SSAO, TAA and anything else reading them
see it, with exact motion vectors for moving blades) and into the deferred
G-buffer. Styles can add a sheen, blend their roots into the ground or shade in
flat toon bands, and load from `.grass.ron` files that reload when saved.
`FarGrassMaterial` keeps grass colour on the ground past the last blade.

A style can also vary its grass the way real fields do: other plants among the
blades (seed heads, flowers, clover, broad leaves, as many species as you
list, each with its share), tufts and bare patches, blades that twist, droop
and fold, and colour patches, dry tips and dead blades. Each is a setting of
`GrassStyle`, off at 0 and free while no field uses it; the `WILD` presets use
them all. Editing comes in a later milestone; the design, the plan and measured results
are in [`bevy-grass-architecture-plan.md`](bevy-grass-architecture-plan.md).

```rust
use bevy::prelude::*;
use bevy_grass::prelude::*;

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, GrassPlugin::default()))
        .add_systems(Startup, setup)
        .run();
}

fn setup(mut commands: Commands, mut styles: ResMut<Assets<GrassStyle>>) {
    commands.spawn(GrassField {
        size: Vec2::splat(64.0),
        style: styles.add(GrassStyle::MEADOW),
        ..default()
    });
}
```

| Bevy | bevy_grass |
| --- | --- |
| 0.20 | 0.1 |

## Examples

| Example | Shows |
| --- | --- |
| `meadow` | Perspective flying camera, a density-mask clearing, two styles, and `FarGrassMaterial` ground past the grass radius (`--no-far`, `--no-fog` to compare) |
| `arpg_camera` | DEMO ARPG GAME's orthographic camera (30° pitch, 45° yaw, 14.64 m frame), a walking hero and six monsters trampling the grass |
| `styles` | The style browser: all 22 presets, loaded from `assets/grass_styles/*.grass.ron` (edit a file and it changes live); 1–4 pick the wind, S turns grass shadows off and on |
| `variation` | A wild style with each kind of variation switched on and off: P plants, T tufts and bare patches, B blade shape, C colour; 1–3 pick the style, `--arpg` DEMO ARPG GAME's camera |
| `passes` | Grass with SSAO (`--ssao`), TAA (`--taa`) and deferred rendering (`--deferred`), and the prepass textures themselves (`--show depth\|normals\|motion`) |
| `bench` | The reference benchmark: 2 × 2 km heightmap, fixed flythrough or DEMO ARPG GAME's camera (`--ortho`), `--interactors N`, `--sheen`/`--tint`/`--toon`, `--species`/`--tufts`/`--shape`/`--variation`, CSV per frame |
| `golden` | Every preset rendered headless and compared with `tests/golden/<renderer>/` (`--bless` to update); CI runs it on software renderers |

Every example takes the same measurement flags:

```text
cargo run --example arpg_camera -- --frames 600 --perf      # p50/p95 frame time, per-pass GPU time
cargo run --example meadow -- --fixed-time --screenshot shots/meadow.png
```

`--fixed-time` advances Bevy's clock exactly 1/60 s per frame, which makes
screenshots repeatable but frame times meaningless; do not combine it with a
measurement.

Correctness of the GPU placement against its CPU mirror needs a GPU, so that
test is opt-in: `cargo test -- --ignored gpu`.

## Styles as files

Every preset ships as a file in [`assets/grass_styles`](assets/grass_styles).
Copy one into your game's assets and load it like any asset:

```rust
let style: Handle<GrassStyle> = asset_server.load("grass/my_meadow.grass.ron");
```

Every field is optional; a missing one comes from `base`, a preset name, so a
variant lists only what differs:

```ron
(
    base: "AUTUMN_FADED",
    tip_color: (0.70, 0.72, 0.74), // sRGB
)
```

With Bevy's `file_watcher` feature, saving the file changes the grass in the
running game.

Other plants mix in through `species`: list as many as you want (up to 15
besides the style's own blades), each a style of its own with its share of
the blades and how much it grows in patches:

```ron
(
    base: "SUMMER_TALL",
    tufts: 0.3,          // gather blades into tufts, bare ground between
    droop: 0.25,         // let tall blades arch over
    patch_amount: 0.4,   // drift toward `patch_color` in patches
    species: [
        (share: 0.10, style: (base: "SEED_HEADS")),
        (share: 0.03, clustering: 0.8, style: (
            base: "WHITE_FLOWERS",
            head_color: (0.75, 0.55, 0.95),
        )),
    ],
)
```

| Setting | What it does |
| --- | --- |
| `species` | Other plants among the blades, each `(share, clustering, style)`. From a species' style only the blade's look is used: height, width, lean, stiffness, colours, root occlusion, translucency, specular and shape |
| `twist`, `droop`, `fold` | Blades twist along their length (turns at the tip), arch over and hang (0–1), fold along the midrib (0–1); each blade varies |
| `head`, `head_width`, `head_color` | A seed head or flower at the top, as a share of the length; it turns toward the camera |
| `patch_color`, `patch_amount`, `patch_size` | Large patches where the blades drift toward another colour |
| `dry_tips`, `dead_blades` | The tops of blades dry toward straw; a share of blades is dead |
| `tufts`, `tuft_size` | Blades gather into tufts and lean out from them; the blade count stays the same |
| `bare_patches`, `bare_patch_size` | A share of the ground is bare, the grass thinning toward each patch's edge |

The plants (`SEED_HEADS`, `WHITE_FLOWERS`, `YELLOW_FLOWERS`, `CLOVER`,
`BROAD_LEAF`) are presets too, so a field of nothing but clover works.

## Rendering paths

| Camera has | Grass is drawn into |
| --- | --- |
| Nothing extra | The main opaque pass |
| `DepthPrepass`, `NormalPrepass`, `MotionVectorPrepass` | The prepass too: SSAO, TAA, SSR and anything else reading it see grass |
| `DeferredPrepass`, with `DefaultOpaqueRendererMethod::deferred()` | The G-buffer, lit by Bevy's deferred lighting pass |

Deferred grass matches forward grass closely; translucency is approximated
less well (a backlit face's ambient light is dimmed too) and toon bands are
forward-only.

## Performance

[`docs/performance.md`](docs/performance.md) records every optimization tried,
what it gained and why, and how to measure: `tools/bench_ab.py` runs paired A/B
rounds of the benchmark. Read it before optimizing.

## License

MIT or Apache-2.0, at your option.
