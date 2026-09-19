import { execFileSync, spawnSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { isOfficialCodexRemote } from "./verify-fork-boundary.mjs";

const projectRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const repositoryRoot = resolve(projectRoot, "../..");

function git(args, options = {}) {
  return execFileSync("git", args, { cwd: repositoryRoot, encoding: "utf8", ...options }).trim();
}

function runGit(args) {
  const result = spawnSync("git", args, { cwd: repositoryRoot, stdio: "inherit" });
  if (result.status !== 0) throw new Error(`git ${args.join(" ")} 未完成；如有冲突，请在当前 codex-sync 分支人工处理`);
}

function option(name, fallback) {
  const prefix = `--${name}=`;
  return process.argv.find((argument) => argument.startsWith(prefix))?.slice(prefix.length) || fallback;
}

function validBranchName(value) {
  return /^codex-sync\/[0-9]{4}-[0-9]{2}-[0-9]{2}(?:-[a-z0-9._-]+)?$/i.test(value);
}

export function syncUpstream() {
  const remote = option("remote", "upstream");
  const ref = option("ref", "main");
  const date = new Date().toISOString().slice(0, 10);
  const branch = option("branch", `codex-sync/${date}`);
  if (!/^[a-z0-9._-]+$/i.test(remote) || !/^[a-z0-9._/-]+$/i.test(ref) || !validBranchName(branch)) throw new Error("同步参数无效");
  if (git(["status", "--porcelain"])) throw new Error("工作树不干净；请先提交或暂存当前工作，禁止同步脚本覆盖本地修改");
  const remoteUrl = git(["remote", "get-url", remote]);
  if (!isOfficialCodexRemote(remoteUrl)) throw new Error(`${remote} 必须指向 OpenAI Codex 官方仓库`);
  runGit(["fetch", remote, ref]);
  runGit(["switch", "-c", branch]);
  runGit(["merge", "--no-ff", "--no-edit", `${remote}/${ref}`]);
  process.stdout.write(`上游已合入 ${branch}。请在合入产品主分支前运行：npm run verify\n`);
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    syncUpstream();
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exitCode = 1;
  }
}
