import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import {
  promoteCandidate,
  rollbackStable,
  validateCandidateManifest,
  writeCandidateValidation,
} from "../upstream-candidate-release.mjs";

function digest(value) {
  return createHash("sha256").update(value).digest("hex");
}

function writeJson(path, value) {
  const text = `${JSON.stringify(value, null, 2)}\n`;
  writeFileSync(path, text);
  return digest(text);
}

function buildIdentity(platform, marker) {
  const body = {
    version: 1,
    product: "codex-cli",
    sourceTreeDigest: marker.repeat(64),
    cargoLockSha256: marker.repeat(64),
    rustc: `rustc fixture\nhost: ${platform}-fixture`,
    buildCommand: "fixture-build",
    artifactPath: `codex-${platform}`,
    target: { platform, architecture: "fixture", rustcHost: `${platform}-fixture` },
    binary: { path: `codex-${platform}`, sha256: marker.repeat(64), version: "codex-cli 0.0.0" },
  };
  return { identityId: digest(JSON.stringify(body)), ...body };
}

function createCandidate(root, id, marker = "a") {
  const directory = join(root, id);
  mkdirSync(directory, { recursive: true });
  const candidateTree = { commitSha: marker.repeat(40), treeSha: marker.repeat(40) };
  const builds = {};
  const platformReports = {};
  for (const [platform, nodePlatform] of [["macos", "darwin"], ["windows", "win32"]]) {
    const identity = buildIdentity(nodePlatform, marker);
    const identityName = `${platform}-build.json`;
    const identityDigest = writeJson(join(directory, identityName), identity);
    const report = {
      version: 1,
      kind: "novel-codex-platform-validation",
      status: "passed",
      candidateId: id,
      candidateTree,
      platform,
      buildIdentity: { identityId: identity.identityId, sha256: identityDigest },
      completedAt: "2026-09-20T10:00:00.000Z",
      checks: ["installation", "execution", "cancellation", "recovery", "upgrade"].map((checkId) => ({ id: checkId, status: "passed" })),
    };
    const reportName = `${platform}-report.json`;
    const reportDigest = writeJson(join(directory, reportName), report);
    builds[platform] = { path: identityName, sha256: identityDigest };
    platformReports[platform] = { path: reportName, sha256: reportDigest };
  }
  const manifestPath = join(directory, "candidate.json");
  writeJson(manifestPath, {
    version: 1,
    kind: "novel-codex-upstream-candidate",
    candidateId: id,
    createdAt: "2026-09-20T10:00:00.000Z",
    candidateTree,
    builds,
    platformReports,
  });
  return { directory, manifestPath };
}

test("validates reports bound to one candidate tree and both platform BuildIdentities", () => {
  const root = mkdtempSync(join(tmpdir(), "candidate-release-"));
  try {
    const candidate = createCandidate(root, "candidate-a");
    const result = validateCandidateManifest(candidate.manifestPath);
    assert.equal(result.candidateId, "candidate-a");
    assert.deepEqual(result.platforms.map(({ platform }) => platform), ["macos", "windows"]);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("rejects missing Windows evidence and mixed candidate trees", () => {
  const root = mkdtempSync(join(tmpdir(), "candidate-release-"));
  try {
    const missing = createCandidate(root, "candidate-missing");
    const manifest = JSON.parse(readFileSync(missing.manifestPath, "utf8"));
    delete manifest.platformReports.windows;
    writeJson(missing.manifestPath, manifest);
    assert.throws(() => validateCandidateManifest(missing.manifestPath), /platformReports\.windows/);

    const mixed = createCandidate(root, "candidate-mixed");
    const reportPath = join(mixed.directory, "windows-report.json");
    const report = JSON.parse(readFileSync(reportPath, "utf8"));
    report.candidateTree.treeSha = "b".repeat(40);
    const reportDigest = writeJson(reportPath, report);
    const mixedManifest = JSON.parse(readFileSync(mixed.manifestPath, "utf8"));
    mixedManifest.platformReports.windows.sha256 = reportDigest;
    writeJson(mixed.manifestPath, mixedManifest);
    assert.throws(() => validateCandidateManifest(mixed.manifestPath), /未绑定当前 candidate tree/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("promotes atomically, refuses changed inputs, and preserves the stable pointer", () => {
  const root = mkdtempSync(join(tmpdir(), "candidate-release-"));
  try {
    const stateDirectory = join(root, "state");
    const candidateA = createCandidate(root, "candidate-a", "a");
    const validationA = join(root, "validation-a.json");
    writeCandidateValidation({ manifestPath: candidateA.manifestPath, outputPath: validationA, now: "2026-09-20T11:00:00.000Z" });
    const promotedA = promoteCandidate({ validationPath: validationA, stateDirectory, now: "2026-09-20T11:01:00.000Z" });
    assert.equal(promotedA.candidateId, "candidate-a");
    const stablePath = join(stateDirectory, "stable.json");
    const beforeFailure = readFileSync(stablePath, "utf8");

    const manifest = JSON.parse(readFileSync(candidateA.manifestPath, "utf8"));
    manifest.candidateTree.treeSha = "c".repeat(40);
    writeJson(candidateA.manifestPath, manifest);
    assert.throws(() => promoteCandidate({ validationPath: validationA, stateDirectory }), /candidate tree|验证后发生变化/);
    assert.equal(readFileSync(stablePath, "utf8"), beforeFailure);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("rolls back through an archived validated candidate with a new atomic generation", () => {
  const root = mkdtempSync(join(tmpdir(), "candidate-release-"));
  try {
    const stateDirectory = join(root, "state");
    const candidateA = createCandidate(root, "candidate-a", "a");
    const candidateB = createCandidate(root, "candidate-b", "b");
    const validationA = join(root, "validation-a.json");
    const validationB = join(root, "validation-b.json");
    writeCandidateValidation({ manifestPath: candidateA.manifestPath, outputPath: validationA, now: "2026-09-20T11:00:00.000Z" });
    writeCandidateValidation({ manifestPath: candidateB.manifestPath, outputPath: validationB, now: "2026-09-20T12:00:00.000Z" });
    promoteCandidate({ validationPath: validationA, stateDirectory, now: "2026-09-20T11:01:00.000Z" });
    const promotedB = promoteCandidate({ validationPath: validationB, stateDirectory, now: "2026-09-20T12:01:00.000Z" });
    const rolledBack = rollbackStable({ stateDirectory, now: "2026-09-20T12:02:00.000Z" });
    assert.equal(promotedB.generation, 2);
    assert.equal(rolledBack.generation, 3);
    assert.equal(rolledBack.action, "rollback");
    assert.equal(rolledBack.candidateId, "candidate-a");
    assert.deepEqual(JSON.parse(readFileSync(join(stateDirectory, "stable.json"), "utf8")), rolledBack);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("refuses a corrupted rollback archive without replacing the stable pointer", () => {
  const root = mkdtempSync(join(tmpdir(), "candidate-release-"));
  try {
    const stateDirectory = join(root, "state");
    const candidateA = createCandidate(root, "candidate-a", "a");
    const candidateB = createCandidate(root, "candidate-b", "b");
    const validationA = join(root, "validation-a.json");
    const validationB = join(root, "validation-b.json");
    const resultA = writeCandidateValidation({ manifestPath: candidateA.manifestPath, outputPath: validationA, now: "2026-09-20T11:00:00.000Z" });
    writeCandidateValidation({ manifestPath: candidateB.manifestPath, outputPath: validationB, now: "2026-09-20T12:00:00.000Z" });
    promoteCandidate({ validationPath: validationA, stateDirectory, now: "2026-09-20T11:01:00.000Z" });
    promoteCandidate({ validationPath: validationB, stateDirectory, now: "2026-09-20T12:01:00.000Z" });
    const stablePath = join(stateDirectory, "stable.json");
    const beforeFailure = readFileSync(stablePath, "utf8");
    writeFileSync(join(stateDirectory, "candidates", `${resultA.validationId}.json`), "{}\n");
    assert.throws(() => rollbackStable({ stateDirectory }), /归档摘要不匹配/);
    assert.equal(readFileSync(stablePath, "utf8"), beforeFailure);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
