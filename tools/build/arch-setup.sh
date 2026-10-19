#!/usr/bin/env bash
set -euo pipefail
arch_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
source "$arch_root/packaging/arch/capycanvas-git/PKGBUILD"
arch_privilege=()
if (( EUID != 0 )); then arch_privilege=(sudo); fi
exec "${arch_privilege[@]}" pacman -Syu --needed \
    base-devel rust devtools namcap mutter gjs pipewire wireplumber gst-plugin-pipewire python-gobject python-pillow libxmlb colord imagemagick \
    "${depends[@]}" "${makedepends[@]}" "$@"
