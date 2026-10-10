import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import electron from "electron";

test("actual release subtitle worker renders with empty PATH and verifies the fixed font and three output frames", { timeout: 180_000 }, async () => {
  for (const key of ["NOVEL_CODEX_FFMPEG", "NOVEL_CODEX_FFPROBE", "NOVEL_CODEX_MEDIA_FONT"]) assert.ok(process.env[key], `${key} must be explicitly configured`);
  const directory = await mkdtemp(join(tmpdir(), "media-runtime-subtitle-smoke-"));
  try {
    const configuration = { directory, ffmpegPath: process.env.NOVEL_CODEX_FFMPEG, ffprobePath: process.env.NOVEL_CODEX_FFPROBE, fontPath: process.env.NOVEL_CODEX_MEDIA_FONT,
      platform: process.platform, architecture: process.arch };
    const env = Object.fromEntries(Object.entries({ PATH: "", TMPDIR: directory, TEMP: directory, TMP: directory, SystemRoot: process.env.SystemRoot, WINDIR: process.env.WINDIR }).filter(([, value]) => typeof value === "string"));
    const stdout = await new Promise((resolve, reject) => {
      const child = execFile(electron, [fileURLToPath(new URL("../../scripts/media-runtime-smoke.cjs", import.meta.url))], { cwd: directory, env, timeout: 170_000, killSignal: "SIGKILL", maxBuffer: 1024 * 1024 },
        (error, stdout, stderr) => error ? reject(new Error(`Real subtitle smoke failed: ${stderr}`, { cause: error })) : resolve(stdout));
      child.stdin.end(JSON.stringify(configuration));
    });
    const lines = stdout.split(/\r?\n/).filter(line => line.startsWith("MEDIA_RUNTIME_SMOKE "));
    assert.equal(lines.length, 1);
    const proof = JSON.parse(lines[0].slice("MEDIA_RUNTIME_SMOKE ".length));
    assert.equal(proof.encode, "h264"); assert.equal(proof.audio, "aac"); assert.equal(proof.frames, "verified"); assert.equal(proof.environmentPath, "empty");
    assert.deepEqual([proof.visiblePixels[0], proof.visiblePixels[2]], [0, 0]); assert.ok(proof.visiblePixels[1] >= 100);
    const digest = createHash("sha256").update(await readFile(configuration.fontPath)).digest("hex");
    assert.equal(proof.subtitleEvidence.length, 1);
    assert.equal(proof.subtitleEvidence[0].fontDigest, digest);
    assert.deepEqual(proof.subtitleEvidence[0].periods, [{ startFrame: 6, endFrame: 18, clipIds: ["caption"] }]);
    assert.ok(proof.subtitleEvidence[0].fontEvidence.every(node => node.fonts.length && node.fonts.every(font => font.isCustomFont && font.glyphCount > 0)));
  } finally { await rm(directory, { recursive: true, force: true }); }
});
