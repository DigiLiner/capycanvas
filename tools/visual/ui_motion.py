#!/usr/bin/env python3
"""Summarise native_ui_motion_recording frames: where pixels change more than once."""
import argparse
import json
import pathlib
import subprocess

import numpy as np


def load(directory):
    frames = json.loads((directory / "frames.json").read_text())
    images, times = [], []
    for index, frame in enumerate(frames):
        raw = np.fromfile(directory / f"{index:04d}.rgb", dtype=np.uint8)
        rows = raw[: frame["stride"] * frame["height"]].reshape(frame["height"], frame["stride"])
        image = rows[:, : frame["width"] * 3].reshape(frame["height"], frame["width"], 3)
        if images and np.array_equal(image, images[-1]):
            continue
        images.append(image)
        times.append(frame.get("pulled_ns", frame["pts_ns"]))
    events = directory / "events.json"
    released = [e["ns"] for e in json.loads(events.read_text()) if e["event"].get("down") is False] if events.exists() else []
    start = released[0] if released else times[0] if times else 0
    return images, [(t - start) / 1e6 for t in times]


def write_png(path, image):
    height, width = image.shape[:2]
    subprocess.run(
        ["magick", "-size", f"{width}x{height}", "-depth", "8", "rgb:-", str(path)],
        input=np.ascontiguousarray(image).tobytes(),
        check=True,
    )


def regions(mask, block):
    height, width = mask.shape
    grid = mask[: height // block * block, : width // block * block]
    grid = grid.reshape(height // block, block, width // block, block).any(axis=(1, 3))
    seen = np.zeros_like(grid)
    found = []
    for y, x in zip(*np.nonzero(grid)):
        if seen[y, x]:
            continue
        stack, cells = [(y, x)], []
        seen[y, x] = True
        while stack:
            cy, cx = stack.pop()
            cells.append((cy, cx))
            for ny, nx in ((cy + 1, cx), (cy - 1, cx), (cy, cx + 1), (cy, cx - 1)):
                if 0 <= ny < grid.shape[0] and 0 <= nx < grid.shape[1] and grid[ny, nx] and not seen[ny, nx]:
                    seen[ny, nx] = True
                    stack.append((ny, nx))
        ys, xs = zip(*cells)
        found.append((int(min(xs)) * block, int(min(ys)) * block, (int(max(xs)) + 1) * block, (int(max(ys)) + 1) * block))
    return sorted(found, key=lambda r: (r[2] - r[0]) * (r[3] - r[1]), reverse=True)


def analyse(directory, output, threshold, block):
    images, times = load(directory)
    output.mkdir(parents=True, exist_ok=True)
    report = {"recording": directory.name, "frames": len(images), "steps": []}
    if len(images) < 2:
        (output / "report.json").write_text(json.dumps(report, indent=1))
        return report
    counts = np.zeros(images[0].shape[:2], dtype=np.int32)
    previous = images[0].astype(np.int16)
    final = images[-1].astype(np.int16)
    initial = previous
    for index in range(1, len(images)):
        current = images[index].astype(np.int16)
        changed = np.abs(current - previous).max(axis=2) > threshold
        counts += changed
        away = (np.abs(current - final).max(axis=2) > threshold) & (np.abs(current - initial).max(axis=2) > threshold)
        report["steps"].append({
            "frame": index,
            "ms": round(times[index], 1),
            "changed": regions(changed, block)[:6],
            "neither_start_nor_end": regions(away, block)[:6],
        })
        previous = current
    report["repeated"] = regions(counts >= 2, block)[:12]
    report["max_changes"] = int(counts.max())
    (output / "report.json").write_text(json.dumps(report, indent=1))
    heat = (images[-1] * 0.35).astype(np.uint8)
    heat[counts == 1] = [40, 170, 60]
    heat[counts == 2] = [230, 200, 20]
    heat[counts >= 3] = [230, 40, 40]
    write_png(output / "heatmap.png", heat)
    for index, image in enumerate(images):
        write_png(output / f"frame-{index:03d}.png", image)
    labels = [arg for index in range(len(images)) for arg in ("-label", f"{index}: {times[index]:.0f} ms", str(output / f"frame-{index:03d}.png"))]
    subprocess.run(["magick", "montage", *labels, "-tile", "4x", "-geometry", "480x+4+4", str(output / "contact.png")], check=True)
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("captures", type=pathlib.Path)
    parser.add_argument("output", type=pathlib.Path)
    parser.add_argument("--threshold", type=int, default=10)
    parser.add_argument("--block", type=int, default=8)
    args = parser.parse_args()
    for directory in sorted(p for p in args.captures.iterdir() if (p / "frames.json").exists()):
        report = analyse(directory, args.output / directory.name, args.threshold, args.block)
        print(f"{report['recording']}: {report['frames']} distinct frames, max changes {report.get('max_changes', 0)}")
        for left, top, right, bottom in report.get("repeated", []):
            print(f"  repeated change at x {left}-{right}, y {top}-{bottom}")


if __name__ == "__main__":
    main()
