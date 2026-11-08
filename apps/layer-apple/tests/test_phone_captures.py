import importlib.util
import json
from pathlib import Path
import struct
import tempfile
import unittest


source = Path(__file__).resolve().parents[1] / "scripts" / "export-phone-captures.py"
spec = importlib.util.spec_from_file_location("export_phone_captures", source)
exporter = importlib.util.module_from_spec(spec)
spec.loader.exec_module(exporter)


class PhoneCaptures(unittest.TestCase):
    summary = {"passedTests": 1, "failedTests": 0, "skippedTests": 0}
    identity = {"family": "menu", "state": "root", "case": "phone-light", "theme": "light", "text_size": 11, "fixture": "Sketch"}

    def reference(self, root, entries=None):
        path = root / "reference.json"
        path.write_text(json.dumps({"references": entries or [dict(self.identity, png="reference.png")]}))
        return path

    def test_pairs_within_each_test_and_preserves_native_pixels(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            png = b"\x89PNG\r\n\x1a\n" + struct.pack(">I", 13) + b"IHDR" + struct.pack(">II", 1206, 2622)
            (root / "image.png").write_bytes(png)
            (root / "metadata.json").write_text(json.dumps(dict(self.identity, schema_version=1, host="apple",
                png_attachment="phone-menu", viewport=[402, 874], dpr=3)))
            (root / "manifest.json").write_text(json.dumps([{"testIdentifier": "phone-light", "attachments": [
                {"exportedFileName": "image.png", "suggestedHumanReadableName": "phone-menu.png"},
                {"exportedFileName": "metadata.json", "suggestedHumanReadableName": "phone-menu-metadata.json"}]}]))
            output = root / "captures"
            captures = exporter.collect(root, output, self.summary, self.reference(root))
            self.assertEqual((output / "phone-menu.png").read_bytes(), png)
            self.assertEqual(json.loads(Path(captures[0]).read_text())["test_identifier"], "phone-light")
            wrong = root / "wrong"
            (root / "metadata.json").write_text((root / "metadata.json").read_text().replace('"dpr": 3', '"dpr": 2'))
            with self.assertRaisesRegex(ValueError, "disagree with viewport/DPR"):
                exporter.collect(root, wrong, self.summary, self.reference(root))

    def test_never_pairs_a_screenshot_from_another_test(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "metadata.json").write_text(json.dumps(dict(self.identity, schema_version=1, host="apple", png_attachment="phone-menu")))
            (root / "manifest.json").write_text(json.dumps([
                {"testIdentifier": "first", "attachments": [{"exportedFileName": "metadata.json", "suggestedHumanReadableName": "metadata"}]},
                {"testIdentifier": "second", "attachments": [{"exportedFileName": "other.png", "suggestedHumanReadableName": "phone-menu.png"}]}]))
            with self.assertRaisesRegex(ValueError, "Expected one PNG for first"):
                exporter.collect(root, root / "captures", self.summary, self.reference(root))

    def test_pairs_xcode_uuid_names_without_matching_another_state(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            png = b"\x89PNG\r\n\x1a\n" + struct.pack(">I", 13) + b"IHDR" + struct.pack(">II", 640, 1136)
            (root / "image.png").write_bytes(png)
            (root / "other.png").write_bytes(png)
            (root / "metadata.json").write_text(json.dumps(dict(self.identity, schema_version=1, host="apple",
                png_attachment="phone-menu", viewport=[320, 568], dpr=2)))
            suffix = "_0_6746869E-74E5-46DC-AADD-41C7741CEAA7.png"
            (root / "manifest.json").write_text(json.dumps([{"testIdentifier": "phone-light", "attachments": [
                {"exportedFileName": "image.png", "suggestedHumanReadableName": "phone-menu" + suffix},
                {"exportedFileName": "other.png", "suggestedHumanReadableName": "phone-menu-other" + suffix},
                {"exportedFileName": "metadata.json", "suggestedHumanReadableName": "phone-menu-metadata.json"}]}]))
            captures = exporter.collect(root, root / "captures", self.summary, self.reference(root))
            self.assertEqual(len(captures), 1)
            self.assertEqual((root / "captures/phone-menu.png").read_bytes(), png)

    def test_rejects_skips_unverified_duplicates_and_incomplete_scope(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "image.png").write_bytes(b"\x89PNG\r\n\x1a\n" + struct.pack(">I", 13) + b"IHDR" + struct.pack(">II", 3, 2))
            value = dict(self.identity, schema_version=1, host="apple", png_attachment="phone-menu", viewport=[3, 2], dpr=1)
            metadata = root / "metadata.json"
            metadata.write_text(json.dumps(value))
            attachments = [{"exportedFileName": "image.png", "suggestedHumanReadableName": "phone-menu.png"},
                {"exportedFileName": "metadata.json", "suggestedHumanReadableName": "phone-menu-metadata.json"}]
            manifest = root / "manifest.json"
            manifest.write_text(json.dumps([{"testIdentifier": "phone", "attachments": attachments}]))
            def collect(name, summary=None, entries=None, scope=None):
                return exporter.collect(root, root / name, summary or self.summary, self.reference(root, entries), scope)
            with self.assertRaisesRegex(ValueError, "without skips"):
                collect("skip", dict(self.summary, skippedTests=1))
            metadata.write_text(json.dumps(dict(value, required_unverified=["root"])))
            with self.assertRaisesRegex(ValueError, "Unverified required"):
                collect("unverified")
            metadata.write_text(json.dumps(value))
            manifest.write_text(json.dumps([{"testIdentifier": name, "attachments": attachments} for name in ("first", "second")]))
            with self.assertRaisesRegex(ValueError, "Duplicate capture key"):
                collect("duplicates")
            manifest.write_text(json.dumps([{"testIdentifier": "phone", "attachments": attachments}]))
            entries = [dict(self.identity, png="ref.png"), dict(self.identity, state="expanded", png="ref.png")]
            with self.assertRaisesRegex(ValueError, "Incomplete phone capture set"):
                collect("incomplete", entries=entries, scope={"families": ["menu"], "cases": ["phone-light"]})
            entries[1]["family"] = "pages"
            self.assertEqual(len(collect("complete", entries=entries, scope={"families": ["menu"], "cases": ["phone-light"]})), 1)
