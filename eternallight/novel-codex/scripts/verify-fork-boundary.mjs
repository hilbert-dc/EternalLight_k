import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const projectRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const repositoryRoot = resolve(projectRoot, "../..");
const allowedCommittedPaths = ["eternallight/", ".github/workflows/novel-codex-"];
const patchRegistryPath = resolve(projectRoot, "codex-runtime/upstream-patches.json");

export function parsePatchRegistry(document) {
  if (document?.version !== 1 || !Array.isArray(document.patches)) throw new Error("上游补丁登记表缺失或版本无效");
  const ids = new Set();
  const paths = new Set();
  for (const patch of document.patches) {
    if (typeof patch?.id !== "string" || !patch.id || ids.has(patch.id)) throw new Error("上游补丁 ID 缺失或重复");
    ids.add(patch.id);
    if (patch.status !== "active" || !/^[a-f0-9]{40}$/.test(patch.upstreamBaseline ?? "") || !/^[a-f0-9]{64}$/.test(patch.expectedDiffSha256 ?? "")) throw new Error(`上游补丁登记无效：${patch.id}`);
    if (!Array.isArray(patch.paths) || patch.paths.length === 0 || !Array.isArray(patch.symbols) || patch.symbols.length === 0 || !Array.isArray(patch.tests) || patch.tests.length === 0) throw new Error(`上游补丁登记缺少路径、符号或测试：${patch.id}`);
    for (const path of patch.paths) {
      if (typeof path !== "string" || !path.startsWith("codex-rs/") || paths.has(path)) throw new Error(`上游补丁路径无效或重复：${path}`);
      paths.add(path);
    }
  }
  return Object.freeze({ patches: Object.freeze(document.patches.map((patch) => Object.freeze({ ...patch }))), paths });
}

function patchRegistry() {
  return parsePatchRegistry(JSON.parse(readFileSync(patchRegistryPath, "utf8")));
}

function git(args, options = {}) {
  return execFileSync("git", args, { cwd: repositoryRoot, encoding: "utf8", ...options }).trim();
}

export function normalizeRemoteUrl(url) {
  return String(url ?? "").trim().replace(/\\/g, "/").replace(/\.git$/, "").replace(/\/$/, "").toLowerCase();
}

export function isOfficialCodexRemote(url) {
  const normalized = normalizeRemoteUrl(url);
  return normalized === "https://github.com/openai/codex"
    || normalized === "http://github.com/openai/codex"
    || normalized === "git@github.com:openai/codex"
    || normalized === "ssh://git@github.com/openai/codex";
}

export function assessRemoteRoles(remotes) {
  const origin = remotes.origin ?? null;
  const upstream = remotes.upstream ?? null;
  const sourceRemote = isOfficialCodexRemote(upstream) ? "upstream" : isOfficialCodexRemote(origin) ? "origin" : null;
  const strictReady = Boolean(origin && upstream && !isOfficialCodexRemote(origin) && isOfficialCodexRemote(upstream) && normalizeRemoteUrl(origin) !== normalizeRemoteUrl(upstream));
  return {
    origin,
    upstream,
    sourceRemote,
    strictReady,
    warning: strictReady ? null : "Fork 尚未完成远程角色配置：origin 应指向产品 Fork，upstream 应指向 https://github.com/openai/codex.git",
  };
}

export function disallowedProductChanges(paths, registeredPaths = patchRegistry().paths) {
  return paths.filter((path) => path && !registeredPaths.has(path) && !allowedCommittedPaths.some((prefix) => path.startsWith(prefix)));
}

function remoteMap() {
  const names = git(["remote"]).split("\n").filter(Boolean);
  return Object.fromEntries(names.map((name) => [name, git(["remote", "get-url", name])]));
}

function changedPaths(ref) {
  git(["rev-parse", "--verify", ref]);
  return git(["diff", "--name-only", `${ref}...HEAD`]).split("\n").filter(Boolean);
}

function verifyRegisteredPatches(ref, registry) {
  for (const patch of registry.patches) {
    git(["merge-base", "--is-ancestor", patch.upstreamBaseline, ref]);
    const diff = git(["diff", "--binary", ref, "--", ...patch.paths]);
    const diffDigest = createHash("sha256").update(diff.endsWith("\n") ? diff : `${diff}\n`, "utf8").digest("hex");
    if (diffDigest !== patch.expectedDiffSha256) throw new Error(`上游补丁差异未登记或已漂移：${patch.id}`);
    for (const marker of [...patch.symbols, ...patch.tests]) {
      if (!diff.includes(marker)) throw new Error(`上游补丁缺少登记符号或测试：${patch.id}/${marker}`);
    }
  }
}

function workingTreePaths() {
  const tracked = git(["diff", "--name-only", "HEAD"]).split("\n").filter(Boolean);
  const untracked = git(["ls-files", "--others", "--exclude-standard"]).split("\n").filter(Boolean);
  return [...new Set([...tracked, ...untracked])];
}

export function verifyForkBoundary({ strict = false } = {}) {
  if (!existsSync(resolve(repositoryRoot, ".git")) || !existsSync(resolve(repositoryRoot, "codex-rs"))) throw new Error("Novel Codex 必须位于完整 Codex Fork 仓库中");
  const roles = assessRemoteRoles(remoteMap());
  if (!roles.sourceRemote) throw new Error("未找到指向 OpenAI Codex 的上游远程仓库");
  const sourceRef = `${roles.sourceRemote}/main`;
  const registry = patchRegistry();
  const committedChanges = changedPaths(sourceRef);
  const workingChanges = workingTreePaths();
  const violations = disallowedProductChanges([...committedChanges, ...workingChanges], registry.paths);
  if (violations.length > 0) throw new Error(`产品分支修改了未批准的上游路径：\n${violations.join("\n")}`);
  verifyRegisteredPatches(sourceRef, registry);
  if (strict && !roles.strictReady) throw new Error(roles.warning);
  return {
    repositoryRoot,
    projectRoot,
    sourceRef,
    committedChanges: committedChanges.length,
    workingChanges: workingChanges.length,
    strictReady: roles.strictReady,
    warning: roles.warning,
    registeredPatches: registry.patches.map((patch) => patch.id),
  };
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const result = verifyForkBoundary({ strict: process.argv.includes("--strict") });
    process.stdout.write(`${JSON.stringify(result, null, 2)}\n`);
    if (result.warning) process.stderr.write(`WARN: ${result.warning}\n`);
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exitCode = 1;
  }
}
