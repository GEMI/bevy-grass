"""Builds the hosted web demo (web/), serves it, and smoke-tests it in a browser.

    python tools/web_demo.py build [--dev]
    python tools/web_demo.py serve [--dev] [--no-build] [--port 8080] [--host 127.0.0.1]
    python tools/web_demo.py smoke [--browser PATH] [--headless] [--no-build] [--timeout 300]
                                   [--max-storage-buffers N]

build: compiles the `bevy_grass_web` package for wasm32-unknown-unknown with
the size-optimised `web` profile, runs wasm-bindgen and, if it is on PATH,
binaryen's wasm-opt, and writes the site to target/web-demo/: index.html,
the JS glue and the wasm. That folder is what GitHub Pages serves. --dev
builds the debug profile instead, faster and with readable panics.

serve: builds, then serves target/web-demo/ on localhost. Open the page in
any browser; with ?smoke the page steps through every quality tier and posts
its report back, and the report is printed here and saved under
target/web-smoke/. That is the by-hand smoke test for Safari, Firefox or a
phone.

smoke: builds, serves, opens the page with ?smoke in a Chromium browser on a
fresh profile, and waits for its report. Exits 0 if every tier drew blades
and the page logged no error, 1 otherwise. The browser is the first of Edge,
Chrome and Brave found, or --browser. --headless needs no window; Edge and
Chrome 155 on Windows draw with the real GPU in it. --max-storage-buffers N
asks WebGPU for a device with at most N storage buffers per shader stage
(the page's ?max-storage-buffers=N): at 10 every pipeline must still pass,
as on a Mac that reports 10; below 10 the expected outcome is "no grass".

Needs `rustup target add wasm32-unknown-unknown` and the wasm-bindgen CLI at
the version in Cargo.lock (see tools/web_example.py); wasm-opt is optional
and takes about a tenth off the download.

WebGPU needs a secure context: http://localhost works, a LAN address does
not. To test a phone, forward the port over USB (chrome://inspect, "Port
forwarding") and open localhost on the phone.
"""

import argparse
import datetime
import functools
import gzip
import http.server
import json
import os
import queue
import shutil
import subprocess
import sys
import tempfile
import threading

from web_example import Handler, check_tools

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
TARGET = "wasm32-unknown-unknown"
PACKAGE = "bevy_grass_web"
SITE = os.path.join(ROOT, "target", "web-demo")
REPORTS = os.path.join(ROOT, "target", "web-smoke")

# Chromium browsers the smoke test can drive, in the order it looks for them.
BROWSERS = {
    "win32": [
        r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
        r"C:\Program Files\Google\Chrome\Application\chrome.exe",
        r"C:\Program Files\BraveSoftware\Brave-Browser\Application\brave.exe",
    ],
    "darwin": [
        "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "/Applications/Brave Browser.app/Contents/MacOS/Brave Browser",
    ],
    "linux": ["microsoft-edge", "google-chrome", "chromium", "brave-browser"],
}


def build(dev):
    check_tools()
    profile = "dev" if dev else "web"
    subprocess.run(["cargo", "build", "-p", PACKAGE, "--target", TARGET, "--profile", profile],
                   cwd=ROOT, check=True)
    wasm = os.path.join(ROOT, "target", TARGET, "debug" if dev else profile, PACKAGE + ".wasm")
    shutil.rmtree(SITE, ignore_errors=True)
    command = ["wasm-bindgen", "--target", "web", "--no-typescript",
               "--out-dir", SITE, "--out-name", PACKAGE, wasm]
    # Function names are about half a release build. A --dev build keeps
    # them, so its panics show readable stack traces.
    if not dev:
        command.append("--remove-name-section")
    subprocess.run(command, check=True)
    out = os.path.join(SITE, PACKAGE + "_bg.wasm")
    if not dev:
        optimise(out)
    shutil.copy(os.path.join(ROOT, "web", "index.html"), SITE)
    with open(out, "rb") as wasm_file:
        data = wasm_file.read()
    print(f"web demo: target/web-demo/ ({len(data) / 2**20:.1f} MiB of wasm, "
          f"{len(gzip.compress(data, 6)) / 2**20:.1f} MiB gzipped, {profile} profile)")


def optimise(wasm):
    """Runs wasm-opt for size, if binaryen is installed."""
    if not shutil.which("wasm-opt"):
        print("wasm-opt not found: the wasm is not optimised further. Install binaryen "
              "(https://github.com/WebAssembly/binaryen/releases) for a smaller download.")
        return
    before = os.path.getsize(wasm)
    subprocess.run(["wasm-opt", "-Os", "--enable-bulk-memory", "--enable-nontrapping-float-to-int",
                    "--enable-sign-ext", "--enable-reference-types", "--enable-multivalue",
                    "-o", wasm, wasm], check=True)
    print(f"wasm-opt: {before / 2**20:.1f} -> {os.path.getsize(wasm) / 2**20:.1f} MiB")


class SmokeHandler(Handler):
    """Serves the site and takes the smoke reports pages post to /smoke."""

    reports = None  # set to a queue.Queue by serve()

    def do_POST(self):
        if self.path != "/smoke":
            self.send_error(404)
            return
        body = self.rfile.read(int(self.headers.get("Content-Length", 0)))
        self.send_response(204)
        self.end_headers()
        try:
            report = json.loads(body)
        except ValueError:
            return
        self.reports.put(report)

    def log_message(self, format, *args):
        # Every file request is noise; errors still print.
        if not args or not str(args[1]).startswith(("2", "3")):
            super().log_message(format, *args)


def save(report):
    """Prints a smoke report's summary and saves it whole."""
    os.makedirs(REPORTS, exist_ok=True)
    stamp = datetime.datetime.now().strftime("%Y%m%d-%H%M%S")
    path = os.path.join(REPORTS, f"{stamp}.json")
    with open(path, "w", encoding="utf-8", newline="\n") as out:
        json.dump(report, out, indent=2)
    limit = report.get("limits", {}).get("maxStorageBuffersPerShaderStage", "?")
    asked = f" (asked for {report['maxStorageBuffers']})" if report.get("maxStorageBuffers") else ""
    print(f"\nsmoke: {'PASS' if report.get('pass') else 'FAIL'}: {report.get('outcome')}")
    print(f"  browser  {report.get('userAgent')}")
    print(f"  adapter  {report.get('adapter')}, {limit} storage buffers per stage{asked}")
    for tier in report.get("tiers", []):
        frame = tier.get("frameMs") or {}
        print(f"  {tier['quality']:<7}  {tier['blades']:>9,} blades, "
              f"frame p50 {frame.get('p50', '?')} ms, p95 {frame.get('p95', '?')} ms")
    for error in report.get("errors", [])[:10]:
        print(f"  error    {error[:300]}")
    for warning in report.get("warnings", [])[:10]:
        print(f"  warning  {warning[:300]}")
    print(f"  saved    {os.path.relpath(path, ROOT)}")


def serve(host, port):
    """Serves the site on a thread; returns the server and the report queue."""
    reports = queue.Queue()
    handler = functools.partial(type("Bound", (SmokeHandler,), {"reports": reports}),
                                directory=SITE)
    server = http.server.ThreadingHTTPServer((host, port), handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    return server, reports


def find_browser(given):
    if given:
        return given
    for candidate in BROWSERS.get(sys.platform, BROWSERS["linux"]):
        found = shutil.which(candidate) or (os.path.isfile(candidate) and candidate)
        if found:
            return found
    sys.exit("smoke: no Chromium browser found; pass --browser PATH")


def smoke(args):
    browser = find_browser(args.browser)
    server, reports = serve(args.host, args.port)
    url = f"http://localhost:{server.server_port}/?smoke"
    if args.max_storage_buffers:
        url += f"&max-storage-buffers={args.max_storage_buffers}"
    profile = tempfile.mkdtemp(prefix="grass-smoke-")
    command = [browser, f"--user-data-dir={profile}", "--no-first-run",
               "--no-default-browser-check", "--enable-unsafe-webgpu", url]
    if args.headless:
        command.insert(1, "--headless=new")
    print(f"smoke: {os.path.basename(browser)} on {url}")
    process = subprocess.Popen(command, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        report = reports.get(timeout=args.timeout)
    except queue.Empty:
        report = {"pass": False, "outcome": f"no report within {args.timeout} s"}
    finally:
        process.terminate()
        server.shutdown()
        shutil.rmtree(profile, ignore_errors=True)
    save(report)
    return 0 if report.get("pass") else 1


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("command", choices=["build", "serve", "smoke"])
    parser.add_argument("--dev", action="store_true", help="debug build, faster to make")
    parser.add_argument("--no-build", action="store_true", help="use the site already built")
    parser.add_argument("--port", type=int, default=8080, help="0 picks a free port")
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--browser", help="the Chromium browser for smoke")
    parser.add_argument("--headless", action="store_true",
                        help="smoke without a window (needs a browser whose headless mode has WebGPU)")
    parser.add_argument("--timeout", type=int, default=300, help="seconds smoke waits for a report")
    parser.add_argument("--max-storage-buffers", type=int,
                        help="smoke on a device with at most this many storage buffers per stage")
    args = parser.parse_args()

    if not args.no_build or not os.path.isdir(SITE):
        build(args.dev)
    if args.command == "build":
        return 0
    if args.command == "smoke":
        return smoke(args)
    server, reports = serve(args.host, args.port)
    print(f"Serving http://localhost:{server.server_port}/ (Ctrl+C to stop); "
          f"add ?smoke for the smoke test")
    try:
        while True:
            # A timeout, so Ctrl+C is noticed on Windows too.
            try:
                save(reports.get(timeout=1))
            except queue.Empty:
                pass
    except KeyboardInterrupt:
        server.shutdown()
    return 0


if __name__ == "__main__":
    sys.exit(main())
