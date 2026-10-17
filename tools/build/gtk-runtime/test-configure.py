#!/usr/bin/env python3
"""Exercise new subproject options and incremental reuse with real Meson."""
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


class ConfigureTest(unittest.TestCase):
    def test_sdk_path_change_replaces_cached_dependency(self):
        with tempfile.TemporaryDirectory(prefix='capy-sdk-change-') as temporary:
            root = Path(temporary)
            source, build = root / 'source', root / 'build'
            source.mkdir()
            (source / 'meson.build').write_text("project('sdk-probe', 'c')\nexecutable('probe', 'probe.c', dependencies: dependency('sdk-probe'))\n")
            (source / 'probe.c').write_text('int main(void) { return 0; }\n')
            previous = None
            for name in ['old-sdk', 'new-sdk']:
                sdk = root / name
                sdk.mkdir()
                (sdk / 'sdk-probe.pc').write_text(f'Name: sdk-probe\nDescription: SDK dependency probe\nVersion: 1.0\nCflags: -I{sdk}/include\nLibs:\n')
                result = subprocess.run([sys.executable, str(Path(__file__).with_name('configure.py')),
                    str(build), str(source), f'-Dpkg_config_path={sdk}'], capture_output=True, text=True)
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                commands = (build / 'build.ninja').read_text()
                self.assertIn(str(sdk / 'include'), commands)
                if previous:
                    self.assertNotIn(str(previous / 'include'), commands)
                previous = sdk

    def test_source_upgrade_replaces_old_build_then_reuses_new_build(self):
        with tempfile.TemporaryDirectory(prefix='capy-gtk-upgrade-') as temporary:
            root = Path(temporary)
            build = root / 'build'
            command = [sys.executable, str(Path(__file__).with_name('configure.py')), str(build)]
            for name in ['old-source', 'new-source']:
                source = root / name
                source.mkdir()
                (source / 'meson.build').write_text("project('runtime-probe')\n")
                result = subprocess.run([*command, str(source)], capture_output=True, text=True)
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                self.assertEqual(json.loads((build / 'meson-info/meson-info.json').read_text())['directories']['source'], str(source))
            sentinel = build / 'retained-build-output'
            sentinel.write_text('keep incremental artifacts')
            result = subprocess.run([*command, str(source)], capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertTrue(sentinel.is_file())

    def test_new_subproject_options_then_incremental_reconfigure(self):
        with tempfile.TemporaryDirectory(prefix='capy-gtk-configure-') as temporary:
            root = Path(temporary)
            source, build = root / 'source', root / 'build'
            codec = source / 'subprojects' / 'codec'
            codec.mkdir(parents=True)
            (source / 'meson.build').write_text("project('cache-probe')\nif get_option('codec')\nsubproject('codec')\nendif\n")
            (source / 'meson.options').write_text("option('codec', type: 'boolean', value: false)\n")
            (codec / 'meson.build').write_text("project('codec-probe')\n")
            (codec / 'meson.options').write_text("option('extra', type: 'feature', value: 'auto')\n")
            command = [sys.executable, str(Path(__file__).with_name('configure.py')), str(build), str(source)]
            for options in [[], ['-Dcodec=true', '-Dcodec:extra=disabled', '-Dcodec:default_library=static']]:
                result = subprocess.run([*command, *options], capture_output=True, text=True)
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            sentinel = build / 'retained-build-output'
            sentinel.write_text('keep incremental artifacts')
            result = subprocess.run([*command, '-Dcodec=true', '-Dcodec:extra=enabled', '-Dcodec:default_library=static'], capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertTrue(sentinel.is_file())
            options = json.loads(subprocess.check_output(['meson', 'introspect', '--buildoptions', str(build)], text=True))
            self.assertEqual(next(option['value'] for option in options if option['name'] == 'codec:extra'), 'enabled')


if __name__ == '__main__':
    unittest.main()
