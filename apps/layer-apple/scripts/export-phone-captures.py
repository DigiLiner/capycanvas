#!/usr/bin/env python3
"""Export paired phone screenshots and metadata from an XCTest result bundle."""
import argparse
import json
from pathlib import Path
import re
import shutil
import struct
import subprocess
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[3] / "tools/visual"))
from phone_audit import CAPTURE_FIELDS, capture_key, manifest_captures


def collect(attachments, output, summary, reference, coverage=None):
    if not summary.get("passedTests") or summary.get("skippedTests") or summary.get("failedTests"):
        raise ValueError("Phone fixture results must pass without skips or failures")
    output.mkdir(parents=True, exist_ok=True)
    captures = []
    keys = set()
    for test in json.loads((attachments / "manifest.json").read_text()):
        entries = test["attachments"]
        for entry in entries:
            source = attachments / entry["exportedFileName"]
            if source.suffix.lower() == ".png":
                continue
            try:
                value = json.loads(source.read_text())
            except (UnicodeError, json.JSONDecodeError) as error:
                name = entry["suggestedHumanReadableName"]
                if name.startswith("phone-") and "-metadata" in name:
                    raise ValueError(f"Invalid phone metadata: {source}") from error
                continue
            if not isinstance(value, dict) or value.get("schema_version") != 1 or value.get("host") != "apple" or "png_attachment" not in value:
                continue
            name = value["png_attachment"]
            image_name = re.compile(re.escape(name) + r"(?:_\d+_[0-9A-Fa-f]{8}(?:-[0-9A-Fa-f]{4}){3}-[0-9A-Fa-f]{12})?(?:\.png)?")
            matches = [item for item in entries if image_name.fullmatch(item["suggestedHumanReadableName"])
                and Path(item["exportedFileName"]).suffix.lower() == ".png"]
            if len(matches) != 1:
                raise ValueError(f"Expected one PNG for {test['testIdentifier']}: {name}")
            png = attachments / matches[0]["exportedFileName"]
            header = png.read_bytes()[:24]
            if header[:8] != b"\x89PNG\r\n\x1a\n" or header[12:16] != b"IHDR":
                raise ValueError(f"Invalid PNG: {png}")
            pixels = struct.unpack(">II", header[16:24])
            expected = tuple(round(dimension * value["dpr"]) for dimension in value["viewport"])
            if pixels != expected:
                raise ValueError(f"PNG pixels {pixels} disagree with viewport/DPR {expected}: {name}")
            if not all(key in value for key in CAPTURE_FIELDS):
                raise ValueError(f"Incomplete phone capture identity: {name}")
            if value.get("required_unverified"):
                raise ValueError(f"Unverified required states: {name}: {value['required_unverified']}")
            key = capture_key(value)
            if key in keys:
                raise ValueError(f"Duplicate capture key: {key}")
            keys.add(key)
            stem = re.sub(r"[^A-Za-z0-9_.-]", "_", name)
            metadata = output / (stem + ".allocations.json")
            if metadata.exists():
                raise ValueError(f"Duplicate capture: {name}")
            shutil.copyfile(png, output / (stem + ".png"))
            value["test_identifier"] = test["testIdentifier"]
            metadata.write_text(json.dumps(value, indent=2) + "\n")
            captures.append(str(metadata.resolve()))
    if not captures:
        raise ValueError("No Apple phone capture attachments found")
    index = {"host": "apple", "captures": captures, "required_unverified": [], "fixture_results": summary}
    if coverage is not None:
        index["coverage"] = coverage
    (output / "index.json").write_text(json.dumps(index, indent=2) + "\n")
    _, _, checked = manifest_captures(reference, output, coverage)
    if not checked["passed"]:
        raise ValueError(f"Incomplete phone capture set: {checked['failures']}")
    index["coverage"] = {key: checked[key] for key in ("families", "cases")}
    (output / "index.json").write_text(json.dumps(index, indent=2) + "\n")
    return captures


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("result", type=Path, help="XCTest .xcresult bundle")
    parser.add_argument("output", type=Path, help="New export directory")
    parser.add_argument("--reference", required=True, type=Path)
    parser.add_argument("--scope-family", action="append")
    parser.add_argument("--scope-case", action="append")
    args = parser.parse_args()
    if bool(args.scope_family) != bool(args.scope_case):
        parser.error("Declare both --scope-family and --scope-case")
    coverage = {"families": args.scope_family, "cases": args.scope_case} if args.scope_family else None
    attachments = args.output / "attachments"
    subprocess.run(["xcrun", "xcresulttool", "export", "attachments", "--path", str(args.result),
        "--output-path", str(attachments)], check=True)
    summary = json.loads(subprocess.run(["xcrun", "xcresulttool", "get", "test-results", "summary",
        "--path", str(args.result)], check=True, capture_output=True, text=True).stdout)
    captures = collect(attachments, args.output / "captures", summary, args.reference, coverage)
    print(f"Exported {len(captures)} phone captures to {args.output / 'captures'}")


if __name__ == "__main__":
    main()
