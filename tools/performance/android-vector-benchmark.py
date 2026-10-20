#!/usr/bin/env python3
"""Drive the release vector-layer benchmark on a crowded_vector_fixture document.

Generate the fixture with `cargo run --release -p layer-core --example
crowded_vector_fixture -- OUT.capy WIDTH HEIGHT PATHS`, push it under
/data/local/tmp and pass the same WIDTH, HEIGHT and PATHS. The planner replays
the fixture's hatching to choose marquees, press points and anchors in document
coordinates. Each unit (scenario and selection size) records its own
SurfaceFlinger trace; summarize with android-vector-report.py.
"""
import argparse
import json
import math
import pathlib
import subprocess
import time

MASK = (1 << 64) - 1
SCENARIOS = "move,scale,rotate,points,erase,width,pan,pinch,rotate_view,fill,commit,merge,open"


def hatching(extent, count):
    state = 0x9E3779B97F4A7C15

    def uniform():
        nonlocal state
        state ^= (state << 13) & MASK
        state ^= state >> 7
        state ^= (state << 17) & MASK
        return (state >> 11) / float(1 << 53)

    def between(a, b):
        return a + (b - a) * uniform()

    paths = []
    for i in range(count):
        start = [between(0., extent[0]), between(0., extent[1])]
        angle, length, bend = between(0.4, 0.9), between(40., 220.), between(-20., 20.)
        points = []
        for k in range(4):
            t = k / 3.
            points.append((start[0] + math.cos(angle) * length * t - math.sin(angle) * bend * t * (1. - t) * 4.,
                           start[1] + math.sin(angle) * length * t + math.cos(angle) * bend * t * (1. - t) * 4.))
        uniform(), uniform(), uniform()
        width = between(2., 6.)
        xs, ys = [p[0] for p in points], [p[1] for p in points]
        reach = width / 2
        paths.append(dict(id=i + 1, points=points, width=width,
                          bounds=(min(xs) - reach, min(ys) - reach, max(xs) + reach, max(ys) + reach)))
    return paths


class Grid:
    def __init__(self, paths, cell=256.):
        self.cell, self.cells = cell, {}
        for index, path in enumerate(paths):
            x0, y0, x1, y1 = path["bounds"]
            for cx in range(int(x0 // cell), int(x1 // cell) + 1):
                for cy in range(int(y0 // cell), int(y1 // cell) + 1):
                    self.cells.setdefault((cx, cy), []).append(index)

    def near(self, point, radius):
        found = set()
        for cx in range(int((point[0] - radius) // self.cell), int((point[0] + radius) // self.cell) + 1):
            for cy in range(int((point[1] - radius) // self.cell), int((point[1] + radius) // self.cell) + 1):
                found.update(self.cells.get((cx, cy), ()))
        return found


def segment_distance(p, a, b):
    dx, dy = b[0] - a[0], b[1] - a[1]
    length = dx * dx + dy * dy
    t = 0. if length == 0 else max(0., min(1., ((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / length))
    return math.hypot(a[0] + dx * t - p[0], a[1] + dy * t - p[1])


def stroke_distance(p, path):
    return min(segment_distance(p, a, b) for a, b in zip(path["points"], path["points"][1:])) - path["width"] / 2


def clearance(paths, grid, point, allowed, radius=400.):
    nearest = radius
    for index in grid.near(point, radius):
        if index not in allowed:
            nearest = min(nearest, stroke_distance(point, paths[index]))
    return nearest


def inside(bounds, rect):
    return bounds[0] >= rect[0] and bounds[1] >= rect[1] and bounds[2] <= rect[2] and bounds[3] <= rect[3]


def best_press(paths, grid, chosen, allowed, bounds):
    centre = ((bounds[0] + bounds[2]) / 2, (bounds[1] + bounds[3]) / 2)
    quarter = ((bounds[2] - bounds[0]) / 4, (bounds[3] - bounds[1]) / 4)
    candidates = []
    for index in chosen:
        pts = paths[index]["points"]
        point = ((pts[1][0] + pts[2][0]) / 2, (pts[1][1] + pts[2][1]) / 2)
        if abs(point[0] - centre[0]) <= quarter[0] and abs(point[1] - centre[1]) <= quarter[1]:
            candidates.append((math.hypot(point[0] - centre[0], point[1] - centre[1]), index, point))
    candidates.sort()
    best = None
    for _, index, point in candidates[:400]:
        value = clearance(paths, grid, point, allowed)
        if best is None or value > best[0]:
            best = (value, index, point)
    return best


def best_anchor(paths, chosen):
    anchors = [(index, k, paths[index]["points"][k]) for index in chosen for k in (1, 2)]
    best = None
    for index, k, point in anchors[:2000]:
        others = [math.hypot(point[0] - q[0], point[1] - q[1]) for j, kk, q in anchors if (j, kk) != (index, k)]
        value = min(others) if others else 1e9
        if best is None or value > best[0]:
            best = (value, point)
    return best


def marquee(paths, grid, size, origin, extent):
    x0, y0 = origin
    w, h = extent

    def chosen(scale):
        rect = (x0, y0, x0 + scale * (w - x0), y0 + scale * (h - y0))
        return rect, [i for i, path in enumerate(paths) if inside(path["bounds"], rect)]
    lo, hi = 0., 1.
    for _ in range(60):
        mid = (lo + hi) / 2
        if len(chosen(mid)[1]) < size:
            lo = mid
        else:
            hi = mid
    upper = hi
    lo, hi = upper, 1.
    for _ in range(60):
        mid = (lo + hi) / 2
        if len(chosen(mid)[1]) <= size:
            lo = mid
        else:
            hi = mid
    rect, selected = chosen((upper + lo) / 2)
    union = [min(paths[i]["bounds"][0] for i in selected), min(paths[i]["bounds"][1] for i in selected),
             max(paths[i]["bounds"][2] for i in selected), max(paths[i]["bounds"][3] for i in selected)]
    value, index, point = best_press(paths, grid, selected, set(selected), union)
    anchor = best_anchor(paths, selected)
    return dict(count=len(selected), rect=list(rect), start_clearance=clearance(paths, grid, (rect[0], rect[1]), set()),
                plateau=[upper, lo], press=list(point), press_clearance=value,
                anchor=list(anchor[1]), anchor_spacing=anchor[0], bounds=union)


def plan(extent, count, sizes=(1, 100, 5000)):
    paths = hatching(extent, count)
    grid = Grid(paths)
    w, h = extent
    margin = 80.
    selections = {}
    for size in sizes:
        if size == 1:
            centre = (w * .55, h * .5)
            order = sorted(range(len(paths)), key=lambda i: math.hypot(paths[i]["bounds"][0] - centre[0], paths[i]["bounds"][1] - centre[1]))[:3000]
            value, index, point = max((clearance(paths, grid, point, {index}), index, point) for index in order
                                      for point in [((a[0] + b[0]) / 2, (a[1] + b[1]) / 2) for a, b in zip(paths[index]["points"], paths[index]["points"][1:])])
            selections["1"] = [dict(count=1, ids=[paths[index]["id"]], press=list(point), press_clearance=value,
                                    anchor=list(paths[index]["points"][1]), bounds=list(paths[index]["bounds"]))]
            continue
        selections[str(size)] = [marquee(paths, grid, size, origin, extent) for origin in ((w * .04, -margin), (-margin, h * .04))]
    ellipse = [w * .62, h * .36, w * .78, h * .6]
    fill_inside = [(ellipse[0] + ellipse[2]) / 2, (ellipse[1] + ellipse[3]) / 2]
    best = None
    for gx in range(40):
        for gy in range(30):
            point = (w * (.08 + .4 * gx / 39), h * (.3 + .5 * gy / 29))
            value = clearance(paths, grid, point, set(), 300.)
            if best is None or value > best[0]:
                best = (value, point)
    return dict(extent=list(extent), paths=count, selections=selections, ellipse=ellipse,
                fill_inside=fill_inside, fill_below=list(best[1]), fill_below_clearance=best[0])


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("output", type=pathlib.Path)
    p.add_argument("--adb", default="adb")
    p.add_argument("--serial", required=True)
    p.add_argument("--package", required=True)
    p.add_argument("--fixture", required=True, help="crowded_vector_fixture .capy under /data/local/tmp")
    p.add_argument("--canvas-width", type=int, required=True)
    p.add_argument("--canvas-height", type=int, required=True)
    p.add_argument("--paths", type=int, required=True, help="PATHS given to crowded_vector_fixture")
    p.add_argument("--scenarios", default=SCENARIOS)
    p.add_argument("--duration", type=int, default=5000)
    p.add_argument("--repeats", type=int, default=3)
    p.add_argument("--eraser-size", type=float, default=64)
    p.add_argument("--ink-size", type=float, default=24)
    p.add_argument("--skip-fill-below", action="store_true", help="Measure Fill inside a closed path only")
    p.add_argument("--trace", action="store_true", help="Add scheduling and app trace sections for attribution")
    p.add_argument("--stats", action="store_true", help="Open Stats for GPU phase timestamps; diagnostic only")
    p.add_argument("--show-layers", default="", help="Comma-separated layer names to show before measuring, such as Paper")
    p.add_argument("--hide-layers", default="", help="Comma-separated layer names to hide before measuring")
    p.add_argument("--profile-seconds", type=int, default=0, help="Sample the app with simpleperf for this long from the start of each unit")
    p.add_argument("--prefix", default="vector")
    args = p.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    adb = [args.adb, "-s", args.serial]

    def run(*items, **kw):
        return subprocess.run(adb + list(items), check=True, **kw)

    label = f"{args.prefix}-{time.strftime('%Y%m%d%H%M%S')}"
    layout = plan((args.canvas_width, args.canvas_height), args.paths)
    (args.output / f"{label}-plan.json").write_text(json.dumps(layout, indent=2))
    remote_plan = f"/data/local/tmp/{args.package}-{label}-plan.json"
    run("push", str(args.output / f"{label}-plan.json"), remote_plan, stdout=subprocess.DEVNULL)
    remote = f"/sdcard/Android/data/{args.package}/files/brush-benchmark"
    cmd = adb + ["shell", "am", "instrument", "-w", "-r"]
    for key, value in dict(brushBenchmark="true", mode="vector", label=label, preset=1, brushSize=args.ink_size,
                           photo=args.fixture, width=args.canvas_width, height=args.canvas_height,
                           paintLayerName="Ink", vectorPlan=remote_plan, vectorScenarios=args.scenarios,
                           durationMs=args.duration, repeats=args.repeats, eraserSize=args.eraser_size,
                           inkSize=args.ink_size, fillBelow=str(not args.skip_fill_below).lower(), statsPanel=str(args.stats).lower(),
                           waitForTrace="true").items():
        cmd += ["-e", key, str(value)]
    for key, value in (("vectorShow", args.show_layers), ("vectorHide", args.hide_layers)):
        if value:
            cmd += ["-e", key, value]
    cmd += [f"{args.package}/art.capycanvas.BrushBenchmarkInstrumentation"]
    app_trace = (f'ftrace_events: "sched/sched_switch"\nftrace_events: "sched/sched_waking"\n'
                 f'atrace_apps: "{args.package}"\n' if args.trace else "")
    config = f'''buffers {{ size_kb: {262144 if args.trace else 65536} fill_policy: RING_BUFFER }}
duration_ms: 900000
write_into_file: true
file_write_period_ms: 2000
data_sources {{ config {{ name: "linux.ftrace" ftrace_config {{
ftrace_events: "ftrace/print"
atrace_categories: "gfx"
{app_trace}}} }} }}
data_sources {{ config {{ name: "linux.process_stats" process_stats_config {{ scan_all_processes_on_start: true }} }} }}
data_sources {{ config {{ name: "android.surfaceflinger.frametimeline" }} }}
'''
    (args.output / f"{label}.pbtxt").write_text(config)
    traces = []
    active = []
    profiles = []

    def stop_profile():
        while profiles:
            unit, profile = profiles.pop()
            profile.wait(timeout=args.profile_seconds + 120)
            remote_data = f"/data/local/tmp/{args.package}-{unit}.perf.data"
            subprocess.run(adb + ["pull", remote_data, str(args.output / f"{label}-{unit}.perf.data")], capture_output=True)
            subprocess.run(adb + ["shell", "rm", "-f", remote_data], capture_output=True)

    def stop_trace():
        while active:
            subprocess.run(adb + ["shell", "perfetto", f"--attach={active.pop()}", "--stop"], capture_output=True)
    with (args.output / f"{label}-instrumentation.txt").open("w") as log:
        process = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
        try:
            for line in process.stdout:
                log.write(line)
                log.flush()
                words = line.replace("stream=", " ").split()
                if "VECTOR_READY" in words:
                    unit = words[words.index("VECTOR_READY") + 2]
                    with (args.output / f"{label}-{unit}-environment-before.txt").open("w") as out:
                        run("shell", "dumpsys thermalservice | grep -E 'Thermal Status|mValue'; cat /proc/meminfo | head -3", stdout=out)
                    trace = f"/data/misc/perfetto-traces/{args.package}-{label}-{unit}.perfetto-trace"
                    session = f"{label}-{unit}"
                    started = subprocess.run(adb + ["shell", "perfetto", "--txt", "-c", "-", "-o", trace, f"--detach={session}"],
                                             input=config, text=True, capture_output=True)
                    if started.returncode:
                        raise RuntimeError(f"Perfetto did not start for {unit}: {started.stderr.strip()}")
                    active.append(session)
                    traces.append((unit, trace))
                    if args.profile_seconds:
                        profile = subprocess.Popen(adb + ["shell", "simpleperf", "record", "--app", args.package, "-e", "cpu-clock",
                            "-f", "499", "--call-graph", "dwarf,16384", "--duration", str(args.profile_seconds),
                            "-o", f"/data/local/tmp/{args.package}-{unit}.perf.data"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                        profiles.append((unit, profile))
                    time.sleep(1.5)
                    run("shell", f"touch {remote}/{label}-{unit}-go")
                    print(f"UNIT {unit} tracing", flush=True)
                elif "VECTOR_DONE" in words:
                    unit = words[words.index("VECTOR_DONE") + 2]
                    stop_trace()
                    stop_profile()
                    with (args.output / f"{label}-{unit}-environment-after.txt").open("w") as out:
                        run("shell", "dumpsys thermalservice | grep -E 'Thermal Status|mValue'; cat /proc/meminfo | head -3", stdout=out)
                    print(f"UNIT {unit} done", flush=True)
            process.wait()
        finally:
            stop_trace()
            stop_profile()
            if process.poll() is None:
                run("shell", "am", "force-stop", args.package)
                process.wait(timeout=60)
    for unit, trace in traces:
        subprocess.run(adb + ["pull", trace, str(args.output / f"{label}-{unit}.perfetto-trace")], capture_output=True)
        subprocess.run(adb + ["shell", "rm", "-f", trace], capture_output=True)
    run("shell", "rm", "-f", remote_plan)
    listing = subprocess.run(adb + ["shell", "ls", remote], capture_output=True, text=True).stdout.split()
    for name in listing:
        if name.startswith(label + "-") and not name.endswith(("-ready", "-go", "-done")):
            run("pull", f"{remote}/{name}", str(args.output / name), stdout=subprocess.DEVNULL)
    log_text = (args.output / f"{label}-instrumentation.txt").read_text()
    if f"VECTOR_COMPLETE {label}" not in log_text:
        with (args.output / f"{label}-crash-logcat.txt").open("w") as out:
            run("logcat", "-d", "-v", "threadtime", stdout=out)
        raise SystemExit(f"Vector benchmark {label} did not complete; see {label}-instrumentation.txt")
    print(f"DONE {label}", flush=True)


if __name__ == "__main__":
    main()
