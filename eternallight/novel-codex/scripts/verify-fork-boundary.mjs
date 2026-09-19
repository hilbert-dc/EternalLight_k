import { execFileSync } from "node:child_process";
import { existsSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const projectRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const repositoryRoot = resolve(projectRoot, "../..");
const allowedCommittedPaths = ["eternallight/", ".github/workflows/novel-codex-"];

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

export function disallowedProductChanges(paths) {
  return paths.filter((path) => path && !allowedCommittedPaths.some((prefix) => path.startsWith(prefix)));
}

function remoteMap() {
  const names = git(["remote"]).split("\n").filter(Boolean);
  return Object.fromEntries(names.map((name) => [name, git(["remote", "get-url", name])]));
}

function changedPaths(ref) {
  try {
    git(["rev-parse", "--verify", ref]);
  } catch {
    return [];
  }
  return git(["diff", "--name-only", `${ref}...HEAD`]).split("\n").filter(Boolean);
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
  const committedChanges = changedPaths(sourceRef);
  const workingChanges = workingTreePaths();
  const violations = disallowedProductChanges([...committedChanges, ...workingChanges]);
  if (violations.length > 0) throw new Error(`产品分支修改了未批准的上游路径：\n${violations.join("\n")}`);
  if (strict && !roles.strictReady) throw new Error(roles.warning);
  return {
    repositoryRoot,
    projectRoot,
    sourceRef,
    committedChanges: committedChanges.length,
    workingChanges: workingChanges.length,
    strictReady: roles.strictReady,
    warning: roles.warning,
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
