import { execFileSync, spawnSync } from "node:child_process";
import { existsSync, mkdirSync, writeFileSync } from "node:fs";
import { dirname, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { isOfficialCodexRemote } from "./verify-fork-boundary.mjs";

const projectRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const repositoryRoot = resolve(projectRoot, "../..");

function git(args, { cwd = repositoryRoot } = {}) {
  return execFileSync("git", args, { cwd, encoding: "utf8" }).trim();
}

function runGit(args, cwd = repositoryRoot) {
  return spawnSync("git", args, { cwd, stdio: "inherit" });
}

function option(name, fallback) {
  const prefix = `--${name}=`;
  return process.argv.find((argument) => argument.startsWith(prefix))?.slice(prefix.length) || fallback;
}

function safeName(value, label, pattern) {
  if (!pattern.test(value)) throw new Error(`${label} 参数无效`);
  return value;
}

function ensureInsideRepository(path) {
  const candidate = resolve(path);
  const relation = relative(repositoryRoot, candidate);
  if (!relation || relation.startsWith("..") || relation.includes("../")) throw new Error("候选 worktree 必须位于仓库内的专用目录");
  return candidate;
}

export function synchronizationIdentity({ baseSha, targetSha, date = new Date().toISOString().slice(0, 10) }) {
  if (!/^[a-f0-9]{40}$/.test(baseSha) || !/^[a-f0-9]{40}$/.test(targetSha) || !/^\d{4}-\d{2}-\d{2}$/.test(date)) throw new Error("同步身份无效");
  return Object.freeze({ baseSha, targetSha, branch: `codex/upstream-sync-${date}-${targetSha.slice(0, 12)}` });
}

function impactAreas(paths) {
  const rules = {
    appServer: ["codex-rs/app-server", "codex-rs/app-server-protocol"],
    modelProtocol: ["codex-rs/core/src/client", "codex-rs/core/src/model_provider"],
    toolsAndMcp: ["codex-rs/core/src/mcp", "codex-rs/mcp"],
    agentLifecycle: ["codex-rs/core/src/agent", "codex-rs/core/src/tools/handlers/multi_agents"],
    contextAndCompaction: ["codex-rs/core/src/context", "codex-rs/core/src/compact"],
    configuration: ["codex-rs/core/src/config", "codex-rs/core/config.schema.json"],
    buildAndDependencies: ["Cargo.lock", "MODULE.bazel.lock", "Cargo.toml"],
  };
  return Object.fromEntries(Object.entries(rules).map(([area, prefixes]) => [area, paths.filter((path) => prefixes.some((prefix) => path.startsWith(prefix)))]));
}

export function syncUpstream() {
  const remote = safeName(option("remote", "upstream"), "remote", /^[a-z0-9._-]+$/i);
  const ref = safeName(option("ref", "main"), "ref", /^[a-z0-9._/-]+$/i);
  const remoteUrl = git(["remote", "get-url", remote]);
  if (!isOfficialCodexRemote(remoteUrl)) throw new Error(`${remote} 必须指向 OpenAI Codex 官方仓库`);
  if (existsSync(resolve(repositoryRoot, ".git/MERGE_HEAD")) || existsSync(resolve(repositoryRoot, ".git/rebase-merge")) || existsSync(resolve(repositoryRoot, ".git/rebase-apply"))) throw new Error("当前仓库存在未完成的 merge 或 rebase");

  const baseSha = git(["rev-parse", "HEAD"]);
  const workingChanges = git(["status", "--porcelain"]).split("\n").filter(Boolean);
  const fetch = runGit(["fetch", remote, ref]);
  if (fetch.status !== 0) throw new Error(`无法 fetch ${remote}/${ref}`);
  const targetSha = git(["rev-parse", `${remote}/${ref}^{commit}`]);
  const identity = synchronizationIdentity({ baseSha, targetSha });
  const branch = option("branch", identity.branch);
  if (!/^codex\/upstream-sync-\d{4}-\d{2}-\d{2}-[a-f0-9]{7,40}$/.test(branch)) throw new Error("branch 参数无效");
  const defaultWorktree = resolve(repositoryRoot, `.codex-upstream-worktrees/${branch.replaceAll("/", "-")}`);
  const worktreePath = ensureInsideRepository(option("worktree", defaultWorktree));
  if (existsSync(worktreePath)) throw new Error(`候选 worktree 已存在：${worktreePath}`);

  mkdirSync(dirname(worktreePath), { recursive: true });
  const add = runGit(["worktree", "add", "-b", branch, worktreePath, baseSha]);
  if (add.status !== 0) throw new Error(`无法创建候选分支或 worktree：${branch}`);

  const changedPaths = git(["diff", "--name-only", `${baseSha}..${targetSha}`]).split("\n").filter(Boolean);
  const merge = runGit(["merge", "--no-ff", "--no-edit", targetSha], worktreePath);
  const candidateSha = merge.status === 0 ? git(["rev-parse", "HEAD"], { cwd: worktreePath }) : null;
  const report = {
    version: 1,
    status: merge.status === 0 ? "merged-unverified" : "conflict",
    createdAt: new Date().toISOString(),
    repositoryRoot,
    baseSha,
    upstream: { remote, ref, sha: targetSha },
    candidate: { branch, worktreePath, sha: candidateSha },
    localWorkingTreeExcluded: workingChanges,
    changedPathCount: changedPaths.length,
    impactAreas: impactAreas(changedPaths),
    validation: { status: "not-run", required: ["fork-boundary", "codex-tests", "agent-engine-tests", "novel-codex-tests", "macos-smoke", "windows-smoke"] },
  };
  const reportDirectory = resolve(projectRoot, ".upstream-sync-reports");
  mkdirSync(reportDirectory, { recursive: true });
  const reportPath = resolve(reportDirectory, `${branch.replaceAll("/", "-")}.json`);
  writeFileSync(reportPath, `${JSON.stringify(report, null, 2)}\n`, { flag: "wx", mode: 0o600 });
  if (merge.status !== 0) throw new Error(`固定上游 SHA 合并发生冲突；候选已保留在 ${worktreePath}，报告：${reportPath}`);
  process.stdout.write(`${JSON.stringify({ branch, worktreePath, baseSha, targetSha, candidateSha, reportPath }, null, 2)}\n`);
  return report;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    syncUpstream();
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exitCode = 1;
  }
}
