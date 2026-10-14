"""Paired A/B runs of the `bench` example, the way the performance pass was measured.

    python tools/bench_ab.py snapshot NAME
        Copy the freshly built bench.exe to target/bench-ab/NAME.exe. Build each
        variant (`cargo build --example bench`), snapshot it, then change the
        code back.
    python tools/bench_ab.py run ROUNDS NAME [NAME ...]
        Run every named variant in both scenes, interleaved, ROUNDS times.
        Add the variant `nograss` to measure the grass-free baseline (it runs
        the first named variant's exe with --no-grass).
    python tools/bench_ab.py report REFERENCE
        For each variant, the median paired difference against the REFERENCE
        run of the same round and scene, in ms and as a share of the
        reference's grass cost (reference minus no-grass, both medians).

Results append to target/bench-ab/results.jsonl, so rounds can be added later.
A run whose log contains ERROR or a panic is rejected: a shader that fails to
compile draws no grass and would otherwise look like a large speed-up.

Measure on a quiet machine, plugged in, one GPU-heavy program at a time. Noise
on the development laptop was about +/-2% of grass cost; 4 rounds resolve 5%.
"""

import json
import os
import re
import shutil
import statistics
import subprocess
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
EXE = ".exe" if os.name == "nt" else ""
BENCH = os.path.join(ROOT, "target", "debug", "examples", "bench" + EXE)
OUT = os.path.join(ROOT, "target", "bench-ab")
RESULTS = os.path.join(OUT, "results.jsonl")
SCENES = {"perspective": [], "demo_arpg_game": ["--ortho"]}
LINE = re.compile(
    r"^\s+(frame|grass trample|grass cull|grass generate|main pass)\s+p50\s+([\d.]+) ms\s+p95\s+([\d.]+) ms",
    re.M,
)


def snapshot(name):
    os.makedirs(OUT, exist_ok=True)
    shutil.copy(BENCH, os.path.join(OUT, name + EXE))
    print("saved", name)


def run_one(name, scene, grass_exe):
    exe = os.path.join(OUT, (grass_exe if name == "nograss" else name) + EXE)
    args = [exe, *SCENES[scene], "--csv", os.path.join(OUT, "last.csv")]
    if name == "nograss":
        args.append("--no-grass")
    out = subprocess.run(args, cwd=ROOT, capture_output=True, text=True, timeout=900)
    text = out.stdout + out.stderr
    if "ERROR" in text or "panicked" in text:
        print(text[-3000:])
        raise SystemExit(f"{name} {scene}: errors in the log, run rejected")
    found = {m.group(1): float(m.group(2)) for m in LINE.finditer(text)}
    if "frame" not in found:
        print(text[-3000:])
        raise SystemExit(f"{name} {scene}: no summary in the log")
    row = {"variant": name, "scene": scene, "time": time.time(), **found}
    with open(RESULTS, "a", encoding="utf-8") as f:
        f.write(json.dumps(row) + "\n")
    print(f"{name:>20} {scene:11} frame p50 {found['frame']:6.3f} ms"
          f"  main pass {found.get('main pass', float('nan')):6.3f} ms", flush=True)


def run(rounds, names):
    grass_exe = next(n for n in names if n != "nograss")
    for _ in range(rounds):
        for scene in SCENES:
            for name in names:
                run_one(name, scene, grass_exe)


def report(reference):
    rows = [json.loads(line) for line in open(RESULTS, encoding="utf-8")]
    medians = {}
    for scene in SCENES:
        for name in ("nograss", reference):
            values = [r["frame"] for r in rows if r["variant"] == name and r["scene"] == scene]
            medians[(name, scene)] = statistics.median(values) if values else None
    last = {}
    deltas = {}
    for r in rows:
        if r["variant"] == reference:
            last[r["scene"]] = r["frame"]
        elif r["variant"] != "nograss" and r["scene"] in last:
            deltas.setdefault((r["variant"], r["scene"]), []).append(r["frame"] - last[r["scene"]])
    print(f"against {reference}; negative is faster")
    for (name, scene), d in sorted(deltas.items()):
        m = statistics.median(d)
        ref, base = medians[(reference, scene)], medians[("nograss", scene)]
        share = f"{100 * m / (ref - base):+6.1f}% of grass cost" if base is not None else ""
        print(f"{name:>20} | {scene:11} | n={len(d)} | {m:+6.3f} ms | {share}")


if __name__ == "__main__":
    command = sys.argv[1] if len(sys.argv) > 1 else ""
    if command == "snapshot":
        snapshot(sys.argv[2])
    elif command == "run":
        run(int(sys.argv[2]), sys.argv[3:])
    elif command == "report":
        report(sys.argv[2])
    else:
        print(__doc__)
