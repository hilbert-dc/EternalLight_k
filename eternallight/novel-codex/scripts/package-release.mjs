import { startupTimeout } from "./startup-timeout.mjs";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, statSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";
import { resolveBuildLayout } from "./build-layout.mjs";
import { acquireBuildLock, artifactManifest, recoverBuildOutput, replaceBuildOutput, resetBuildDirectory, sha256Artifact, writeBuildReport } from "./build-artifacts.mjs";

export function packageRelease({ target = "dir", layout = resolveBuildLayout(), execute = spawnSync, startupTimeoutMs = 30000 } = {}) {
  startupTimeoutMs = startupTimeout(startupTimeoutMs);
  if (!["dir", "mac", "win"].includes(target)) throw new Error("package target must be dir, mac, or win");
  if (!["darwin", "win32"].includes(layout.platform)
    || target === "mac" && layout.platform !== "darwin"
    || target === "win" && layout.platform !== "win32") throw new Error("打包和启动验证必须在对应的 macOS/Windows 主机上执行");
  const unlock = acquireBuildLock(join(layout.staging, "package.lock"));
  const previous = join(layout.staging, "previous-release");
  const reportPath = join(layout.staging, "package-report.json");
  const report = { version: 1, status: "running", target, platform: layout.platform, architecture: layout.architecture,
    startupTimeoutMs, startedAt: new Date().toISOString(), stage: "prepare", checks: {} };
  function save() { writeBuildReport(reportPath, report); }
  function run(stage, program, args) {
    report.stage = stage;
    save();
    const result = execute(program, args, { cwd: layout.productRoot, stdio: "inherit", shell: process.platform === "win32" && program === "npm" });
    if (result.error || result.status !== 0) throw result.error ?? new Error(`${stage} 失败 (${result.status})`);
    report.checks[stage] = "passed";
    save();
  }
  try {
    recoverBuildOutput(layout.output, previous);
    resetBuildDirectory(layout.packageOutput);
    save();
    run("staging", "npm", ["run", "stage:release-runtime"]);
    run("runtime-verification", "npm", ["run", "verify:release-runtime", "--", String(startupTimeoutMs)]);
    const resourcesManifest = join(layout.resources, "release-manifest.json");
    const resources = JSON.parse(readFileSync(resourcesManifest, "utf8"));
    report.resourcesManifestSha256 = sha256Artifact(resourcesManifest);
    run("frontend-build", "npm", ["run", "build"]);

    const packageJson = JSON.parse(readFileSync(join(layout.productRoot, "package.json"), "utf8"));
    const configPath = join(layout.staging, "electron-builder.generated.json");
    writeBuildReport(configPath, {
      ...packageJson.build,
      directories: { ...(packageJson.build.directories ?? {}), output: layout.packageOutput },
      extraResources: [{ from: layout.resources, to: ".", filter: ["**/*"] }],
    });
    const builder = join(layout.productRoot, "node_modules", "electron-builder", "cli.js");
    run("packaging", process.execPath, [builder, target === "dir" ? "--dir" : `--${target}`, `--${layout.architecture}`, "--publish", "never", "--config", configPath]);

    const unpacked = layout.platform === "darwin"
      ? `mac${layout.architecture === "x64" ? "" : `-${layout.architecture}`}`
      : `win${layout.architecture === "x64" ? "" : `-${layout.architecture}`}-unpacked`;
    const packagedExecutable = layout.platform === "darwin"
      ? join(layout.packageOutput, unpacked, `${packageJson.productName}.app`, "Contents", "MacOS", packageJson.productName)
      : join(layout.packageOutput, unpacked, `${packageJson.productName}.exe`);
    if (!statSync(packagedExecutable).isFile()) throw new Error("找不到本次打包的可执行文件");
    const packagedResources = layout.platform === "darwin"
      ? join(layout.packageOutput, unpacked, `${packageJson.productName}.app`, "Contents", "Resources")
      : join(layout.packageOutput, unpacked, "resources");
    if (sha256Artifact(join(packagedResources, "release-manifest.json")) !== report.resourcesManifestSha256) {
      throw new Error("包内资源清单与本次验证的资源不一致");
    }
    for (const [name, digest] of Object.entries(resources.resources.files)) {
      if (sha256Artifact(join(packagedResources, name)) !== digest) throw new Error(`包内资源摘要不匹配：${name}`);
    }
    report.checks["packaged-resources"] = "passed";
    report.stage = "application-smoke";
    save();
    const smokeHome = mkdtempSync(join(tmpdir(), "novel-codex-package-smoke-"));
    try {
      const smoke = execute(packagedExecutable, [], {
        cwd: layout.productRoot,
        encoding: "utf8",
        timeout: startupTimeoutMs,
        env: Object.fromEntries(Object.entries({
          PATH: process.env.PATH,
          PATHEXT: process.env.PATHEXT,
          SystemRoot: process.env.SystemRoot,
          WINDIR: process.env.WINDIR,
          ComSpec: process.env.ComSpec,
          TEMP: process.env.TEMP,
          TMP: process.env.TMP,
          TMPDIR: process.env.TMPDIR,
          NOVEL_CODEX_PACKAGE_SMOKE: "1",
          NOVEL_CODEX_USER_DATA_DIR: smokeHome,
        }).filter(([, value]) => typeof value === "string" && value)),
      });
      if (smoke.error || smoke.status !== 0 || !smoke.stdout?.includes("NOVEL_CODEX_PACKAGE_SMOKE_OK")) {
        throw new Error(`最终应用 smoke 失败 (${smoke.status}): ${smoke.stderr || smoke.stdout}`);
      }
    } finally {
      rmSync(smokeHome, { recursive: true, force: true });
    }

    report.checks["application-smoke"] = "passed";
    report.stage = "publish";
    save();
    report.source = resources.product;
    report.codexBuildIdentity = resources.codex.buildIdentityId;
    report.executable = relative(layout.packageOutput, packagedExecutable).replaceAll("\\", "/");
    report.files = artifactManifest(layout.packageOutput);
    const completed = { ...report, status: "passed", stage: "complete", completedAt: new Date().toISOString() };
    writeBuildReport(join(layout.packageOutput, "package-report.json"), completed);
    replaceBuildOutput(layout.packageOutput, layout.output, previous);
    Object.assign(report, completed);
    save();
    return report;
  } catch (error) {
    report.status = "failed";
    report.error = error.message;
    report.completedAt = new Date().toISOString();
    save();
    throw error;
  } finally { unlock(); }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  packageRelease({ target: process.argv[2] ?? "dir" });
  process.stdout.write("打包和启动验证通过：release/（摘要与验证记录：release/package-report.json）\n");
}
