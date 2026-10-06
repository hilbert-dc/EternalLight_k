import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { createHash, randomBytes, randomUUID } from "node:crypto";
import { mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { createInterface } from "node:readline";
import { fileURLToPath } from "node:url";

import { ModelBridge, PersistentTaskBudget, compileCodexConfig, compileCodexModelCatalog,
  createExecutionLease } from "@eternallight/agent-engine";

import { CodexAppServerAdapter } from "../electron/codex-adapter.mjs";
import { CodexCoreIntegrationRunner } from "../electron/codex-core-integration-runner.mjs";
import { NOVEL_SUBAGENT_DEVELOPER_INSTRUCTIONS, createQualificationContextDigest, credentialScopesFromEnvironment,
  loadCodexEvidence, resolveCodexCli, resolveCodexProvider, verifyCodexConnection } from "../electron/codex-runtime.mjs";
import { loadProjectDevelopmentEnvironment, resolveDevelopmentEnvironment } from "../electron/development-credentials.mjs";
import { taskProviderBudgetOptions } from "../electron/task-provider-budget.mjs";
import { NOVEL_ROOT_DEVELOPER_INSTRUCTIONS } from "../electron/creative-mission.mjs";

const projectRoot = resolve(fileURLToPath(new URL("..", import.meta.url)));
const suffix = process.platform === "win32" ? ".exe" : "";
const binary = (name) => join(projectRoot, "native", "target", "debug", `${name}${suffix}`);
const premise = "在一座逐渐失去记忆的港城，一位不再相信预言的抄写员必须决定是否为未来留下证词。";

export function limited(promise, milliseconds, label) {
  let timer;
  return Promise.race([
    promise,
    new Promise((_, reject) => { timer = setTimeout(() => reject(new Error(`${label} timed out`)), milliseconds); }),
  ]).finally(() => clearTimeout(timer));
}

export async function startRuntime(path, authorToken, candidateToken) {
  const child = spawn(binary("novel-codex-story-runtime"), ["--port", "0", "--db", path,
    "--engine-budget-db", `${path}.budget`, "--token-stdin"], { stdio: ["pipe", "pipe", "pipe"] });
  const listening = new Promise((resolveOrigin, reject) => {
    const lines = createInterface({ input: child.stdout });
    lines.on("line", (line) => {
      if (line.startsWith("Story Runtime listening on ")) resolveOrigin(line.slice("Story Runtime listening on ".length));
    });
    child.once("error", reject);
    child.once("exit", () => reject(new Error("Story Runtime exited before listening")));
  });
  child.stdin.end(`${JSON.stringify({ authorToken, candidateToken })}\n`);
  try {
    return { child, origin: await limited(listening, 90_000, "Story Runtime startup") };
  } catch (error) {
    if (child.exitCode === null && child.signalCode === null) child.kill();
    throw error;
  }
}

export async function request(origin, authorToken, method, path, body) {
  const response = await fetch(`${origin}${path}`, { method, signal: AbortSignal.timeout(10_000),
    headers: { authorization: `Bearer ${authorToken}`, "content-type": "application/json" },
    ...(body === undefined ? {} : { body: JSON.stringify(body) }) });
  const value = await response.json();
  if (!response.ok) throw new Error(`Story Runtime ${method} ${path} returned ${response.status}: ${value.error?.code ?? "unknown"}`);
  return value;
}

export async function probeLiveStory() {
  const environment = resolveDevelopmentEnvironment(loadProjectDevelopmentEnvironment(projectRoot, process.env));
  if (!environment.DEEPSEEK_API_KEY) throw new Error("本工程没有可用的 DeepSeek 开发凭据");
  const cli = resolveCodexCli({ explicitPath: environment.NOVEL_CODEX_CLI, isPackaged: false,
    projectRoot, resourcesPath: projectRoot });
  if (!cli.path) throw new Error(cli.reason ?? "Codex CLI unavailable");
  const baseInstructions = (await readFile(join(projectRoot, "..", "..", "codex-rs", "models-manager", "prompt.md"), "utf8")).trim();
  const root = await mkdtemp(join(tmpdir(), "novel-live-story-probe-"));
  let runtime, bridge, adapter, budget;
  try {
    const salt = join(root, "credential-salt");
    await writeFile(salt, `${randomBytes(32).toString("base64url")}\n`, { mode: 0o600 });
    const credentialScopes = credentialScopesFromEnvironment(environment, salt);
    const qualificationContextDigest = createQualificationContextDigest({ baseInstructions, cliPath: cli.path });
    const evidencePath = join(root, "evidence.json");
    const coreRunner = new CodexCoreIntegrationRunner({ baseEnvironment: environment, baseInstructions,
      developerInstructions: NOVEL_ROOT_DEVELOPER_INSTRUCTIONS, subagentDeveloperInstructions: NOVEL_SUBAGENT_DEVELOPER_INSTRUCTIONS,
      command: [cli.path, "app-server"], cwd: root, nodeCommand: process.execPath });
    await verifyCodexConnection({ baseEnvironment: environment, coreRunner, credentialScopes, evidencePath,
      provider: "deepseek", model: "deepseek-flash", qualificationContextDigest });
    const provider = resolveCodexProvider({ provider: "deepseek", model: "deepseek-flash", credentialScopes,
      evidence: await loadCodexEvidence(evidencePath), qualificationContextDigest });
    const authorToken = randomBytes(32).toString("hex");
    const candidateToken = randomBytes(32).toString("hex");
    const capability = randomBytes(32).toString("hex");
    runtime = await startRuntime(join(root, "story.sqlite3"), authorToken, candidateToken);
    const { project } = await request(runtime.origin, authorToken, "POST", "/api/projects", { title: "真实模型问答探针" });
    const base = `/api/projects/${project.id}`;
    await request(runtime.origin, authorToken, "POST", `${base}/artifacts/brief`, {
      expectedRevision: 0, expectedCandidateRevisionId: null, idempotencyKey: "probe-brief",
      payload: { premise },
    });
    const before = await request(runtime.origin, authorToken, "GET", `${base}/creative-workspace`);

    bridge = new ModelBridge({ sessionPlan: provider.sessionPlan, connections: provider.connections,
      credentialResolver: async (ref) => environment[ref], token: randomBytes(24).toString("hex") });
    const address = await bridge.start();
    const home = join(root, "codex-home");
    await mkdir(join(home, "sqlite"), { recursive: true, mode: 0o700 });
    const catalog = join(home, "catalog.json");
    await writeFile(catalog, JSON.stringify(compileCodexModelCatalog(provider.sessionPlan, { baseInstructions })), { mode: 0o600 });
    await writeFile(join(home, "config.toml"), compileCodexConfig({ sessionPlan: provider.sessionPlan,
      developerInstructions: NOVEL_ROOT_DEVELOPER_INSTRUCTIONS, subagentDeveloperInstructions: NOVEL_SUBAGENT_DEVELOPER_INSTRUCTIONS,
      modelCatalogPath: catalog, bridge: { baseUrl: address.baseUrl, credentialRef: "AGENT_ENGINE_BRIDGE_TOKEN" },
      mcpServers: [{ id: "novel_workspace", command: binary("novel-workspace-mcp"), args: [],
        envVars: ["NOVEL_CODEX_RUNTIME_URL", "NOVEL_CODEX_CANDIDATE_TOKEN", "NOVEL_CODEX_TASK_CAPABILITY", "NOVEL_CODEX_OPERATION"],
        approvalMode: "approve" }] }), { mode: 0o600 });
    adapter = new CodexAppServerAdapter({ command: [cli.path, "app-server"], cwd: root,
      env: { PATH: process.env.PATH, CODEX_HOME: home, CODEX_SQLITE_HOME: join(home, "sqlite"),
        AGENT_ENGINE_BRIDGE_TOKEN: address.token, NOVEL_CODEX_RUNTIME_URL: runtime.origin,
        NOVEL_CODEX_CANDIDATE_TOKEN: candidateToken, NOVEL_CODEX_TASK_CAPABILITY: capability,
        NOVEL_CODEX_OPERATION: "advisory" }, model: provider.id, provider: provider.provider });
    const events = [];
    let ready;
    const mcpReady = new Promise((resolveReady) => { ready = resolveReady; });
    adapter.setCallbacks({ onEvent: (event) => {
      if (event.method === "mcpServer/startupStatus/updated" && event.params?.name === "novel_workspace"
        && event.params?.status === "ready") ready();
      if (event.method === "item/completed" && event.params?.item?.type === "mcpToolCall") events.push(event.params.item);
    } });
    await limited(adapter.ensureSession(), 20_000, "Codex session startup");
    const task = await request(runtime.origin, authorToken, "POST", `${base}/tasks`, {
      operation: "advisory", target: { type: "project" }, inputRevision: before.revision,
      threadId: adapter.threadId, authorIntent: "当前小说讲了什么？仅基于当前作品回答，不修改作品。",
      capabilityHash: createHash("sha256").update(capability).digest("hex"),
      capabilityExpiresAt: Math.floor(Date.now() / 1000) + 180,
      initialBudget: { authorizationId: randomUUID(), authorMessageId: randomUUID(), expectedBaseSnapshotId: before.headSnapshotId,
        expectedSourceDigest: before.sourceDigest, maxRequests: 6, maxTokens: 40_000,
        deadlineAt: Math.floor(Date.now() / 1000) + 180 },
    });
    const grant = await request(runtime.origin, authorToken, "GET", `${base}/tasks/${task.id}/provider-budget`);
    budget = new PersistentTaskBudget({ databasePath: join(root, "story.sqlite3.budget"), runId: task.id,
      ...taskProviderBudgetOptions(grant, { taskId: task.id, workTaskId: `work-${task.id}`, deadlineAt: grant.deadlineAt }) });
    await bridge.beginTaskBudget(budget);
    await bridge.beginExecutionLease(createExecutionLease({ sessionPlan: provider.sessionPlan,
      runId: task.id, budgetId: budget.id }));
    await limited(mcpReady, 15_000, "Novel MCP startup");
    const binding = { taskId: task.id, projectId: project.id, threadId: adapter.threadId,
      expectedRevision: before.revision, purpose: "planner", budget: 12, byteBudget: 16_000 };
    const turn = await limited(adapter.sendTurn([{ type: "text", text: `先调用 novel_workspace.read_context，参数为 ${JSON.stringify(binding)}。然后仅根据返回的作品资料用中文回答：当前小说讲了什么？明确区分候选设定与已经写成的正文；不要调用写入工具。` }],
      { disableEnvironment: true }), 60_000, "model answer");
    const after = await request(runtime.origin, authorToken, "GET", `${base}/creative-workspace`);
    assert.equal(after.sourceDigest, before.sourceDigest, "a read-only answer changed the story source");
    assert.equal(after.headSnapshotId, before.headSnapshotId, "a read-only answer created a draft snapshot");
    assert.ok(events.length > 0, "the model did not complete a Novel MCP read");
    assert.match(turn.message ?? "", /港城|记忆|抄写员/, "the answer did not use the seeded story premise");
    await request(runtime.origin, authorToken, "POST", `${base}/tasks/${task.id}/complete`, {
      threadId: adapter.threadId, summary: "只读作品问答探针完成",
    });
    budget.complete();
    await request(runtime.origin, authorToken, "POST", `${base}/work-tasks/work-${task.id}/provider-usage`, { taskId: task.id });
    return { status: "passed", model: provider.id, mcpReads: events.length,
      answer: Array.from(turn.message).slice(0, 800).join(""), sourceUnchanged: true };
  } finally {
    if (adapter) {
      try { await adapter.close(); } catch { /* Preserve the probe result. */ }
    }
    if (bridge) {
      try { await bridge.close(); } catch { /* Preserve the probe result. */ }
    }
    budget?.close();
    if (runtime?.child && runtime.child.exitCode === null && runtime.child.signalCode === null) {
      const exited = new Promise((resolveExit) => runtime.child.once("exit", resolveExit));
      runtime.child.kill();
      await limited(exited, 5_000, "Story Runtime shutdown").catch(() => {});
    }
    await rm(root, { recursive: true, force: true });
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  if (!process.argv.includes("--confirm-cost")) {
    process.stderr.write("此探针会请求真实 DeepSeek 并产生费用；请显式传入 --confirm-cost。\n");
    process.exitCode = 2;
  } else {
    try {
      process.stdout.write(`${JSON.stringify(await probeLiveStory())}\n`);
    } catch (error) {
      const message = String(error?.message ?? error).replace(/sk-[A-Za-z0-9_-]+/g, "[REDACTED]");
      process.stderr.write(`${JSON.stringify({ status: "failed", message })}\n`);
      process.exitCode = 1;
    }
  }
}
