import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { chmodSync, copyFileSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
import { dirname, join } from "node:path";
import { tmpdir } from "node:os";
import { createInterface } from "node:readline";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import { assertProviderOperation, buildCodexConfig, composeNovelCodexPrompt, createCodexEnvironment, createRuntimeEnvironment, createTaskCapability, loadNovelCodexSkills, prepareIsolatedStorage, providerConfigurationError, resolveCodexCli, resolveCodexProvider } from "../codex-runtime.mjs";

function temporaryDirectory() {
  return mkdtempSync(join(tmpdir(), "novel-codex-runtime-"));
}

const runtimeRoot = fileURLToPath(new URL("../../codex-runtime/", import.meta.url));
const modelCatalogSha256 = createHash("sha256").update(readFileSync(join(runtimeRoot, "model-catalog.json"))).digest("hex");
const codexModelCatalogSha256 = createHash("sha256").update(readFileSync(join(runtimeRoot, "codex-model-catalog.json"))).digest("hex");

function fakeExecutable(path) {
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, "#!/usr/bin/env node\nprocess.stdout.write('codex-cli 0.0.0\\n');\n");
  chmodSync(path, 0o755);
}

function executableChecksum(path) {
  return createHash("sha256").update(readFileSync(path)).digest("hex");
}

function writeCliPolicy(root, digests = {}) {
  const path = join(root, "codex-runtime", "supported-cli.json");
  mkdirSync(dirname(path), { recursive: true });
  copyFileSync(join(runtimeRoot, "model-catalog.json"), join(root, "codex-runtime", "model-catalog.json"));
  copyFileSync(join(runtimeRoot, "codex-model-catalog.json"), join(root, "codex-runtime", "codex-model-catalog.json"));
  const versions = ["codex-cli 0.0.0"];
  writeFileSync(path, JSON.stringify({ version: 1, appServerProtocol: "v2", mcpProtocol: "2025-06-18", modelCatalogSha256, codexModelCatalogSha256, sources: { bundled: { packaged: true, versions, digests: digests.bundled ?? {} }, "project-runtime": { packaged: false, versions, digests: digests["project-runtime"] ?? {} }, "workspace-build": { packaged: false, developmentOnly: true, versions, digests: digests["workspace-build"] ?? {} }, explicit: { packaged: false, developmentOnly: true, versions, digests: digests.explicit ?? {} } } }));
}

test("resolves the packaged CLI instead of PATH", () => {
  const resourcesPath = temporaryDirectory();
  const cliPath = join(resourcesPath, "codex-runtime", "bin", "codex");
  fakeExecutable(cliPath);
  writeCliPolicy(resourcesPath, { bundled: { "darwin-arm64": executableChecksum(cliPath) } });
  try {
    assert.deepEqual(resolveCodexCli({ architecture: "arm64", isPackaged: true, platform: "darwin", projectRoot: "/unused", resourcesPath }), {
      path: realpathSync(cliPath),
      source: "bundled",
      reason: null,
    });
  } finally {
    rmSync(resourcesPath, { recursive: true, force: true });
  }
});

test("rejects a packaged CLI whose checksum is missing or stale", () => {
  const resourcesPath = temporaryDirectory();
  const cliPath = join(resourcesPath, "codex-runtime", "bin", "codex");
  fakeExecutable(cliPath);
  writeCliPolicy(resourcesPath);
  try {
    const missing = resolveCodexCli({ architecture: "arm64", isPackaged: true, platform: "darwin", projectRoot: "/unused", resourcesPath });
    assert.equal(missing.path, null);
    assert.match(missing.reason, /SHA-256/);
    writeCliPolicy(resourcesPath, { bundled: { "darwin-arm64": "0".repeat(64) } });
    const stale = resolveCodexCli({ architecture: "arm64", isPackaged: true, platform: "darwin", projectRoot: "/unused", resourcesPath });
    assert.equal(stale.path, null);
    assert.match(stale.reason, /平台 SHA-256 或版本/);
  } finally {
    rmSync(resourcesPath, { recursive: true, force: true });
  }
});

test("requires a SHA manifest for a project runtime and permits only the development workspace build fallback", () => {
  const root = temporaryDirectory();
  const projectRoot = join(root, "app", "novel");
  const projectCli = join(projectRoot, "codex-runtime", "bin", "codex");
  const workspaceCli = join(projectRoot, "..", "..", "codex-rs", "target", "debug", "codex");
  fakeExecutable(projectCli);
  fakeExecutable(workspaceCli);
  writeCliPolicy(projectRoot, { "workspace-build": { "darwin-arm64": executableChecksum(workspaceCli) } });
  try {
    const fallback = resolveCodexCli({ architecture: "arm64", isPackaged: false, platform: "darwin", projectRoot, resourcesPath: "/unused" });
    assert.equal(fallback.source, "workspace-build");
    assert.equal(fallback.path, realpathSync(workspaceCli));
    writeCliPolicy(projectRoot, { "project-runtime": { "darwin-arm64": executableChecksum(projectCli) }, "workspace-build": { "darwin-arm64": executableChecksum(workspaceCli) } });
    const pinned = resolveCodexCli({ architecture: "arm64", isPackaged: false, platform: "darwin", projectRoot, resourcesPath: "/unused" });
    assert.equal(pinned.source, "project-runtime");
    assert.equal(pinned.path, realpathSync(projectCli));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("rejects an explicit CLI in a packaged build", () => {
  const root = temporaryDirectory();
  writeCliPolicy(root);
  const desktopCli = join(root, "Applications", "ChatGPT.app", "Contents", "Resources", "codex");
  fakeExecutable(desktopCli);
  try {
    const result = resolveCodexCli({ architecture: "arm64", explicitPath: desktopCli, isPackaged: true, platform: "darwin", projectRoot: "/unused", resourcesPath: root });
    assert.equal(result.path, null);
    assert.equal(result.source, "missing");
    assert.match(result.reason, /未获.*授权/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("rejects a digest-pinned CLI whose reported version is not supported", () => {
  const resourcesPath = temporaryDirectory();
  const cliPath = join(resourcesPath, "codex-runtime", "bin", "codex");
  mkdirSync(dirname(cliPath), { recursive: true });
  writeFileSync(cliPath, "#!/usr/bin/env node\nprocess.stdout.write('codex-cli 99.0.0\\n');\n");
  chmodSync(cliPath, 0o755);
  writeCliPolicy(resourcesPath, { bundled: { "darwin-arm64": executableChecksum(cliPath) } });
  try {
    const result = resolveCodexCli({ architecture: "arm64", isPackaged: true, platform: "darwin", projectRoot: "/unused", resourcesPath });
    assert.equal(result.path, null);
    assert.match(result.reason, /平台 SHA-256 或版本/);
  } finally {
    rmSync(resourcesPath, { recursive: true, force: true });
  }
});

test("keeps inherited provider credentials out of the isolated Codex child environment", () => {
  const env = createCodexEnvironment({ CODEX_HOME: "/desktop", CODEX_API_KEY: "implicit-key", CODEX_ACCESS_TOKEN: "secret", OPENAI_API_KEY: "key", PATH: "/usr/bin" }, {
    codexHome: "/novel/codex-home",
    provider: "deepseek",
    sqliteHome: "/novel/codex-home/sqlite",
  });
  assert.equal(env.CODEX_HOME, "/novel/codex-home");
  assert.equal(env.CODEX_SQLITE_HOME, "/novel/codex-home/sqlite");
  assert.equal(env.CODEX_API_KEY, undefined);
  assert.equal(env.CODEX_ACCESS_TOKEN, undefined);
  assert.equal(env.OPENAI_API_KEY, undefined);
  assert.equal(env.PATH, "/usr/bin");
});

test("allows only catalogued provider/model operations", () => {
  const expected = {
    provider: "deepseek",
    model: "deepseek-flash",
    apiKeyEnv: "DEEPSEEK_API_KEY",
    operations: ["advisory", "generateBrief", "generateOutline", "generatePlan", "generateChapter", "reviewPlanningSeat"],
  };
  assert.deepEqual(resolveCodexProvider({}), expected);
  assert.deepEqual(resolveCodexProvider({ model: "deepseek-flash", provider: "deepseek" }), expected);
  for (const operation of ["advisory", "generateBrief", "generateOutline", "generatePlan", "generateChapter", "reviewPlanningSeat"]) {
    assert.equal(assertProviderOperation({ provider: "deepseek", model: "deepseek-flash" }, operation).model, "deepseek-flash");
  }
  assert.throws(() => resolveCodexProvider({ model: "gpt-5.6-terra", provider: "openai" }), /未审核/);
  assert.throws(() => resolveCodexProvider({ model: "gpt-arbitrary", provider: "openai" }), /未审核/);
  assert.throws(() => resolveCodexProvider({ model: "DeepSeek-V4.1-Flash", provider: "deepseek" }), /未审核/);
});

test("creates a short-lived task capability without exposing its hash as the secret", () => {
  const capability = createTaskCapability(1_800_000);
  assert.equal(capability.secret.length >= 24, true);
  assert.equal(capability.hash, createHash("sha256").update(capability.secret).digest("hex"));
  assert.equal(capability.expiresAt, 3_600);
  assert.notEqual(capability.secret, capability.hash);
});

test("starts the current runtime schema in a fresh database", () => {
  const root = temporaryDirectory();
  try {
    const storage = prepareIsolatedStorage(root);
    assert.equal(storage.runtimeDb, join(root, "creative-runtime", "novel-codex-v2.sqlite3"));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("builds a DeepSeek provider without persisting the API key", () => {
  const config = buildCodexConfig({ model: "deepseek-flash", provider: "deepseek", mcpServerCommand: "python3", mcpServerPath: "/runtime/mcp_server.py" });
  assert.match(config, /model_provider = "deepseek"/);
  assert.match(config, /model_catalog_json = /);
  assert.match(config, /base_url = "https:\/\/api\.deepseek\.com"/);
  assert.match(config, /env_key = "DEEPSEEK_API_KEY"/);
  assert.doesNotMatch(config, /sk-/);
  assert.match(config, /\[shell_environment_policy\]/);
  assert.match(config, /inherit = "core"/);
  assert.match(config, /ignore_default_excludes = false/);
  assert.match(config, /"NOVEL_CODEX_\*"/);
  assert.match(config, /"DEEPSEEK_API_KEY"/);
  assert.match(config, /"CODEX_API_KEY"/);
  assert.match(config, /web_search = "disabled"/);
  assert.match(config, /approval_policy = "never"/);
  assert.match(config, /sandbox_mode = "read-only"/);
  for (const feature of ["collab", "multi_agent", "multi_agent_v2", "tool_search", "memory_tool", "connectors", "computer_use", "in_app_browser", "image_generation", "imagegenext", "js_repl"]) {
    assert.match(config, new RegExp(`${feature} = false`));
  }
});

test("places the selected DeepSeek provider at the TOML root", () => {
  const config = buildCodexConfig({ model: "deepseek-flash", provider: "deepseek", mcpServerCommand: "python3", mcpServerPath: "/runtime/mcp_server.py" });
  const python = process.env.NOVEL_CODEX_PYTHON ?? (process.platform === "win32" ? "python" : "python3");
  const parsed = spawnSync(python, ["-c", "import json,sys,tomllib; print(json.dumps(tomllib.loads(sys.stdin.read())))"], {
    encoding: "utf8",
    input: config,
  });
  assert.equal(parsed.status, 0, parsed.stderr);
  const document = JSON.parse(parsed.stdout);
  assert.equal(document.model_provider, "deepseek");
  assert.equal(document.shell_environment_policy.model_provider, undefined);
  assert.equal(document.model_providers.deepseek.name, "DeepSeek");
});

test("builds a cross-platform candidate-only MCP sidecar config", () => {
  const config = buildCodexConfig({
    mcpServerCommand: "C:\\Python313\\python.exe",
    mcpServerPath: "C:\\Novel Codex\\runtime\\mcp_server.py",
    model: "deepseek-flash",
    platform: "win32",
    provider: "deepseek",
  });
  assert.match(config, /\[mcp_servers\.novel_workspace\]/);
  assert.match(config, /command = "C:\\\\Python313\\\\python\.exe"/);
  assert.match(config, /args = \["C:\\\\Novel Codex\\\\runtime\\\\mcp_server\.py"\]/);
  assert.match(config, /env_vars = \["NOVEL_CODEX_RUNTIME_URL", "NOVEL_CODEX_CANDIDATE_TOKEN", "NOVEL_CODEX_TASK_CAPABILITY", "NOVEL_CODEX_OPERATION"\]/);
  assert.doesNotMatch(config, /AUTHOR_TOKEN|candidate-secret|accept|undo/);
});

test("passes only the key for the explicitly selected provider", () => {
  const base = { DEEPSEEK_API_KEY: "deepseek-secret", OPENAI_API_KEY: "openai-secret", PATH: "/usr/bin" };
  const deepSeekEnv = createCodexEnvironment(base, { codexHome: "/novel/home", sqliteHome: "/novel/sqlite", provider: "deepseek" });
  const openAiEnv = createCodexEnvironment(base, { codexHome: "/novel/home", sqliteHome: "/novel/sqlite", provider: "openai" });
  assert.equal(deepSeekEnv.DEEPSEEK_API_KEY, "deepseek-secret");
  assert.equal(openAiEnv.DEEPSEEK_API_KEY, undefined);
  assert.equal(deepSeekEnv.OPENAI_API_KEY, undefined);
  assert.equal(openAiEnv.OPENAI_API_KEY, undefined);
});

test("fresh isolated homes fail closed until DeepSeek has an API key", () => {
  const deepSeek = resolveCodexProvider({ model: "deepseek-flash", provider: "deepseek" });
  assert.equal(providerConfigurationError(deepSeek, {}), "缺少 DEEPSEEK_API_KEY，deepseek 请求将无法认证");
  assert.equal(providerConfigurationError(deepSeek, { DEEPSEEK_API_KEY: "deepseek-test" }), null);
});

test("isolates Runtime and author credentials from the Codex child", () => {
  const base = {
    NOVEL_CODEX_AUTHOR_TOKEN: "author-secret",
    NOVEL_CODEX_CANDIDATE_TOKEN: "inherited-candidate-secret",
    NOVEL_CODEX_RUNTIME_URL: "http://inherited.invalid",
    CODEX_API_KEY: "implicit-codex-key",
    OPENAI_API_KEY: "desktop-key",
    PATH: "/usr/bin",
  };
  const codexEnv = createCodexEnvironment(base, {
    candidateToken: "candidate-secret",
    codexHome: "/novel/home",
    runtimeUrl: "http://127.0.0.1:43123",
    sqliteHome: "/novel/sqlite",
    provider: "deepseek",
  });
  assert.equal(codexEnv.NOVEL_CODEX_RUNTIME_URL, "http://127.0.0.1:43123");
  assert.equal(codexEnv.NOVEL_CODEX_CANDIDATE_TOKEN, "candidate-secret");
  assert.equal(codexEnv.NOVEL_CODEX_AUTHOR_TOKEN, undefined);
  assert.equal(codexEnv.CODEX_API_KEY, undefined);
  assert.equal(codexEnv.OPENAI_API_KEY, undefined);

  const runtimeEnv = createRuntimeEnvironment(base, "/novel/project");
  assert.equal(runtimeEnv.NOVEL_CODEX_RUNTIME_URL, undefined);
  assert.equal(runtimeEnv.NOVEL_CODEX_CANDIDATE_TOKEN, undefined);
  assert.equal(runtimeEnv.NOVEL_CODEX_AUTHOR_TOKEN, undefined);
  assert.equal(runtimeEnv.CODEX_API_KEY, undefined);
  assert.equal(runtimeEnv.OPENAI_API_KEY, undefined);
  assert.equal(runtimeEnv.PYTHONPATH, "/novel/project");
  assert.equal(runtimeEnv.PATH, "/usr/bin");
});

test("MCP sidecar lists only candidate tools and forwards bearer-authenticated calls", async (t) => {
  const requests = [];
  const server = createServer((request, response) => {
    const chunks = [];
    request.on("data", (chunk) => chunks.push(chunk));
    request.on("end", () => {
      const rawBody = Buffer.concat(chunks).toString("utf8");
      requests.push({
        authorization: request.headers.authorization,
        capability: request.headers["x-novel-task-capability"],
        body: rawBody ? JSON.parse(rawBody) : null,
        method: request.method,
        url: request.url,
      });
      response.writeHead(200, { "Content-Type": "application/json" });
      response.end(JSON.stringify({ artifactId: "outline-1", revisionId: "outline-rev-2" }));
    });
  });
  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolve);
  });
  t.after(() => new Promise((resolve) => server.close(resolve)));

  const address = server.address();
  const sidecarPath = realpathSync(join(dirname(fileURLToPath(import.meta.url)), "../../runtime/mcp_server.py"));
  const child = spawn(process.env.NOVEL_CODEX_PYTHON || (process.platform === "win32" ? "python" : "python3"), [sidecarPath], {
    env: {
      ...process.env,
      NOVEL_CODEX_CANDIDATE_TOKEN: "candidate-secret",
      NOVEL_CODEX_TASK_CAPABILITY: "task-capability-secret-1234567890",
      NOVEL_CODEX_OPERATION: "generateOutline",
      NOVEL_CODEX_RUNTIME_URL: `http://127.0.0.1:${address.port}`,
    },
    stdio: ["pipe", "pipe", "pipe"],
  });
  t.after(() => child.kill());
  const lines = createInterface({ input: child.stdout });
  const responses = [];
  lines.on("line", (line) => responses.push(JSON.parse(line)));
  const send = async (message) => {
    child.stdin.write(`${JSON.stringify(message)}\n`);
    const deadline = Date.now() + 5_000;
    while (!responses.some((response) => response.id === message.id)) {
      if (Date.now() > deadline) throw new Error("timed out waiting for MCP response");
      await new Promise((resolve) => setTimeout(resolve, 5));
    }
    return responses.find((response) => response.id === message.id);
  };

  const initialized = await send({ jsonrpc: "2.0", id: 1, method: "initialize", params: { protocolVersion: "2025-06-18", capabilities: {}, clientInfo: { name: "test", version: "1" } } });
  assert.equal(initialized.result.serverInfo.name, "novel-codex-workspace");
  child.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", method: "notifications/initialized", params: {} })}\n`);
  const listed = await send({ jsonrpc: "2.0", id: 2, method: "tools/list", params: {} });
  assert.deepEqual(listed.result.tools.map((tool) => tool.name), [
    "read_context",
    "save_outline",
    "complete_task",
  ]);
  assert.ok(listed.result.tools.every((tool) => ["taskId", "projectId", "threadId", "expectedRevision"].every((key) => tool.inputSchema.required.includes(key))));

  const called = await send({
    jsonrpc: "2.0",
    id: 3,
    method: "tools/call",
    params: {
      name: "save_outline",
      arguments: { taskId: "task 1", projectId: "project/1", threadId: "thread-1", expectedRevision: 4, expectedCandidateRevisionId: null, idempotencyKey: "outline-task-1", payload: { nodes: [{ clientKey: "book", kind: "book", position: 0, title: "白塔未亮", purpose: "追查被改写的记录", summary: "林夕留下证词，港城记住名字", dependencyKeys: [], certainty: "tentative", protection: "free" }] } },
    },
  });
  assert.equal(called.result.isError, undefined);
  assert.deepEqual(JSON.parse(called.result.content.map((item) => item.text).join("")), { artifactId: "outline-1", revisionId: "outline-rev-2" });
  await send({
    jsonrpc: "2.0",
    id: 4,
    method: "tools/call",
    params: {
      name: "complete_task",
      arguments: { taskId: "task 1", projectId: "project/1", threadId: "thread-1", expectedRevision: 4, summary: "候选已保存" },
    },
  });
  assert.deepEqual(requests, [
    {
      authorization: "Bearer candidate-secret",
      capability: "task-capability-secret-1234567890",
      body: { taskId: "task 1", threadId: "thread-1", expectedRevision: 4, expectedCandidateRevisionId: null, idempotencyKey: "outline-task-1", payload: { nodes: [{ clientKey: "book", kind: "book", position: 0, title: "白塔未亮", purpose: "追查被改写的记录", summary: "林夕留下证词，港城记住名字", dependencyKeys: [], certainty: "tentative", protection: "free" }] } },
      method: "POST",
      url: "/api/projects/project%2F1/artifacts/outline",
    },
    {
      authorization: "Bearer candidate-secret",
      capability: "task-capability-secret-1234567890",
      body: { threadId: "thread-1", expectedRevision: 4, summary: "候选已保存" },
      method: "POST",
      url: "/api/projects/project%2F1/tasks/task%201/complete",
    },
  ]);
});

test("loads only the Novel Codex skill bundle and composes a bounded prompt", () => {
  const projectRoot = realpathSync(join(dirname(fileURLToPath(import.meta.url)), "../.."));
  const skills = loadNovelCodexSkills(projectRoot);
  assert.match(skills, /novel-canon/);
  assert.match(skills, /narrative-ir/);
  assert.match(skills, /story-tests/);
  assert.match(skills, /novel-workflow/);
  const prompt = composeNovelCodexPrompt(skills, "检查这一章的 Scene Anchor").map((item) => item.text).join("");
  assert.match(prompt, /Novel Codex 专用工作约束/);
  assert.match(prompt, /用户当前指令：\n检查这一章的 Scene Anchor/);
  assert.throws(() => composeNovelCodexPrompt(skills, "x".repeat(60_000)), /超过安全上限/);
  assert.throws(() => composeNovelCodexPrompt("", "🌙 ".repeat(5_000)), /超过安全上限/);
  assert.throws(() => composeNovelCodexPrompt("🌙 ".repeat(5_000), "小输入"), /超过安全上限/);
});
