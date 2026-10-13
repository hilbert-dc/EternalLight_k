import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { promisify } from "node:util";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { build } from "esbuild";
import electron from "electron";

test("unfinished turns keep public progress and short replies readable while long drafts, saved replies and recovery retain their boundaries", { timeout: 40000 }, async () => {
  const directory = await mkdtemp(path.join(os.tmpdir(), "novel-unfinished-output-"));
  try {
    await build({ entryPoints: [fileURLToPath(new URL("./fixtures/unfinished-output-ui.tsx", import.meta.url))],
      outfile: path.join(directory, "test.js"), bundle: true, platform: "browser", jsx: "automatic",
      plugins: [{ name: "observe-production-transcript", setup(builder) {
        builder.onLoad({ filter: /\/src\/App\.tsx$/ }, async ({ path: source }) => ({
          // Expose existing production renderers only inside this temporary test
          // bundle. Production exports, APIs and execution are unchanged.
          contents: `${await readFile(source, "utf8")}\nexport { Composer, EarlierTurn, EnhancedAgentTranscript };`, loader: "tsx",
        }));
      } }],
    });
    await writeFile(path.join(directory, "index.html"), '<html><head><meta charset="utf-8"><link rel="stylesheet" href="test.css"></head><body><div id="root"></div><script src="test.js"></script></body></html>');
    let output;
    try { output = await promisify(execFile)(electron, [fileURLToPath(new URL("./fixtures/creative-workspace-ui-main.cjs", import.meta.url)), path.join(directory, "index.html")],
      { timeout: 35000, env: { ...process.env, ELECTRON_RUN_AS_NODE: "" } }); }
    catch (error) { throw new Error(`${error.stdout ?? ""}\n${error.stderr ?? ""}`, { cause: error }); }
    const result = JSON.parse(output.stdout.trim());
    assert.equal(result.passed, true, `${result.error ?? ""}\n${result.dom ?? ""}`);
    assert.deepEqual(result.cases, ["failed draft and public progress", "unicode threshold", "short unknown reply", "short absent reply",
      "short saved streams", "terminal progress continuity", "completed answer", "saved final reply", "cancelled content", "unknown read", "absent reply",
      "identity mismatch", "late identity response", "reader and timeline", "history", "history commentary field", "work panel recovery"]);
    assert.deepEqual(result.availabilityCases, ["ready", "qualified after failed turn", "history loading",
      "history failure with qualified model", "conversation changing", "archived conversation", "other conversation running",
      "runtime unavailable with qualified model", "model qualification unavailable", "model configuration applying",
      "active turn steering and stop", "availability restored"]);
  } finally { await rm(directory, { recursive: true, force: true }); }
});
