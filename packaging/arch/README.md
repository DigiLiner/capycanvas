# Arch Linux packaging

[Linux development](../../docs/development/linux.md)

`capycanvas-git/` contains the AUR recipe and `.SRCINFO`. It builds upstream Git
with desktop integration, runtime filters, original license notices and a private
patched GTK library. GTK 4.24+, a Wayland session and a hardware Vulkan driver
are required.

The recipe downloads checksum-pinned GTK and codec sources through makepkg.
`prepare()` fetches Rust dependencies and original license notices; compilation
and checks run offline. Filters install under `/usr/share/capycanvas/filters`.
The native packager strips application debug data; makepkg preserves the bundled
GTK library so its installed checksum manifest remains valid.
makepkg's LTO is disabled because bundled SQLite's GCC objects cannot link with
Rust's LLVM linker.

## Build and install

From `packaging/arch/capycanvas-git`:

```bash
makepkg --syncdeps --install
```

For source development, use the [Linux prerequisites](../../docs/development/linux.md#prerequisites)
and launcher instead.

## Validate and submit

Build a clean package with Arch's `devtools`, then inspect it with `namcap`:

```bash
extra-x86_64-build
namcap PKGBUILD capycanvas-git-*.pkg.tar.zst
makepkg --printsrcinfo > .SRCINFO
```

Check installation, launch and removal in an isolated Arch environment. Submit
`PKGBUILD` and the matching `.SRCINFO` to the `capycanvas-git` AUR repository;
package archives and local build files stay out of that repository.
