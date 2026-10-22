"""Checks that the crate builds with no warnings in every feature set.

    python tools/lint.py

Runs clippy, with warnings as errors, on the library alone for each feature
set a game can build it with, then on every target with all features. The
library alone matters: tests and examples turn on `debug` through the
dev-dependency on this crate, so code that only `debug` or the tests use
looks used to `--all-targets` and dead to a game. `-D warnings` goes to
clippy, not RUSTFLAGS, so Bevy is not rebuilt. CI and the Claude Code stop
hook both run this; exits non-zero on the first set that warns.

The web demo (web/) is linted too: natively, and for wasm32-unknown-unknown
when that target is installed, since its calls from the page exist only
there.
"""

import os
import shutil
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

RUNS = [
    ["--lib"],
    ["--lib", "--features", "debug"],
    ["--lib", "--features", "rapier"],
    ["--lib", "--features", "avian"],
    ["--all-targets", "--all-features"],
    ["-p", "bevy_grass_web", "--all-targets"],
]
WASM = ["-p", "bevy_grass_web", "--target", "wasm32-unknown-unknown"]


def cargo():
    found = shutil.which("cargo")
    if found:
        return found
    # The user's PowerShell has no cargo on PATH.
    home = os.path.join(os.path.expanduser("~"), ".cargo", "bin", "cargo")
    return home + (".exe" if os.name == "nt" else "")


def wasm_installed():
    rustup = shutil.which("rustup")
    if not rustup:
        return False
    installed = subprocess.run([rustup, "target", "list", "--installed"], cwd=ROOT,
                               capture_output=True, text=True).stdout
    return "wasm32-unknown-unknown" in installed.split()


def main():
    runs = RUNS + [WASM] if wasm_installed() else RUNS
    if runs is RUNS:
        print("lint: wasm32-unknown-unknown not installed, the web demo's page calls are not linted")
    for args in runs:
        command = [cargo(), "clippy", "--quiet", *args, "--", "-D", "warnings"]
        print("lint:", " ".join(command[1:]), flush=True)
        if subprocess.run(command, cwd=ROOT).returncode != 0:
            print(f"lint: warnings with {' '.join(args)}", file=sys.stderr)
            return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
