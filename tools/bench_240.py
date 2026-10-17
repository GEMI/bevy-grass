"""The 240 Hz acceptance gate.

Runs the three reference configurations on one named reference machine:

  no-grass      the Bevy + terrain + camera floor
  high          GrassQuality::High, the default
  competitive   GrassQuality::Competitive, the 240 Hz-oriented tier

Each run is the `bench` example at 1920x1080 with no vsync, measuring 1,380
frames after warm-up. The pass/fail threshold is frame p95 <= 1000 / 240 ms.
The default is three rounds, and each reported number is the median over
them: a single OS/GPU hitch cannot decide the gate. Grass cost is the paired
difference against the no-grass run of the same round and frames. Results go
to target/bench-240/ as JSONL, CSV, logs and a Markdown table for
docs/performance.md.

    python tools/bench_240.py --machine t1200
    python tools/bench_240.py --machine rtx3060 --scene perspective
    python tools/bench_240.py --machine rtx4070 --profile release-lto --no-gpu-timing
    python tools/bench_240.py --machine t1200 --no-build --report-only

A failing gate exits non-zero unless --report-only is given. The benchmark's
GPU timing queries add about 0.15-0.2 ms on some machines, so pass
--no-gpu-timing for the strictest check of the frame budget.
"""

import argparse
import json
import os
import re
import statistics
import subprocess
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
EXE = ".exe" if os.name == "nt" else ""
MACHINES = os.path.join(ROOT, "tools", "reference-machines.json")
OUT = os.path.join(ROOT, "target", "bench-240")
RESULTS = os.path.join(OUT, "results.jsonl")
TARGET_HZ = 240.0
BUDGET_MS = 1000.0 / TARGET_HZ

SCENES = {
    "perspective": [],
    "arpg": ["--ortho"],
}

# The no-grass run comes first: the other two use its CSV as the baseline.
CONFIGS = [
    ("no-grass", ["--no-grass"]),
    ("high", ["--tier", "high"]),
    ("competitive", ["--tier", "competitive"]),
]

SUMMARY = re.compile(
    r"^\s+(frame|grass trample|grass cull|grass generate|main pass|shadow pass|grass cost)"
    r"\s+p50\s+([\d.]+) ms\s+p95\s+([\d.]+) ms",
    re.M,
)
BUDGET = re.compile(
    r"^\s+budget\s+(.+?): frame p50 ([\d.]+) ms, p95 ([\d.]+) ms -> (PASS|FAIL)",
    re.M,
)


def load_machines():
    with open(MACHINES, encoding="utf-8") as file:
        return json.load(file)


def exe_path(profile):
    # Cargo's dev profile writes to target/debug, not target/dev.
    directory = "debug" if profile == "dev" else profile
    return os.path.join(ROOT, "target", directory, "examples", "bench" + EXE)


def build(profile):
    cargo = os.environ.get("CARGO", "cargo")
    command = [cargo, "build", "--profile", profile, "--example", "bench"]
    print("+", " ".join(command), flush=True)
    subprocess.run(command, cwd=ROOT, check=True)


def parse_log(text):
    stats = {}
    for match in SUMMARY.finditer(text):
        stats[match.group(1)] = {
            "p50": float(match.group(2)),
            "p95": float(match.group(3)),
        }
    budget = BUDGET.search(text)
    if budget:
        stats["budget"] = {
            "description": budget.group(1),
            "p50": float(budget.group(2)),
            "p95": float(budget.group(3)),
            "pass": budget.group(4) == "PASS",
        }
    return stats


def run_one(exe, machine, scene, round_number, name, flags, run_dir, baseline):
    csv_path = os.path.join(run_dir, f"{scene}-{round_number}-{name}.csv")
    log_path = os.path.join(run_dir, f"{scene}-{round_number}-{name}.log")
    command = [
        exe,
        *SCENES[scene],
        *flags,
        "--machine",
        machine,
        "--target-hz",
        str(int(TARGET_HZ)),
        "--csv",
        csv_path,
    ]
    if baseline:
        command += ["--baseline-csv", baseline]
    print("+", " ".join(command), flush=True)
    started = time.time()
    out = subprocess.run(
        command,
        cwd=ROOT,
        capture_output=True,
        text=True,
        timeout=1800,
        check=False,
    )
    text = out.stdout + out.stderr
    with open(log_path, "w", encoding="utf-8") as file:
        file.write(text)
    stats = parse_log(text)
    if "frame" not in stats:
        print(text[-4000:])
        raise SystemExit(f"{scene} {name}: no summary in the log (see {log_path})")
    row = {
        "machine": machine,
        "scene": scene,
        "round": round_number,
        "config": name,
        "timestamp": time.time(),
        "seconds": round(time.time() - started, 1),
        "exit_code": out.returncode,
        "csv": os.path.relpath(csv_path, ROOT),
        "log": os.path.relpath(log_path, ROOT),
        **stats,
    }
    with open(RESULTS, "a", encoding="utf-8") as file:
        file.write(json.dumps(row) + "\n")
    return row


def fmt(value):
    return "-" if value is None else f"{value:.3f}"


def median_at(runs, section, key):
    values = [
        run[section][key]
        for run in runs
        if section in run and key in run[section] and run[section][key] is not None
    ]
    return statistics.median(values) if values else None


def aggregate(raw_rows):
    """Median each statistic over the rounds, per scene and configuration."""
    groups = {}
    for row in raw_rows:
        groups.setdefault((row["scene"], row["config"]), []).append(row)
    rows = []
    for (scene, config), runs in groups.items():
        row = {"scene": scene, "config": config, "rounds": len(runs)}
        for section in ("frame", "grass cost", "main pass", "shadow pass"):
            values = {
                key: median_at(runs, section, key)
                for key in ("p50", "p95")
            }
            if values["p50"] is not None or values["p95"] is not None:
                row[section] = values
        p95 = row.get("frame", {}).get("p95")
        row["budget"] = {
            "p95": p95,
            "pass": p95 is not None and p95 <= BUDGET_MS,
        }
        rows.append(row)
    return rows


def markdown(rows, machine, rounds):
    lines = [
        f"### 240 Hz gate: {machine}",
        "",
        f"Median of {rounds} round(s) per configuration. Budget: frame p95 <= {BUDGET_MS:.3f} ms. "
        "GPU timing is included unless `--no-gpu-timing` was used.",
        "",
        "| Scene | Config | Frame p50 | Frame p95 | Grass cost p50 | Grass cost p95 | Main pass p95 | Shadow p95 | Gate |",
        "| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | :---: |",
    ]
    for row in rows:
        stats = row
        frame = stats.get("frame", {})
        cost = stats.get("grass cost", {})
        main = stats.get("main pass", {})
        shadow = stats.get("shadow pass", {})
        budget = stats.get("budget", {})
        gate = "PASS" if budget.get("pass") else "FAIL"
        lines.append(
            "| {scene} | {config} | {fp50} | {fp95} | {gp50} | {gp95} | {mp95} | {sp95} | {gate} |".format(
                scene=row["scene"],
                config=row["config"],
                fp50=fmt(frame.get("p50")),
                fp95=fmt(frame.get("p95")),
                gp50=fmt(cost.get("p50")),
                gp95=fmt(cost.get("p95")),
                mp95=fmt(main.get("p95")),
                sp95=fmt(shadow.get("p95")),
                gate=gate,
            )
        )
    lines.append("")
    return "\n".join(lines)


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "--machine",
        required=False,
        help="A name from tools/reference-machines.json (t1200, rtx3060, rtx4070).",
    )
    parser.add_argument(
        "--scene",
        choices=["both", *SCENES],
        default="both",
        help="Which benchmark scene to run (default: both).",
    )
    parser.add_argument(
        "--profile",
        default="release-lto",
        help="Cargo profile to build and run (default: release-lto).",
    )
    parser.add_argument("--no-build", action="store_true", help="Use the existing executable.")
    parser.add_argument(
        "--rounds",
        type=int,
        default=3,
        help="Rounds to run; each result is the median over them (default: 3).",
    )
    parser.add_argument(
        "--no-gpu-timing",
        action="store_true",
        help="Leave out Bevy's GPU timestamp queries: a stricter frame-time check, but no pass times.",
    )
    parser.add_argument(
        "--report-only",
        action="store_true",
        help="Always exit zero; useful while a target is still being brought up.",
    )
    parser.add_argument(
        "--list-machines",
        action="store_true",
        help="Print the named reference machines and exit.",
    )
    args = parser.parse_args()

    machines = load_machines()
    if args.list_machines:
        for name, info in machines.items():
            print(f"{name:10} {info['label']} ({info['class']})")
        return 0
    if args.rounds < 1:
        parser.error("--rounds must be at least 1")
    if not args.machine:
        parser.error("--machine is required (see --list-machines)")
    if args.machine not in machines:
        parser.error(
            f"unknown machine {args.machine!r}; known: {', '.join(machines)}"
        )

    exe = exe_path(args.profile)
    if not args.no_build:
        build(args.profile)
    if not os.path.exists(exe):
        raise SystemExit(f"missing {exe}; run without --no-build or build the bench example")

    stamp = time.strftime("%Y%m%d-%H%M%S")
    run_dir = os.path.join(OUT, args.machine, stamp)
    os.makedirs(run_dir, exist_ok=True)
    scenes = list(SCENES) if args.scene == "both" else [args.scene]
    raw_rows = []
    for scene in scenes:
        for round_number in range(1, args.rounds + 1):
            baseline = None
            for name, flags in CONFIGS:
                run_flags = list(flags)
                if args.no_gpu_timing:
                    run_flags.append("--no-gpu-timing")
                row = run_one(
                    exe,
                    args.machine,
                    scene,
                    round_number,
                    name,
                    run_flags,
                    run_dir,
                    baseline,
                )
                raw_rows.append(row)
                if name == "no-grass":
                    baseline = row["csv"]
                    if not os.path.isabs(baseline):
                        baseline = os.path.join(ROOT, baseline)
                    baseline = os.path.abspath(baseline)
                print(
                    f"{scene:11} round {round_number}/{args.rounds} {name:12}"
                    f" frame p50 {row['frame']['p50']:6.3f} ms"
                    f"  p95 {row['frame']['p95']:6.3f} ms"
                    f"  -> {'PASS' if row.get('budget', {}).get('pass') else 'FAIL'}",
                    flush=True,
                )

    rows = aggregate(raw_rows)
    table = markdown(
        rows,
        f"{args.machine} — {machines[args.machine]['label']}",
        args.rounds,
    )
    table_path = os.path.join(OUT, "latest.md")
    with open(table_path, "w", encoding="utf-8") as file:
        file.write(table)
    print()
    print(table)
    print(f"results: {os.path.relpath(RESULTS, ROOT)}")
    print(f"table:   {os.path.relpath(table_path, ROOT)}")

    failed = [row for row in rows if not row.get("budget", {}).get("pass", False)]
    if failed and not args.report_only:
        print(f"\n240 Hz gate FAILED for {len(failed)} of {len(rows)} runs", file=sys.stderr)
        return 1
    if failed:
        print(f"\n240 Hz gate failed for {len(failed)} of {len(rows)} runs (report-only)", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
