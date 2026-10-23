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
- **Storage-buffer budget.** A unit test counts the storage buffers in every
  grass pipeline's layouts: at most 10 per shader stage, none in the draws'
  fragment stage. The GPU tests run on a device lowered to 10, and
  `cargo run --example golden -- --max-storage-buffers 10` renders the
  goldens on one.
- **Golden images.** Every preset is rendered headless at a fixed time and
  compared with reference images in `tests/golden/`, on a hardware GPU and on
  two software renderers, WARP (Windows) and llvmpipe (Linux, in CI).
  `cargo run --example golden -- --bless` updates them. The same run draws
  `MEADOW` through eight render paths (`path_*.png`): forward with and
  without MSAA, the depth prepass, every prepass with TAA, deferred, HDR
  with bloom, an orthographic camera, and a camera without grass shadows.
  These are the paths a Bevy release breaks first, and a broken shader or
  pipeline there draws nothing and logs an `ERROR` rather than failing, so
  the run also fails on any `ERROR` in the log. Metal has no
  references of its own: on an Apple M2 Pro every preset came within a mean
  PSNR of 40.5 dB of WARP's (lowest 29.14 dB, `elephant_grass`), and the
  unit, doc and GPU tests pass there.
- **240 Hz gate.** `tools/bench_240.py` runs High, Competitive and no-grass on
  a named reference machine and fails unless frame p95 is at most 4.167 ms.
  It is manual or nightly on self-hosted GPUs, because hosted CI runners
  cannot enforce frame times; see
  [The 240 Hz gate](performance.md#the-240-hz-gate).
- **Browser (WebGPU).** CI checks that the library and examples build for
  `wasm32-unknown-unknown`; it cannot run them, because hosted runners have
  no WebGPU. `python tools/web_example.py` builds `minimal` and `meadow`
  (or the examples named) and serves them on localhost. Each page shows a
  report of the browser's limits, its errors and the frame time, with a
  button that copies it as JSON. The examples' flags go in the query string:
  `meadow/?no-grass` is the same scene without grass. The browsers run so
  far are listed in [Running in the browser](web.md#browsers).
- **Web demo smoke test.** The hosted demo (`web/`, published to GitHub
  Pages by `.github/workflows/pages.yml`) has its own check, run by hand
  since hosted runners have no WebGPU. `python tools/web_demo.py smoke`
  builds the demo, opens it with `?smoke` in Edge, Chrome or Brave on a fresh
  profile, and passes if every quality tier drew blades and the page logged
  no error; it prints the adapter, its storage-buffer limit and each tier's
  frame time. For any other browser or a phone, `python tools/web_demo.py
  serve` and open `http://localhost:8080/?smoke`: the page posts the same
  report back, and the terminal prints and saves it under
  `target/web-smoke/`. A shader the browser rejects is logged with the
  compiler's reason and line. The demo also runs natively:
  `cargo run -p bevy_grass_web --release`.
- **Weaker devices.** `GrassSupport`'s check has unit tests against wgpu's
  WebGL2 limits and against too few storage buffers. An example run with
  `--max-storage-buffers 8` asks the GPU for no more than a weaker device or
  browser offers: it must log one `bevy_grass` warning, draw no grass and
  raise no wgpu error.
- **Lints.** `unsafe_code` is forbidden; clippy's pedantic set and rustdoc's
  missing-docs check pass.
