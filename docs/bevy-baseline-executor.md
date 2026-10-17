# Which executor makes Bevy's empty-frame floor?

A split-executor experiment for Bevy 0.20, run on the i7 laptop. This is the
follow-up to [`docs/cpu-floor-test.md`](cpu-floor-test.md): the earlier test
found that `--single-threaded` cuts the empty-scene frame, but it changed both
the main app and the render app at once, so it did not say which one owned the
cost.

## The question

On this machine the empty scene is 1.35 ms p50 with Bevy's default
multi-threaded schedule executors and about 1.0 ms with `--single-threaded`.
Is the saving in the main app's `First`/`PreUpdate`/`Update`/`PostUpdate`/`Last`
schedules, in the render app's `ExtractSchedule`/`Render` schedules, or in both?

## The test change

The example harness gained two diagnostic flags:

| Flag | Effect |
| --- | --- |
| `--single-threaded-main` | `SingleThreadedExecutor` for the main app's per-frame schedules only |
| `--single-threaded-render` | `SingleThreadedExecutor` for `ExtractSchedule` and `Render` only |
| `--single-threaded` | Both, as before |

Pipelined rendering is kept in every case, so the two apps still run on
separate threads. Only the executor inside each app's schedules changes. These
flags are benchmark instrumentation, not part of the `bevy_grass` library API.

## Machine

- Intel Core i7-11850H, 8 cores / 16 threads, 2.50 GHz
- NVIDIA T1200 Laptop GPU, driver 32.0.15.9647
- Windows 11 Pro 10.0.26300
- Power scheme: Balanced (the only scheme available on this machine)
- `release-lto` build, branch `exp/bevy-baseline-split`, parent `87e700b`
- No runs rejected: no errors or panics in the log

The Balanced power plan is a caveat: core parking may exaggerate executor
wake-up costs. The relative result was stable across all five rounds, but the
absolute saving should be rechecked on a high-performance plan.

## Method

Two scenes, four executor configurations, five rounds, interleaved in the same
order each round:

- **Empty**: `--no-plugin --no-gpu-timing --no-terrain --no-shadows`
- **High control**: `--no-gpu-timing` (terrain, shadows and grass at the High tier)

Both at 1920 × 1080, 4× MSAA, no vsync, 1,380 measured frames after warm-up.
Configurations: default multi-threaded, main-only single-threaded, render-only
single-threaded, both single-threaded. Medians of the five rounds.

## Results

### Empty scene

| Configuration | p50 | p95 |
| --- | --- | --- |
| Default multi-threaded | **1.351 ms** [1.336–1.420] | 1.636 ms [1.495–1.676] |
| Main app single-threaded | 1.349 ms [1.338–1.360] | 1.631 ms [1.497–1.667] |
| **Render app single-threaded** | **1.029 ms** [1.020–1.059] | 1.247 ms [1.187–1.303] |
| Both single-threaded | **0.983 ms** [0.979–1.016] | 1.331 ms [1.232–1.558] |

Paired p50 difference against the default multi-threaded run of the same round:

| Configuration | Median difference | Median change | Per-round p50 changes |
| --- | --- | --- | --- |
| Main app single-threaded | −0.002 ms | −0.1% | +0.4%, −0.1%, −0.1%, −2.0%, −4.2% |
| **Render app single-threaded** | **−0.322 ms** | **−23.8%** | −23.7%, −23.8%, −23.8%, −24.5%, −25.4% |
| Both single-threaded | −0.369 ms | −27.3% | −26.7%, −27.3%, −24.8%, −28.0%, −29.8% |

Render-only single-threading accounts for about **87% of the total saving**
(0.322 of 0.369 ms). The main-app executor contributes almost nothing on this
empty scene. The remaining ~0.05 ms is the main app's share.

### High-tier control

| Configuration | p50 | p95 |
| --- | --- | --- |
| Default multi-threaded | 5.724 ms [5.603–5.765] | 6.840 ms [6.075–6.924] |
| Main app single-threaded | 5.715 ms [5.604–5.741] | 6.835 ms [6.063–6.902] |
| Render app single-threaded | 5.682 ms [5.564–5.721] | 6.781 ms [5.990–6.800] |
| Both single-threaded | 5.642 ms [5.531–5.656] | 6.128 ms [5.953–6.783] |

Paired p50 changes are −0.2% (main only), −0.8% (render only) and −1.4% (both).
The frame is GPU-bound on the T1200, so the executor saving is small in
absolute terms and does not change the 240 FPS verdict here.

## Reading

1. **The empty-frame executor cost is in the render app, not the main app.**
   Main-only single-threading is indistinguishable from the default. Render-only
   single-threading removes 0.32 ms of the 1.35 ms p50, or 24%.

2. **This is not an argument against multithreading in general.** Pipelined
   rendering stays on, extra cores still help, and the High-tier scene barely
   moves. The specific cost is the render app's multi-threaded executor
   coordinating many small render systems on an otherwise light frame.

3. **This matches the earlier trace.** The old i5 trace found the render thread
   on the critical path, spending much of its `Render` schedule waiting while
   the multi-threaded executor handed systems to the compute pool, with about
   340 `futex` calls per frame. This experiment says the same cost is present on
   a much newer CPU and is concentrated in the render app.

4. **The likely Bevy-side work is adaptive scheduling, not removing
   parallelism.** Options to investigate upstream: avoid waking worker threads
   when a render schedule has little parallel work, use a cheaper executor for
   small render schedules, batch more render-node work per task, or reduce
   handoffs/futexes. A per-schedule executor choice already exists; the question
   is whether the render app's default should adapt to the workload.

## Reproduce

Build once:

```bash
cargo build --profile release-lto --example bench
```

Linux/macOS:

```bash
B=target/release-lto/examples/bench
for round in 1 2 3 4 5; do
  echo "== round $round"
  for cfg in "" "--single-threaded-main" "--single-threaded-render" "--single-threaded"; do
    echo "-- empty $cfg"
    $B $cfg --no-plugin --no-gpu-timing --no-terrain --no-shadows 2>&1 | grep -E '^\s+frame '
    echo "-- high $cfg"
    $B $cfg --no-gpu-timing 2>&1 | grep -E '^\s+frame '
  done
done
```

Windows (PowerShell):

```powershell
$B = ".\target\release-lto\examples\bench.exe"
foreach ($round in 1..5) {
  "== round $round"
  foreach ($cfg in @(@(), @("--single-threaded-main"), @("--single-threaded-render"), @("--single-threaded"))) {
    "-- empty $cfg"
    & $B @cfg --no-plugin --no-gpu-timing --no-terrain --no-shadows 2>&1 | Select-String '^\s+frame '
    "-- high $cfg"
    & $B @cfg --no-gpu-timing 2>&1 | Select-String '^\s+frame '
  }
}
```

Report the median p50/p95 of each configuration and the paired change against
the default run of the same round. On Windows, use the high-performance power
plan if available. The raw log from this run is
`target/bevy-baseline-split.log` (not committed).

## Next experiments

- Repeat the same matrix on the i5-4570S / RTX 4070 Ti, where the empty frame is
  ~2.1 ms and the earlier single-threaded gain was ~10%. If the render app owns
  the saving there too, the finding is not specific to this laptop.
- Split `--single-threaded-render` further into `Render` and `ExtractSchedule`
  to see which render schedule dominates.
- Capture a trace on this machine to confirm the executor handoff/futex story
  and to give Bevy maintainers a profile, not only frame-time differences.
