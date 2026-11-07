import hashlib
import importlib.util
import json
from pathlib import Path
from tempfile import TemporaryDirectory
import unittest
from unittest.mock import patch


spec = importlib.util.spec_from_file_location("windows_vm_password", Path(__file__).with_name("windows-vm.py"))
vm = importlib.util.module_from_spec(spec)
spec.loader.exec_module(vm)


class PasswordPolicyTests(unittest.TestCase):
    def prepare(self, *, expired=False, desktop=True, cached=True, failure=None):
        with TemporaryDirectory() as directory:
            machine = vm.Machine("owned", Path(directory))
            script = (vm.GUEST_FILES / "prepare.ps1").read_text()
            if cached:
                machine.prepared.write_text(hashlib.sha256(f"{vm.VGA}\n{script}".encode()).hexdigest())
            events = []

            def execute(machine, script, **options):
                if "stdout" in options:
                    events.append("policy")
                    options["stdout"].write(json.dumps({"password_expired": expired, "desktop_ready": desktop}).encode())
                elif script == (vm.GUEST_FILES / "prepare.ps1").read_text():
                    events.append("prepare")
                else:
                    events.append("desktop")
                    self.assertIn("AddSeconds(120)", script)
                    self.assertIn("Get-Process explorer -IncludeUserName", script)
                    if failure:
                        raise RuntimeError(failure)

            with patch.object(vm, "guest_script", side_effect=execute), \
                    patch.object(vm, "restart", side_effect=lambda machine: events.append("restart")):
                if failure:
                    with self.assertRaisesRegex(RuntimeError, failure):
                        vm.prepare(machine)
                else:
                    vm.prepare(machine)
            return events

    def test_cached_preparation_still_repairs_expired_private_logon(self):
        self.assertEqual(self.prepare(expired=True, desktop=False), ["policy", "restart", "desktop"],
                         "Cached preparation skipped expired-password repair before desktop startup")

    def test_live_desktop_and_nonexpired_accounts_never_trigger_repair_restart(self):
        for expired, desktop in ((False, True), (True, True), (False, False)):
            with self.subTest(expired=expired, desktop=desktop):
                expected = ["policy"] if desktop else ["policy", "desktop"]
                self.assertEqual(self.prepare(expired=expired, desktop=desktop), expected)

    def test_expired_repair_precedes_interactive_preparation(self):
        self.assertEqual(self.prepare(expired=True, desktop=False, cached=False),
                         ["policy", "restart", "desktop", "prepare", "restart"])

    def test_unavailable_desktop_fails_without_repeated_restart_or_preparation(self):
        for expired in (False, True):
            with self.subTest(expired=expired):
                expected = ["policy", "restart", "desktop"] if expired else ["policy", "desktop"]
                self.assertEqual(self.prepare(expired=expired, desktop=False, failure="Specific logon failure"), expected)

    def test_both_image_paths_apply_the_verified_policy(self):
        for name in ("bootstrap.ps1", "provision-server.ps1"):
            with self.subTest(name=name):
                self.assertIn("password-policy.ps1", (vm.GUEST_FILES / name).read_text())
        policy = (vm.GUEST_FILES / "password-policy.ps1").read_text()
        self.assertIn("/maxpwage:unlimited", policy)
        self.assertIn("Unlimited", policy)
        self.assertGreaterEqual(policy.count("$LASTEXITCODE"), 2)


if __name__ == "__main__":
    unittest.main()
