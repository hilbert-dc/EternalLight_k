import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { promisify } from "node:util";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { build } from "esbuild";
import electron from "electron";

for (const fixture of ["search-experience-ui.tsx", "app-search-experience-ui.tsx"]) test(`search experience ${fixture}`, async t => {
  const directory = await mkdtemp(path.join(os.tmpdir(), "novel-search-ui-"));
  try {
    await build({ entryPoints: [fileURLToPath(new URL(`./fixtures/${fixture}`, import.meta.url))], outfile: path.join(directory, "test.js"), bundle: true, platform: "browser", jsx: "automatic",
      plugins: fixture.startsWith("app-") ? [{ name: "search-navigation-target", setup(builder) { builder.onResolve({ filter: /^\.\/CreativeWorkspaceView$/ }, () => ({ path: fileURLToPath(new URL("./fixtures/app-search-target.tsx", import.meta.url)) })); } }] : [] });
    await writeFile(path.join(directory, "index.html"), '<html><head><link rel="stylesheet" href="test.css"></head><body><div id="root"></div><script src="test.js"></script></body></html>');
    const { stdout } = await promisify(execFile)(electron, [fileURLToPath(new URL("./fixtures/creative-workspace-ui-main.cjs", import.meta.url)), path.join(directory, "index.html")], { timeout: 35000, env: { ...process.env, ELECTRON_RUN_AS_NODE: "" } });
    const result = JSON.parse(stdout.trim());
    assert.equal(result.passed, true, result.error);
    if (fixture.startsWith("app-")) {
      assert.equal(result.dirtyGate, true);
      assert.equal(result.searchCalls, 5);
      assert.equal(result.modelCalls, 0);
      t.assert.snapshot(result.appNavigation);
    } else {
      t.assert.snapshot(result.states);
      assert.deepEqual(result.targets, [
      { kind: "chapter", chapterId: "chapter-one" }, { kind: "chapter", chapterId: "chapter-one" },
      { kind: "view", view: "brief" }, { kind: "view", view: "outline" }, { kind: "view", view: "plans" },
      ]);
    }
  } finally { await rm(directory, { recursive: true, force: true }); }
});
