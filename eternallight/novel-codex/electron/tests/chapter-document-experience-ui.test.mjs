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

test("chapter editing and publication preserve scene identity, bookmarks and preview recovery", async t => {
  const directory = await mkdtemp(path.join(os.tmpdir(), "novel-chapter-experience-"));
  try {
    await build({ entryPoints: [fileURLToPath(new URL("./fixtures/chapter-document-experience-ui.tsx", import.meta.url))], outfile: path.join(directory, "test.js"), bundle: true, platform: "browser", jsx: "automatic" });
    await writeFile(path.join(directory, "index.html"), '<html><head><link rel="stylesheet" href="test.css"></head><body><div id="root"></div><script src="test.js"></script></body></html>');
    let output;
    try {
      output = await promisify(execFile)(electron, [fileURLToPath(new URL("./fixtures/creative-workspace-ui-main.cjs", import.meta.url)), path.join(directory, "index.html")], { timeout: 35000, env: { ...process.env, ELECTRON_RUN_AS_NODE: "" } });
    } catch (error) { throw new Error(`${error.stdout ?? ""}\n${error.stderr ?? ""}`, { cause: error }); }
    const result = JSON.parse(output.stdout.trim());
    assert.equal(result.passed, true, result.error);
    assert.equal(result.themeChecks.length, 19);
    assert.ok(result.themeChecks.every(check => check.contrast >= 4.5), "Chapter text and enabled buttons must remain readable");
    const darkText = result.themeChecks.find(check => check.mode === "dark" && check.selector === ".story-reading-text p");
    const lightText = result.themeChecks.find(check => check.mode === "light" && check.selector === ".story-reading-text p");
    assert.notEqual(darkText.foreground, lightText.foreground, "Chapter text must follow the active theme");
    t.assert.snapshot({ dirtyRoundTrip: result.dirtyRoundTrip, generationPhaseChecks: result.generationPhaseChecks, publication: result.publicationStates, diff: result.initialDiff, metadata: result.metadataDiffSnapshot, adoption: result.adoptionHints, reviewProgress: result.reviewProgress, generationStatuses: result.generationStatuses, journey: result.candidateJourney, planning: result.planningAuthority, bookmark: result.sceneBookmark, layout: result.layoutChecks });
  } finally { await rm(directory, { recursive: true, force: true }); }
});
