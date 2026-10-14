# bevy_grass

GPU-driven, wind-animated tall grass for [Bevy](https://bevy.org) 0.20.

Status: **milestone M3 (DEMO ARPG GAME v1)**. Blades are placed, culled, given a
level of detail and blown by the wind on the GPU every frame, per camera; the
CPU only describes the fields when they change. Grass is lit by Bevy's own PBR
functions, fogged, casts directional shadows, shows light through blades seen
against the sun, varies in colour by clump, and lies down under anything with a
`GrassInteractor`, leaving trails that recover. Editing and the remaining
shading come in later milestones; the design, the plan and measured results are
in [`bevy-grass-architecture-plan.md`](bevy-grass-architecture-plan.md).

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
| `meadow` | Perspective flying camera, a density-mask clearing, two styles |
| `arpg_camera` | DEMO ARPG GAME's orthographic camera (30° pitch, 45° yaw, 14.64 m frame), a walking hero and six monsters trampling the grass |
| `bench` | The reference benchmark: 2 × 2 km heightmap, fixed flythrough or DEMO ARPG GAME's camera (`--ortho`), `--interactors N`, CSV per frame |

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

## Performance

[`docs/performance.md`](docs/performance.md) records every optimization tried,
what it gained and why, and how to measure: `tools/bench_ab.py` runs paired A/B
rounds of the benchmark. Read it before optimizing.

## License

MIT or Apache-2.0, at your option.
