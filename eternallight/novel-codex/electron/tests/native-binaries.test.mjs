import assert from "node:assert/strict";
import test from "node:test";
import { resolve, join } from "node:path";
import { fileURLToPath } from "node:url";
import { nativeTestBinary } from "./helpers/native-binaries.mjs";

for (const name of ["novel-codex-story-runtime", "novel-workspace-mcp"]) {
  test(`current Native build selection is consistent for ${name}`, () => {
    const file = `${name}${process.platform === "win32" ? ".exe" : ""}`;
    const explicit = resolve("selected-native");
    assert.equal(nativeTestBinary(name, { NOVEL_CODEX_NATIVE_DIR: explicit, CARGO_TARGET_DIR: "ignored-target" }), join(explicit, file));
    assert.equal(nativeTestBinary(name, { CARGO_TARGET_DIR: "current-target" }), resolve("current-target", "debug", file));
    assert.equal(nativeTestBinary(name, {}), fileURLToPath(new URL(`../../native/target/debug/${file}`, import.meta.url)));
    assert.throws(() => nativeTestBinary(name, { NOVEL_CODEX_NATIVE_DIR: "relative" }), /must be absolute/);
  });
}
