// Build and ship the pinned GTK tablet fixes, with replaceable library and source.
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { cpSync, mkdirSync, readFileSync } from "node:fs";
import { join, resolve } from "node:path";

export function stageGtkRuntime(root, output) {
  const build = resolve(process.env.CAPY_GTK_BUILD_DIR || join(root, "target/gtk-runtime"));
  const prefix = join(build, "prefix");
  execFileSync("bash", [join(root, "tools/build/gtk-runtime/build.sh"), build, prefix], { stdio: "inherit" });
  const manifest = JSON.parse(readFileSync(join(prefix, "manifest.json"), "utf8"));
  for (const [file, expected] of Object.entries(manifest.files)) {
    if (createHash("sha256").update(readFileSync(join(prefix, file))).digest("hex") !== expected)
      throw new Error(`GTK runtime checksum mismatch: ${file}`);
  }
  const library = join(output, "lib/capycanvas/gtk");
  mkdirSync(library, { recursive: true });
  const docs = join(output, "share/doc/capycanvas-gtk");
  cpSync(join(prefix, "share/doc/capycanvas-gtk"), docs, { recursive: true });
  // Paths in the shipped manifest describe the actual relocated payload.
  for (const [file, digest] of Object.entries(manifest.files)) {
    if (!file.startsWith("lib/")) continue;
    const staged = file.replace(/^lib\//, "lib/capycanvas/gtk/");
    cpSync(join(prefix, file), join(output, staged));
    manifest.files[staged] = digest;
    delete manifest.files[file];
  }
  return manifest;
}
