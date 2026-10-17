# Is the CPU what adds ~2 ms to every frame?

A test for a faster machine. It takes about 30 minutes, most of it the build.

## The question

On the original test PC, an **empty** Bevy scene takes about **2.1 ms per frame**. That is half of a 240 FPS frame (4.167 ms) before any game exists. Loading the grass plugin adds about 0.1 ms to that frame, and drawing the grass adds almost nothing more.

That PC is an RTX 4070 Ti with an **Intel i5-4570S** (4 cores, no hyper-threading, 2013). We do not know whether the 2 ms is that old CPU, or Bevy's own per-frame cost that any CPU pays. This test separates the two.

Already ruled out (see `docs/performance.md`, "The frame-time floor" and "Leaving plugins out of `DefaultPlugins`"):

- The GPU: it was busy about 47% of the time, and a faster GPU did not change the frame.
- Unused `DefaultPlugins` members: leaving all of them out changed the frame by 0.2%, which is noise.
- The grass library: about 0.2 ms of the frame, and not where the floor comes from.

## Set up the machine

Run on a quiet desktop (or a plugged-in laptop on its high-performance power plan).

- Close the browser, chat, video, and anything else that uses CPU or GPU.
- Stock GPU clocks. Do not lock clocks or overclock for this test.
- Keep the mouse and keyboard idle while a run is going. The bench opens a 1920 × 1080 window.
- Same code as the reference numbers: `main` at `6577d5f` or later.

Write down: CPU model, cores/threads, GPU, OS, commit (`git rev-parse --short HEAD`).

## 1. Build

```text
git pull
cargo build --profile release-lto --example bench
```

Fat LTO links everything at once and took about 10 minutes on the 2013 CPU. Do not use the plain dev or release profile for the numbers: LTO alone cut the frame by about 12% on the old PC.

The binary is `target/release-lto/examples/bench` (`bench.exe` on Windows). Run it from the repository root.

## 2. The ladder

Four runs, each adding one thing. Every run is the `bench` example at 1920 × 1080, 4× MSAA, no vsync, 1,380 frames after warm-up (about 10 seconds). The result is the line starting with `frame`, which shows **p50** and **p95** in ms.

| Step | Flags | What it measures |
| --- | --- | --- |
| 1 | `--no-plugin --no-gpu-timing --no-terrain --no-shadows` | An empty Bevy scene: window, camera, sun. **This is the floor.** |
| 2 | `--no-plugin --no-gpu-timing` | Step 1 plus the terrain mesh and the sun's shadows |
| 3 | `--no-gpu-timing --no-grass` | Step 2 plus the grass plugin, with no grass field |
| 4 | `--no-gpu-timing` | Step 3 plus grass at the High tier |

`--no-gpu-timing` leaves out Bevy's GPU timestamp queries, which cost 0.15–0.2 ms on their own. Always pass it.

Run **5 rounds**, going through steps 1 to 4 in order each round (so drift hits all steps equally). Report the median p50 and p95 of each step.

Linux or macOS:

```bash
B=target/release-lto/examples/bench
for round in 1 2 3 4 5; do
  echo "== round $round"
  echo "-- 1 empty";           $B --no-plugin --no-gpu-timing --no-terrain --no-shadows 2>&1 | grep -E '^\s+frame '
  echo "-- 2 +terrain,shadows"; $B --no-plugin --no-gpu-timing 2>&1 | grep -E '^\s+frame '
  echo "-- 3 +grass plugin";    $B --no-gpu-timing --no-grass 2>&1 | grep -E '^\s+frame '
  echo "-- 4 +grass High";      $B --no-gpu-timing 2>&1 | grep -E '^\s+frame '
done
```

Windows (PowerShell):

```powershell
$B = ".\target\release-lto\examples\bench.exe"
foreach ($round in 1..5) {
  "== round $round"
  "-- 1 empty";            & $B --no-plugin --no-gpu-timing --no-terrain --no-shadows 2>&1 | Select-String '^\s+frame '
  "-- 2 +terrain,shadows";  & $B --no-plugin --no-gpu-timing 2>&1 | Select-String '^\s+frame '
  "-- 3 +grass plugin";     & $B --no-gpu-timing --no-grass 2>&1 | Select-String '^\s+frame '
  "-- 4 +grass High";       & $B --no-gpu-timing 2>&1 | Select-String '^\s+frame '
}
```

Throw away and redo a run if its log contains `ERROR` or a panic, or if something else on the machine woke up during it.

### Reference: the original test PC

Same commands, `release-lto`, median of 5 rounds (the range of the five p50s in brackets):

| Step | p50 | p95 |
| --- | --- | --- |
| 1 empty scene | **2.075 ms** [2.047–2.115] | 2.644 ms |
| 2 + terrain, shadows | 2.548 ms [2.511–2.607] | 3.234 ms |
| 3 + grass plugin, no field | 2.656 ms [2.633–2.726] | 3.492 ms |
| 4 + grass, High tier | 2.669 ms [2.659–2.735] | 3.314 ms |

## 3. What the faster CPU tells us

Compare step 1 first. It is the floor.

| Step 1 p50 on the faster CPU | Reading |
| --- | --- |
| Around 1.0 ms or less | The 2 ms was mostly the old CPU. A 240 FPS Bevy game is realistic, with about 3 ms of the 4.167 ms budget left for the game. |
| Around 1.0–1.5 ms | Partly the CPU. The floor shrank but is still a quarter to a third of the budget. |
| Around 1.5 ms or more (little change from 2.1) | The floor is Bevy's own per-frame cost, not the old CPU. Building a 240 FPS game on Bevy is then a real risk, and the next step is a trace of what the frame is doing. |

These thresholds are rules of thumb, not measured limits. What matters is how far the floor moves compared with how much faster the CPU is. A CPU with 2× the single-thread speed that brings the floor down only 20% is telling you the cost is not CPU speed.

Also look at steps 2–4 against step 1. On the old PC, the terrain and shadows add about 0.5 ms, the grass plugin about 0.1 ms, and the grass itself about 0.01 ms. If those additions shrink along with the floor, the whole frame is CPU-bound. If they stay the same while the floor drops, they are GPU work.

## 4. Two follow-up runs that separate speed from core count

Faster cores and more cores are different things. Run step 1 and step 4 again with these variations (5 rounds each, same method).

**Only 4 cores.** Restrict the process to 4 cores to mimic the old CPU's core count on the new CPU's faster cores:

```bash
# Linux
taskset -c 0-3 $B --no-plugin --no-gpu-timing --no-terrain --no-shadows
# Windows (cmd.exe)
start /wait /affinity F .\target\release-lto\examples\bench.exe --no-plugin --no-gpu-timing --no-terrain --no-shadows
```

(If the CPU has hyper-threading, pick 4 *physical* cores.) Read:

- Pinned to 4 cores is about as fast as unpinned: the cost is per-core speed or fixed overhead, and extra cores do not help.
- Pinned is clearly slower: extra cores are helping, and the old PC lost frame time to having only 4.

**One thread per schedule.** On the old PC, `--single-threaded` cut the frame by about 10% because waking threads cost more than the systems took. On a CPU with many cores it may lose:

```bash
$B --single-threaded --no-plugin --no-gpu-timing --no-terrain --no-shadows
```

If `--single-threaded` still wins on the faster CPU, the cost is scheduling overhead, not compute. If it loses, the faster CPU has enough cores to make Bevy's parallel executor pay off.

## 5. What to send back

- The machine details from the setup section.
- The `frame` lines (p50 and p95) for every run, or at least the medians for the four ladder steps, and for the two follow-up runs.
- Anything unusual: a run that was rejected and redone, background load, thermal throttling, a power plan other than high performance.

Add the result to `docs/performance.md` as a new section, with the machine and commit, like the earlier ones.
