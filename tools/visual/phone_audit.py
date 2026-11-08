#!/usr/bin/env python3
"""Compare paired GTK/Web phone captures and their actual widget bounds, or candidate captures against a reference manifest."""
import argparse
import hashlib
import json
import math
from pathlib import Path
import re
import subprocess
import sys


def load(path):
    value = json.loads(path.read_text())
    if value["schema_version"] != 1:
        raise ValueError(f"Unsupported audit schema: {path}")
    for key in ("host", "state", "viewport", "dpr", "theme", "text_scale", "preset", "status", "phone_active"):
        if key not in value:
            raise ValueError(f"Missing metadata {key}: {path}")
    elements = {}
    for element in value["elements"]:
        key = element["id"]
        if not isinstance(key, str) or not key or not isinstance(element["role"], str) or not element["role"] or type(element["visible"]) is not bool:
            raise ValueError(f"Invalid element identity/visibility: {path}")
        if key in elements:
            raise ValueError(f"Duplicate element {key}: {path}")
        bounds = element["bounds"]
        if bounds is None and element["visible"] is False:
            elements[key] = element
            continue
        if not isinstance(bounds, list) or len(bounds) != 4 or not all(type(v) in (int, float) and math.isfinite(v) for v in bounds) or min(bounds[2:]) < 0:
            raise ValueError(f"Invalid bounds for {key}: {path}")
        if element["visible"] and min(bounds[2:]) <= 0:
            raise ValueError(f"Empty visible allocation for {key}: {path}")
        elements[key] = element
    return value, elements


def compare(reference, candidate):
    gtk, left = load(reference)
    web, right = load(candidate)
    host_roles = {"native-reservation", "window-move"}
    host_elements = {"gtk": [element for element in left.values() if element["role"] in host_roles],
        "web": [element for element in right.values() if element["role"] in host_roles]}
    left = {key: element for key, element in left.items() if element["role"] not in host_roles}
    right = {key: element for key, element in right.items() if element["role"] not in host_roles}
    metadata = [key for key in ("state", "viewport", "dpr", "theme", "text_scale", "preset") if gtk.get(key) != web.get(key)]
    if gtk.get("host") != "gtk" or web.get("host") != "web":
        metadata.append("host")
    missing = sorted(set(left) ^ set(right) | set(gtk.get("missing", [])) | set(web.get("missing", [])))
    differences = []
    for key in sorted(set(left) & set(right)):
        a, b = left[key], right[key]
        delta = [y - x for x, y in zip(a["bounds"], b["bounds"])] if a["bounds"] is not None and b["bounds"] is not None else []
        if a["visible"] != b["visible"] or a["role"] != b["role"] or any(abs(d) > 1 for d in delta):
            differences.append({"id": key, "gtk": a, "web": b, "delta": delta})
    unavailable = [entry.get("host") for entry in (gtk, web) if entry.get("status") != "captured" or not entry.get("phone_active")]
    return {"metadata_mismatches": metadata, "missing": missing, "unavailable": unavailable,
        "host_elements": host_elements,
        "allocation_differences": differences, "tolerance_pixels": 1,
        "passed": not (metadata or missing or differences or unavailable) and bool(left) and bool(right)}


def image_path(path):
    suffix = ".allocations.json"
    return path.with_name(path.name[:-len(suffix)] + ".png") if path.name.endswith(suffix) else path.with_suffix(".png")


def compare_pixels(reference, candidate, output):
    output.mkdir(parents=True, exist_ok=True)
    result = subprocess.run([sys.executable, str(Path(__file__).with_name("compare.py")),
        str(reference), str(candidate), "--output", str(output), "--sheet"], capture_output=True, text=True)
    if result.returncode not in (0, 1) or not result.stdout.strip():
        raise ValueError(result.stderr.strip() or "Pixel comparison failed")
    return json.loads(result.stdout)


def compare_pair(gtk, web, output):
    geometry = compare(gtk, web)
    pixels = compare_pixels(image_path(gtk), image_path(web), output)
    metadata, _ = load(gtk)
    expected = [round(value * metadata["dpr"]) for value in metadata["viewport"]]
    if pixels["dimensions"] != expected:
        raise ValueError(f"Capture dimensions {pixels['dimensions']} do not match viewport/DPR {expected}")
    report = {"gtk": str(gtk), "web": str(web), "geometry": geometry,
        "pixels": pixels, "passed": geometry["passed"] and pixels["passed"]}
    (output / "phone-comparison.json").write_text(json.dumps(report, indent=2) + "\n")
    return report


def allocation_key(value):
    return json.dumps([value["state"], value["viewport"], float(value["dpr"]), value["theme"],
        float(value["text_scale"]), value["preset"]], separators=(",", ":"))


def index(directory):
    records = {}
    for path in sorted(directory.rglob("*.allocations.json")):
        value, _ = load(path)
        key = allocation_key(value)
        if key in records:
            raise ValueError(f"Duplicate capture state: {records[key]} and {path}")
        records[key] = path
    return records


def compare_matrix(gtk, web, output):
    left, right = index(gtk), index(web)
    reports = []
    for key in sorted(left.keys() & right.keys()):
        state, viewport, _, theme, scale, _ = json.loads(key)
        stem = re.sub(r"[^A-Za-z0-9_.-]", "_", f"{viewport[0]}x{viewport[1]}-{theme}-{scale:g}-{state}")[:120]
        directory = output / (stem + "-" + hashlib.sha256(key.encode()).hexdigest()[:8])
        result = compare_pair(left[key], right[key], directory)
        reports.append({"key": json.loads(key), "report": str(directory / "phone-comparison.json"), "passed": result["passed"]})
    manifest = web / "index.json"
    unavailable = json.loads(manifest.read_text()).get("unreachable", []) if manifest.exists() else []
    reference_manifest = gtk / "index.json"
    reference_failures = json.loads(reference_manifest.read_text()).get("failed_cases", []) if reference_manifest.exists() else []
    report = {"states": reports, "unpaired_gtk": [str(left[key]) for key in sorted(left.keys() - right.keys())],
        "unpaired_web": [str(right[key]) for key in sorted(right.keys() - left.keys())],
        "unavailable_states": unavailable, "failed_reference_cases": reference_failures,
        "passed": bool(reports) and not (unavailable or reference_failures) and all(result["passed"] for result in reports) and left.keys() == right.keys()}
    output.mkdir(parents=True, exist_ok=True)
    (output / "matrix.json").write_text(json.dumps(report, indent=2) + "\n")
    return report


CAPTURE_FIELDS = ("family", "state", "case", "theme", "text_size", "fixture")


def capture_key(value):
    return tuple(float(value[key]) if key == "text_size" else value[key] for key in CAPTURE_FIELDS)


def manifest_captures(manifest, candidates, coverage=None):
    source = json.loads(manifest.read_text())
    references = {}
    for entry in sorted(source["references"], key=lambda entry: entry.get("duplicate_of") is not None):
        references.setdefault(capture_key(entry), entry)
    index_path = candidates / "index.json"
    metadata = json.loads(index_path.read_text()) if index_path.exists() else {}
    coverage = coverage if coverage is not None else metadata.get("coverage")
    if coverage is None:
        coverage = {"families": sorted({key[0] for key in references}), "cases": sorted({key[2] for key in references})}
    if not isinstance(coverage, dict) or set(coverage) != {"families", "cases"} or any(
        not isinstance(coverage[key], list) or not coverage[key] or any(not isinstance(value, str) or not value for value in coverage[key])
        for key in ("families", "cases")):
        raise ValueError("Declare nonempty coverage families and cases; state subsets are not supported")
    if set(coverage["families"]) - {key[0] for key in references} or set(coverage["cases"]) - {key[2] for key in references}:
        raise ValueError("Declared coverage is absent from the reference manifest")
    included = lambda key: key[0] in coverage["families"] and key[2] in coverage["cases"]
    references = {key: entry for key, entry in references.items() if included(key)}
    if not references:
        raise ValueError("Declared coverage contains no reference captures")
    failures = []
    for key in ("required_unverified", "failed_cases", "fixture_skips", "skipped_tests", "unreachable"):
        if metadata.get(key):
            failures.append({"field": key, "value": metadata[key]})
    results = metadata.get("fixture_results")
    if results is not None and (not results.get("passedTests") or results.get("skippedTests") or results.get("failedTests")):
        failures.append({"field": "fixture_results", "value": results})
    records = {}
    for path in sorted(candidates.rglob("*.json")):
        value = json.loads(path.read_text())
        if not isinstance(value, dict) or value.get("schema_version") != 1 or "family" not in value:
            continue
        if not all(field in value for field in CAPTURE_FIELDS):
            raise ValueError(f"Incomplete capture identity: {path}")
        key = capture_key(value)
        if key in records:
            raise ValueError(f"Duplicate candidate capture key: {records[key][0]} and {path}")
        records[key] = (path, value)
        if value.get("required_unverified"):
            failures.append({"field": "required_unverified", "candidate": str(path), "value": value["required_unverified"]})
        if not included(key):
            failures.append({"field": "outside_coverage", "candidate": str(path)})
        if not image_path(path).is_file():
            failures.append({"field": "missing_image", "candidate": str(path)})
    if "captures" in metadata and {str(path.resolve()) for path, _ in records.values()} != {
        str((Path(path) if Path(path).is_absolute() else candidates / path).resolve()) for path in metadata["captures"]
    }:
        failures.append({"field": "captures", "value": "Capture index does not match capture metadata files"})
    missing = sorted(references.keys() - records.keys())
    if missing:
        failures.append({"field": "missing_candidates", "keys": [list(key) for key in missing]})
    if records.keys() - references.keys():
        failures.append({"field": "missing_references", "keys": [list(key) for key in sorted(records.keys() - references.keys())]})
    return references, records, dict(coverage, passed=bool(records) and not failures, failures=failures)


def compare_manifest(manifest, candidates, output, coverage=None):
    references, records, coverage = manifest_captures(manifest, candidates, coverage)
    states = []
    for key, (path, value) in records.items():
        state = {field: value[field] for field in CAPTURE_FIELDS}
        state["candidate"] = str(image_path(path))
        entry = references.get(key)
        if entry is None:
            state["status"] = "missing"
        else:
            identity = json.dumps(key, separators=(",", ":"))
            directory = output / value["family"] / value["case"] / (value["state"] + "-" + hashlib.sha256(identity.encode()).hexdigest()[:8])
            state["reference"] = entry["png"]
            try:
                pixels = compare_pixels(Path(entry["png"]), image_path(path), directory)
                state.update(status="compared", sheet=str((directory / "sheet.png").relative_to(output)),
                             different_fraction=pixels["exact_different_fraction"], passed=pixels["passed"])
            except ValueError as error:
                state.update(status="error", error=str(error))
        states.append(state)
    for key in sorted(references.keys() - records.keys()):
        states.append(dict(zip(CAPTURE_FIELDS, key), reference=references[key]["png"], status="missing_candidate"))
    output.mkdir(parents=True, exist_ok=True)
    pixels_passed = bool(states) and all(state.get("passed") is True for state in states)
    report = {"manifest": str(manifest), "candidates": str(candidates), "coverage": coverage, "states": states,
              "pairing_passed": coverage["passed"], "pixels_passed": pixels_passed,
              "passed": coverage["passed"] and pixels_passed}
    (output / "index.json").write_text(json.dumps(report, indent=2) + "\n")
    rows = ["| Family | Case | State | Status | Different pixels | Sheet |", "| --- | --- | --- | --- | --- | --- |"]
    for state in states:
        fraction = f"{state['different_fraction']:.4f}" if "different_fraction" in state else ""
        sheet = f"[sheet]({state['sheet']})" if "sheet" in state else state.get("error", "")
        rows.append(f"| {state['family']} | {state['case']} | {state['state']} | {state['status']} | {fraction} | {sheet} |")
    (output / "index.md").write_text("\n".join(rows) + "\n")
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("gtk", nargs="?", type=Path, help="GTK allocation JSON or capture directory")
    parser.add_argument("web", nargs="?", type=Path, help="Web allocation JSON or capture directory")
    parser.add_argument("--reference", type=Path, help="Reference manifest JSON")
    parser.add_argument("--candidate", type=Path, help="Candidate capture directory")
    parser.add_argument("--scope-family", action="append")
    parser.add_argument("--scope-case", action="append")
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    try:
        if (args.scope_family or args.scope_case) and not args.reference:
            raise ValueError("Coverage scope requires --reference and --candidate")
        if args.reference or args.candidate:
            if not (args.reference and args.candidate) or args.gtk or args.web:
                raise ValueError("Use --reference with --candidate and no GTK/Web captures")
            if bool(args.scope_family) != bool(args.scope_case):
                raise ValueError("Declare both --scope-family and --scope-case")
            coverage = {"families": args.scope_family, "cases": args.scope_case} if args.scope_family else None
            report = compare_manifest(args.reference, args.candidate, args.output, coverage)
        elif not (args.gtk and args.web):
            raise ValueError("Provide GTK and Web captures, or --reference with --candidate")
        elif args.gtk.is_dir() != args.web.is_dir():
            raise ValueError("Provide two allocation JSON files or two capture directories")
        else:
            report = compare_matrix(args.gtk, args.web, args.output) if args.gtk.is_dir() else compare_pair(args.gtk, args.web, args.output)
        print(json.dumps(report, indent=2))
        return 0 if report["passed"] else 1
    except (ValueError, KeyError, OSError, TypeError) as error:
        parser.error(str(error))


if __name__ == "__main__":
    raise SystemExit(main())
