import assert from "node:assert/strict";
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, renameSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import test from "node:test";
import { packageRelease } from "../package-release.mjs";
import { resolveBuildLayout } from "../build-layout.mjs";
import { acquireBuildLock, recoverBuildOutput, replaceBuildOutput, resetBuildDirectory, sha256Artifact, verifyPackageReport } from "../build-artifacts.mjs";

function fixture(t, platform = "darwin", architecture = "arm64") {
  const root = mkdtempSync(join(realpathSync(tmpdir()), "package-layout-"));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const staging = join(root, ".release");
  const layout = { productRoot: root, staging, resources: join(staging, "resources"), packageOutput: join(staging, "package"), output: join(root, "release"), platform, architecture };
  mkdirSync(layout.output);
  writeFileSync(join(layout.output, "old-app"), "previous successful build");
  writeFileSync(join(root, "package.json"), JSON.stringify({ productName: "Test App", build: {} }));
  const calls = [];
  let generation = 0, fail = null;
  const execute = (program, args, options) => {
    calls.push({ program, args, timeout: options.timeout });
    if (args[0] === "run") {
      if (fail === args[1]) return { status: 1 };
      if (args[1] === "stage:release-runtime") {
        mkdirSync(layout.resources, { recursive: true });
        writeFileSync(join(layout.resources, "runtime"), "verified binary");
        writeFileSync(join(layout.resources, "release-manifest.json"), JSON.stringify({ product: { commit: "test", dirty: true }, codex: { buildIdentityId: "fixture" }, resources: { files: { runtime: sha256Artifact(join(layout.resources, "runtime")) } } }));
      }
    } else if (args.includes("--config")) {
      const config = JSON.parse(readFileSync(args.at(-1), "utf8"));
      assert.equal(config.directories.output, layout.packageOutput);
      assert.equal(existsSync(join(layout.packageOutput, "stale-file")), false);
      const unpacked = platform === "darwin" ? `mac${architecture === "x64" ? "" : `-${architecture}`}` : `win${architecture === "x64" ? "" : `-${architecture}`}-unpacked`;
      const app = platform === "darwin"
        ? join(layout.packageOutput, unpacked, "Test App.app", "Contents", "MacOS", "Test App")
        : join(layout.packageOutput, unpacked, "Test App.exe");
      mkdirSync(dirname(app), { recursive: true });
      writeFileSync(app, `generation ${++generation}`);
      const resources = platform === "darwin" ? join(dirname(dirname(app)), "Resources") : join(dirname(app), "resources");
      cpSync(layout.resources, resources, { recursive: true });
      if (fail === "resource-tampering") writeFileSync(join(resources, "runtime"), "changed during packaging");
      if (fail === "builder") return { status: 1 };
    } else {
      assert.ok(program.startsWith(layout.packageOutput));
      assert.equal(options.env.NOVEL_CODEX_PACKAGE_SMOKE, "1");
      assert.ok(existsSync(options.env.NOVEL_CODEX_USER_DATA_DIR));
      if (fail === "smoke") return { status: 1, stdout: "", stderr: "startup failed" };
      return { status: 0, stdout: "NOVEL_CODEX_PACKAGE_SMOKE_OK" };
    }
    return { status: 0 };
  };
  return { layout, execute, calls, fail: value => { fail = value; } };
}

test("fixed output layout rejects retired output overrides", () => {
  const layout = resolveBuildLayout({});
  assert.equal(layout.output, join(layout.productRoot, "release"));
  for (const key of ["NOVEL_CODEX_RELEASE_OUTPUT", "NOVEL_CODEX_RELEASE_RESOURCES"]) {
    assert.throws(() => resolveBuildLayout({ [key]: "/arbitrary" }), /已移除/);
  }
});

for (const [platform, architecture] of [["darwin", "arm64"], ["darwin", "x64"], ["win32", "x64"], ["win32", "arm64"]]) {
  test(`repeated ${platform}/${architecture} package replaces output only after smoke and binds file hashes`, t => {
    const f = fixture(t, platform, architecture);
    mkdirSync(f.layout.packageOutput, { recursive: true });
    writeFileSync(join(f.layout.packageOutput, "stale-file"), "obsolete");
    const first = packageRelease(f);
    assert.equal(existsSync(join(f.layout.output, "old-app")), false);
    assert.equal(verifyPackageReport(f.layout.output).status, "passed");
    const second = packageRelease(f);
    assert.notDeepEqual(first.files, second.files);
    assert.equal(existsSync(join(f.layout.staging, "previous-release")), false);
    assert.equal(existsSync(join(f.layout.staging, "package.lock")), false);
    writeFileSync(join(f.layout.output, second.executable), "changed after smoke");
    assert.throws(() => verifyPackageReport(f.layout.output), /不一致/);
  });
}

for (const failure of ["stage:release-runtime", "verify:release-runtime", "build", "builder", "resource-tampering", "smoke"]) {
  test(`${failure} failure preserves previous output and records failed stage`, t => {
    const f = fixture(t);
    const previous = packageRelease(f);
    f.fail(failure);
    assert.throws(() => packageRelease(f), /失败|摘要不匹配/);
    assert.deepEqual(verifyPackageReport(f.layout.output), previous);
    const failed = JSON.parse(readFileSync(join(f.layout.staging, "package-report.json"), "utf8"));
    assert.equal(failed.status, "failed");
    assert.notEqual(failed.stage, "complete");
    assert.equal(existsSync(join(f.layout.staging, "package.lock")), false);
  });
}

test("concurrent package refuses without changing candidate or previous output", t => {
  const f = fixture(t);
  const unlock = acquireBuildLock(join(f.layout.staging, "package.lock"));
  try {
    assert.throws(() => packageRelease(f), /构建锁已存在/);
    assert.equal(f.calls.length, 0);
    assert.equal(readFileSync(join(f.layout.output, "old-app"), "utf8"), "previous successful build");
  } finally { unlock(); }
});

test("failed replacement restores previous output; interrupted rename recovers", t => {
  const { layout } = fixture(t);
  mkdirSync(layout.staging);
  const previous = join(layout.staging, "previous-release");
  assert.throws(() => replaceBuildOutput(join(layout.staging, "missing"), layout.output, previous), /ENOENT/);
  assert.ok(existsSync(join(layout.output, "old-app")));
  renameSync(layout.output, previous);
  recoverBuildOutput(layout.output, previous);
  assert.ok(existsSync(join(layout.output, "old-app")));
});

test("cleanup rejects symlink output without touching its target", t => {
  const { layout } = fixture(t);
  const link = join(layout.productRoot, "linked");
  symlinkSync(layout.output, link, process.platform === "win32" ? "junction" : "dir");
  assert.throws(() => resetBuildDirectory(link), /普通目录/);
  assert.ok(existsSync(join(layout.output, "old-app")));
});

 test("startup wait is bounded, propagated and recorded without skipping smoke", t => {
  const f = fixture(t);
  for (const value of [0, -1, 999, 300001, Infinity, "180000"]) {
    assert.throws(() => packageRelease({ ...f, startupTimeoutMs: value }), /startupTimeoutMs/);
  }
  assert.equal(f.calls.length, 0);
  const report = packageRelease({ ...f, startupTimeoutMs: 180000 });
  assert.equal(report.startupTimeoutMs, 180000);
  assert.deepEqual(f.calls.find(call => call.args[1] === "verify:release-runtime").args,
    ["run", "verify:release-runtime", "--", "180000"]);
  assert.equal(f.calls.find(call => call.program.startsWith(f.layout.packageOutput)).timeout, 180000);
  assert.equal(report.checks["application-smoke"], "passed");
  f.fail("smoke");
  assert.throws(() => packageRelease({ ...f, startupTimeoutMs: 180000 }), /失败/);
});
