# Draft issue for the Bevy repository

**Title:** Render app's multi-threaded schedule executor adds ~0.32 ms (24%) to an empty Bevy frame on Windows

## Summary

On an empty 1920 × 1080 Bevy 0.20 scene (window, camera and sun only), switching
only the render app's `Render` and `ExtractSchedule` schedules to
`SingleThreadedExecutor` cuts the frame p50 from **1.351 ms to 1.029 ms
(−23.8%)**. Switching only the main app's schedules changes nothing
(**−0.1%**). Switching both gives **0.983 ms (−27.3%)**. Pipelined rendering
stays enabled in every case, so this is executor coordination inside the render
app, not the two apps' threading.

The earlier Bevy trace on an older machine put the render thread on the critical
path and found its `Render` schedule waiting while the multi-threaded executor
handed systems to the compute pool, with ~340 `futex` calls/frame. These
measurements say the same cost is still present on a much newer CPU.

## Environment

- Bevy `0.20.0`, `release-lto` (`lto = "fat"`, `codegen-units = 1`)
- Windows 11 Pro 10.0.26300
- Intel Core i7-11850H, 8 cores / 16 threads, 2.50 GHz
- NVIDIA T1200 Laptop GPU, driver 32.0.15.9647
- 1920 × 1080, 4× MSAA, no vsync (`PresentMode::AutoNoVsync`)
- Power scheme: Balanced (the only scheme available on this machine)
- 1,380 measured frames after warm-up; 5 interleaved rounds

## Reproduction

The benchmark is in the `bevy_grass` repository, branch
[`exp/bevy-baseline-split`](https://github.com/GEMI/bevy-grass/tree/exp/bevy-baseline-split):

```bash
cargo build --profile release-lto --example bench
B=target/release-lto/examples/bench   # bench.exe on Windows

# empty scene, default multi-threaded executors
$B --no-plugin --no-gpu-timing --no-terrain --no-shadows

# same scene, main-app schedules single-threaded only
$B --single-threaded-main --no-plugin --no-gpu-timing --no-terrain --no-shadows

# same scene, render-app schedules single-threaded only
$B --single-threaded-render --no-plugin --no-gpu-timing --no-terrain --no-shadows

# same scene, both apps single-threaded
$B --single-threaded --no-plugin --no-gpu-timing --no-terrain --no-shadows
```

The executor switch itself is:

```rust
let single = |schedule: &mut Schedule| {
    schedule.set_executor(SingleThreadedExecutor::new());
};

// Main app only:
app.edit_schedule(First, single)
    .edit_schedule(PreUpdate, single)
    .edit_schedule(Update, single)
    .edit_schedule(PostUpdate, single)
    .edit_schedule(Last, single);

// Render app only:
let render = app.sub_app_mut(RenderApp);
render.edit_schedule(Render, single);
render.edit_schedule(ExtractSchedule, single);
```

The benchmark is a minimal Bevy scene: the grass plugin is disabled with
`--no-plugin`, the terrain and shadows are removed, and Bevy's GPU timing
queries are disabled. What remains is the window, camera, light and the ordinary
render pipeline.

## Results

Empty scene, median of five rounds (p50 range in brackets):

| Executor configuration | p50 | p95 |
| --- | --- | --- |
| Default multi-threaded | **1.351 ms** [1.336–1.420] | 1.636 ms |
| Main app single-threaded | 1.349 ms [1.338–1.360] | 1.631 ms |
| **Render app single-threaded** | **1.029 ms** [1.020–1.059] | 1.247 ms |
| Both single-threaded | **0.983 ms** [0.979–1.016] | 1.331 ms |

Paired p50 difference against the default multi-threaded run of the same round:

| Configuration | Median difference | Median change |
| --- | --- | --- |
| Main app single-threaded | −0.002 ms | −0.1% |
| **Render app single-threaded** | **−0.322 ms** | **−23.8%** |
| Both single-threaded | −0.369 ms | −27.3% |

Render-only single-threading accounts for about 87% of the total saving; the
main-app executor adds the remaining ~0.05 ms on this scene. The p50 result was
stable across the rounds (render-only changes: −23.7%, −23.8%, −23.8%, −24.5%,
−25.4%). p95 was noisier, but moved in the same direction.

As a GPU-bound control, the same matrix on the High-tier grass scene moved only
−0.2% (main only), −0.8% (render only) and −1.4% (both). The frame there is
limited by the T1200, so the executor difference is small in absolute terms.

## What this suggests

- The measurable empty-frame executor cost is in the render app, not the main
  app's schedules.
- It is specifically the multi-threaded executor's coordination/wake-up cost on
  a light render schedule, not pipelined rendering or parallelism in general.
- Possible directions on Bevy's side: avoid waking worker threads when a render
  schedule has little parallel work, use a cheaper/adaptive executor for small
  render schedules, batch more render work per task, or reduce the
  executor handoffs and futexes. The exact fix is for maintainers to decide; the
  measurement just locates the cost.

## Caveats

- One machine and one OS. A repeat on an i5-4570S / RTX 4070 Ti is planned, where
  the empty frame is ~2.1 ms and the combined single-threaded saving was ~10%.
- The Windows power plan was Balanced, not High performance, so core parking may
  exaggerate the wake-up cost. The relative result was consistent across rounds.
- No profile/trace is attached yet. The earlier trace on the old machine pointed
  at render-executor handoffs and `futex`; a fresh trace on this machine would
  confirm it.

I can run additional configurations, attach a `trace_chrome` capture, or turn
this into a smaller Bevy-only reproduction if useful.
