import assert from "node:assert/strict";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { buildMediaRuntime, validateArchiveEntries, verifySource } from "../build-media-runtime.mjs";
import { acquireBuildLock, sha256Artifact } from "../build-artifacts.mjs";
import { resolveBuildLayout } from "../build-layout.mjs";

function fixture(t) {
  const root = mkdtempSync(join(realpathSync(tmpdir()), "media-build-test-"));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const staging = join(root, ".release"), mediaRuntime = join(staging, "media-runtime", "darwin-arm64");
  mkdirSync(mediaRuntime, { recursive: true });
  writeFileSync(join(mediaRuntime, "previous"), "verified bundle");
  return { staging, mediaRuntime, platform: process.platform, architecture: process.arch };
}

test("source verification refuses corruption and symlinks", t => {
  const f = fixture(t), path = join(f.staging, "source");
  writeFileSync(path, "trusted");
  const source = { file: "source", sha256: sha256Artifact(path) };
  verifySource(path, source);
  symlinkSync(path, `${path}.link`);
  assert.throws(() => verifySource(`${path}.link`, source), /摘要不匹配/);
  writeFileSync(path, "corrupted");
  assert.throws(() => verifySource(path, source), /摘要不匹配/);
});

test("archive paths and build targets cannot escape owned directories", () => {
  validateArchiveEntries("source/\nsource/a.c\n", "source");
  for (const entries of ["", "/source/a", "source/../escape", "source/dir/../../escape", "other/a", "source/dir\\escape"]) {
    assert.throws(() => validateArchiveEntries(entries, "source"), /路径越界/);
  }
  for (const key of ["NOVEL_CODEX_TARGET_PLATFORM", "NOVEL_CODEX_TARGET_ARCH"]) {
    assert.throws(() => resolveBuildLayout({ [key]: "../../escape" }), /不能包含路径/);
  }
  const layout = resolveBuildLayout({ NOVEL_CODEX_TARGET_PLATFORM: "darwin", NOVEL_CODEX_TARGET_ARCH: "arm64" });
  assert.equal(layout.mediaRuntime, join(layout.staging, "media-runtime", "darwin-arm64"));
});

test("corrupted cache preserves previous bundle and records failure before compilation", { skip: process.platform !== "darwin" }, t => {
  const layout = fixture(t);
  const lock = JSON.parse(readFileSync(new URL("../media-runtime-sources.json", import.meta.url)));
  const cache = join(layout.staging, "media-sources");
  mkdirSync(cache);
  writeFileSync(join(cache, lock.sources.ffmpeg.file), "corrupted cache");
  assert.throws(() => buildMediaRuntime({ layout }), /摘要不匹配/);
  assert.equal(readFileSync(join(layout.mediaRuntime, "previous"), "utf8"), "verified bundle");
  const report = JSON.parse(readFileSync(join(layout.staging, "media-build-report.json")));
  assert.equal(report.status, "failed");
  assert.deepEqual(report.commands, []);
  assert.equal(existsSync(join(layout.staging, "media-build.lock")), false);
});

test("concurrent media build refuses without touching previous output", { skip: process.platform !== "darwin" }, t => {
  const layout = fixture(t), unlock = acquireBuildLock(join(layout.staging, "media-build.lock"));
  try {
    assert.throws(() => buildMediaRuntime({ layout }), /构建锁已存在/);
    assert.equal(readFileSync(join(layout.mediaRuntime, "previous"), "utf8"), "verified bundle");
  } finally { unlock(); }
});
