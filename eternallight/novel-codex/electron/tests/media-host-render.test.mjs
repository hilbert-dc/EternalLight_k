import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { promisify } from "node:util";
import { fileURLToPath } from "node:url";
import { test } from "node:test";
import electron from "electron";

test("real isolated Electron renders imported picture and balloons into PNG and a two-page PDF with fixed revision manifest", async () => {
  const directory = await mkdtemp(join(tmpdir(), "media-host-render-"));
  try {
    const result = await promisify(execFile)(electron, [fileURLToPath(new URL("./fixtures/media-host-render-main.cjs", import.meta.url)),
      fileURLToPath(new URL("../", import.meta.url)), directory], { timeout: 45_000, killSignal: "SIGKILL", env: { ...process.env, ELECTRON_RUN_AS_NODE: "" } });
    const output = JSON.parse(result.stdout);
    assert.equal(output.passed, true); assert.deepEqual(output.png, { width: 200, height: 300 }); assert.equal(output.pdfPages, 2);
    assert.ok(output.requests.includes("/api/projects/project/editions/edition/workspace?revisionId=saved-version"));
  } finally { await rm(directory, { recursive: true, force: true }); }
});
