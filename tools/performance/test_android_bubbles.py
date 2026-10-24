import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("android_bubbles", Path(__file__).with_name("android-bubbles.py"))
bubbles = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bubbles)


class BubblePresentsTest(unittest.TestCase):
    def test_surface_names_accept_android_16_requested_state_wrappers(self):
        name = "owned.app/art.capycanvas.MainActivity#12"
        self.assertEqual(bubbles.surface_names(name + "\nRequestedLayerState{" + name + " parentId=9 z=1}\n"), [name, name])

    def test_pending_slots_and_duplicates_do_not_count_as_presents(self):
        self.assertEqual(bubbles.latency("11111111\n0 0 0\n1 9223372036854775807 3\n1 123 3\n2 123 4\n"), (11111111, [123]))
        with self.assertRaises(ValueError):
            bubbles.latency("11111111\n1 2\n")

    def test_private_activity_identity_uses_the_resumed_exact_component(self):
        text = "topResumedActivity=ActivityRecord{abc u0 owned.app/art.capycanvas.MainActivity t123}"
        self.assertEqual(bubbles.activity_identity(text, "owned.app"), ("abc", 123))
        with self.assertRaises(ValueError):
            bubbles.activity_identity(text, "other.app")

    def test_short_palette_diagnostic_retains_poses_without_fps(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            contact = dict(diagnostic=True, warmup=False, begin_ns=0, end_ns=20_000_000,
                           moving_windows_ns=[[0, 20_000_000]], drawn_centers=[[0] + [0] * 10, [10_000_000, 1] + [0] * 9])
            directory.joinpath("result.json").write_text(json.dumps(dict(contacts=[contact])))
            directory.joinpath("instrumentation.log").write_text("OK (1 test)")
            directory.joinpath("surface-latency.json").write_text(json.dumps(dict(surface="owned", samples=[dict(text="10000000\n1 5000000 3\n1 15000000 3\n")])))
            result = bubbles.report(directory)
            self.assertNotIn("presented_hz", result)
            self.assertEqual(result["transitions"][0]["distinct_drawn_centers"], [2, 1, 1, 1, 1])
            self.assertEqual(result["transitions"][0]["actual_present_timestamps_ns"], [5_000_000, 15_000_000])

    def test_long_gaps_are_preserved(self):
        result = bubbles.summarize([0, 10_000_000, 20_000_000, 120_000_000])
        self.assertEqual(result["gap_p99_ms"], 100)
        self.assertEqual(result["presented_hz"], 25)

    def test_report_excludes_warmup_and_rejects_ring_rollover(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            contacts = [dict(run=i, warmup=i == 0, begin_ns=i * 10_000_000_000, end_ns=i * 10_000_000_000 + 6_000_000_000,
                             distinct_centers=[20, 2, 3, 4, 5]) for i in range(4)]
            directory.joinpath("result.json").write_text(json.dumps(dict(contacts=contacts)))
            times = [c["begin_ns"] + n * 10_000_000 for c in contacts for n in range(601)]
            capture = dict(surface="owned.app/MainActivity#1", samples=[dict(text="10000000\n" + "\n".join(f"1 {t} 3" for t in times))])
            directory.joinpath("surface-latency.json").write_text(json.dumps(capture))
            result = bubbles.report(directory)
            self.assertEqual([c["run"] for c in result["contacts"]], [1, 2, 3])
            self.assertEqual(result["contacts"][0]["presented_hz"], 100)
            contacts[1]["reset_end_ns"] = contacts[1]["begin_ns"] + 1
            directory.joinpath("result.json").write_text(json.dumps(dict(contacts=contacts)))
            with self.assertRaisesRegex(ValueError, "reset overlaps"):
                bubbles.report(directory)
            del contacts[1]["reset_end_ns"]
            directory.joinpath("result.json").write_text(json.dumps(dict(contacts=contacts)))
            capture["samples"] = [dict(text="10000000\n1 11000000000 3\n"), dict(text="10000000\n1 14000000000 3\n")]
            directory.joinpath("surface-latency.json").write_text(json.dumps(capture))
            with self.assertRaisesRegex(ValueError, "ring rollover"):
                bubbles.report(directory)


if __name__ == "__main__":
    unittest.main()
