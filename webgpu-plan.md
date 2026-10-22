# bevy_grass WebGPU/WASM migration plan

Oct 22, 2026 · @Gediminas

## Verdict

The plan is realistic and mostly correct. Its estimate of 1.5 to 2.5 weeks holds. The direction is right: one portable GPU layout, no second renderer, and no `cfg(wasm32)` in the render code. Three things change the plan:

- **We target 10 storage buffers per stage, not the spec's 8 (decided 2026-10-22).** Bevy 0.20 asks for the adapter's full limits. 98% of WebGPU devices report 10 or more, and almost none report exactly 8 ([web3dsurvey](https://web3dsurvey.com/webgpu/limits/maxStorageBuffersPerShaderStage)). Today's 13 and 14 fail on the 20% between 10 and 15: most of Chrome on macOS, Samsung Internet and some of Firefox. Devices below 10 get one warning and no grass.
- **Only one pipeline goes over 10.** `generate` uses 11 storage buffers. The others reach 13 only because every compute pipeline shares one layout.
- **The real unknowns are the browsers' shader compilers and performance, not the layout.** Neither is in the plan's findings. A browser spike should come before the refactor, not after it.

On sequencing: the refactor is needed before we can promise web support, but it should not be the first step. Order: spike, then the capability guard, then the layout refactor to 10, then the demo. Targeting 10 also drops the table textures, so the refactor is smaller than in the original plan.

## Findings review

Each claim was checked against `main` at 6e2a868. The counts, the `file_watcher` error and the wasm build are confirmed. The "hard limit" claim is overstated, and a few layout details are wrong or out of date.

| Claim | Verdict | What the code shows |
| --- | --- | --- |
| The library builds for `wasm32` with `bevy/webgpu` | Confirmed | Re-ran on 2026-10-22: it builds. Time uses `bevy::platform::time::Instant`, which works on wasm. |
| The examples cannot build for wasm | Confirmed | `cargo check --example minimal` fails with `file_watcher`'s `compile_error!`. |
| Compute uses 13 storage buffers, 14 for `size_dispatch` | Confirmed | `GrassLayouts::new` (`src/render/resources.rs`): 7 shared, 6 view, 1 dispatch. Every pipeline uses the shared and view layouts (`init_generate_pipelines`). |
| "No headroom, no option to request a higher limit" | Overstated | 8 is the spec floor. Bevy uses `adapter.limits()` (`bevy_render` `renderer/mod.rs:332`), and most devices report 16 or more ([web3dsurvey](https://web3dsurvey.com/webgpu/limits/maxStorageBuffersPerShaderStage)). We target 10, which 98% of devices report. Almost none report only 8. |
| The refactor is needed to fit 8 | Partly | Only `generate` uses more than 10: 11 buffers. Trample uses 2, cull 4, `size_dispatch` 2, `write_draw_args` 2. Separate layouts per pass would cut most of the 13. |
| Tables as textures: tile `Rgba32Float` 32×32, clumps `R32Uint` 64×64 | Sizes wrong | The tile is 1,024 `vec2` points, so `Rg32Float` 32×32. The clump table has 256×256 entries, packed four to a word: `R32Uint` 128×128, or `R8Uint` 256×256. Tufts `R32Uint` 256×256 is right. At a limit of 10, no textures are needed. |
| Merge masks and heights, with `height_offset` in `Frame` | Simpler option exists | Heights are already found per field through `placement.w`. Adding the mask length to it when the tables are packed avoids changing `Frame` and its WESL twin. |
| The prepass fragment needs the noise texture and sampler | Out of date | Since the commit that packs the look's varyings (`5d81afede50b`), the look is computed in the vertex stage. The fragment stage reads only `grass_view` (a uniform). Only `grass_view` needs `VERTEX_FRAGMENT`. |
| Draw storage should be `VERTEX` only | Confirmed, low risk | Nothing in a fragment shader reads `instances` or `fields`. Bevy's clustered lights already use fragment storage buffers. |
| Depth-clip-control and readback already work | Confirmed | `supported: device.features().contains(DEPTH_CLIP_CONTROL)` in `bevy_glue.rs`. Readback polls a state machine across frames and never waits. |

The review also found:

- **No `first_instance` is needed.** The draw arguments pass 0, and `blade_ref` decodes the bucket from `base_vertex`. That avoids WebGPU's optional `indirect-first-instance` feature.
- **Browser shader compilers are stricter than native naga.** Tint (Chrome), WebKit, and naga in Firefox check uniformity strictly and add bounds checks to every storage read. The bounds checks may slow down `generate`, the heaviest pass.
- **wasm runs on one thread.** Table rebuilds and edit flushes run on the main thread, and masks and heightmaps load over HTTP.
- **The budget can be tested on desktop.** `WgpuSettings::constrained_limits` with `max_storage_buffers_per_shader_stage = 10` reproduces Chrome on macOS on a native GPU. The GPU tests and goldens can then check the refactor without a browser.
- **Merging masks and heights shares one binding.** They then share one 128 MiB binding instead of having one each, which matters for very large terrains.

## Refactor first?

The refactor must come before the release, but not before the spike. He is right that the layout has to fit our target of 10 before we can say "WebGPU supported": otherwise the crate fails in Chrome on most Macs and in Samsung Internet. Doing it first, though, means spending 2 to 3 days without knowing what else the browsers reject. Three reasons to reorder:

1. **A one-day spike on today's code tests everything except the budget.** It runs on any adapter that reports 16 or more storage buffers. It will show any shader compiler errors and the real frame time, and the refactor cannot show either.
2. **The capability guard is useful whatever we decide.** WebGL2 builds, weak native adapters and any device below 10 should get one warning and no grass, not a pipeline error. It is a small change and can ship in a patch release.
3. **The refactor can be checked on desktop.** With `constrained_limits` set to 10, the native GPU tests, goldens and bench are the acceptance test. The browser is needed only for the final check.

The refactor itself needs no public API break. Every change is in how data is packed for the GPU and in the WESL bindings, and the CPU model stays the same. The only new public item worth adding is a way to ask whether grass can render on this device (see phase 1).

## Migration plan

Five phases, about 1.5 to 2 weeks for one developer. The budget is 10 storage buffers per stage, and devices below 10 get a warning and no grass. Phases 0 and 1 can ship on their own as a patch release.

1. **Phase 0: spike on today's code (1 to 2 days).** ✅ **Done** (`954a22a`; [results](#phase-0-results-2026-10-22-branch-webgpu-phase0)). Open: recordings on macOS, Firefox and Android, and the ~22 ms browser grass cost on the Intel GPU.
   - Turn on `file_watcher` only for native targets (`[target.'cfg(not(target_arch = "wasm32"))'.dev-dependencies]`), and `bevy/webgpu` only for wasm. Then the existing examples build for the browser.
   - Add the CI job `cargo check --target wasm32-unknown-unknown --lib --examples`.
   - Run `minimal` and `meadow` with `wasm-server-runner` in Chrome and Edge on Windows, Chrome and Safari on macOS, Firefox, and Chrome on Android. For each, record `maxStorageBuffersPerShaderStage`, console errors, shader compiler errors and frame time.
   - Exit: a list of blockers, each with the browser it fails in. If something other than the budget fails, it is scheduled before phase 2.
2. **Phase 1: capability guard (1 day).** ✅ **Done** (`9622419`; [results](#phase-1-results-2026-10-22-branch-webgpu-phase0)).
   - Add `src/render/platform.rs`. It works out the storage buffers each pipeline needs from `GrassLayouts`, not from a hardcoded number, and checks for compute support and the binding sizes the tables need.
   - If the device falls short, skip queueing the grass pipelines, warn once (`bevy_grass: ...`) and report it in `GrassStats`. The warning names the limit the device reports and the 10 the crate needs.
   - Exit: a native run with `constrained_limits` at 8, and a WebGL2 build, each log one warning, show no grass and raise no wgpu error.
3. **Phase 2: fit every pipeline into 10 (2 to 3 days).** ✅ **Done** (`955e9b0`, `a5f046c`, `266c855`; [results](#phase-2-results-2026-10-22-branch-webgpu-phase0)). Open: a check on a real 10-buffer browser. Commit and benchmark each step on its own:
   - Give each group of passes its own group-1 layout (table below). This brings every pipeline except `generate` to 10 or fewer, with no change to the data.
   - Merge masks and heights into one `scene` buffer, and add the mask length to `placement.w` when packing. This brings `generate` from 11 to 10.
   - Make the draw layout's storage `VERTEX` only, and its noise texture and sampler too. Keep `grass_view` at `VERTEX_FRAGMENT`.
   - Add a unit test that counts storage entries in each pipeline's combined layout: at most 10 per stage, and none in the fragment stage of the draw layout. One named constant holds the 10, for this test and for the phase 1 guard.
   - No table textures. Tile, clumps and tufts stay as they are.
   - Exit: `cargo test`, `cargo test -- --ignored gpu` and the goldens pass with `constrained_limits` at 10 and with the adapter's own limits. The goldens show no pixel changes. `tools/bench_ab.py` stays within the user's current bar (3% as of 2026-10-15) at p50 and p95, in both scenes.
4. **Phase 3: web demo and CI (2 to 3 days).** ⏳ Next.
   - Add a `web/` package for the hosted demo (built with `wasm-bindgen`, plus `index.html`, deployed to GitHub Pages), and add `/web/` to `exclude`.
   - Start the demo at `GrassSettings::MEDIUM`, with a quality switch.
   - Run the browser smoke test by hand or nightly. Hosted CI keeps only the wasm compile check.
   - Exit: the demo runs without console errors in each browser from phase 0, including on Macs that report 10.
5. **Phase 4: docs and release (1 to 2 days).** Not started.
   - Add `docs/web.md`: supported browsers, no WebGL2, the 10-buffer budget and the warning below it, binding sizes, the shadow caveat, the timestamp caveat and a performance tier per device class.
   - Update `rendering.md` (the layout table), `testing.md` (the constrained-limits run), `roadmap.md`, the README and `CHANGELOG.md`.
   - Release as 0.21.0. The budget limits what future features can add, even though no public item changes.

Target layout, counting storage buffers per pipeline:

| Pipeline | Group 0 (shared) | Group 1 | Group 2 | Total |
| --- | --- | --- | --- | --- |
| `generate`, `cull` | fields, chunks, scene, tile, clumps, tufts | visible, counters, instances, trample | none | 10 |
| `trample_decay`, `trample_stamp` | the same 6 | trample, interactors | none | 8 |
| `size_dispatch`, `write_draw_args` | the same 6 | counters, draw_args | dispatch | 9 |
| Grass draws (vertex) | Bevy's view groups | none | instances, fields | 2 + Bevy's |

`generate` is exactly at 10. When a future feature needs another buffer, there are two ways to make room without textures. One buffer for the three tables brings `generate` to 8. Moving the visible list into the start of the instance buffer brings it down one more. Both wait until a feature needs the room.

## Risks and open decisions

The biggest risk is the browsers' shader compilers. They cannot be tested on desktop, which is why phase 0 comes first.

| Risk or decision | What to measure or decide | Fallback |
| --- | --- | --- |
| A browser rejects WGSL that native naga accepts | Phase 0 console logs, per browser | Rewrite that construct. The fix is the same for every target. |
| Bounds checks slow `generate` in browsers | Frame time, `generate` against native | A lower default tier on the web (`MEDIUM` or `LOW`). |
| Budget of 10, not 8 | Decided 2026-10-22: 98% of devices report 10 or more | Devices below 10 get one warning and no grass. |
| `generate` has no spare buffer at 10 | The budget test fails when a feature adds one | One buffer for the three tables (frees 2), or the visible list in the instance buffer (frees 1). |
| Bevy's own fragment storage with optional features (OIT, clustered decals) | Draw pipelines under `constrained_limits` at 10, with those features on | The draw layout is already vertex-only. Document any Bevy feature that does not fit. |
| `scene` holds masks and heights in one 128 MiB binding | Bytes in the largest example terrain | Done: the upload checks the sum against the device's limit, and over it logs one warning and draws no grass. Split it again if a user hits the limit. |
| Long table rebuilds on wasm's single thread | `GrassStats` rebuild time in the browser | Document field-size guidance for the web. |
| The GPU tests and goldens cannot run in a browser | Constrained native runs stand in for them | A nightly browser smoke test that logs blade counts from `GrassStats`. |

Two questions are for you: whether to keep the 3% bar for this refactor, and whether the hosted demo belongs on GitHub Pages or on the project site.

## Phase 0 results (2026-10-22, branch `webgpu-phase0`)

On the development laptop the crate runs in Edge 155 and Brave 1.97 (Chromium 155) without errors. One problem needs work before phase 2: in Edge, grass costs about 22 ms a frame on the integrated GPU, against under 1 ms natively on the same GPU. The Chrome 147 failure turned out to be a stale, unused Chrome install; current Chromium is fine.

Machine: Windows 11, i7-11850H, Intel UHD Graphics (Xe, `gen-12lp`) and NVIDIA T1200. Chromium ignores `powerPreference` on Windows ([crbug 369219127](https://crbug.com/369219127)), so the browser's own GPU setting decides: Edge renders on the Intel GPU and Brave on the T1200. Native Bevy picks the T1200 unless told otherwise.

| Browser | Storage buffers per stage | Result | `meadow` frame, p50 at 1896×988 |
| --- | --- | --- | --- |
| Edge 155, Windows | 16 | `minimal` and `meadow` draw correctly: wind, shadows, trampling, far grass. No errors, no warnings, no WGSL compiler messages. | 30.5 ms with grass, 8.4 ms with `?no-grass` |
| Brave 1.97 (Chromium 155), Windows, on the T1200 | 16 | `minimal` and `meadow` draw correctly, and every F4 settings profile works. No errors or warnings. | 16.7 ms (vsync at 60 Hz), with or without grass |
| Chrome 147, Windows (stale install, not the user's browser) | 16 | Fails: every `dispatchWorkgroupsIndirect` is rejected with "ImmediateAddressSpace feature is not enabled". Fixed in later Chromium: Edge 155 and Brave 1.97 run the same repro page. | none |
| Chrome and Safari on macOS, Firefox, Chrome on Android | not run | Needs those devices. | none |

Native, same Intel GPU, D3D12, 1920×1080: `meadow` p50 4.1 to 4.8 ms with grass, 3.7 to 4.5 ms with `--no-grass`; `grass_generate` 0.63 ms of GPU time.

### Blockers

1. **Grass costs about 22 ms a frame in the browser.** That is 30.5 ms against 8.4 ms without grass, measured in two interleaved pairs that agreed to 0.2 ms. Natively, the same GPU spends under 1 ms on grass. Turning off Dawn's robustness and validation (`disable_robustness`, `skip_validation`) changed nothing: 30.4, 30.2 and 30.1 ms. The cause is still unknown. This is scheduled before phase 2, since the cause may change which layout work is worth doing. Bevy logs no per-pass GPU time in the browser, so the next step is to bisect with settings (no grass shadows, the `LOW` tier, fewer blades), then time single passes with `timestamp-query`.

### Not blockers

- **Chrome 147 rejected every indirect dispatch.** A plain WebGPU page fails the same way, so it is a browser bug, fixed by Chromium 155 (Edge 155 and Brave 1.97 pass). `docs/web.md` should name a minimum Chromium version.
- **The budget is not tested yet.** This laptop reports 16, which fits today's 14. A 10-buffer device, or phase 1's `constrained_limits`, is still needed.
- **The wasm is large.** A release build is 64.8 MiB once function names are stripped; the names were another 60 MiB. Phase 3 needs a size-optimised profile and `wasm-opt`.
- **Bevy without grass is also slower in the browser:** 8.4 ms against about 4 ms natively. Bevy logs "Some GPU preprocessing are limited on this device" there.

### What the branch adds

- `Cargo.toml`: `file_watcher` only on native targets, `webgpu` and `web-sys` only on wasm.
- `examples/common`: in a browser, assets load from `assets/` next to the page, the canvas fits the page, and flags come from the query string (`?perf&no-grass`). There is also a new `--no-grass` flag on every platform.
- `tools/web_example.py`: builds examples for wasm, runs `wasm-bindgen`, writes a page with a report panel (limits, features, errors, frame time, "Copy report") and serves it on localhost.
- CI: a `wasm` job that runs `cargo check --target wasm32-unknown-unknown --lib --examples`.
- `docs/testing.md` and `CHANGELOG.md`.

### Still to record

Each needs the device. Run `python tools/web_example.py`, open the page and press "Copy report":

- [x] Chromium on Windows: Brave 1.97 (Chromium 155) works
- [ ] Chrome on macOS (expected to report 10)
- [ ] Safari on macOS
- [ ] Firefox
- [ ] Chrome on Android (over USB port forwarding, since WebGPU needs localhost or HTTPS)
- [ ] Edge on the T1200: set Edge to "High performance" in Windows graphics settings, then compare with native

## Phase 1 results (2026-10-22, branch `webgpu-phase0`)

Done; both exit checks pass.

- `src/render/platform.rs` decides `GrassSupport` once, in `GrassRenderPlugin::finish`, from the adapter's downlevel flags and the device's limits. It checks compute shaders, indirect execution, vertex storage, storage buffers per stage, and the storage binding size (the largest blade buffer, 128,000,000 bytes at `MAX_INSTANCE_CAPACITY`).
- The storage buffers needed are counted from the layouts the pipelines are made from (`GrassLayouts::compute(entry)` is now what `init_generate_pipelines` uses too): 14 today. Phase 2's budget test can use the same count.
- On a device that falls short, every render-world grass system is skipped by a run condition, so no grass pipeline, buffer or bind group is made. One warning names the reason, for example "the device allows 8 storage buffers per shader stage and grass binds 14".
- It is reported as a new resource, `GrassSupport`, not a field of `GrassStats`: a new field would break struct literals of `GrassStats`, and a resource keeps phases 0 and 1 a patch release.
- The examples take `--max-storage-buffers N` (it sets `WgpuSettings::constrained_limits`).

Exit checks:

| Run | Result |
| --- | --- |
| `meadow --max-storage-buffers 8`, native, T1200 | One warning, no grass, no wgpu error, rendering continues. Before the guard, the same run raised "Too many bindings of type StorageBuffers ... limit is 8, count was 13" from `create_pipeline_layout`. |
| `meadow` built with WebGL2, Brave | One warning ("the device has no compute shaders"), no grass, no errors. |
| `meadow`, native, no limit | Grass as before. |

Open: with no blades, the ground near the camera shows bare under `FarGrassMaterial`, which fades in the meadow colour only past the grass radius. Whether the far material should cover the whole ground on such a device is a look decision.

## Phase 2 results (2026-10-22, branch `webgpu-phase0`)

Done; every pipeline binds at most 10 storage buffers per shader stage, down from 14. Three commits, measured together against `9622419`:

1. `955e9b0`: a group-1 layout per family of compute passes, with the binding numbers the shaders already used (no shader change). 14 → 11.
2. `a5f046c`: masks and heightmaps in one `scene` buffer. Only the render world changed: it uploads the masks and then the heightmaps as `f32` bits, and adds the masks' length in words to each field's `placement.w` as it uploads the field table. The CPU tables, the mirror and the incremental rebuilds are unchanged. 11 → 10. A new GPU test grows a field with both a mask and a heightmap.
3. `266c855`: the draws' storage buffers, noise texture and sampler are vertex-only; the grass view uniform stays `VERTEX_FRAGMENT`. `STORAGE_BUFFER_BUDGET` (10) and a unit test keep every pipeline within it and the draws' fragment stage free of storage buffers. The GPU tests run on a device lowered to 10, and the golden example takes `--max-storage-buffers`.

Also `d28b896`: the look GPU test waited a fixed 120 frames for its pipelines and failed when the other GPU tests ran at once, on earlier commits too. It now waits for blades, up to 600 frames.

Exit checks:

| Check | Result |
| --- | --- |
| `cargo test`, and the GPU tests at 10 | Pass (21 GPU tests) |
| Goldens at the adapter's limits and at 10 | Every image matches |
| `meadow --max-storage-buffers 10`, native | Grass, no warning, no error |
| `tools/bench_ab.py`, 4 rounds, both scenes, p50 and p95 | Within the 3% bar; largest +2.4% (C, p95, ARPG camera) |

Not yet checked: a real 10-buffer browser (Chrome on a Mac). The browser grass cost on the Intel GPU (Phase 0) is still open.

