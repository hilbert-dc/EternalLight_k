import { createHash, randomUUID } from "node:crypto";
import {
  closeSync,
  existsSync,
  fsyncSync,
  lstatSync,
  mkdirSync,
  openSync,
  readFileSync,
  renameSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { basename, dirname, isAbsolute, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { readBuildIdentity } from "./build-provenance.mjs";

const scriptPath = fileURLToPath(import.meta.url);
const REQUIRED_PLATFORMS = Object.freeze({ macos: "darwin", windows: "win32" });
const REQUIRED_CHECKS = Object.freeze(["installation", "execution", "cancellation", "recovery", "upgrade"]);
const MAX_JSON_BYTES = 1024 * 1024;

function fail(message) {
  throw new Error(message);
}

function sha256(value) {
  return createHash("sha256").update(value).digest("hex");
}

function fileDigest(path) {
  return sha256(readFileSync(path));
}

function readJson(path, label) {
  const resolved = resolve(path);
  const stat = lstatSync(resolved);
  if (!stat.isFile() || stat.isSymbolicLink()) fail(`${label} 必须是普通文件`);
  if (stat.size > MAX_JSON_BYTES) fail(`${label} 超过 ${MAX_JSON_BYTES} 字节限制`);
  try {
    return JSON.parse(readFileSync(resolved, "utf8"));
  } catch (error) {
    fail(`${label} 不是有效 JSON：${error instanceof Error ? error.message : String(error)}`);
  }
}

function assertObject(value, label) {
  if (!value || typeof value !== "object" || Array.isArray(value)) fail(`${label} 必须是对象`);
  return value;
}

function assertString(value, label, pattern, maximum = 256) {
  if (typeof value !== "string" || value.length === 0 || value.length > maximum || (pattern && !pattern.test(value))) fail(`${label} 无效`);
  return value;
}

function assertTimestamp(value, label) {
  assertString(value, label, null, 64);
  if (!Number.isFinite(Date.parse(value))) fail(`${label} 无效`);
  return value;
}

function assertDigest(value, label) {
  return assertString(value, label, /^[a-f0-9]{64}$/);
}

function assertGitObject(value, label) {
  return assertString(value, label, /^(?:[a-f0-9]{40}|[a-f0-9]{64})$/);
}

function sameTree(left, right) {
  return left.commitSha === right.commitSha && left.treeSha === right.treeSha;
}

function referencedFile(manifestPath, reference, label) {
  assertString(reference, `${label}.path`, null, 512);
  if (isAbsolute(reference)) fail(`${label}.path 必须相对于候选清单`);
  const base = dirname(resolve(manifestPath));
  const path = resolve(base, reference);
  const relation = relative(base, path);
  if (!relation || relation.startsWith("..") || isAbsolute(relation)) fail(`${label}.path 必须位于候选目录内`);
  const stat = lstatSync(path);
  if (!stat.isFile() || stat.isSymbolicLink()) fail(`${label}.path 必须是候选目录内的普通文件`);
  return path;
}

function normalizeTree(value, label) {
  const tree = assertObject(value, label);
  return Object.freeze({
    commitSha: assertGitObject(tree.commitSha, `${label}.commitSha`),
    treeSha: assertGitObject(tree.treeSha, `${label}.treeSha`),
  });
}

function verifyChecks(value, platform) {
  if (!Array.isArray(value)) fail(`${platform} 验证报告缺少 checks`);
  const byId = new Map();
  for (const entry of value) {
    assertObject(entry, `${platform}.checks[]`);
    const id = assertString(entry.id, `${platform}.checks[].id`, /^[a-z][a-z0-9-]{0,63}$/);
    if (byId.has(id)) fail(`${platform} 验证报告包含重复检查：${id}`);
    byId.set(id, entry.status);
  }
  for (const id of REQUIRED_CHECKS) {
    if (byId.get(id) !== "passed") fail(`${platform} 必需检查未通过：${id}`);
  }
}

function verifyPlatform({ candidateId, candidateTree, manifestPath, platform, buildReference, reportReference }) {
  const expectedNodePlatform = REQUIRED_PLATFORMS[platform];
  const buildRef = assertObject(buildReference, `builds.${platform}`);
  const buildPath = referencedFile(manifestPath, buildRef.path, `builds.${platform}`);
  const declaredBuildDigest = assertDigest(buildRef.sha256, `builds.${platform}.sha256`);
  if (fileDigest(buildPath) !== declaredBuildDigest) fail(`${platform} BuildIdentity 文件摘要不匹配`);
  const buildIdentity = readBuildIdentity(buildPath);
  if (buildIdentity.target?.platform !== expectedNodePlatform) fail(`${platform} BuildIdentity 平台不匹配`);

  const reportRef = assertObject(reportReference, `platformReports.${platform}`);
  const reportPath = referencedFile(manifestPath, reportRef.path, `platformReports.${platform}`);
  const declaredReportDigest = assertDigest(reportRef.sha256, `platformReports.${platform}.sha256`);
  if (fileDigest(reportPath) !== declaredReportDigest) fail(`${platform} 验证报告文件摘要不匹配`);
  const report = assertObject(readJson(reportPath, `${platform} 验证报告`), `${platform} 验证报告`);
  if (report.version !== 1 || report.kind !== "novel-codex-platform-validation" || report.status !== "passed") fail(`${platform} 验证报告未通过`);
  if (report.platform !== platform || report.candidateId !== candidateId) fail(`${platform} 验证报告候选身份不匹配`);
  if (!sameTree(normalizeTree(report.candidateTree, `${platform}.candidateTree`), candidateTree)) fail(`${platform} 验证报告未绑定当前 candidate tree`);
  const reportBuild = assertObject(report.buildIdentity, `${platform}.buildIdentity`);
  if (reportBuild.identityId !== buildIdentity.identityId || reportBuild.sha256 !== declaredBuildDigest) fail(`${platform} 验证报告未绑定当前 BuildIdentity`);
  assertTimestamp(report.completedAt, `${platform}.completedAt`);
  verifyChecks(report.checks, platform);
  return Object.freeze({
    platform,
    buildIdentity: Object.freeze({ identityId: buildIdentity.identityId, sha256: declaredBuildDigest }),
    report: Object.freeze({ sha256: declaredReportDigest }),
  });
}

export function validateCandidateManifest(manifestPath) {
  const resolvedManifest = resolve(manifestPath);
  const manifest = assertObject(readJson(resolvedManifest, "候选清单"), "候选清单");
  if (manifest.version !== 1 || manifest.kind !== "novel-codex-upstream-candidate") fail("候选清单类型或版本无效");
  const candidateId = assertString(manifest.candidateId, "candidateId", /^[a-z0-9][a-z0-9._-]{0,127}$/);
  assertTimestamp(manifest.createdAt, "createdAt");
  const candidateTree = normalizeTree(manifest.candidateTree, "candidateTree");
  const builds = assertObject(manifest.builds, "builds");
  const reports = assertObject(manifest.platformReports, "platformReports");
  const platforms = Object.keys(REQUIRED_PLATFORMS).map((platform) => verifyPlatform({
    candidateId,
    candidateTree,
    manifestPath: resolvedManifest,
    platform,
    buildReference: builds[platform],
    reportReference: reports[platform],
  }));
  return Object.freeze({
    candidateId,
    candidateTree,
    manifest: Object.freeze({ path: resolvedManifest, sha256: fileDigest(resolvedManifest) }),
    platforms: Object.freeze(platforms),
  });
}

function durableAtomicJson(path, value, { replace }) {
  const resolved = resolve(path);
  const directory = dirname(resolved);
  mkdirSync(directory, { recursive: true });
  if (!replace && existsSync(resolved)) fail(`目标文件已存在：${resolved}`);
  const temporary = resolve(directory, `.${basename(resolved)}.${process.pid}.${randomUUID()}.tmp`);
  let descriptor = null;
  try {
    descriptor = openSync(temporary, "wx", 0o600);
    writeFileSync(descriptor, `${JSON.stringify(value, null, 2)}\n`, "utf8");
    fsyncSync(descriptor);
    closeSync(descriptor);
    descriptor = null;
    renameSync(temporary, resolved);
    try {
      const directoryDescriptor = openSync(directory, "r");
      try { fsyncSync(directoryDescriptor); } finally { closeSync(directoryDescriptor); }
    } catch {
      // Directory fsync is not supported by every target filesystem. The file
      // itself is already fsynced and the atomic rename has completed.
    }
  } finally {
    if (descriptor !== null) closeSync(descriptor);
    rmSync(temporary, { force: true });
  }
}

export function writeCandidateValidation({ manifestPath, outputPath, now = new Date().toISOString() }) {
  assertTimestamp(now, "validatedAt");
  const candidate = validateCandidateManifest(manifestPath);
  const body = {
    version: 1,
    kind: "novel-codex-candidate-validation",
    status: "passed",
    validatedAt: now,
    candidate,
  };
  const validation = Object.freeze({ validationId: sha256(JSON.stringify(body)), ...body });
  durableAtomicJson(outputPath, validation, { replace: false });
  return validation;
}

function readValidation(validationPath) {
  const resolved = resolve(validationPath);
  const validation = assertObject(readJson(resolved, "候选验证结果"), "候选验证结果");
  const { validationId, ...body } = validation;
  if (validation.version !== 1 || validation.kind !== "novel-codex-candidate-validation" || validation.status !== "passed") fail("候选验证结果未通过");
  if (validationId !== sha256(JSON.stringify(body))) fail("候选验证结果摘要无效");
  const current = validateCandidateManifest(validation.candidate?.manifest?.path);
  if (current.manifest.sha256 !== validation.candidate.manifest.sha256 || JSON.stringify(current) !== JSON.stringify(validation.candidate)) fail("候选输入在验证后发生变化");
  return { path: resolved, sha256: fileDigest(resolved), validation, candidate: current };
}

function withReleaseLock(stateDirectory, operation) {
  const state = resolve(stateDirectory);
  mkdirSync(state, { recursive: true });
  const lock = resolve(state, ".stable-release.lock");
  try {
    mkdirSync(lock);
  } catch {
    fail("稳定版本正在被其他进程更新");
  }
  try {
    return operation(state);
  } finally {
    rmSync(lock, { recursive: true, force: true });
  }
}

function stablePointer(state) {
  const path = resolve(state, "stable.json");
  if (!existsSync(path)) return null;
  const pointer = assertObject(readJson(path, "稳定指针"), "稳定指针");
  if (pointer.version !== 1 || !Number.isSafeInteger(pointer.generation) || pointer.generation < 1) fail("稳定指针版本或 generation 无效");
  assertDigest(pointer.promotionId, "稳定指针 promotionId");
  assertString(pointer.action, "稳定指针 action", /^(?:promote|rollback)$/);
  assertString(pointer.candidateId, "稳定指针 candidateId", /^[a-z0-9][a-z0-9._-]{0,127}$/);
  normalizeTree(pointer.candidateTree, "稳定指针 candidateTree");
  assertDigest(pointer.validationId, "稳定指针 validationId");
  assertDigest(pointer.validationFileSha256, "稳定指针 validationFileSha256");
  assertDigest(pointer.candidateSnapshotSha256, "稳定指针 candidateSnapshotSha256");
  assertTimestamp(pointer.updatedAt, "稳定指针 updatedAt");
  const history = assertObject(readJson(resolve(state, "history", `${pointer.promotionId}.json`), "稳定指针历史"), "稳定指针历史");
  const { previousStable: _previousStable, ...recordPointer } = history;
  if (JSON.stringify(recordPointer) !== JSON.stringify(pointer)) fail("稳定指针与历史记录不一致");
  return pointer;
}

function archiveValidation(state, checked) {
  const path = resolve(state, "candidates", `${checked.validation.validationId}.json`);
  const snapshot = {
    version: 1,
    kind: "novel-codex-validated-candidate",
    validationId: checked.validation.validationId,
    validationFileSha256: checked.sha256,
    candidate: checked.candidate,
  };
  if (existsSync(path)) {
    if (JSON.stringify(readJson(path, "候选归档")) !== JSON.stringify(snapshot)) fail("候选归档冲突");
  } else {
    durableAtomicJson(path, snapshot, { replace: false });
  }
  return { snapshot, sha256: fileDigest(path) };
}

function commitStable(state, { action, candidate, candidateSnapshotSha256, validationId, validationFileSha256, now, previous }) {
  const generation = (previous?.generation ?? 0) + 1;
  const promotionId = sha256(JSON.stringify({ action, candidateId: candidate.candidateId, generation, now, previousPromotionId: previous?.promotionId ?? null, validationId }));
  const next = {
    version: 1,
    generation,
    promotionId,
    action,
    candidateId: candidate.candidateId,
    candidateTree: candidate.candidateTree,
    validationId,
    validationFileSha256,
    candidateSnapshotSha256,
    updatedAt: now,
  };
  const record = { ...next, previousStable: previous };
  durableAtomicJson(resolve(state, "history", `${promotionId}.json`), record, { replace: false });
  durableAtomicJson(resolve(state, "stable.json"), next, { replace: true });
  return Object.freeze(next);
}

export function promoteCandidate({ validationPath, stateDirectory, now = new Date().toISOString() }) {
  assertTimestamp(now, "promotedAt");
  return withReleaseLock(stateDirectory, (state) => {
    const checked = readValidation(validationPath);
    const previous = stablePointer(state);
    if (previous?.validationId === checked.validation.validationId) fail("该候选已经是稳定版本");
    const archived = archiveValidation(state, checked);
    return commitStable(state, {
      action: "promote",
      candidate: checked.candidate,
      candidateSnapshotSha256: archived.sha256,
      validationId: checked.validation.validationId,
      validationFileSha256: checked.sha256,
      now,
      previous,
    });
  });
}

export function rollbackStable({ stateDirectory, now = new Date().toISOString() }) {
  assertTimestamp(now, "rolledBackAt");
  return withReleaseLock(stateDirectory, (state) => {
    const current = stablePointer(state);
    if (!current) fail("当前没有稳定版本可回退");
    const currentRecord = readJson(resolve(state, "history", `${current.promotionId}.json`), "当前晋升记录");
    const target = currentRecord.previousStable;
    if (!target) fail("当前稳定版本没有可回退的前序版本");
    const snapshotPath = resolve(state, "candidates", `${target.validationId}.json`);
    if (fileDigest(snapshotPath) !== target.candidateSnapshotSha256) fail("回退候选归档摘要不匹配");
    const snapshot = readJson(snapshotPath, "回退候选归档");
    if (snapshot.validationId !== target.validationId || snapshot.candidate?.candidateId !== target.candidateId) fail("回退候选归档身份不匹配");
    return commitStable(state, {
      action: "rollback",
      candidate: snapshot.candidate,
      candidateSnapshotSha256: target.candidateSnapshotSha256,
      validationId: snapshot.validationId,
      validationFileSha256: snapshot.validationFileSha256,
      now,
      previous: current,
    });
  });
}

function options(argv) {
  return Object.fromEntries(argv.map((argument) => {
    if (!argument.startsWith("--") || !argument.includes("=")) fail(`参数格式无效：${argument}`);
    const [name, ...parts] = argument.slice(2).split("=");
    return [name, parts.join("=")];
  }));
}

export function runCli(argv = process.argv.slice(2)) {
  const [command, ...rest] = argv;
  const args = options(rest);
  if (command === "validate") {
    if (!args.manifest || !args.output) fail("validate 需要 --manifest 和 --output");
    return writeCandidateValidation({ manifestPath: args.manifest, outputPath: args.output });
  }
  if (command === "promote") {
    if (!args.validation || !args["state-dir"]) fail("promote 需要 --validation 和 --state-dir");
    return promoteCandidate({ validationPath: args.validation, stateDirectory: args["state-dir"] });
  }
  if (command === "rollback") {
    if (!args["state-dir"]) fail("rollback 需要 --state-dir");
    return rollbackStable({ stateDirectory: args["state-dir"] });
  }
  fail("命令必须是 validate、promote 或 rollback");
}

if (process.argv[1] && resolve(process.argv[1]) === scriptPath) {
  try {
    process.stdout.write(`${JSON.stringify(runCli(), null, 2)}\n`);
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exitCode = 1;
  }
}
