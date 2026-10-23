# Test assets and harnesses

This directory holds reference data, not the test code itself:

- `golden/<renderer>/` — reference PNGs for the golden-image check. The
  renderer directory is part of the path because GPUs and software
  rasterizers round differently.
- The harness that renders and compares those images is
  [`examples/golden.rs`](../examples/golden.rs). It is an example so it can be
  run directly with a GPU:
  `cargo run --example golden`, or `cargo run --example golden -- --bless` to
  rewrite the images for the current renderer.

Other test layers live next to the code they test:

- Unit tests live in sibling `src/**/tests/mod.rs` modules (such as
  `src/settings/tests/mod.rs` next to `src/settings.rs`), plus
  `src/tests/mod.rs` for crate-level tests. The `tests/` folder, rather than a
  `tests.rs` file, keeps them out of the line count crates.io shows, which
  skips any path containing `tests/`.
- `src/render/gpu_test.rs`, with its suites under `src/render/gpu_test/`, is
  the opt-in GPU readback test (`cargo test -- --ignored gpu`).
- `examples/bench.rs` contains the benchmark assertions, run by CI with
  `cargo test --example bench`.

Keeping the golden images here lets them be versioned and picked up by the
CI artifact job without becoming Cargo integration tests.
