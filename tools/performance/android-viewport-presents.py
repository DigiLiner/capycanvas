#!/usr/bin/env python3
"""Run AndroidViewportBenchmarkTest under a SurfaceFlinger trace and report
presented canvas frames, presentation gaps and the viewport pass's GPU time.

Install the benchmark APK and its benchmark test APK under your own
application ID first. Presents join the canvas SurfaceView's buffer numbers to
SurfaceFlinger's actual display-frame times, so no clock conversion is needed.
"""
import argparse
import csv
import io
import json
import math
import pathlib
import subprocess
import time


def quantile(values, q):
    values = sorted(values)
    return values[max(0, math.ceil(len(values) * q) - 1)] if values else None


def presents(processor, trace, package):
    query = f"""SELECT s.name latch, f.ts + f.dur presented_at
FROM slice s JOIN ancestor_slice(s.id) a ON a.name GLOB 'commit [0-9]*'
JOIN actual_frame_timeline_slice f ON f.layer_name IS NULL AND CAST(f.name AS INT) = CAST(SUBSTR(a.name, 8) AS INT)
WHERE s.name GLOB 'latchBuffer *SurfaceView*{package}/*' ORDER BY presented_at"""
    out = subprocess.run([processor, "query", str(trace), query], capture_output=True, text=True, check=True).stdout
    rows = list(csv.DictReader(io.StringIO(out[out.index('"latch"'):]))) if '"latch"' in out else []
    return {int(r["latch"].rsplit(" ", 1)[1]): int(r["presented_at"]) for r in rows}


def summarize(run, shown):
    begin, end = run["begin_ns"], run["end_ns"]
    frames = sorted(row[0] for row in run["completions"] if begin <= row[1] <= end)
    times = sorted({shown[f] for f in frames if f in shown})
    gaps = [(b - a) / 1e6 for a, b in zip(times, times[1:])]
    gpu = [sample[1] / 1e6 for batch in run["presentation"] for sample in batch if sample[2] == 1]
    return dict(presented_per_s=(len(times) - 1) * 1e9 / (times[-1] - times[0]) if len(times) > 1 else None,
                presented_gap_p99_ms=quantile(gaps, .99), submitted_per_s=len(frames) / ((end - begin) / 1e9),
                viewport_gpu_p50_ms=quantile(gpu, .5), viewport_gpu_p95_ms=quantile(gpu, .95))


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("output", type=pathlib.Path)
    p.add_argument("--serial", required=True)
    p.add_argument("--package", required=True)
    p.add_argument("--processor", required=True, help="Perfetto trace_processor_shell")
    p.add_argument("--photo", required=True)
    p.add_argument("--width", type=int, required=True)
    p.add_argument("--height", type=int, required=True)
    p.add_argument("--motions", default="pan,pinch,rotate")
    p.add_argument("--label", required=True)
    p.add_argument("--duration-ms", type=int, default=5000, help="Gesture length; slow tiers need more than 100 GPU samples per run")
    p.add_argument("--extra", default="", help="Further instrumentation arguments as key=value,key=value")
    args = p.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    adb = ["adb", "-s", args.serial]
    remote = f"/sdcard/Android/data/{args.package}/files/viewport-benchmark"
    config = '''buffers { size_kb: 65536 fill_policy: RING_BUFFER }
duration_ms: 600000
write_into_file: true
file_write_period_ms: 2000
data_sources { config { name: "linux.ftrace" ftrace_config { ftrace_events: "ftrace/print" atrace_categories: "gfx" } } }
data_sources { config { name: "android.surfaceflinger.frametimeline" } }
'''
    summary = {}
    for motion in args.motions.split(","):
        label = f"{args.label}-{motion}"
        cmd = adb + ["shell", "am", "instrument", "-w", "-e", "class", "art.capycanvas.AndroidViewportBenchmarkTest#retainedViewport",
                     "-e", "viewportBenchmark", "true", "-e", "motion", motion, "-e", "photo", args.photo,
                     "-e", "width", str(args.width), "-e", "height", str(args.height), "-e", "durationMs", str(args.duration_ms),
                     "-e", "repeats", "3", "-e", "label", label]
        if max(args.width, args.height) > 8192:
            cmd += ["-e", "openQueryPhoto", "true"]
        for item in filter(None, args.extra.split(",")):
            key, value = item.split("=", 1)
            cmd += ["-e", key, value]
        cmd += [f"{args.package}.test/androidx.test.runner.AndroidJUnitRunner"]
        trace = f"/data/misc/perfetto-traces/{args.package}-{label}.perfetto-trace"
        session = f"{args.package}-{label}"
        subprocess.run(adb + ["shell", "perfetto", "--txt", "-c", "-", "-o", trace, f"--detach={session}"],
                       input=config, text=True, check=True, capture_output=True)
        try:
            time.sleep(1)
            log = subprocess.run(cmd, capture_output=True, text=True).stdout
        finally:
            subprocess.run(adb + ["shell", "perfetto", f"--attach={session}", "--stop"], capture_output=True)
        (args.output / f"{label}-instrument.log").write_text(log)
        local = args.output / f"{label}.perfetto-trace"
        subprocess.run(adb + ["pull", trace, str(local)], check=True, capture_output=True)
        subprocess.run(adb + ["shell", "rm", "-f", trace], capture_output=True)
        if "OK (1 test)" not in log:
            summary[motion] = {"error": log[-2000:]}
            continue
        for suffix in ["info", "0", "1", "2"]:
            subprocess.run(adb + ["pull", f"{remote}/{label}-{suffix}.json", str(args.output)], check=True, capture_output=True)
        shown = presents(args.processor, local, args.package)
        info = json.loads((args.output / f"{label}-info.json").read_text())
        runs = [summarize(json.loads((args.output / f"{label}-{i}.json").read_text()), shown) for i in range(3)]
        summary[motion] = {"zoom": info["state"]["camera"]["zoom"], "work_area": info["state"]["camera"]["work_area"],
                           "glass_regions": info["display"].get("glass_regions"), "thermal_status": info.get("thermal_status"),
                           "runs": runs}
        print(motion, json.dumps(summary[motion]), flush=True)
    (args.output / f"{args.label}-summary.json").write_text(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()
