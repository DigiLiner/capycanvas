#!/usr/bin/env python3
"""Check the native SDK before compiling the private runtime."""
import os
import shlex
import shutil
import subprocess
import sys

compiler = shlex.split(os.environ.get('CC', 'cc'))
cxx = shlex.split(os.environ.get('CXX', 'c++'))
tools = [compiler[0], cxx[0], 'pkg-config', 'meson', 'ninja', 'patchelf',
         'glslc', 'sassc', 'glib-compile-resources', 'curl', 'patch', 'tar']
missing = [tool for tool in tools if not shutil.which(tool)]
if missing:
    sys.exit('Missing native build tools: ' + ', '.join(missing)
             + '\nInstall the Linux build prerequisites before rebuilding.')

packages = ['gtk4 >= 4.22', 'libadwaita-1 >= 1.9', 'vulkan', 'libffi', 'mount', 'libpcre2-8']
flags = shlex.split(subprocess.check_output(['pkg-config', '--cflags', *packages], text=True))
source = '''#define PCRE2_CODE_UNIT_WIDTH 8
#include <gtk/gtk.h>
#include <adwaita.h>
#include <vulkan/vulkan.h>
#include <glib.h>
#include <gio/gio.h>
#include <ffi.h>
#include <libmount/libmount.h>
#include <pcre2.h>
'''
compiler += shlex.split(os.environ.get('CPPFLAGS', '')) + shlex.split(os.environ.get('CFLAGS', ''))
result = subprocess.run([*compiler, *flags, '-x', 'c', '-fsyntax-only', '-'], input=source, text=True)
if result.returncode:
    sys.exit('Native development headers are incomplete. Install the Linux build prerequisites before rebuilding.')
subprocess.run(['glib-compile-resources', '--version'], check=True, stdout=subprocess.DEVNULL)
