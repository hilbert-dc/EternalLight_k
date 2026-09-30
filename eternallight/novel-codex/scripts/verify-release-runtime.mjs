import { spawn, spawnSync } from "node:child_process";
import { createHash, randomBytes } from "node:crypto";
import { existsSync, mkdtempSync, readFileSync, readdirSync, rmSync } from "node:fs";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { createInterface } from "node:readline";
import { fileURLToPath } from "node:url";
import { readBuildIdentity } from "./build-provenance.mjs";
import { resolveBuildLayout } from "./build-layout.mjs";

const scriptPath = fileURLToPath(import.meta.url);
const root = resolve(fileURLToPath(new URL("..", import.meta.url)));
const defaultLayout = resolveBuildLayout();

function sha256(path) {
  return createHash("sha256").update(readFileSync(path)).digest("hex");
}

function assertEqual(actual, expected, label) {
  if (actual !== expected) throw new Error(`${label} 摘要或版本不匹配`);
}

function verifyDeclaredFiles(resources, files) {
  if (!files || typeof files !== "object" || Array.isArray(files) || Object.keys(files).length === 0) throw new Error("发布清单没有逐文件资源摘要");
  const actual = [];
  const visit = (directory) => {
    for (const entry of readdirSync(directory, { withFileTypes: true })) {
      const path = join(directory, entry.name);
      if (entry.isDirectory()) visit(path);
      else if (entry.isFile() && entry.name !== "release-manifest.json") actual.push(path.slice(resources.length + 1).replaceAll("\\", "/"));
    }
  };
  visit(resources);
  const declared = Object.keys(files).sort();
  actual.sort();
  if (JSON.stringify(actual) !== JSON.stringify(declared)) throw new Error("发布资源集合与清单不一致");
  for (const relativePath of declared) {
    const path = join(resources, relativePath);
    if (!existsSync(path)) throw new Error(`发布资源缺失：${relativePath}`);
    assertEqual(sha256(path), files[relativePath], `发布资源 ${relativePath}`);
  }
}

async function availablePort() {
  const server = createServer();
  await new Promise((resolveListen, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolveListen);
  });
  const port = server.address().port;
  await new Promise((resolveClose) => server.close(resolveClose));
  return port;
}

async function waitForExit(child, timeoutMs = 5_000) {
  if (child.exitCode !== null || child.signalCode !== null) return;
  await Promise.race([
    new Promise((resolveExit) => child.once("exit", resolveExit)),
    new Promise((resolveTimeout) => setTimeout(resolveTimeout, timeoutMs)),
  ]);
}

async function verifyMcp(path, timeoutMs) {
  const child = spawn(path, [], {
    env: {
      PATH: process.env.PATH,
      NOVEL_CODEX_CANDIDATE_TOKEN: randomBytes(24).toString("base64url"),
      NOVEL_CODEX_OPERATION: "advisory",
      NOVEL_CODEX_RUNTIME_URL: "http://127.0.0.1:9",
      NOVEL_CODEX_TASK_CAPABILITY: randomBytes(24).toString("base64url"),
    },
    stdio: ["pipe", "pipe", "pipe"],
  });
  let stderr = "";
  child.stderr.on("data", (chunk) => { stderr = `${stderr}${chunk}`.slice(-2_000); });
  try {
    const response = new Promise((resolveResponse, reject) => {
      const timer = setTimeout(() => reject(new Error(`MCP 初始化超时：${stderr}`)), timeoutMs);
      createInterface({ input: child.stdout }).once("line", (line) => {
        clearTimeout(timer);
        try { resolveResponse(JSON.parse(line)); } catch (error) { reject(error); }
      });
      child.once("exit", (code) => {
        if (code !== null && code !== 0) reject(new Error(`MCP 在初始化前退出 (${code})：${stderr}`));
      });
    });
    child.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", id: 1, method: "initialize", params: { protocolVersion: "2025-06-18", capabilities: {}, clientInfo: { name: "release-smoke", version: "1" } } })}\n`);
    const initialized = await response;
    if (initialized.id !== 1 || initialized.result?.serverInfo?.name !== "novel-codex-workspace") throw new Error("MCP 初始化响应无效");
  } finally {
    child.kill();
    await waitForExit(child);
  }
}

async function verifyRuntime(path, timeoutMs) {
  const temporary = mkdtempSync(join(tmpdir(), "novel-codex-release-smoke-"));
  const port = await availablePort();
  const child = spawn(path, ["--host", "127.0.0.1", "--port", String(port), "--db", join(temporary, "runtime.sqlite3"), "--engine-budget-db", join(temporary, "engine-budget.sqlite3"), "--token-stdin"], {
    env: { PATH: process.env.PATH, TEMP: process.env.TEMP, TMP: process.env.TMP, TMPDIR: process.env.TMPDIR },
    stdio: ["pipe", "pipe", "pipe"],
  });
  let stderr = "";
  child.stderr.on("data", (chunk) => { stderr = `${stderr}${chunk}`.slice(-2_000); });
  child.stdin.write(`${JSON.stringify({ candidateToken: randomBytes(24).toString("base64url"), authorToken: randomBytes(24).toString("base64url") })}\n`);
  const deadline = Date.now() + timeoutMs;
  try {
    while (Date.now() < deadline) {
      if (child.exitCode !== null) throw new Error(`Creative Runtime 提前退出 (${child.exitCode})：${stderr}`);
      try {
        const response = await fetch(`http://127.0.0.1:${port}/health`, { signal: AbortSignal.timeout(1_000) });
        const body = await response.json();
        if (response.ok && body?.ok === true && body?.service === "creative-runtime") return;
      } catch {
        await new Promise((resolveWait) => setTimeout(resolveWait, 100));
      }
    }
    throw new Error(`Creative Runtime 健康检查超时：${stderr}`);
  } finally {
    child.kill();
    await waitForExit(child);
    rmSync(temporary, { recursive: true, force: true });
  }
}

export async function verifyReleaseRuntime({ resources = defaultLayout.resources, platform = defaultLayout.platform, timeoutMs = 30_000 } = {}) {
  const resolvedResources = resolve(resources);
  const executableSuffix = platform === "win32" ? ".exe" : "";
  const cli = join(resolvedResources, "codex-runtime", "bin", `codex${executableSuffix}`);
  const runtime = join(resolvedResources, "runtime", "bin", `creative-runtime${executableSuffix}`);
  const mcp = join(resolvedResources, "runtime", "bin", `novel-workspace-mcp${executableSuffix}`);
  const manifest = JSON.parse(readFileSync(join(resolvedResources, "release-manifest.json"), "utf8"));
  verifyDeclaredFiles(resolvedResources, manifest.resources.files);
  const buildIdentityPath = join(resolvedResources, "codex-runtime", "build-identity.json");
  const buildIdentity = readBuildIdentity(buildIdentityPath);
  assertEqual(buildIdentity.identityId, manifest.codex.buildIdentityId, "Codex BuildIdentity");
  assertEqual(sha256(buildIdentityPath), manifest.codex.buildIdentitySha256, "Codex BuildIdentity 文件");
  assertEqual(buildIdentity.binary.sha256, manifest.codex.unsignedBinarySha256, "Codex 未签名构建来源");
  assertEqual(sha256(cli), manifest.codex.binarySha256, "Codex CLI");
  assertEqual(sha256(runtime), manifest.resources.creativeRuntimeSha256, "Creative Runtime");
  assertEqual(sha256(mcp), manifest.resources.novelWorkspaceMcpSha256, "Novel MCP");
  assertEqual(sha256(join(resolvedResources, "codex-runtime", "model-catalog.json")), manifest.resources.modelCatalogSha256, "模型目录");
  assertEqual(sha256(join(resolvedResources, "codex-runtime", "model-certifications.json")), manifest.resources.modelCertificationsSha256, "模型认证");
  assertEqual(sha256(join(resolvedResources, "codex-runtime", "engine-prompt.md")), manifest.resources.engineInstructionsSha256, "Codex 指令");
  const version = spawnSync(cli, ["--version"], { encoding: "utf8", timeout: timeoutMs });
  if (version.status !== 0) throw new Error(`Codex CLI 启动失败：${version.stderr}`);
  assertEqual(version.stdout.trim(), manifest.codex.binaryVersion, "Codex CLI 版本");
  await verifyMcp(mcp, timeoutMs);
  await verifyRuntime(runtime, timeoutMs);
  return { cli: "verified", mcp: "verified", runtime: "verified" };
}

if (process.argv[1] && resolve(process.argv[1]) === scriptPath) {
  const result = await verifyReleaseRuntime({ resources: process.env.NOVEL_CODEX_RELEASE_RESOURCES });
  process.stdout.write(`${JSON.stringify(result)}\n`);
}
