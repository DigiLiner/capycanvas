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
                     'sassc', 'glib-compile-resources', 'curl', 'patch', 'tar']:
            path = self.bin / tool
            path.write_text('#!/bin/sh\nexit 0\n')
            path.chmod(0o755)
        flags = shlex.join(['-I' + str(self.include)])
        (self.bin / 'pkg-config').write_text('#!/bin/sh\nprintf "%s\\n" ' + shlex.quote(flags) + '\n')
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


if __name__ == '__main__':
    unittest.main()
