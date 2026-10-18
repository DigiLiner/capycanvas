"""Reject incomplete SDKs even when pkg-config reports success."""
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import sys
import tempfile
import unittest


class EnvironmentTest(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix='capy-native-sdk-')
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.bin = self.root / 'bin'
        self.bin.mkdir()
        self.include = self.root / 'include'
        for header in ['gtk/gtk.h', 'adwaita.h', 'vulkan/vulkan.h', 'glib.h',
                       'gio/gio.h', 'ffi.h', 'libmount/libmount.h', 'pcre2.h']:
            path = self.include / header
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text('')
        for tool in ['pkg-config', 'meson', 'ninja', 'patchelf', 'glslc',
                     'wayland-scanner', 'glib-compile-resources', 'curl', 'patch', 'tar', 'xz']:
            path = self.bin / tool
            path.write_text('#!/bin/sh\nexit 0\n')
            path.chmod(0o755)
        flags = shlex.join(['-I' + str(self.include)])
        (self.bin / 'pkg-config').write_text('#!/bin/sh\ncase "$1" in\n'
            '--modversion) printf "1.26.0\\n" ;;\n'
            '--variable=wayland_scanner) printf "%s\\n" ' + shlex.quote(str(self.bin / 'wayland-scanner')) + ' ;;\n'
            '--cflags) printf "%s\\n" ' + shlex.quote(flags) + ' ;;\nesac\n')
        (self.bin / 'meson').write_text('#!/bin/sh\nprintf "1.8.0\\n"\n')
        (self.bin / 'wayland-scanner').write_text('#!/bin/sh\nprintf "wayland-scanner 1.26.0\\n" >&2\n')
        self.env = {**os.environ, 'PATH': str(self.bin),
                    'CC': shutil.which('cc'), 'CXX': shutil.which('c++'),
                    'CPPFLAGS': '', 'CFLAGS': '-nostdinc'}

    def check(self):
        return subprocess.run([sys.executable, str(Path(__file__).with_name('check-env.py'))],
                              env=self.env, capture_output=True, text=True)

    def test_complete_sdk_compiles(self):
        result = self.check()
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_pkg_config_success_does_not_hide_missing_headers(self):
        (self.include / 'vulkan/vulkan.h').unlink()
        result = self.check()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('vulkan/vulkan.h', result.stderr)
        self.assertIn('Native development headers are incomplete', result.stderr)

    def test_builder_rejects_missing_headers_before_downloading_sources(self):
        (self.include / 'vulkan/vulkan.h').unlink()
        for tool in ['dirname', 'realpath', 'mkdir']:
            (self.bin / tool).symlink_to(shutil.which(tool))
        (self.bin / 'python3').symlink_to(sys.executable)
        downloaded = self.root / 'downloaded'
        (self.bin / 'curl').write_text('#!/bin/sh\nprintf attempted > ' + shlex.quote(str(downloaded)) + '\nexit 99\n')
        result = subprocess.run([shutil.which('bash'), str(Path(__file__).with_name('build.sh')),
            str(self.root / 'build'), str(self.root / 'prefix')], env=self.env, capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('Native development headers are incomplete', result.stderr)
        self.assertFalse(downloaded.exists())

    def test_missing_codegen_tool_is_reported_before_compilation(self):
        (self.bin / 'glib-compile-resources').unlink()
        result = self.check()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('Missing native build tools: glib-compile-resources', result.stderr)

    def test_old_meson_is_rejected_before_compilation(self):
        (self.bin / 'meson').write_text('#!/bin/sh\nprintf "1.7.2\\n"\n')
        result = self.check()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('Meson >= 1.8 is required; found 1.7.2', result.stderr)

    def test_mismatched_wayland_scanner_is_rejected_before_compilation(self):
        (self.bin / 'wayland-scanner').write_text('#!/bin/sh\nprintf "wayland-scanner 1.25.0\\n" >&2\n')
        result = self.check()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('Wayland scanner 1.25.0 must match client 1.26.0', result.stderr)
        self.assertIn('one coherent distribution SDK', result.stderr)

    def test_missing_native_dependency_is_reported_before_downloading_sources(self):
        (self.bin / 'pkg-config').write_text('#!/bin/sh\n[ "$2" != "harfbuzz >= 11.0.0" ]\n')
        for tool in ['dirname', 'realpath', 'mkdir']:
            (self.bin / tool).symlink_to(shutil.which(tool))
        (self.bin / 'python3').symlink_to(sys.executable)
        downloaded = self.root / 'downloaded'
        (self.bin / 'curl').write_text('#!/bin/sh\nprintf attempted > ' + shlex.quote(str(downloaded)) + '\nexit 99\n')
        result = subprocess.run([shutil.which('bash'), str(Path(__file__).with_name('build.sh')),
            str(self.root / 'build'), str(self.root / 'prefix')], env=self.env, capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('Missing or outdated native development packages: harfbuzz >= 11.0.0', result.stderr)
        self.assertIn('docs/development/linux.md#prerequisites', result.stderr)
        self.assertFalse(downloaded.exists())


if __name__ == '__main__':
    unittest.main()
