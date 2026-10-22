"""Builds examples for the browser (WebGPU) and serves them.

    python tools/web_example.py [EXAMPLE ...] [--dev] [--port 8080]
                                [--host 127.0.0.1] [--no-serve]

Builds each example (default: minimal and meadow) for wasm32-unknown-unknown,
runs wasm-bindgen on it and writes a page per example to target/web/<name>/,
with a copy of assets/ next to it. Then serves target/web/ until Ctrl+C.

Each page carries a report panel: the browser, the adapter, the WebGPU
limits the crate depends on (maxStorageBuffersPerShaderStage first), the
optional features it uses, every error and warning the page logs, and the
frame time measured with requestAnimationFrame. "Copy report" puts it on the
clipboard as JSON, which is how a browser is recorded for docs/web.md.

Needs `rustup target add wasm32-unknown-unknown` and the wasm-bindgen CLI at
the version in Cargo.lock:

    cargo install wasm-bindgen-cli --version <version> --locked

WebGPU needs a secure context: http://localhost works, a LAN address does
not. To test a phone, forward the port over USB (chrome://inspect, "Port
forwarding") and open localhost on the phone, or serve over HTTPS.
"""

import argparse
import functools
import http.server
import os
import re
import shutil
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
TARGET = "wasm32-unknown-unknown"
OUT = os.path.join(ROOT, "target", "web")
DEFAULT_EXAMPLES = ["minimal", "meadow"]


def locked_wasm_bindgen():
    """The wasm-bindgen version in Cargo.lock, which the CLI must match."""
    with open(os.path.join(ROOT, "Cargo.lock"), encoding="utf-8") as lock:
        found = re.search(r'name = "wasm-bindgen"\nversion = "([^"]+)"', lock.read())
    return found.group(1) if found else None


def check_tools():
    wanted = locked_wasm_bindgen()
    try:
        have = subprocess.run(
            ["wasm-bindgen", "--version"], capture_output=True, text=True, check=True
        ).stdout.split()[-1]
    except (OSError, subprocess.CalledProcessError):
        sys.exit(f"wasm-bindgen not found: cargo install wasm-bindgen-cli --version {wanted} --locked")
    if wanted and have != wanted:
        sys.exit(f"wasm-bindgen {have} does not match Cargo.lock's {wanted}: "
                 f"cargo install wasm-bindgen-cli --version {wanted} --locked")


def build(name, dev):
    profile = "debug" if dev else "release"
    command = ["cargo", "build", "--target", TARGET, "--example", name]
    if not dev:
        command.append("--release")
    subprocess.run(command, cwd=ROOT, check=True)
    wasm = os.path.join(ROOT, "target", TARGET, profile, "examples", name + ".wasm")
    page = os.path.join(OUT, name)
    shutil.rmtree(page, ignore_errors=True)
    command = ["wasm-bindgen", "--target", "web", "--no-typescript",
               "--out-dir", page, "--out-name", name, wasm]
    # Function names are about half a release build (60 of 125 MiB). A
    # --dev build keeps them, so its panics show readable stack traces.
    if not dev:
        command.append("--remove-name-section")
    subprocess.run(command, check=True)
    shutil.copytree(os.path.join(ROOT, "assets"), os.path.join(page, "assets"))
    with open(os.path.join(page, "index.html"), "w", encoding="utf-8", newline="\n") as html:
        html.write(PAGE.replace("{{NAME}}", name).replace("{{PROFILE}}", profile))
    size = os.path.getsize(os.path.join(page, name + "_bg.wasm")) / 2**20
    print(f"{name}: target/web/{name}/ ({size:.1f} MiB of wasm, {profile})")


def write_index(names):
    links = "\n".join(f'<li><a href="{n}/">{n}</a></li>' for n in sorted(names))
    with open(os.path.join(OUT, "index.html"), "w", encoding="utf-8", newline="\n") as html:
        html.write(f"<!doctype html><meta charset=utf-8><title>bevy_grass on the web</title>"
                   f"<h1>bevy_grass examples</h1><ul>{links}</ul>\n")


class Handler(http.server.SimpleHTTPRequestHandler):
    """Serves wasm with its MIME type and asks the browser not to cache, so a
    rebuild shows on reload."""

    extensions_map = {**http.server.SimpleHTTPRequestHandler.extensions_map,
                      ".wasm": "application/wasm", ".js": "text/javascript"}

    def end_headers(self):
        self.send_header("Cache-Control", "no-store")
        super().end_headers()


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("examples", nargs="*", default=DEFAULT_EXAMPLES)
    parser.add_argument("--dev", action="store_true", help="debug build, faster to make")
    parser.add_argument("--port", type=int, default=8080)
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--no-serve", action="store_true", help="build only")
    args = parser.parse_args()

    check_tools()
    os.makedirs(OUT, exist_ok=True)
    for name in args.examples:
        build(name, args.dev)
    built = [n for n in os.listdir(OUT) if os.path.isdir(os.path.join(OUT, n))]
    write_index(built)
    if args.no_serve:
        return
    handler = functools.partial(Handler, directory=OUT)
    server = http.server.ThreadingHTTPServer((args.host, args.port), handler)
    print(f"Serving http://localhost:{args.port}/ (Ctrl+C to stop)")
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass


# The page each example runs in. The report is plain JavaScript beside the
# example, so it reads the same limits the browser gives Bevy and keeps
# working when the example itself fails.
PAGE = r"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>bevy_grass: {{NAME}}</title>
<style>
  html, body { margin: 0; height: 100%; background: #111; overflow: hidden; }
  canvas { display: block; outline: none; }
  #report { position: fixed; top: 8px; left: 8px; max-width: min(560px, calc(100vw - 32px));
    max-height: calc(100vh - 32px); overflow: auto; padding: 8px 12px; border-radius: 6px;
    background: rgba(0, 0, 0, 0.78); color: #eee; font: 12px/1.45 ui-monospace, monospace; }
  #report summary { cursor: pointer; font-weight: 600; }
  #report .bad { color: #ff8a80; } #report .ok { color: #a5d6a7; }
  #report pre { white-space: pre-wrap; margin: 4px 0; }
  #report button { font: inherit; margin-top: 6px; }
</style>
</head>
<body>
<details id="report" open>
  <summary>{{NAME}} ({{PROFILE}}): <span id="headline">starting</span></summary>
  <pre id="facts"></pre>
  <pre id="errors"></pre>
  <button id="copy">Copy report</button>
</details>
<script>
  // Set up before the example loads, so nothing it logs is missed.
  const report = { example: "{{NAME}}", profile: "{{PROFILE}}", userAgent: navigator.userAgent,
    adapter: null, limits: {}, features: [], errors: [], warnings: [], frameMs: null };
  const keep = (list, args) => {
    const text = Array.from(args, String).join(" ");
    // winit ends its start-up with this exception on purpose.
    if (text.includes("Using exceptions for control flow")) return;
    if (list.length < 50) list.push(text.slice(0, 2000));
    render();
  };
  for (const [level, list] of [["error", report.errors], ["warn", report.warnings]]) {
    const original = console[level].bind(console);
    console[level] = (...args) => { keep(list, args); original(...args); };
  }
  addEventListener("error", (e) => keep(report.errors, [e.message]));
  addEventListener("unhandledrejection", (e) => keep(report.errors, [e.reason]));

  // The limits and features bevy_grass depends on.
  const LIMITS = ["maxStorageBuffersPerShaderStage", "maxStorageBufferBindingSize",
    "maxBufferSize", "maxBindGroups", "maxUniformBuffersPerShaderStage",
    "maxComputeWorkgroupStorageSize", "maxComputeInvocationsPerWorkgroup",
    "maxInterStageShaderVariables"];
  const FEATURES = ["depth-clip-control", "timestamp-query", "indirect-first-instance",
    "float32-filterable"];
  // What the crate needs today and what the plan targets.
  const NEEDS_NOW = 14, TARGET = 10;

  async function probe() {
    if (!navigator.gpu) { report.adapter = "navigator.gpu missing: no WebGPU here"; return render(); }
    const adapter = await navigator.gpu.requestAdapter({ powerPreference: "high-performance" });
    if (!adapter) { report.adapter = "requestAdapter returned null"; return render(); }
    const info = adapter.info || {};
    report.adapter = [info.vendor, info.architecture, info.device, info.description]
      .filter(Boolean).join(" / ") || "(no adapter info)";
    for (const name of LIMITS) report.limits[name] = adapter.limits[name];
    report.features = FEATURES.filter((f) => adapter.features.has(f));
    render();
  }

  // Frame time from requestAnimationFrame: p50 and p95 of the last 300
  // frames, refreshed every second. It measures the page, so it includes
  // everything the browser does, not only the GPU.
  const deltas = [];
  let last = 0, lastShown = 0;
  function frame(now) {
    if (last) { deltas.push(now - last); if (deltas.length > 300) deltas.shift(); }
    last = now;
    if (now - lastShown > 1000 && deltas.length > 30) {
      lastShown = now;
      const sorted = [...deltas].sort((a, b) => a - b);
      const at = (q) => sorted[Math.min(sorted.length - 1, Math.floor(q * sorted.length))];
      report.frameMs = { p50: +at(0.5).toFixed(2), p95: +at(0.95).toFixed(2), frames: sorted.length,
        canvas: (() => { const c = document.querySelector("canvas");
          return c ? `${c.width}x${c.height}` : null; })() };
      render();
    }
    requestAnimationFrame(frame);
  }
  requestAnimationFrame(frame);

  function render() {
    const storage = report.limits.maxStorageBuffersPerShaderStage;
    const fit = storage === undefined ? "" : storage >= NEEDS_NOW
      ? `<span class="ok">fits today's ${NEEDS_NOW}</span>`
      : storage >= TARGET ? `<span class="bad">below today's ${NEEDS_NOW}, fits the target ${TARGET}</span>`
      : `<span class="bad">below the target ${TARGET}</span>`;
    document.getElementById("headline").innerHTML =
      `${report.errors.length} errors, ${report.warnings.length} warnings` +
      (report.frameMs ? `, ${report.frameMs.p50} ms p50` : "");
    const lines = [
      `adapter   ${report.adapter ?? "probing"}`,
      `storage   ${storage ?? "?"} per stage ${fit}`,
      ...LIMITS.slice(1).map((n) => `          ${n} ${report.limits[n] ?? "?"}`),
      `features  ${report.features.join(", ") || "none of " + FEATURES.join(", ")}`,
      `frame     ${report.frameMs ? `p50 ${report.frameMs.p50} ms, p95 ${report.frameMs.p95} ms at ${report.frameMs.canvas}` : "measuring"}`,
      `browser   ${report.userAgent}`,
    ];
    document.getElementById("facts").innerHTML = lines.join("\n");
    const escape = (s) => s.replace(/[&<>]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;" })[c]);
    document.getElementById("errors").innerHTML =
      report.errors.map((e) => `<span class="bad">error</span> ${escape(e)}`).concat(
      report.warnings.map((w) => `warn  ${escape(w)}`)).join("\n");
  }
  document.getElementById("copy").addEventListener("click", () =>
    navigator.clipboard.writeText(JSON.stringify(report, null, 2)));
  probe().catch((e) => keep(report.errors, ["probe: " + e]));
  window.grassReport = report;
</script>
<script type="module">
  import init from "./{{NAME}}.js";
  init().catch((e) => {
    if (!String(e).includes("Using exceptions for control flow")) console.error("init: " + e);
  });
</script>
</body>
</html>
"""

if __name__ == "__main__":
    main()
