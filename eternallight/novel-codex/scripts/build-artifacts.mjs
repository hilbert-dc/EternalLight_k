import { createHash } from "node:crypto";
import { closeSync, existsSync, lstatSync, mkdirSync, openSync, readFileSync, readlinkSync, readSync, readdirSync, renameSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";

// Output roots are owned by the build, but must never redirect cleanup through a symlink.
export function assertBuildDirectory(directory) {
  let current = resolve(directory);
  while (true) {
    try {
      const stat = lstatSync(current);
      if (!stat.isDirectory() || stat.isSymbolicLink()) throw new Error(`构建目录不是普通目录：${current}`);
    } catch (error) { if (error.code !== "ENOENT") throw error; }
    const parent = dirname(current);
    if (parent === current) break;
    current = parent;
  }
}

export function acquireBuildLock(path) {
  assertBuildDirectory(dirname(path));
  mkdirSync(dirname(path), { recursive: true });
  let fd;
  try { fd = openSync(path, "wx"); }
  catch (error) {
    if (error.code === "EEXIST") throw new Error(`构建锁已存在：${path}；如前次进程异常退出，确认该进程已结束后移除锁再重试`);
    throw error;
  }
  writeFileSync(fd, `${JSON.stringify({ pid: process.pid, startedAt: new Date().toISOString() })}\n`);
  closeSync(fd);
  return () => rmSync(path);
}

export function resetBuildDirectory(directory) {
  assertBuildDirectory(directory);
  rmSync(directory, { recursive: true, force: true });
  mkdirSync(directory, { recursive: true });
}

// Both renames stay on the same filesystem. A failed second rename restores the last output.
export function recoverBuildOutput(output, previous) {
  assertBuildDirectory(output);
  assertBuildDirectory(previous);
  if (!existsSync(previous)) return;
  if (!existsSync(output)) renameSync(previous, output);
  else rmSync(previous, { recursive: true });
}

export function replaceBuildOutput(candidate, output, previous) {
  recoverBuildOutput(output, previous);
  assertBuildDirectory(candidate);
  if (existsSync(output)) renameSync(output, previous);
  try { renameSync(candidate, output); }
  catch (error) {
    if (existsSync(previous)) renameSync(previous, output);
    throw error;
  }
  rmSync(previous, { recursive: true, force: true });
}

export function sha256Artifact(path) {
  const hash = createHash("sha256"), buffer = Buffer.alloc(1024 * 1024);
  const fd = openSync(path, "r");
  try {
    let count;
    while ((count = readSync(fd, buffer, 0, buffer.length, null)) > 0) hash.update(buffer.subarray(0, count));
  } finally { closeSync(fd); }
  return hash.digest("hex");
}

export function artifactManifest(directory) {
  const files = {};
  function visit(current, prefix = "") {
    for (const entry of readdirSync(current, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name))) {
      const name = prefix ? `${prefix}/${entry.name}` : entry.name;
      if (name === "package-report.json") continue;
      const path = join(current, entry.name);
      if (entry.isDirectory()) visit(path, name);
      else if (entry.isSymbolicLink()) files[name] = { symlink: readlinkSync(path) };
      else if (entry.isFile()) files[name] = { sha256: sha256Artifact(path), size: lstatSync(path).size, mode: lstatSync(path).mode & 0o777 };
      else throw new Error(`不支持的构建产物类型：${path}`);
    }
  }
  visit(directory);
  return files;
}

export function writeBuildReport(path, report) {
  const temporary = `${path}.tmp`;
  writeFileSync(temporary, `${JSON.stringify(report, null, 2)}\n`);
  renameSync(temporary, path);
}

export function verifyPackageReport(output) {
  const report = JSON.parse(readFileSync(join(output, "package-report.json"), "utf8"));
  if (report.status !== "passed") throw new Error("当前产物没有通过本次打包验证");
  if (JSON.stringify(report.files) !== JSON.stringify(artifactManifest(output))) throw new Error("最终产物与打包验证报告不一致");
  return report;
}
