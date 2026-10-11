import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import {
  mkdtempSync,
  mkdirSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { runJourney } from "./journeys.mjs";

test("browser journeys require an error check", async () => {
  await assert.rejects(runJourney([[true, async () => {}]]), TypeError);
});

test("browser journeys await their error check", async () => {
  const failure = new Error("browser error");
  await assert.rejects(runJourney([[true, async () => {}, async () => { throw failure; }]]), error => error === failure);
});

const shell = process.env.LAYER_TEST_BASH || "/bin/bash";
const shellPath = path => process.platform === "win32" ? path.replaceAll("\\", "/").replace(/^([A-Za-z]):/, (_, drive) => "/" + drive.toLowerCase()) : path;
const launcher = fileURLToPath(new URL("run.sh", import.meta.url));
for (const [name, installed, customHome, override, expected] of [
  ["Cargo default directory outside PATH", ["home"], false, null, "home"],
  ["custom CARGO_HOME outside PATH", ["cargo"], true, null, "cargo"],
  ["PATH takes precedence", ["path", "home"], false, null, "path"],
  [
    "explicit executable takes precedence",
    ["path", "tool"],
    false,
    "tool",
    "tool",
  ],
  ["explicit command name", ["path"], false, "wasm-bindgen", "path"],
  ["missing tool", [], false, null, null],
  [
    "invalid override does not silently fall back",
    ["path"],
    false,
    "missing",
    null,
  ],
]) {
  test(name, (t) => {
    const root = mkdtempSync(join(tmpdir(), "layer-launcher-"));
    t.after(() => rmSync(root, { recursive: true, force: true }));
    const bin = join(root, "bin");
    const home = join(root, "user home");
    const cargo = join(root, "custom cargo");
    const locations = {
      path: join(bin, "wasm-bindgen"),
      home: join(home, ".cargo/bin/wasm-bindgen"),
      cargo: join(cargo, "bin/wasm-bindgen"),
      tool: join(root, "explicit tool/wasm-bindgen"),
    };
    function executable(path, message) {
      mkdirSync(dirname(path), { recursive: true });
      writeFileSync(path, `#!/bin/sh\n${message ? `printf '%s\\n' '${message}'` : ':'}\n`, {
        mode: 0o755,
      });
    }
    executable(join(bin, "cargo"), "build");
    writeFileSync(join(bin, "python3"), '#!/bin/sh\nif [ "$1" = "-m" ]; then printf "serve\\n"; fi\n', { mode: 0o755 });
    writeFileSync(join(bin, "dirname"), '#!/bin/sh\nexec /usr/bin/dirname "$@"\n', { mode: 0o755 });
    writeFileSync(join(bin, "bash"), '#!/bin/sh\nexec /bin/bash "$@"\n', { mode: 0o755 });
    for (const tool of installed) executable(locations[tool], tool);
    const env = { HOME: shellPath(home), PATH: shellPath(bin) };
    if (customHome) env.CARGO_HOME = shellPath(cargo);
    if (override) env.LAYER_WASM_BINDGEN = locations[override] ? shellPath(locations[override]) : override;
    const result = spawnSync(shell, [shellPath(launcher)], {
      env,
      encoding: "utf8",
    });
    assert.ifError(result.error);
    assert.equal(result.status, expected ? 0 : 1, result.stderr);
    if (expected) {
      assert.equal(result.stdout, `build\n${expected}\nserve\n`);
      assert.equal(result.stderr, "");
    } else {
      assert.equal(result.stdout, "", "must stop before building or serving");
      assert.match(
        result.stderr,
        override ? /LAYER_WASM_BINDGEN/ : /cargo install wasm-bindgen-cli/,
      );
    }
  });
}
