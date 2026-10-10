import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { promisify } from "node:util";
import { fileURLToPath } from "node:url";
import test from "node:test";
import electron from "electron";

test("real isolated Electron uses the explicit CJK font and exact MP4 subtitle frames, and Host persists their actual evidence", { timeout: 180_000 }, async () => {
  for (const key of ["NOVEL_CODEX_FFMPEG", "NOVEL_CODEX_FFPROBE", "NOVEL_CODEX_MEDIA_FONT"]) assert.ok(process.env[key], `${key} must be explicitly configured for actual subtitle rendering`);
  const directory = await mkdtemp(join(tmpdir(), "media-subtitle-actual-"));
  try {
    const { stdout } = await promisify(execFile)(electron, [fileURLToPath(new URL("./fixtures/media-subtitle-render-main.cjs", import.meta.url)),
      fileURLToPath(new URL("../", import.meta.url)), directory], { timeout: 170_000, killSignal: "SIGKILL", maxBuffer: 64 * 1024,
      env: { ...process.env, ELECTRON_RUN_AS_NODE: "" } });
    const result = JSON.parse(stdout);
    assert.equal(result.passed, true); assert.equal(result.rasters, 3);
    assert.equal(result.transparent, true); assert.ok(result.fontEvidence.every(record => record.fonts.every(font => font.isCustomFont && font.glyphCount > 0)));
    assert.equal(result.frames[5], 0); assert.ok(result.frames[6] > 10); assert.equal(result.frames[24], 0);
    assert.ok(result.frames[18] > result.frames[17], "overlapping subtitles are stacked rather than covering one another");
    assert.deepEqual(result.boundaries, [[6, 12], [12, 18], [18, 24]]);
    assert.equal(result.negativeChecks, 4); assert.equal(result.captionPreserved, true);
    assert.equal(result.networkBlocked, true); assert.equal(result.permissionDenied, true);
    assert.equal(result.hostManifestVerified, true); assert.deepEqual(result.emptyHostEvidence, []);
    assert.equal(result.hostFrameCounts[0], 0); assert.ok(result.hostFrameCounts[1] > 10); assert.ok(result.hostFrameCounts[2] > 10); assert.equal(result.hostFrameCounts[3], 0);
  } finally { await rm(directory, { recursive: true, force: true }); }
});
