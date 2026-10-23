# Porting to a new Bevy

Every place bevy_grass touches Bevy's renderer, and how to port it. A Bevy
minor release changes render internals nearly every time; the rest of the
crate (fields, styles, editing, trampling, wind, the physics bridges) uses
Bevy's ECS, assets and maths, which change far less. The list is short on
purpose: render code outside these files is a bug in the layering, and the
files here hide Bevy's details from the rest
([Production cleanup](proposals/prod-cleanup.md) has why).

## The touch points

| File | What of Bevy it uses | How it breaks |
| --- | --- | --- |
| `src/render/bevy_glue.rs` | Phase items and their keys (`Opaque3dBatchSetKey`, `OpaqueNoLightmap3dBatchSetKey`, `ShadowBatchSetKey` and the bin keys), `MeshPipeline::specialize`, the prepass's shader definitions, `DefaultOpaqueRendererMethod`, Bevy's view bind-group render commands, `ViewKeyCache`, `ViewKeyPrepassCache`, `ViewLightEntities`, `LightEntity` cascades | Key fields fail to compile, the good kind. The bind-group order, the prepass definitions and the deferred default compile and fail at runtime; each has a debug assertion or unit test (below) |
| `src/render/mod.rs` | The render app's scheduling: `RenderStartup` after `MeshPipelineSystems` and `init_prepass_pipeline`, `ExtractSchedule`, `RenderSystems` sets, and `compute::run_generate` in the `RenderGraph` schedule before `camera_driver`; `add_render_command` per phase | Renamed sets fail to compile. A reordered schedule compiles and runs the compute passes at the wrong time: blades a frame late, or none |
| `src/render/extract.rs` | `Extract`, `RenderEntity` (`extract_cameras` copies `GrassCamera` and `NoGrass`) | Compile errors |
| `src/render/resources.rs`, `view.rs`, `compute.rs` | Buffers, bind group layouts, `PipelineCache`, `ViewUniforms`, `ExtractedView`, `RenderContext`, render diagnostics | Mostly compile errors. `WindUniform` and the other `ShaderType`s are checked by the GPU tests |
| `src/render/platform.rs` | wgpu limits and downlevel flags | A limit renamed fails to compile; one whose meaning changes shows in the storage-buffer tests |
| `src/shaders/bevy_bridge.wesl` | Every Bevy shader import of the grass draws: the view, lights, `PbrInput`, `apply_pbr_lighting`, `main_pass_post_lighting_processing`, the G-buffer, the prepass bindings and conventions | Only at runtime: no grass and an `ERROR` in the log |
| `src/far.rs`, `src/shaders/far_ground.wesl` | `ExtendedMaterial<StandardMaterial>`, `AsBindGroup`, `pbr_input_from_standard_material`, Bevy's forward and prepass I/O | `far.rs` fails to compile; `far_ground.wesl` only at runtime |
| `src/gpu_layout.rs` | `ShaderType` for the field and chunk tables | A change to how `ShaderType` lays out a struct shows in the GPU tests |
| `src/sun.rs` | `DirectionalLight`, `CascadeShadowConfig`, `DirectionalLightShadowMap`: the built-in light adapter and the shadow-setup warnings | Compile errors |
| `src/edit.rs`, `src/ground.rs` | `Extent3d`, `TextureDimension`, `TextureFormat` to build `Image`s | Rarely; wgpu re-exports |
| `Cargo.toml` | The Bevy features, a strict subset of a game's | A feature renamed or split fails to resolve |

The checks that catch what compiles but is wrong:

- `main_pass_layout` asserts that `MeshPipeline::specialize` still builds
  `[view, binding array, mesh]`: grass binds Bevy's groups 0 and 1 with
  Bevy's own render commands and its own data as 2.
- `deferred_by_default` asserts the shape of `DefaultOpaqueRendererMethod`,
  which it reads through reflection; a unit test checks both answers.
- `prepass_shader_defs` copies the definitions Bevy's private
  `PrepassPipeline::specialize` (`bevy_pbr`, `prepass/mod.rs`) sets. Diff the
  two by hand: a definition Bevy renames compiles and is ignored.
- The golden check draws every preset and eight render paths (forward with
  and without MSAA, the prepasses with TAA, deferred, HDR with bloom,
  orthographic, no grass shadows) and fails on any `ERROR` in the log.

## The port, in order

1. Bump `bevy` in both `[dependencies]` and `[dev-dependencies]`, and the
   physics bridges to the versions built for it (`bevy_rapier3d`, `avian3d`;
   a breaking change, in the CHANGELOG).
2. `cargo build --all-targets --all-features` and fix the compile errors,
   which should all be in the files above. Read Bevy's migration guide for
   each.
3. Diff `prepass_shader_defs` against `PrepassPipeline::specialize`, and
   `bevy_bridge.wesl` against the Bevy shader functions it calls (`PbrInput`'s
   fields, `pbr_input_new`, the lighting and post-processing entry points,
   `deferred_gbuffer_from_pbr_input`, `previous_view_uniforms`).
4. `cargo test`, then `cargo test -- --ignored gpu`: the tripwires above and
   the GPU's blades against the CPU mirror.
5. `cargo run --example golden`, until it passes: no `ERROR` in the log and
   every image matches. A Bevy change to lighting, tonemapping or shadows can
   change the picture legitimately; re-bless only after a look at the
   differences (`target/golden/`), and say so in the CHANGELOG.
6. Run the examples that cover what the goldens do not: `passes` with
   `--ssao` and `--deferred`, `meadow`, `editing`, and the web demo
   (`python tools/web_demo.py smoke`).
7. Measure against the previous release with `tools/bench_ab.py`
   ([Performance notes](performance.md)) and run the 240 Hz gate before
   tagging ([Testing](testing.md)).
8. Update the versions named in this page and in the comments it lists
   (`bevy_pbr` 0.20 in `bevy_glue.rs`).
