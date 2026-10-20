# Testing

- **Unit tests and doctests.** `cargo test` covers settings and presets, the
  style file format, visibility and render layers, reflection (scene-style
  inserts and inspector-style edits), the sun and cascade warnings,
  statistics, trampling and footprints, the physics bridge, and incremental
  table updates: after every kind of change the incremental tables must equal
  tables built from scratch. Run it with `--features rapier`, `--features
  avian` or both to include the engine backends.
- **CPU mirror.** The GPU's placement, thinning, wind, blade curve and
  variation have pure-Rust twins with unit tests (density reached exactly,
  thinning only removes blades, species shares, blade length under bend).
- **GPU tests.** `cargo test -- --ignored gpu` reads blades back from a real
  GPU and compares them with the mirror. They also cover layers and
  visibility, runtime settings, two cameras at different settings, buffer
  overflow reporting, trample limits, the way a moving footprint lays the
  grass, and the tunables: the GPU against the CPU mirror at other distances
  and densities, blade rows, wind shaping and orthographic extents.
- **Golden images.** Every preset is rendered headless at a fixed time and
  compared with reference images in `tests/golden/`, on a hardware GPU and on
  two software renderers, WARP (Windows) and llvmpipe (Linux, in CI).
  `cargo run --example golden -- --bless` updates them.
- **240 Hz gate.** `tools/bench_240.py` runs High, Competitive and no-grass on
  a named reference machine and fails unless frame p95 is at most 4.167 ms.
  It is manual or nightly on self-hosted GPUs, because hosted CI runners
  cannot enforce frame times; see
  [The 240 Hz gate](performance.md#the-240-hz-gate).
- **Lints.** `unsafe_code` is forbidden; clippy's pedantic set and rustdoc's
  missing-docs check pass.
