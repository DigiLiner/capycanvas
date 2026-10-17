"""Check Apple deployment targets at the Cargo process boundary."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[3]
SCRIPT = ROOT / "apps/layer-apple/scripts/rust.sh"


class RustBuildTests(unittest.TestCase):
    def build(self, platform="iphoneos", deployment="17.5", **overrides):
        with tempfile.TemporaryDirectory() as directory:
            directory = Path(directory)
            capture = directory / "cargo.json"
            mock = directory / "cargo.py"
            mock.write_text("import json, os, sys\nfrom pathlib import Path\n"
                            "Path(os.environ['CAPY_TEST_CAPTURE']).write_text(json.dumps({"
                            "'args': sys.argv[1:], 'root': os.environ.get('CARGO_TARGET_DIR'),"
                            "'sdk': os.environ.get('SDKROOT'),"
                            "'deployment': os.environ.get('IPHONEOS_DEPLOYMENT_TARGET')}))\n")
            bash_env = directory / "bash_env"
            bash_env.write_text('cargo() { python3 "$CAPY_TEST_MOCK" "$@"; }\n')
            environment = os.environ.copy()
            for key in ["CARGO_TARGET_DIR", "CAPY_RUST_BUILD_ROOT", "IPHONEOS_DEPLOYMENT_TARGET"]:
                environment.pop(key, None)
            environment.update(PLATFORM_NAME=platform, CONFIGURATION="Debug", SDKROOT="/test/sdk",
                               BASH_ENV=str(bash_env), CAPY_TEST_MOCK=str(mock),
                               CAPY_TEST_CAPTURE=str(capture))
            if deployment is not None:
                environment["IPHONEOS_DEPLOYMENT_TARGET"] = deployment
            environment.update(overrides)
            result = subprocess.run(["bash", str(SCRIPT)], env=environment, text=True, capture_output=True)
            return result, json.loads(capture.read_text()) if capture.exists() else None

    def test_deployment_changes_isolate_device_builds(self):
        captures = []
        for deployment in ["17.5", "18.0", "17.5"]:
            result, capture = self.build(deployment=deployment)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(capture["root"], str(ROOT / "target" / f"ios-{deployment}"))
            self.assertEqual(capture["deployment"], deployment)
            self.assertIsNone(capture["sdk"])
            self.assertEqual(capture["args"], ["build", "--locked", "-p", "layer-apple-link",
                                              "--target", "aarch64-apple-ios", "--profile", "dev"])
            captures.append(capture["root"])
        self.assertEqual(captures[0], captures[2])
        self.assertNotEqual(captures[0], captures[1])

    def test_custom_build_root_matches_requested_link_location(self):
        result, capture = self.build(CAPY_RUST_BUILD_ROOT="/custom/ios-build", CARGO_TARGET_DIR="/stale/output")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(capture["root"], "/custom/ios-build")

    def test_simulator_uses_deployment_root_and_release_profile(self):
        result, capture = self.build(platform="iphonesimulator", CONFIGURATION="Release")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(capture["root"], str(ROOT / "target/ios-17.5"))
        self.assertEqual(capture["args"][-3:], ["aarch64-apple-ios-sim", "--profile", "release"])
        self.assertIsNone(capture["sdk"])

    def test_missing_ios_deployment_rejects_build_before_cargo(self):
        for platform in ["iphoneos", "iphonesimulator"]:
            result, capture = self.build(platform=platform, deployment=None)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("Set the iOS deployment target before building", result.stderr)
            self.assertIsNone(capture)

    def test_macos_retains_existing_target_directory_and_sdk(self):
        result, capture = self.build(platform="macosx", deployment=None, CARGO_TARGET_DIR="/existing/target")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(capture["root"], "/existing/target")
        self.assertEqual(capture["sdk"], "/test/sdk")
        self.assertEqual(capture["args"][-3:], ["aarch64-apple-darwin", "--profile", "dev"])


if __name__ == "__main__":
    unittest.main()
