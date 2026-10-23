---
name: bevy-grass-style
description: House rules for changing bevy_grass. Load before writing or editing any .rs or .wesl file, Cargo.toml, example, doc page or CHANGELOG entry, and before any performance work, so the change keeps the Rust and WESL twins in step, follows the crate's doc, failure, test and dependency rules, is measured the way this repo measures, and does not "fix" the documented deviations from generic Rust advice.
---

# bevy_grass house rules

One crate: a Bevy 0.20 renderer for grass and foliage (WESL shaders in
`src/shaders/`) with a CPU twin of the shaders. `rust-version = 1.97.1`,
toolchain `1.98.1`, `edition = 2024`. These rules describe what the crate
already does. Where they disagree with generic Rust or API advice, they win
until a human changes them here; [Deliberate deviations](#deliberate-deviations)
lists the generic rules that are waived, so you do not "fix" them.

This file is shared: Claude Code reads it from `.claude/skills/`, and Pi reads
the same file through `.pi/settings.json`. Edit it here only.

## Before you write

1. Read the module you are changing, its `tests/mod.rs` neighbour, and, for
   anything the GPU does, its WESL twin (see [Twins](#rust--wesl-twins)).
2. Read the `docs/` page that owns the feature. It is the spec: `api.md`
   (items), `design.md` (why it is fast), `terrain.md`, `styles.md`,
   `settings.md`, `editing.md`, `trampling.md`, `wind.md`, `rendering.md`,
   `presets.md`, `tooling.md`, `performance.md`, `testing.md`, `roadmap.md`.
3. Search before adding a helper (`rg 'fn name' src`). Table rebuilds, packing
   and the CPU models already exist; two copies drift.
4. Check Bevy APIs against the 0.20 source in
   `~/.cargo/registry/src/*/bevy_*-0.20.*`, not memory. Render internals
   change between Bevy releases, and examples from other versions are wrong.
5. If the request has two readings, say so instead of picking silently, and
   state the tradeoff before a non-trivial design.

Touch only what the task needs. Do not reformat, reword or "improve" nearby
code and comments; unrelated cleanups belong in their own change.

## Lints

`Cargo.toml` holds the table (it is the consuming game's), and
`python tools/lint.py` checks it with `-D warnings` for every feature set. CI
and a Claude Code Stop hook run it too, so formatting, `missing_docs` and
unused code are caught for you. The judgement calls are yours:

- `unsafe_code = "forbid"`. Never add `unsafe` or an override for it. Reach
  the GPU through Bevy's safe API; `bytemuck::cast_slice` is the only reason
  bytemuck is a dependency.
- No blanket `allow`. A targeted override is
  `#[expect(lint, reason = "...")]`, and the reason names the tradeoff
  (`clippy::type_complexity` on Bevy queries, `manual_midpoint` in
  `src/shader_model.rs`). Use `#[allow(lint, reason = "...")]` only where an
  `expect` cannot be fulfilled everywhere it compiles, such as
  `examples/common/mod.rs`, whose helpers each example uses a different
  subset of.
- `cast_*`, `float_cmp`, `needless_pass_by_value` and `struct_excessive_bools`
  are allowed crate-wide for graphics maths, exact float comparisons and
  Bevy's by-value system parameters. Do not extend that list without a reason
  next to it in the table.
- `lint.py` checks the library alone per feature set because tests and
  examples turn on `debug`: code only `debug` or tests use looks used to
  `--all-targets` and dead to a game. Gate it with `#[cfg(...)]`, not an
  `allow(dead_code)`.

## Rust ↔ WESL twins

The GPU decides where blades grow and how they move; Rust has to reproduce it
exactly for tests, for `GrassSampler`, and for the CPU-side tables. Two
modules hold the Rust side:

- `src/shader_model.rs` — the runtime CPU model: hashes, the blue-noise tile,
  chunk layout, distance LOD, the clump, tuft and gust tables.
- `src/mirror.rs` — `#[cfg(test)]` only: the oracle for placement, thinning,
  wind, the blade curve, species and variation, which the unit tests and the
  GPU readback test compare against.

Rules:

- A twin is marked on both sides: `/// Twin of \`mirror::hash\`.` in WESL,
  "The WESL twin is `TRAMPLE_TEXELS`" in Rust. A new twin gets both markers.
  A marker names the module the twin lives in now (`shader_model::hash`, not
  `mirror::hash`) and the function's current name; moving or renaming a twin
  updates the markers that point at it (`rg 'twin' src`).
- Change one side and you change the other in the same commit: functions,
  constants (`clock::TIME_PERIOD` ↔ `TIME_PERIOD`, `trample::TEXELS` ↔
  `TRAMPLE_TEXELS`) and data layouts.
- Keep the arithmetic identical, including the order of operations and
  integer wrapping. Do not let clippy or a "cleaner" formula rewrite the Rust
  side; that is what the `manual_midpoint` expects are for.
- A `ShaderType` struct that says "Mirrors `Frame` in `common.wesl`" has the
  same fields in the same order and types as the WESL struct. Add a field to
  both, or the uniform silently reads garbage.

## WESL shaders

- Each file opens with `//! bevy_grass::shaders::<name> — <what it does>` and
  says which passes use it. Keep that header true.
- A library module is registered with `load_shader_library!`, an entry-point
  shader with `embedded_asset!`, in the `build` of the plugin that uses it:
  `GrassPlugin` (`src/lib.rs`) for the grass passes, `FarGrassPlugin`
  (`src/far.rs`) for `far_ground.wesl`. A new file that is not registered
  fails only at runtime.
- `common.wesl` holds layouts and pure functions only, no bindings.
- `bevy_bridge.wesl` is the only grass shader that imports `bevy_pbr` or
  `bevy_render`; the others reach Bevy's view, lights, `PbrInput`, G-buffer
  and prepass conventions through its functions, so a Bevy port edits one
  shader. `far_ground.wesl`, a `StandardMaterial` extension, is the one
  exception. Add what a shader needs from Bevy to the bridge, in grass's
  terms, rather than importing it.
- Shaders compile when the pipeline is created, not at `cargo build`. A broken
  shader draws no grass, logs an `ERROR` and otherwise looks like a large
  speed-up. After a shader change, run an example and read the log, then the
  GPU tests (`cargo test -- --ignored gpu`) and the goldens.

## Documentation

- Public types, functions, methods and consts: one summary line (under ~15
  words), a blank line, then the extended explanation. The summary is what
  docs.rs shows in its item lists. `GrassStyle::from_ron` / `to_ron` in
  `src/style_file.rs` are the shape to copy. Field docs never appear in those
  lists and may open with a longer paragraph (`GrassSettings`' fields do).
- `# Errors` on every fallible public function, naming the conditions;
  `# Panics` on anything that can panic, saying why it cannot in practice. No
  boilerplate sections for conditions that cannot happen.
- Examples are doctests: `no_run` when they need a window, a GPU or an asset
  server, never `ignore`.
- Module docs (`//!`) say why the module exists and what cost it avoids, not
  what it contains. `src/packed.rs` and `src/lib.rs` are the models.
- Comments explain why. A comment that restates the code is noise. A
  hardcoded number (cell size, capacity, readback interval) gets a named
  constant or a comment saying why that value and what breaks if it changes.
- Prose is British: colour, centre, behaviour, grey. Identifiers follow Bevy
  (`Color`, `center`). Do not respell either.
- No compliance markers such as `// Rust guideline compliant <date>`.

## Public API

- Public components, resources and assets derive `Reflect` with
  `#[reflect(Component)]` / `#[reflect(Resource)]` / `ReflectAsset`, are
  registered, and get a row in `docs/api.md` (or the docs page `api.md`
  points to, such as `terrain.md` for far grass). A public item missing any
  of the three is unfinished.
- Register in `register_types` (`src/lib.rs`) when `GrassPlugin` owns the
  type, or in the `build` of the plugin that adds it when that plugin can be
  left out or is feature-gated (`GrassDebugPlugin`, `GrassDiagnosticsPlugin`).
  An asset needs `register_asset_reflect` as well as `register_type`; Bevy's
  `MaterialPlugin` registers neither for a material.
- Every public item is re-exported by name from the crate root. The prelude
  (`pub mod prelude`, `src/lib.rs`) holds what a game names in its own code:
  plugins, components, resources, assets, settings, messages and system sets.
  Error types, asset loaders, sub-structs of a resource (`TableStats`,
  `GpuStats`, `EditStats`), GPU data (`FarGrassUniform`), gizmo groups and
  diagnostic paths stay at the root only. No glob re-exports, no third
  import path, no per-module prelude.
- `Debug` on all public types; `Clone`, `Copy`, `PartialEq`, `Default` where
  they make sense. Components use `#[require(...)]` for what they need
  (`GrassField` requires `Transform` and `Visibility`).
- Public fields plus `Default` (and named consts such as `GrassStyle::MEADOW`
  or `GrassSettings::ULTRA`) are preferred over builders. Consuming `with_*`
  setters next to public fields are fine for a value usually written inline
  (`GrassStyle::with_density`, `GrassEdit::with_strength`,
  `FarGrass::with_fade`); they never replace the field. A constructor that
  does real work stays inherent (`GrassField::new`, `FarGrass::new`), and one
  that enforces an invariant returns `Result`.

## Failure policy

- Library code returns `Result`; a panic means a bug in the caller or in us.
  `GrassStyleFileError` and `GrassStyle::from_ron` are the pattern.
- `.expect("a sentence saying why this cannot fail")` is acceptable where the
  invariant is local and checked by construction (`src/style_file.rs`,
  `src/render/bevy_glue.rs`). No bare `unwrap()` or `panic!()` outside tests
  and `src/render/gpu_test/`.
- Validate at the edge and keep rendering: clamp in the render world and
  report through `GrassSettings::problems()` + `warn_about_settings`
  (`src/lib.rs`). Never panic a game over a setting.
- No `println!`, `dbg!` or `eprintln!` in library code. Use `warn!` /
  `info!`, start the message with `bevy_grass: `, name the type and entity,
  say what to do instead, and log once per change, not per frame.

## Tests

- Unit tests live in `src/<module>/tests/mod.rs` next to the module, never
  in a `tests.rs` file: crates.io counts lines of code in any path without
  `tests/` in it, so a `tests.rs` inflates the crate's reported size. A
  single-file module with no folder of its own may keep a short inline
  `#[cfg(test)] mod tests` instead (`src/shader_model.rs`,
  `src/wind/clock.rs`); once it grows past a screen, move it to a
  `tests/mod.rs`. `tests/` holds golden PNGs and its README only, and GPU
  harnesses are examples (`examples/golden.rs`, `examples/bench.rs`);
  `tests/README.md` says why.
- New behaviour needs a test that fails without the change. A change to a
  twinned function also tests the twin (see above), and a change to the GPU
  path runs the readback suite: `cargo test -- --ignored gpu`.
- Incremental table updates must equal a rebuild from scratch after every
  kind of change; existing tests assert this, keep them passing and extend
  them for a new kind of change.
- Golden images are blessed for a named renderer only
  (`cargo run --example golden -- --bless`), never hand-edited or copied
  between renderer directories, and only after the user has accepted the new
  look (see [Changes to the look](#changes-to-the-look)).

## Performance

Measured, never argued. Before proposing an idea, read
`docs/performance.md`: most obvious ideas are already there, kept or
rejected, with numbers.

- Measure with `tools/bench_ab.py`: build each variant
  (`cargo build --example bench`), `snapshot` it, then `run` interleaved
  paired rounds (add `nograss` for the baseline) and `report` against the
  reference. About 4 rounds resolve 5%; a pass is roughly 10 minutes.
- Judge as a share of grass cost (reference minus `--no-grass`), at p50 and
  p95, in both scenes (perspective and the ARPG camera).
- The keep bar is set by the user per pass (it has been 10%, 5% and 3%). Ask,
  or reuse the latest one they gave. Keep only what clears it, revert the
  rest.
- One idea per variant. Do not build while a bench runs. Compare pass times
  only within one run; they drift between sessions.
- Record every pass, kept or rejected, as a section in
  `docs/performance.md`.
- The frame-time gate is `tools/bench_240.py` on a machine from
  `tools/reference-machines.json`; it cannot run in hosted CI.

Code shape for hot paths:

- Per-frame systems allocate nothing: reuse buffers, share packed data as
  `Arc<Vec<_>>`, preallocate with `with_capacity`. Per-frame results must not
  depend on `HashMap` iteration order.
- Rebuild tables on change, not per frame (`GrassSystems::DetectChanges`,
  then `Edit`, then `Rebuild`). A style-only change must not re-read masks or
  heightmaps.

## A new style or look term

A new look term (a field of `GrassStyle` or `GrassBlade` the shaders read)
touches, in one commit:

1. `src/style.rs`: the field, its doc, its value in `Default` and in every
   preset that should use it (`src/presets/`).
2. `src/style_file.rs`: the field in the macro's field list, with an alias
   if it replaces an old name, so `.grass.ron` files keep loading.
3. `src/gpu_layout.rs`: where it goes in `FieldGpu` (a free component of an
   existing `Vec4` before a new one), filled in `fields.rs`.
4. `src/shaders/common.wesl`: the same slot in `Field`.
5. `src/shaders/look.wesl` (or `blade.wesl` for geometry): the term, behind a
   `GRASS_*` definition and a `LookFeatures` bit if it is optional.
6. `src/mirror.rs`: the CPU twin, if the term changes placement, shape or
   colour that a test reads back.
7. `docs/styles.md`, `CHANGELOG.md` (with `..default()` for literals) and a
   golden re-bless once the look is accepted.

## Changes to the look

A change that alters pixels is the user's decision, even when it is faster or
passes the perf bar (the per-vertex sun shadow passed and was rejected as
looking cheap). Show before/after crops and the golden diff numbers, then
wait. Only after a yes: re-bless the goldens, and if a README preset changed
visibly, re-record the film (`python tools/record_walk.py`, keep it under
about 7.7 MB).

## Dependencies and features

- A new dependency needs a comment in `Cargo.toml` saying why it is already in
  Bevy's tree or why nothing smaller will do, a licence `deny.toml` allows,
  and `cargo machete` staying clean.
- Bevy features stay a strict subset of the consuming game's, so adding this
  crate never changes how Bevy is compiled. Never turn one on for
  convenience; examples get theirs from `[dev-dependencies]`.
- Features are additive. `debug` may only add `bevy/bevy_gizmos`.
- The physics bridges (`rapier`, `avian`) must match the game's engine
  version and keep `default-features = false`. Bumping one is a breaking
  change and goes in the CHANGELOG as such.

## Same change, same commit

A user-visible change updates, together:

- the `docs/` page that describes it (and `docs/api.md` for a new item),
- `CHANGELOG.md`, newest first, saying what to write instead for a breaking
  change,
- the prelude and `Reflect` registration, and an example if it is new API,
- both sides of any twin it touches,
- a test that would fail without the change.

## Editing files from scripts

On this Windows machine Python defaults to CRLF and cp1252. Open files with
`open(p, encoding="utf-8", newline="")`, and check for CR with Python on raw
bytes (`b"\r" in open(p, "rb").read()`), not `grep`. `.gitattributes` forces
LF for Rust, WESL, Markdown and TOML. Do not write Rust string literals that
continue with a trailing `\` from a non-raw Python string.

## Verify before saying done

The same steps as `.github/workflows/ci.yml`:

```bash
cargo fmt --check
python tools/lint.py                      # clippy -D warnings, every feature set
cargo test
cargo test --features rapier
cargo test --features avian
cargo test --example bench
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features
cargo +1.97.1 check --all-targets --all-features    # MSRV
typos && cargo machete && cargo deny check
```

When it applies, also run `python -m py_compile` on a changed `tools/` script,
`cargo test -- --ignored gpu` for a GPU-path change, and
`cargo run --example golden` for anything that can change pixels. CI runs
`cargo semver-checks` on pull requests; a public API change that breaks it
needs a CHANGELOG entry saying what to write instead.

Report anything you could not run (GPU suites, goldens, the 240 Hz gate)
instead of claiming it passes.

## Deliberate deviations

These are choices, not oversights. Do not "correct" them:

- **`prelude` exists.** It is how Bevy users import a plugin crate;
  `docs/api.md` calls it the front door.
- **Bevy types in public signatures** (`Vec2`, `Handle<T>`, `Assets<T>`,
  `Color`). Wrapping them would fight the engine and break reflection,
  scenes and inspectors.
- **Unit tests under `src/`**, GPU harnesses as `examples/`, for the reasons
  in `tests/README.md`.
- **Prose-heavy rustdoc and `docs/`.** The measured "why" is the value: the
  crate's selling point is the numbers in `docs/design.md` and
  `docs/performance.md`. Do not enable `too_long_first_doc_paragraph`.
- **Exact float comparisons and `as` casts** in the maths and twins: the CPU
  side must match the GPU bit for bit.
- **No `unsafe`.** `unsafe_code = "forbid"`, so Miri has nothing to check.
- **Bevy features coupled to the consuming game's build**, as documented in
  `Cargo.toml`.
