from contextlib import nullcontext
import importlib.util
import json
from pathlib import Path
from tempfile import TemporaryDirectory
from types import SimpleNamespace
import unittest
from unittest.mock import patch


spec = importlib.util.spec_from_file_location("windows_vm_rust", Path(__file__).with_name("windows-vm.py"))
vm = importlib.util.module_from_spec(spec)
spec.loader.exec_module(vm)


class RustToolchainTests(unittest.TestCase):
    def test_check_prepares_compiler_before_first_cargo_consumer(self):
        events = []

        def begin_check(*args, **kwargs):
            events.append("check")
            raise InterruptedError

        with TemporaryDirectory() as directory, \
                patch.object(vm, "selected", return_value=None), patch.object(vm, "boot"), \
                patch.object(vm, "claim", return_value=nullcontext()), patch.object(vm, "sync_machine"), \
                patch.object(vm, "prepare_rust", create=True, side_effect=lambda machine: events.append("rust")), \
                patch.object(vm, "guest_script", side_effect=lambda *args, **kwargs: events.append("upload")), \
                patch.object(vm, "desktop", side_effect=begin_check), patch.object(vm, "guest"), \
                patch.object(vm, "retrieve", return_value=Path(directory)):
            with self.assertRaises(InterruptedError):
                vm.check(SimpleNamespace(debug=False))
        self.assertEqual(events, ["rust", "upload", "check"], "Windows check used Cargo before validating the guest compiler")

    def test_fixture_build_and_reuse_prepare_compiler(self):
        for no_build in (False, True):
            with self.subTest(no_build=no_build), TemporaryDirectory() as directory:
                machine = SimpleNamespace(directory=Path(directory))
                record = {"profile": "release", "inputs": {}, "files": {}}
                vm.build_record_path(machine, False, "release").write_text(json.dumps(record))
                events = []
                with patch.object(vm, "prepare_rust", create=True, side_effect=lambda machine: events.append("rust")), \
                        patch.object(vm, "guest", side_effect=lambda *args: events.append("build")), \
                        patch.object(vm, "build_files", side_effect=lambda *args: events.append("files") or {}):
                    vm.fixture_build(machine, {}, no_build, False)
                expected = ["rust", "files"] if no_build else ["rust", "build", "files"]
                self.assertEqual(events, expected, "Fixture build or cached GPU check skipped the compiler prerequisite")


if __name__ == "__main__":
    unittest.main()
