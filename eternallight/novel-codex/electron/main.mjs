import { app, BrowserWindow, ipcMain, session } from "electron";
import { spawn } from "node:child_process";
import { randomBytes, randomUUID } from "node:crypto";
import { existsSync, readFileSync, renameSync, writeFileSync } from "node:fs";
import { createServer } from "node:net";
import { isAbsolute, join } from "node:path";
import { fileURLToPath } from "node:url";

import { AgentEngineHost, CodexSessionRuntime, HostRecoveryJournal, ModelBridgeProcess, PersistentTaskBudget, RecoveryIndex, TaskBudget, assertTaskCompletable, bindExecutionTask, createTaskExecution, requestTaskCancellation, shouldMarkTaskFailed } from "@eternallight/agent-engine";
import { CodexAppServerAdapter } from "./codex-adapter.mjs";
import { CodexConformanceRunner } from "./codex-conformance-runner.mjs";
import { CodexCoreIntegrationRunner } from "./codex-core-integration-runner.mjs";
import { applyAgentConfiguration } from "./agent-configuration.mjs";
import { AdvisorySessions } from "./advisory-sessions.mjs";
import { assertProviderOperation, buildCodexConfig, buildCodexModelCatalog, composeNovelCodexPrompt, createCodexEnvironment, createQualificationContextDigest, createRuntimeEnvironment, createTaskCapability, credentialScopesFromEnvironment, inspectCodexConnection, installManagedAgentRoles, listCodexConnections, loadCodexEvidence, loadNovelCodexSkills, prepareIsolatedStorage, providerConfigurationError, recordRuntimeQualificationFailure, resolveCodexCli, resolveCodexProvider, resolveRuntimeBinaries, verifyCodexConnection, verifyCodexConnectionEdge } from "./codex-runtime.mjs";
import { chapterGenerationTask, planningGenerationTask } from "./creative-task-contract.mjs";
import { isAllowedRendererNavigation, isTrustedIpcSender } from "./navigation-policy.mjs";
import { reconcileRecoveryIndex } from "./recovery-reconciliation.mjs";
import { safeSendToWebContents } from "./webcontents-safety.mjs";
import { createTaskBoundHandoff, taskBoundInstructions } from "./workflow-handoff.mjs";
import { validateAuthorIntent } from "../shared/author-input-budget.mjs";

const __dirname = fileURLToPath(new URL(".", import.meta.url));
const configuredUserDataDir = process.env.NOVEL_CODEX_USER_DATA_DIR ?? null;
if (configuredUserDataDir !== null) {
  if (!isAbsolute(configuredUserDataDir) || configuredUserDataDir.includes("\0")) throw new Error("NOVEL_CODEX_USER_DATA_DIR 必须是绝对路径");
  app.setPath("userData", configuredUserDataDir);
}
const rendererUrl = process.env.NOVEL_CODEX_RENDERER_URL ?? "http://127.0.0.1:5173";
const parsedRendererUrl = new URL(rendererUrl);
if (parsedRendererUrl.protocol !== "http:" || !["127.0.0.1", "localhost"].includes(parsedRendererUrl.hostname)) throw new Error("NOVEL_CODEX_RENDERER_URL 必须是回环 HTTP 地址");
const configuredRuntimePort = process.env.NOVEL_CODEX_RUNTIME_PORT ? Number(process.env.NOVEL_CODEX_RUNTIME_PORT) : null;
if (configuredRuntimePort !== null && (!Number.isInteger(configuredRuntimePort) || configuredRuntimePort < 1 || configuredRuntimePort > 65_535)) throw new Error("NOVEL_CODEX_RUNTIME_PORT 无效");
let runtimePort = configuredRuntimePort;
let runtimeUrl = "";
const runtimeCandidateToken = randomBytes(32).toString("base64url");
const runtimeAuthorToken = randomBytes(32).toString("base64url");
const projectRoot = join(__dirname, "..");
const runtimeAssetRoot = app.isPackaged ? process.resourcesPath : projectRoot;
const packagedEntry = join(__dirname, "../dist/index.html");
let runtimeProcess;
let bundledRuntime = null;
let runtimeError = null;
let agentWebContents = null;
let removeAgentWebContentsListener = null;
let agentStorage = null;
let agentCli = null;
let agentProviderConfig = null;
let modelBridge = null;
let configurationRequestEpoch = 0;
let novelSkillPrompt = "";
let engineBaseInstructions = "";
let engineQualificationContextDigest = null;
let activeAgentProjectId = null;
let activeAgentPurpose = null;
let activeAgentTargetKey = null;
let activeRuntimeTask = null;
let activeAgentExecution = null;
let recoveryIndex = null;
let recoveryJournal = null;
const advisorySessions = new AdvisorySessions();
let rendererWebContents = null;
let allowWindowClose = false;
let quitRequested = false;
let codexAdapter = new CodexAppServerAdapter({ command: null, cwd: projectRoot });
const electronIpcMain = ipcMain;
const trustedIpcMain = {
  handle(channel, listener) {
    electronIpcMain.handle(channel, (event, ...args) => {
      assertTrustedIpcSender(event);
      return listener(event, ...args);
    });
  },
};

function isTrustedRendererUrl(url) {
  return isAllowedRendererNavigation(url, { isPackaged: app.isPackaged, rendererUrl, packagedEntry });
}

function assertTrustedIpcSender(event) {
  if (!isTrustedIpcSender(event, rendererWebContents, { isPackaged: app.isPackaged, rendererUrl, packagedEntry })) {
    throw new Error("拒绝来自非受信 Renderer 的 IPC 调用");
  }
}

function createWindow() {
  const window = new BrowserWindow({
    width: 1540,
    height: 960,
    minWidth: 900,
    minHeight: 720,
    backgroundColor: "#f5f6f8",
    title: "Novel Codex",
    webPreferences: {
      preload: join(__dirname, "preload.cjs"),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
    },
  });

  rendererWebContents = window.webContents;
  window.on("closed", () => {
    if (!quitRequested) allowWindowClose = false;
  });
  window.on("close", (event) => {
    if (allowWindowClose || window.webContents.isDestroyed()) return;
    event.preventDefault();
    safeSendToWebContents(window.webContents, "app:close-requested", {});
  });
  window.webContents.once("destroyed", () => {
    if (rendererWebContents === window.webContents) rendererWebContents = null;
  });
  window.webContents.setWindowOpenHandler(() => ({ action: "deny" }));
  window.webContents.on("will-navigate", (event, url) => {
    if (!isTrustedRendererUrl(url)) event.preventDefault();
  });
  window.webContents.on("will-redirect", (event, url) => {
    if (!isTrustedRendererUrl(url)) event.preventDefault();
  });

  session.defaultSession.setPermissionRequestHandler((_webContents, _permission, callback) => {
    callback(false);
  });

  if (app.isPackaged) {
    window.loadFile(packagedEntry);
  } else {
    window.loadURL(rendererUrl);
  }
}

function pythonCommand() {
  if (process.env.NOVEL_CODEX_PYTHON) return process.env.NOVEL_CODEX_PYTHON;
  const virtualEnvPython = process.platform === "win32" ? join(projectRoot, ".venv", "Scripts", "python.exe") : join(projectRoot, ".venv", "bin", "python");
  return existsSync(virtualEnvPython) ? virtualEnvPython : process.platform === "win32" ? "python" : "python3";
}

function findFreeLoopbackPort() {
  return new Promise((resolvePort, reject) => {
    const probe = createServer();
    probe.once("error", reject);
    probe.listen(0, "127.0.0.1", () => {
      const address = probe.address();
      const port = typeof address === "object" && address ? address.port : null;
      probe.close((error) => error ? reject(error) : resolvePort(port));
    });
  });
}

async function waitForRuntimeReady(child, url) {
  const deadline = Date.now() + 20_000;
  while (Date.now() < deadline) {
    if (child.exitCode !== null) throw new Error(`Creative Runtime exited (${child.exitCode})`);
    try {
      const response = await fetch(`${url}/health`);
      if (response.ok) return;
    } catch {
      // Runtime has not opened its loopback listener yet.
    }
    await new Promise((resolveDelay) => setTimeout(resolveDelay, 80));
  }
  throw new Error("Creative Runtime 启动超时");
}

async function startRuntime(runtimeDb) {
  const attempts = configuredRuntimePort === null ? 3 : 1;
  let lastError;
  for (let attempt = 0; attempt < attempts; attempt += 1) {
    runtimePort = configuredRuntimePort ?? await findFreeLoopbackPort();
    runtimeUrl = `http://127.0.0.1:${runtimePort}`;
    runtimeError = null;
    const runtimeCommand = bundledRuntime?.creativeRuntime ?? pythonCommand();
    const runtimeArgs = [...(bundledRuntime ? [] : [join(runtimeAssetRoot, "runtime", "server.py")]), "--host", "127.0.0.1", "--port", String(runtimePort), "--db", runtimeDb, "--token-stdin"];
    const child = spawn(runtimeCommand, runtimeArgs, {
      cwd: runtimeAssetRoot,
      env: createRuntimeEnvironment(process.env, runtimeAssetRoot),
      stdio: ["pipe", "pipe", "pipe"],
    });
    runtimeProcess = child;
    child.stdin.end(`${JSON.stringify({ candidateToken: runtimeCandidateToken, authorToken: runtimeAuthorToken })}\n`);
    child.stdout.on("data", (chunk) => console.log(`[creative-runtime] ${chunk.toString().trim()}`));
    child.stderr.on("data", (chunk) => console.error(`[creative-runtime] ${chunk.toString().trim()}`));
    child.on("error", (error) => {
      if (runtimeProcess === child) runtimeError = error.message;
    });
    child.on("exit", (code, signal) => {
      if (runtimeProcess === child && code !== 0 && signal !== "SIGTERM") runtimeError = `Creative Runtime exited (${code ?? signal ?? "unknown"})`;
    });
    try {
      await waitForRuntimeReady(child, runtimeUrl);
      await runtimeRequest("/api/tasks/recover-orphans", { method: "POST", body: "{}" });
      return;
    } catch (error) {
      lastError = error;
      if (!child.killed) child.kill();
    }
  }
  throw lastError ?? new Error("Creative Runtime 无法启动");
}

function ensureCodexConfig(codexHome, providerConfig, bridge = null) {
  const configPath = join(codexHome, "config.toml");
  const modelCatalogPath = join(codexHome, "model-catalog.json");
  const catalogTemporaryPath = `${modelCatalogPath}.${process.pid}.tmp`;
  const configTemporaryPath = `${configPath}.${process.pid}.tmp`;
  writeFileSync(catalogTemporaryPath, `${JSON.stringify(buildCodexModelCatalog({ ...providerConfig, baseInstructions: engineBaseInstructions }), null, 2)}\n`, { mode: 0o600 });
  renameSync(catalogTemporaryPath, modelCatalogPath);
  installManagedAgentRoles(codexHome, join(runtimeAssetRoot, "codex-runtime", "agents"));
  writeFileSync(configTemporaryPath, buildCodexConfig({
    ...providerConfig,
    bridge,
    mcpServerCommand: bundledRuntime?.novelWorkspaceMcp ?? pythonCommand(),
    mcpServerPath: bundledRuntime ? null : join(runtimeAssetRoot, "runtime", "mcp_server.py"),
    mcpServerArgs: bundledRuntime ? [] : null,
    modelCatalogPath,
  }), { encoding: "utf8", mode: 0o600 });
  renameSync(configTemporaryPath, configPath);
}

async function prepareModelTransport(providerConfig, storage = agentStorage) {
  const instance = new ModelBridgeProcess({
    sessionPlan: providerConfig.sessionPlan,
    connections: providerConfig.connections ?? [providerConfig.connection],
    credentialResolver: async (reference) => process.env[reference] ?? null,
    qualificationObserver: (failure) => recordRuntimeQualificationFailure({ ...failure, baselineEvidence: providerConfig.evidence, evidencePath: storage.evidencePath }),
  });
  const started = await instance.start();
  return {
    instance,
    bridge: { baseUrl: started.baseUrl, credentialRef: "AGENT_ENGINE_BRIDGE_TOKEN" },
    bridgeCredential: { ref: "AGENT_ENGINE_BRIDGE_TOKEN", value: started.token },
  };
}

function clearAgentCallbacks() {
  removeAgentWebContentsListener?.();
  removeAgentWebContentsListener = null;
  agentWebContents = null;
  codexAdapter.setCallbacks({ onEvent: () => {}, onUserInputRequest: () => {} });
}

function forwardAgentEvent(message) {
  sendAgentMessage("agent:event", message);
}

function recordAgentEvent(message) {
  void recoveryJournal?.observe(message).catch((error) => {
    console.error(`[novel-codex] recovery journal failed: ${error instanceof Error ? error.message : String(error)}`);
  });
}

async function beginRecoveryRun(task, projectId, threadId) {
  if (!recoveryJournal) return;
  await recoveryJournal.beginRun({
    runId: task.id,
    planId: agentProviderConfig?.sessionPlan?.id ?? "unresolved-session-plan",
    sessionPlan: agentProviderConfig?.sessionPlan ?? null,
    threadId,
    projectId,
    operation: task.operation ?? null,
  });
}

async function interruptAgentTurn(execution = activeAgentExecution) {
  if (execution?.engineHost && execution?.engineRunId) return execution.engineHost.cancelRun(execution.engineRunId, "author-interrupt");
  const bridge = execution?.modelBridge ?? modelBridge;
  const adapter = execution?.adapter ?? codexAdapter;
  await bridge?.cancelActiveRequests();
  return adapter.interruptTurn();
}

async function assertCurrentAgentPlan(operation, providerConfig = agentProviderConfig) {
  if (!providerConfig?.sessionPlan) throw new Error("Agent SessionPlan 尚未就绪");
  if (!engineQualificationContextDigest) throw new Error("Agent QualificationContext 尚未就绪");
  const evidence = agentStorage
    ? await loadCodexEvidence(agentStorage.evidencePath)
    : providerConfig.evidence ?? [];
  const current = resolveCodexProvider({
    credentialScopes: agentStorage ? credentialScopesFromEnvironment(process.env, agentStorage.credentialSaltPath) : {},
    evidence,
    model: providerConfig.model,
    operation,
    provider: providerConfig.provider,
    qualificationContextDigest: engineQualificationContextDigest,
  });
  if (current.sessionPlan.id !== providerConfig.sessionPlan.id) {
    throw Object.assign(new Error("模型能力证据或路由计划已经变化，请重新选择模型后再开始任务"), { code: "SESSION_PLAN_STALE" });
  }
  return current.sessionPlan;
}

async function beginAgentExecution(execution, operation) {
  const bridge = modelBridge;
  const adapter = codexAdapter;
  const providerConfig = agentProviderConfig;
  execution.modelBridge = bridge;
  execution.adapter = adapter;
  execution.providerConfig = providerConfig;
  execution.operation = operation;
  activeAgentExecution = execution;
  try {
    execution.sessionPlan = await assertCurrentAgentPlan(operation, providerConfig);
  } catch (error) {
    if (activeAgentExecution === execution) activeAgentExecution = null;
    throw error;
  }
}

async function finishAgentExecution(execution) {
  let cleanupError = null;
  try {
    if (execution.engineHost && execution.engineSessionId) await execution.engineHost.closeSession(execution.engineSessionId);
  } catch (error) {
    cleanupError = error;
  }
  if (recoveryIndex) await reconcileRecoveryIndex(recoveryIndex, runtimeRequest, (projectId, phase, error) => {
    console.error(`[novel-codex] recovery reconciliation failed (${projectId}/${phase}): ${error instanceof Error ? error.message : String(error)}`);
  });
  if (activeAgentExecution === execution) activeAgentExecution = null;
  if (cleanupError) throw cleanupError;
}

async function sendAgentTurn(execution, input, options) {
  if (!execution.modelBridge || !execution.adapter || !execution.sessionPlan) throw new Error("Agent Engine 执行环境尚未准备完成");
  const runId = execution.task?.id ?? randomUUID();
  const runtime = new CodexSessionRuntime({
    appServer: execution.adapter,
    bridge: execution.modelBridge,
    bridgeOwnership: "borrowed",
    closeAppServer: (adapter) => adapter.finishTaskOperation(),
    budgetFactory: async ({ lease }) => agentStorage?.budgetDb
      ? new PersistentTaskBudget({ databasePath: agentStorage.budgetDb, runId: lease.runId, id: lease.budgetId })
      : new TaskBudget({ id: lease.budgetId }),
    reconcile: async () => {
      await recoveryJournal?.flush();
      const report = await recoveryIndex?.recoveryReport() ?? [];
      const unresolvedOperations = report.find((entry) => entry.runId === runId)?.requests
        ?.filter((request) => request.recovery === "manual-reconciliation")
        .map((request) => ({ requestId: request.requestId, operation: request.operation })) ?? [];
      return { outcome: unresolvedOperations.length ? "outcome-unknown" : "cancelled", unresolvedOperations };
    },
  });
  const host = new AgentEngineHost({
    connections: execution.providerConfig.connections,
    evidenceProvider: async () => { throw new Error("the product must resolve a trusted SessionPlan before runtime creation"); },
    runtimeFactory: async () => runtime,
  });
  const sessionId = randomUUID();
  execution.engineHost = host;
  execution.engineSessionId = sessionId;
  execution.engineRunId = runId;
  try {
    await host.startSession({ sessionPlan: execution.sessionPlan, sessionId });
    const started = await host.startRun({ sessionId, runId, budgetId: randomUUID(), mode: execution.operation, input, turnOptions: options });
    const result = await started.completion;
    host.closeRun(runId);
    execution.engineRunId = null;
    await host.closeSession(sessionId);
    execution.engineSessionId = null;
    return result;
  } finally {
    if (execution.engineSessionId) await host.closeSession(sessionId).catch(() => undefined);
    execution.engineHost = null;
    execution.engineSessionId = null;
    execution.engineRunId = null;
  }
}

async function runCoreTurnWithLease(operation, callback) {
  if (activeAgentExecution) throw new Error("已有创作引擎任务正在运行，请先等待或停止当前任务");
  const execution = createTaskExecution();
  await beginAgentExecution(execution, operation);
  try {
    assertTaskCompletable(execution);
    const result = await sendAgentTurn(execution, [], { dispatch: callback });
    assertTaskCompletable(execution);
    return result;
  } finally {
    await finishAgentExecution(execution);
  }
}

function attachAgentWebContents(webContents) {
  if (agentWebContents === webContents && !webContents.isDestroyed()) return;
  clearAgentCallbacks();
  if (webContents.isDestroyed()) return;
  const onDestroyed = () => {
    if (agentWebContents === webContents) clearAgentCallbacks();
  };
  webContents.once("destroyed", onDestroyed);
  removeAgentWebContentsListener = () => webContents.removeListener("destroyed", onDestroyed);
  agentWebContents = webContents;
}

function sendAgentMessage(channel, payload) {
  const target = agentWebContents;
  if (!safeSendToWebContents(target, channel, payload) && target?.isDestroyed?.()) clearAgentCallbacks();
}

async function assertThreadProject(projectId, threadId) {
  if (typeof projectId !== "string" || !projectId.trim() || typeof threadId !== "string" || !threadId.trim()) throw new Error("作品或任务 ID 无效");
  const { tasks } = await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/tasks/lookup`, {
    method: "POST", body: JSON.stringify({ threadIds: [threadId] }),
  });
  if (!tasks.length) throw new Error("该任务不属于当前作品");
  return tasks[0];
}

async function prepareNovelWorkflow(projectId, prompt, task, purpose) {
  const workspace = await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/creative-workspace`);
  const target = task.target ?? {};
  if (purpose !== "planner") {
    const currentChapter = workspace.chapters?.find((chapter) => chapter.id === target.chapterId);
    if (!currentChapter || currentChapter.number !== target.chapterNumber || !Number.isInteger(target.sceneOrder)) {
      throw new Error("任务缺少精确的章节与场景范围，不能启动生成");
    }
  }
  const handoff = createTaskBoundHandoff(projectId, workspace, task);
  codexAdapter.emit({
    method: "agent/workflow",
    params: { workflow: { task, target, revision: workspace.revision } },
  });
  return {
    workflow: { task, target, revision: workspace.revision },
    prompt: composeNovelCodexPrompt(novelSkillPrompt, prompt, taskBoundInstructions(handoff)),
  };
}

async function runCreativeAgentTask(event, projectId, prompt, taskType, target = null) {
  if (!codexAdapter.enabled) throw new Error("Novel Agent Engine 未配置，当前只能手动编辑，不能生成伪造产物");
  if (activeAgentExecution) throw new Error("已有创作引擎任务正在运行，请先等待或停止当前任务");
  assertProviderOperation(agentProviderConfig, taskType.operation);
  attachAgentWebContents(event.sender);
  codexAdapter.setCallbacks({
    onEvent: forwardAgentEvent,
    onUserInputRequest: (request) => sendAgentMessage("agent:user-input-request", request),
  });
  const execution = createTaskExecution();
  await beginAgentExecution(execution, taskType.operation);
  const adapter = execution.adapter;
  let task = null;
  try {
    assertTaskCompletable(execution);
    const capability = createTaskCapability();
    adapter.prepareTaskOperation(taskType.operation, capability.secret);
    const taskPurpose = taskType.operation === "generateChapter" ? "writer" : "planner";
    const targetKey = target?.chapterId ? `${projectId}:${taskPurpose}:${target.chapterId}` : `${projectId}:${taskPurpose}`;
    activeAgentProjectId = projectId;
    activeAgentPurpose = taskPurpose;
    activeAgentTargetKey = targetKey;
    const threadId = await adapter.ensureSession();
    assertTaskCompletable(execution);
    const workspace = await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/creative-workspace`);
    assertTaskCompletable(execution);
    task = await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/tasks`, {
      method: "POST",
      body: JSON.stringify({
        target,
        inputRevision: workspace.revision,
        threadId,
        model: execution.providerConfig?.model ?? null,
        operation: taskType.operation,
        authorIntent: prompt,
        capabilityHash: capability.hash,
        capabilityExpiresAt: capability.expiresAt,
      }),
    });
    task = bindExecutionTask(execution, task);
    activeRuntimeTask = task;
    if (execution.cancellationRequested) {
      await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/tasks/${encodeURIComponent(task.id)}/cancel`, { method: "POST", body: "{}" });
      throw Object.assign(new Error("生成已停止"), { code: "TASK_CANCELLED" });
    }
    const prepared = await prepareNovelWorkflow(projectId, prompt, task, task.purpose);
    await beginRecoveryRun(task, projectId, threadId);
    assertTaskCompletable(execution);
    const result = await sendAgentTurn(execution, prepared.prompt, { disableEnvironment: true, threadName: prompt });
    assertTaskCompletable(execution);
    await recoveryJournal?.flush();
    const completion = await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/tasks/${encodeURIComponent(task.id)}/complete`, {
      method: "POST",
      body: JSON.stringify({ threadId, summary: Array.from(result.message ?? "").slice(0, 4_000).join("") }),
    });
    await recoveryJournal?.completeRun();
    if (activeRuntimeTask?.id === task.id) activeRuntimeTask = null;
    return { ...result, projectId, workflow: prepared.workflow, completion };
  } catch (error) {
    if (shouldMarkTaskFailed(task)) {
      try {
        await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/tasks/${encodeURIComponent(task.id)}`, {
          method: "PATCH",
          body: JSON.stringify({ threadId: task.threadId, status: "failed", error: error.message }),
        });
      } catch { /* Preserve the original Agent failure and any saved checkpoints. */ }
    }
    if (activeRuntimeTask?.id === task?.id) activeRuntimeTask = null;
    if (execution.cancellationRequested) throw Object.assign(new Error("生成已停止"), { code: "TASK_CANCELLED" });
    throw error;
  } finally {
    await recoveryJournal?.abandonRun().catch(() => undefined);
    await adapter.finishTaskOperation();
    await finishAgentExecution(execution);
  }
}

async function runPlanningReview(event, projectId, request) {
  if (!codexAdapter.enabled) throw new Error("Novel Agent Engine 未配置，当前只能手动编辑");
  if (activeAgentExecution) throw new Error("已有创作引擎任务正在运行，请先等待或停止当前任务");
  if (!request || typeof request !== "object" || !["outline", "plan"].includes(request.artifactKind)) throw new Error("规划评审目标无效");
  if (typeof request.revisionId !== "string" || !request.revisionId.trim()) throw new Error("规划评审缺少明确版本");
  validateAuthorIntent(request.authorIntent);
  const roles = request.roles;
  if (!Array.isArray(roles) || ![2, 3].includes(roles.length) || new Set(roles).size !== roles.length || roles.some((role) => !["structure", "character", "experience"].includes(role))) throw new Error("规划评审角色无效");
  assertProviderOperation(agentProviderConfig, "reviewPlanningSeat");
  attachAgentWebContents(event.sender);
  codexAdapter.setCallbacks({
    onEvent: forwardAgentEvent,
    onUserInputRequest: (request) => sendAgentMessage("agent:user-input-request", request),
  });
  const execution = createTaskExecution();
  execution.projectId = projectId;
  await beginAgentExecution(execution, "reviewPlanningSeat");
  const adapter = execution.adapter;
  let run = null;
  try {
    assertTaskCompletable(execution);
    run = await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/planning-reviews`, {
      method: "POST",
      body: JSON.stringify(request),
    });
    execution.runId = run.id;
    assertTaskCompletable(execution);
    for (const role of run.roles) {
      if (execution.cancellationRequested) break;
      let task = null;
      try {
        await assertCurrentAgentPlan("reviewPlanningSeat");
        assertTaskCompletable(execution);
        const capability = createTaskCapability();
        adapter.prepareTaskOperation("reviewPlanningSeat", capability.secret);
        const threadId = await adapter.ensureSession();
        assertTaskCompletable(execution);
        task = await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/planning-reviews/${encodeURIComponent(run.id)}/seats/${encodeURIComponent(role)}/tasks`, {
          method: "POST",
          body: JSON.stringify({ threadId, model: execution.providerConfig?.model ?? null, capabilityHash: capability.hash, capabilityExpiresAt: capability.expiresAt }),
        });
        task = bindExecutionTask(execution, task);
        activeRuntimeTask = task;
        if (execution.cancellationRequested) {
          await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/tasks/${encodeURIComponent(task.id)}/cancel`, { method: "POST", body: "{}" });
          break;
        }
        const workspace = await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/creative-workspace`);
        const handoff = createTaskBoundHandoff(projectId, workspace, task);
        adapter.emit({ method: "agent/workflow", params: { workflow: { task, planningReviewId: run.id, role, revision: workspace.revision } } });
        const prompt = composeNovelCodexPrompt(novelSkillPrompt, request.authorIntent, `你是规划深审中的独立 ${role} 席。先按以下绑定调用 read_context；不得读取其他席报告，不得修改任何规划、正文或正式事实。按冻结的 checks 逐项检查，允许没有问题或无法判断。然后调用 submit_planning_review_report 保存结构化报告，最后调用 complete_task。\n${JSON.stringify(handoff, null, 2)}`);
        await beginRecoveryRun(task, projectId, threadId);
        assertTaskCompletable(execution);
        const result = await sendAgentTurn(execution, prompt, { disableEnvironment: true, threadName: `规划深审 · ${role}` });
        assertTaskCompletable(execution);
        await recoveryJournal?.flush();
        await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/tasks/${encodeURIComponent(task.id)}/complete`, { method: "POST", body: JSON.stringify({ threadId, summary: Array.from(result.message ?? "").slice(0, 4_000).join("") }) });
        await recoveryJournal?.completeRun();
      } catch (error) {
        if (shouldMarkTaskFailed(task)) {
          try { await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/tasks/${encodeURIComponent(task.id)}`, { method: "PATCH", body: JSON.stringify({ threadId: task.threadId, status: "failed", error: error.message }) }); } catch { /* Preserve reports from other seats. */ }
        }
        if (error?.code === "SESSION_PLAN_STALE") throw error;
      } finally {
        await recoveryJournal?.abandonRun().catch(() => undefined);
        if (activeRuntimeTask?.id === task?.id) activeRuntimeTask = null;
        await adapter.finishTaskOperation();
      }
    }
    if (execution.cancellationRequested) {
      await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/planning-reviews/${encodeURIComponent(run.id)}/cancel`, { method: "POST", body: "{}" });
      throw Object.assign(new Error("规划深审已停止；已保存报告仍会保留"), { code: "TASK_CANCELLED" });
    }
    const finalized = await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/planning-reviews/${encodeURIComponent(run.id)}/finalize`, { method: "POST", body: "{}" });
    return { projectId, run: finalized };
  } finally {
    await finishAgentExecution(execution);
  }
}

async function runAdvisoryAgentTask(event, projectId, prompt) {
  if (!codexAdapter.enabled) throw new Error("Novel Agent Engine 未配置，当前只能手动编辑");
  if (activeAgentExecution) throw new Error("已有创作引擎任务正在运行，请先等待或停止当前任务");
  assertProviderOperation(agentProviderConfig, "advisory");
  attachAgentWebContents(event.sender);
  codexAdapter.setCallbacks({
    onEvent: forwardAgentEvent,
    onUserInputRequest: (request) => sendAgentMessage("agent:user-input-request", request),
  });
  const execution = createTaskExecution();
  await beginAgentExecution(execution, "advisory");
  const adapter = execution.adapter;
  let task = null;
  try {
    assertTaskCompletable(execution);
    const capability = createTaskCapability();
    const advisoryThreadId = advisorySessions.preferredThread(projectId);
    adapter.prepareAdvisorySession(capability.secret);
    if (adapter.threadId && !advisorySessions.canReuseCurrent(projectId, adapter.threadId, activeAgentProjectId, activeAgentPurpose)) {
      adapter.detachThread();
    }
    if (!adapter.threadId && advisoryThreadId) await adapter.resumeThread(advisoryThreadId);
    activeAgentProjectId = projectId;
    activeAgentPurpose = "advisory";
    activeAgentTargetKey = `${projectId}:advisory`;
    const threadId = await adapter.ensureSession();
    assertTaskCompletable(execution);
    advisorySessions.remember(projectId, threadId);
    const workspace = await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/creative-workspace`);
    assertTaskCompletable(execution);
    task = await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/tasks`, {
      method: "POST",
      body: JSON.stringify({ target: { type: "project" }, inputRevision: workspace.revision, threadId, model: execution.providerConfig?.model ?? null, operation: "advisory", authorIntent: prompt, capabilityHash: capability.hash, capabilityExpiresAt: capability.expiresAt }),
    });
    task = bindExecutionTask(execution, task);
    activeRuntimeTask = task;
    if (execution.cancellationRequested) {
      await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/tasks/${encodeURIComponent(task.id)}/cancel`, { method: "POST", body: "{}" });
      throw Object.assign(new Error("生成已停止"), { code: "TASK_CANCELLED" });
    }
    const handoff = createTaskBoundHandoff(projectId, workspace, task);
    adapter.emit({ method: "agent/workflow", params: { workflow: { task, revision: workspace.revision } } });
    const advisoryPrompt = composeNovelCodexPrompt(novelSkillPrompt, prompt, `只通过 novel_workspace.read_context 读取本次任务的冻结上下文，调用时原样传入以下绑定中的 taskId、projectId、threadId、expectedRevision，并使用 purpose=planner、budget=24、byteBudget=60000：\n${JSON.stringify(handoff, null, 2)}\n\n这是自由讨论任务：只提供分析、问题和建议，不得保存或采用领域产物。讨论结论会由界面带入创作简报、大纲或章纲的专用生成入口。`);
    await beginRecoveryRun(task, projectId, threadId);
    assertTaskCompletable(execution);
    const result = await sendAgentTurn(execution, advisoryPrompt, { disableEnvironment: true, threadName: prompt });
    assertTaskCompletable(execution);
    await recoveryJournal?.flush();
    const completion = await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/tasks/${encodeURIComponent(task.id)}/complete`, { method: "POST", body: JSON.stringify({ threadId, summary: Array.from(result.message ?? "").slice(0, 4_000).join("") }) });
    await recoveryJournal?.completeRun();
    if (activeRuntimeTask?.id === task.id) activeRuntimeTask = null;
    return { ...result, projectId, workflow: { task, revision: workspace.revision }, completion };
  } catch (error) {
    if (shouldMarkTaskFailed(task)) {
      try { await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/tasks/${encodeURIComponent(task.id)}`, { method: "PATCH", body: JSON.stringify({ threadId: task.threadId, status: "failed", error: error.message }) }); } catch { /* Preserve the original advisory failure. */ }
    }
    if (activeRuntimeTask?.id === task?.id) activeRuntimeTask = null;
    if (execution.cancellationRequested) throw Object.assign(new Error("生成已停止"), { code: "TASK_CANCELLED" });
    throw error;
  } finally {
    await recoveryJournal?.abandonRun().catch(() => undefined);
    await adapter.finishTaskOperation();
    await finishAgentExecution(execution);
  }
}

async function runtimeRequest(path, options = {}) {
  if (runtimeError) throw new Error(runtimeError);
  const response = await fetch(`${runtimeUrl}${path}`, {
    ...options,
    headers: { "Content-Type": "application/json", Authorization: `Bearer ${runtimeAuthorToken}`, ...(options.headers ?? {}) },
  });
  const payload = await response.json();
  if (!response.ok) {
    const detail = payload?.error ?? payload?.detail;
    const fallback = "Creative Runtime request failed";
    let message = fallback;
    if (typeof detail === "string" && detail.trim()) message = detail;
    else if (typeof detail?.message === "string" && detail.message.trim()) message = detail.message;
    else if (detail !== undefined && detail !== null) {
      try { message = JSON.stringify(detail) || fallback; } catch { /* JSON payloads should be serializable. */ }
    }
    throw new Error(message);
  }
  return payload;
}

function registerIpcHandlers(ipcMain = trustedIpcMain) {
  ipcMain.handle("app:confirm-close", () => {
    allowWindowClose = true;
    if (quitRequested) app.quit();
    else BrowserWindow.getAllWindows().forEach((window) => window.close());
    return { closing: true };
  });
  ipcMain.handle("app:cancel-close", () => {
    quitRequested = false;
    return { closing: false };
  });
  ipcMain.handle("runtime:status", async () => {
    try {
      await runtimeRequest("/health");
      return { state: "online", mode: "local", error: null };
    } catch (error) {
      return { state: "offline", mode: "local", error: error.message };
    }
  });
  ipcMain.handle("runtime:list-projects", () => runtimeRequest("/api/projects"));
  ipcMain.handle("runtime:create-project", (_event, title, logline) => {
    if (typeof title !== "string" || !title.trim() || title.length > 120 || /[\r\n]/.test(title)) throw new Error("作品标题无效");
    if (typeof logline !== "string" || logline.length > 500) throw new Error("作品简介过长");
    return runtimeRequest("/api/projects", { method: "POST", body: JSON.stringify({ title, logline }) });
  });
  ipcMain.handle("runtime:metrics", () => runtimeRequest("/api/metrics"));
  ipcMain.handle("runtime:metrics-abandon", (_event, stage, idempotencyKey) => {
    if (!["brief", "outline", "plan", "draft"].includes(stage)) throw new Error("请选择创作阶段");
    if (typeof idempotencyKey !== "string" || !/^[a-f0-9-]{36}$/.test(idempotencyKey)) throw new Error("统计操作凭证无效");
    return runtimeRequest("/api/metrics/abandon", { method: "POST", body: JSON.stringify({ stage, idempotencyKey }) });
  });
  ipcMain.handle("runtime:get-state", (_event, projectId) => runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/state`));
  ipcMain.handle("runtime:export-project", (_event, projectId, options) => runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/export`, { method: "POST", body: JSON.stringify(options) }));
  ipcMain.handle("runtime:get-creative-workspace", (_event, projectId) => runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/creative-workspace`));
  ipcMain.handle("runtime:cancel-planning-review", async (_event, projectId, runId) => {
    if (typeof projectId !== "string" || !projectId.trim() || typeof runId !== "string" || !runId.trim()) throw new Error("规划评审标识无效");
    if (activeAgentExecution?.projectId === projectId && activeAgentExecution?.runId === runId) {
      requestTaskCancellation(activeAgentExecution);
      requestTaskCancellation(activeAgentExecution.task ?? activeRuntimeTask);
      await interruptAgentTurn().catch(() => undefined);
    }
    return runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/planning-reviews/${encodeURIComponent(runId)}/cancel`, { method: "POST", body: "{}" });
  });
  ipcMain.handle("runtime:set-working-baseline", (_event, projectId, kind, objectId, revisionId, expectedSelectionRevision) => {
    if (!["brief", "outline", "plan", "draft"].includes(kind)) throw new Error("工作版本类型无效");
    if (typeof revisionId !== "string" || !revisionId.trim() || !Number.isInteger(expectedSelectionRevision) || expectedSelectionRevision < 0) throw new Error("工作版本选择无效");
    return runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/working-baselines/${kind}`, { method: "PUT", body: JSON.stringify({ objectId, revisionId, expectedSelectionRevision }) });
  });
  ipcMain.handle("runtime:clear-working-baseline", (_event, projectId, kind, objectId, expectedSelectionRevision) => {
    if (!["brief", "outline", "plan", "draft"].includes(kind) || !Number.isInteger(expectedSelectionRevision) || expectedSelectionRevision < 1) throw new Error("工作版本选择无效");
    return runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/working-baselines/${kind}`, { method: "DELETE", body: JSON.stringify({ objectId, expectedSelectionRevision }) });
  });
  ipcMain.handle("runtime:save-continuation-intent", (_event, projectId, kind, targetId, workingRevisionId, authorIntent) => {
    if (!["brief", "outline", "plan", "draft"].includes(kind) || typeof workingRevisionId !== "string" || !workingRevisionId.trim()) throw new Error("续创目标无效");
    validateAuthorIntent(authorIntent);
    return runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/continuations/${kind}`, { method: "PUT", body: JSON.stringify({ targetId, workingRevisionId, authorIntent }) });
  });
  ipcMain.handle("runtime:get-writer-preview", (_event, projectId, projectRevision, readingChapter, sceneOrder, povCharacterId = null, storyTime = null) => {
    if (!Number.isInteger(projectRevision) || projectRevision < 0 || !Number.isInteger(readingChapter) || readingChapter < 1 || !Number.isInteger(sceneOrder) || sceneOrder < 0) throw new Error("Writer 预览范围无效");
    if (povCharacterId !== null && (typeof povCharacterId !== "string" || !povCharacterId.trim())) throw new Error("Writer 预览 POV 无效");
    if (storyTime !== null && !Number.isInteger(storyTime)) throw new Error("Writer 预览故事时间无效");
    return runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/context`, { method: "POST", body: JSON.stringify({ purpose: "writer", projectRevision, readingChapter, sceneOrder, povCharacterId, storyTime, budget: 24, byteBudget: 60_000 }) });
  });
  ipcMain.handle("runtime:list-planning-impacts", (_event, projectId, cursor = null, limit = 25) => {
    if (cursor !== null && typeof cursor !== "string") throw new Error("规划反馈 cursor 无效");
    if (!Number.isInteger(limit) || limit < 1 || limit > 50) throw new Error("规划反馈分页大小无效");
    const query = new URLSearchParams({ limit: String(limit), ...(cursor ? { cursor } : {}) });
    return runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/impacts?${query}`);
  });
  ipcMain.handle("runtime:save-artifact", (_event, projectId, kind, payload, expectedRevision, expectedCandidateRevisionId = null, linkedImpactIds = []) => {
    if (!["brief", "outline", "plan"].includes(kind) || payload === null || typeof payload !== "object" || Array.isArray(payload)) throw new Error("创作产物无效");
    if (!Number.isInteger(expectedRevision) || expectedRevision < 0) throw new Error("作品版本无效");
    if (!Array.isArray(linkedImpactIds) || linkedImpactIds.length > 50 || linkedImpactIds.some((id) => typeof id !== "string" || !id.trim())) throw new Error("规划反馈链接无效");
    return runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/artifacts/${kind}`, { method: "POST", body: JSON.stringify({ payload, expectedRevision, expectedCandidateRevisionId, linkedImpactIds, idempotencyKey: `desktop-save-${kind}-${randomUUID()}` }) });
  });
  ipcMain.handle("runtime:materialize-chapter-plan", (_event, projectId, chapterPlanId, expectedRevision) => runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/chapters/materialize-from-plan`, { method: "POST", body: JSON.stringify({ chapterPlanId, expectedRevision, idempotencyKey: `desktop-materialize-${chapterPlanId}-${expectedRevision}` }) }));
  ipcMain.handle("runtime:materialize-standalone-chapter", (_event, projectId, title, expectedRevision) => runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/chapters/materialize-standalone`, { method: "POST", body: JSON.stringify({ title, expectedRevision, idempotencyKey: `desktop-standalone-${expectedRevision}-${String(title).slice(0, 40)}` }) }));
  ipcMain.handle("runtime:associate-chapter-plan", (_event, projectId, chapterId, chapterPlanId, expectedRevision) => {
    if (typeof chapterId !== "string" || !chapterId.trim() || typeof chapterPlanId !== "string" || !chapterPlanId.trim()) throw new Error("章节规划关联参数无效");
    return runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/chapters/associate-plan`, { method: "POST", body: JSON.stringify({ chapterId, chapterPlanId, expectedRevision, idempotencyKey: `desktop-associate-${chapterId}-${chapterPlanId}-${expectedRevision}` }) });
  });
  ipcMain.handle("runtime:resolve-planning-impact", (_event, projectId, impactId, expectedRevision, resolution, note = "") => {
    if (typeof impactId !== "string" || !impactId.trim() || !["kept", "false-positive"].includes(resolution) || typeof note !== "string" || note.length > 2_000) throw new Error("规划反馈处理参数无效");
    return runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/impacts/${encodeURIComponent(impactId)}/resolve`, { method: "POST", body: JSON.stringify({ expectedRevision, resolution, note, idempotencyKey: `desktop-impact-${impactId}-${expectedRevision}-${resolution}` }) });
  });
  ipcMain.handle("runtime:set-artifact-lock", (_event, projectId, kind, locked, expectedRevision) => {
    if (!['brief', 'outline', 'plan'].includes(kind) || typeof locked !== 'boolean') throw new Error("产物锁定参数无效");
    if (!Number.isInteger(expectedRevision) || expectedRevision < 0) throw new Error("作品版本无效");
    return runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/artifacts/${kind}/lock`, { method: "PATCH", body: JSON.stringify({ locked, expectedRevision }) });
  });
  ipcMain.handle("runtime:save-draft-checkpoint", (_event, projectId, payload, expectedRevision, expectedCandidateRevisionId = null) => {
    if (payload === null || typeof payload !== "object" || Array.isArray(payload)) throw new Error("草稿内容无效");
    if (!Number.isInteger(expectedRevision) || expectedRevision < 0) throw new Error("作品版本无效");
    return runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/drafts/checkpoints`, { method: "POST", body: JSON.stringify({ payload, expectedRevision, expectedCandidateRevisionId, idempotencyKey: `desktop-save-draft-${randomUUID()}` }) });
  });
  ipcMain.handle("runtime:create-review", (_event, projectId, artifactKind, revisionId, proposals = [], selections = {}) => {
    if (!["brief", "outline", "plan", "draft"].includes(artifactKind)) throw new Error("审阅产物类型无效");
    if (typeof revisionId !== "string" || !revisionId.trim()) throw new Error("修订 ID 无效");
    if (!Array.isArray(proposals) || proposals.length > 200 || selections === null || typeof selections !== "object" || Array.isArray(selections)) throw new Error("审阅内容无效");
    return runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/reviews`, { method: "POST", body: JSON.stringify({ artifactKind, revisionId, proposals, selections, idempotencyKey: `desktop-review-${randomUUID()}` }) });
  });
  ipcMain.handle("runtime:accept-review", (_event, projectId, reviewId, expectedRevision, selections = {}) => {
    if (typeof reviewId !== "string" || !reviewId.trim() || !Number.isInteger(expectedRevision) || expectedRevision < 0) throw new Error("审阅参数无效");
    const idempotencyKey = `desktop-${reviewId}-${expectedRevision}`;
    return runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/reviews/${encodeURIComponent(reviewId)}/accept`, { method: "POST", body: JSON.stringify({ expectedRevision, idempotencyKey, selections }) });
  });
  ipcMain.handle("runtime:undo-commit", (_event, projectId, commitId, expectedRevision) => {
    if (typeof commitId !== "string" || !commitId.trim() || !Number.isInteger(expectedRevision) || expectedRevision < 0) throw new Error("撤销参数无效");
    return runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/commits/${encodeURIComponent(commitId)}/undo`, { method: "POST", body: JSON.stringify({ expectedRevision, idempotencyKey: `desktop-undo-${commitId}-${expectedRevision}` }) });
  });
  ipcMain.handle("runtime:get-diffs", (_event, projectId, chapterId) => runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/chapters/${encodeURIComponent(chapterId)}/diffs`));
  ipcMain.handle("runtime:story-tests", (_event, projectId) => runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/story-tests`, { method: "POST" }));
  ipcMain.handle("runtime:search", (_event, projectId, query, limit = 20, kind = null) => {
    if (typeof projectId !== "string" || !projectId.trim()) throw new Error("作品 ID 无效");
    if (typeof query !== "string" || query.length > 240) throw new Error("搜索内容无效");
    if (!Number.isInteger(limit) || limit < 1 || limit > 50) throw new Error("搜索数量无效");
    if (kind !== null && typeof kind !== "string") throw new Error("搜索类型无效");
    const params = new URLSearchParams({ q: query, limit: String(limit) });
    if (kind) params.set("kind", kind);
    return runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/search?${params}`);
  });
  ipcMain.handle("agent:status", () => codexAdapter.status());
  ipcMain.handle("agent:recovery-report", () => recoveryIndex?.recoveryReport() ?? []);
  ipcMain.handle("agent:resolve-recovery", async (_event, projectId, runId) => {
    if (activeAgentExecution) throw new Error("已有创作引擎任务正在运行，不能结束恢复核对");
    if (typeof projectId !== "string" || !projectId.trim() || typeof runId !== "string" || !runId.trim()) throw new Error("恢复任务参数无效");
    if (!recoveryIndex) throw new Error("恢复索引尚未就绪");
    const report = await recoveryIndex.recoveryReport();
    const run = report.find((entry) => entry.runId === runId && entry.projectId === projectId);
    if (!run) throw new Error("恢复任务不存在或不属于当前作品");
    if (!run.requests.some((request) => request.recovery === "manual-reconciliation")) throw new Error("该任务不需要人工核对");
    await recoveryIndex.resolveRun(runId, "manual-reviewed");
    return { resolved: true, runId };
  });
  ipcMain.handle("agent:list-connections", async () => ({
    connections: listCodexConnections({
      credentialScopes: agentStorage ? credentialScopesFromEnvironment(process.env, agentStorage.credentialSaltPath) : {},
      evidence: agentStorage ? await loadCodexEvidence(agentStorage.evidencePath) : [],
      qualificationContextDigest: engineQualificationContextDigest ?? "unavailable",
    }),
  }));
  ipcMain.handle("agent:verify-connection", async (_event, provider, model) => {
    if (activeAgentExecution) throw new Error("已有创作引擎任务正在运行，不能验证连接");
    if (!agentStorage || !agentCli?.path) throw new Error("Novel Codex 内核尚未就绪，无法执行真实能力认证");
    const conformanceRunner = new CodexConformanceRunner({
      baseEnvironment: process.env,
      baseInstructions: engineBaseInstructions,
      command: [agentCli.path, "app-server"],
      cwd: agentStorage.workspaceHome,
      nodeCommand: process.execPath,
    });
    const coreRunner = new CodexCoreIntegrationRunner({
      baseEnvironment: process.env,
      baseInstructions: engineBaseInstructions,
      command: [agentCli.path, "app-server"],
      cwd: agentStorage.workspaceHome,
      nodeCommand: process.execPath,
    });
    return verifyCodexConnection({
      baseEnvironment: process.env,
      conformanceRunner,
      coreRunner,
      credentialScopes: credentialScopesFromEnvironment(process.env, agentStorage.credentialSaltPath),
      evidencePath: agentStorage.evidencePath,
      provider,
      qualificationContextDigest: engineQualificationContextDigest ?? "unavailable",
      model,
    });
  });
  ipcMain.handle("agent:verify-connection-edge", async (_event, parentProvider, parentModel, childProvider, childModel) => {
    if (activeAgentExecution) throw new Error("已有创作引擎任务正在运行，不能验证连接");
    if (!agentStorage || !agentCli?.path) throw new Error("Novel Codex 内核尚未就绪，无法执行真实能力认证");
    const conformanceRunner = new CodexConformanceRunner({
      baseEnvironment: process.env,
      baseInstructions: engineBaseInstructions,
      command: [agentCli.path, "app-server"],
      cwd: agentStorage.workspaceHome,
      nodeCommand: process.execPath,
    });
    return verifyCodexConnectionEdge({
      baseEnvironment: process.env,
      conformanceRunner,
      credentialScopes: credentialScopesFromEnvironment(process.env, agentStorage.credentialSaltPath),
      evidencePath: agentStorage.evidencePath,
      parentProvider,
      parentModel,
      childProvider,
      childModel,
      qualificationContextDigest: engineQualificationContextDigest ?? "unavailable",
    });
  });
  ipcMain.handle("agent:configure", async (_event, provider, model) => {
    if (activeAgentExecution) throw new Error("已有创作引擎任务正在运行，请先等待或停止当前任务");
    if (typeof provider !== "string" || provider.length > 40 || /[\r\n]/.test(provider)) throw new Error("Provider 无效");
    if (typeof model !== "string" || model.length > 120 || /[\r\n]/.test(model)) throw new Error("Model 无效");
    if (!agentStorage || !agentCli) throw new Error("Novel Codex 配置尚未就绪");
    const requestEpoch = ++configurationRequestEpoch;
    const evidence = await loadCodexEvidence(agentStorage.evidencePath);
    const next = resolveCodexProvider({
      credentialScopes: credentialScopesFromEnvironment(process.env, agentStorage.credentialSaltPath),
      evidence,
      provider,
      model,
      qualificationContextDigest: engineQualificationContextDigest ?? "unavailable",
    });
    const configurationError = providerConfigurationError(next, process.env);
    if (configurationError) throw new Error(configurationError);
    const transport = await prepareModelTransport(next);
    const previousBridge = modelBridge;
    const previousAdapter = codexAdapter;
    const nextAdapter = new CodexAppServerAdapter({ command: null, cwd: projectRoot });
    let status;
    try {
      if (requestEpoch !== configurationRequestEpoch) throw new Error("已有更新的模型配置，本次迟到结果不会发布");
      if (activeAgentExecution) throw new Error("模型配置准备期间已有任务启动，已取消本次切换");
      status = applyAgentConfiguration({
        adapter: nextAdapter,
        configuration: {
          apiKey: null,
          command: agentCli.path ? [agentCli.path, "app-server"] : null,
          commandSource: agentCli.source,
          configurationError,
          cwd: agentStorage.workspaceHome,
          env: createCodexEnvironment(process.env, { ...agentStorage, credentialRef: transport.bridge ? null : next.credentialRef, bridgeCredential: transport.bridgeCredential, runtimeUrl, candidateToken: runtimeCandidateToken }),
          model: next.model,
          provider: next.provider,
        },
        persist: () => ensureCodexConfig(agentStorage.codexHome, next, transport.bridge),
      });
    } catch (error) {
      if (transport.instance) await transport.instance.close();
      nextAdapter.close();
      throw error;
    }
    // Single publication point: readers can observe only the old or the new
    // adapter/bridge/plan tuple. Cleanup of the old tuple is deliberately later.
    codexAdapter = nextAdapter;
    modelBridge = transport.instance;
    agentProviderConfig = next;
    previousAdapter.close();
    if (previousBridge) await previousBridge.close();
    return status;
  });
  ipcMain.handle("agent:interrupt", async () => {
    const execution = requestTaskCancellation(activeAgentExecution);
    const task = requestTaskCancellation(execution?.task ?? activeRuntimeTask);
    let interrupted = { interrupted: false };
    const interruptedExecution = execution;
    const interruptedAdapter = execution?.adapter ?? codexAdapter;
    try {
      interrupted = await interruptAgentTurn(interruptedExecution);
    } finally {
      try {
        if (execution?.runId && execution?.projectId) {
          await runtimeRequest(`/api/projects/${encodeURIComponent(execution.projectId)}/planning-reviews/${encodeURIComponent(execution.runId)}/cancel`, { method: "POST", body: "{}" });
        }
        if (task) {
          await runtimeRequest(`/api/projects/${encodeURIComponent(task.projectId)}/tasks/${encodeURIComponent(task.id)}/cancel`, { method: "POST", body: "{}" });
          if (activeRuntimeTask?.id === task.id) activeRuntimeTask = null;
        }
      } finally {
        await interruptedAdapter.finishTaskOperation();
      }
    }
    return { ...interrupted, interrupted: interrupted.interrupted || Boolean(execution) };
  });
  ipcMain.handle("agent:steer", (_event, prompt) => {
    validateAuthorIntent(prompt);
    return codexAdapter.steerTurn(composeNovelCodexPrompt(novelSkillPrompt, prompt));
  });
  ipcMain.handle("agent:list-threads", async (_event, projectId, archived = false, cursor = null) => {
    if (typeof projectId !== "string" || !projectId.trim()) throw new Error("作品 ID 无效");
    if (typeof archived !== "boolean") throw new Error("archived 必须是布尔值");
    if (cursor !== null && typeof cursor !== "string") throw new Error("任务列表 cursor 无效");
    const result = await codexAdapter.listThreads({ archived, cursor });
    if (result.threads.length === 0) return result;
    const { tasks } = await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/tasks/lookup`, {
      method: "POST", body: JSON.stringify({ threadIds: result.threads.map((thread) => thread.id) }),
    });
    const allowed = new Set(tasks.map((task) => task.threadId));
    return { ...result, threads: result.threads.filter((thread) => allowed.has(thread.id)) };
  });
  for (const [channel, restore] of [
    ["agent:resume-thread", (threadId) => codexAdapter.resumeThread(threadId)],
    ["agent:reconnect-thread", (threadId) => codexAdapter.reconnectThread(threadId)],
  ]) {
    ipcMain.handle(channel, async (_event, projectId, threadId) => {
      if (activeAgentExecution) throw new Error("已有创作引擎任务正在运行，请先等待或停止当前任务");
      const execution = createTaskExecution();
      activeAgentExecution = execution;
      try {
        const task = await assertThreadProject(projectId, threadId);
        const purpose = task.purpose;
        if (!["advisory", "planner", "writer", "critic"].includes(purpose)) throw new Error("任务缺少有效的上下文用途");
        const result = await restore(threadId);
        activeAgentProjectId = projectId;
        activeAgentPurpose = purpose;
        activeAgentTargetKey = task.target?.chapterId
          ? `${projectId}:${purpose}:${task.target.chapterId}`
          : `${projectId}:${purpose}`;
        if (purpose === "advisory") advisorySessions.remember(projectId, threadId);
        return result;
      } finally {
        if (activeAgentExecution === execution) activeAgentExecution = null;
      }
    });
  }
  ipcMain.handle("agent:read-thread", async (_event, projectId, threadId) => { await assertThreadProject(projectId, threadId); return codexAdapter.readThread(threadId); });
  ipcMain.handle("agent:review-thread", async (_event, projectId, threadId) => { await assertThreadProject(projectId, threadId); return runCoreTurnWithLease("advisory", () => codexAdapter.reviewThread(threadId)); });
  ipcMain.handle("agent:compact-thread", async (_event, projectId, threadId) => { await assertThreadProject(projectId, threadId); return runCoreTurnWithLease("advisory", () => codexAdapter.compactThread(threadId)); });
  ipcMain.handle("agent:list-background-terminals", async (_event, projectId, threadId) => { await assertThreadProject(projectId, threadId); return codexAdapter.listBackgroundTerminals(threadId); });
  ipcMain.handle("agent:terminate-background-terminal", async (_event, projectId, threadId, processId) => { await assertThreadProject(projectId, threadId); return codexAdapter.terminateBackgroundTerminal(threadId, processId); });
  ipcMain.handle("agent:set-thread-pinned", async (_event, projectId, threadId, isPinned) => {
    if (typeof isPinned !== "boolean") throw new Error("isPinned 必须是布尔值");
    await assertThreadProject(projectId, threadId);
    return codexAdapter.setThreadPinned(threadId, isPinned);
  });
  ipcMain.handle("agent:archive-thread", async (_event, projectId, threadId) => { await assertThreadProject(projectId, threadId); return codexAdapter.archiveThread(threadId); });
  ipcMain.handle("agent:unarchive-thread", async (_event, projectId, threadId) => { await assertThreadProject(projectId, threadId); return codexAdapter.unarchiveThread(threadId); });
  ipcMain.handle("agent:send-prompt", async (event, projectId, prompt) => {
    if (typeof projectId !== "string" || !projectId.trim() || /[\r\n]/.test(projectId)) throw new Error("作品 ID 无效");
    validateAuthorIntent(prompt);
    return runAdvisoryAgentTask(event, projectId, prompt);
  });
  ipcMain.handle("agent:generate-planning", async (event, projectId, operation, intent) => {
    if (typeof projectId !== "string" || !projectId.trim() || /[\r\n]/.test(projectId)) throw new Error("作品 ID 无效");
    const task = planningGenerationTask(operation, intent);
    return runCreativeAgentTask(event, projectId, task.prompt, task.taskType, task.target);
  });
  ipcMain.handle("agent:generate-chapter", async (event, projectId, request) => {
    const task = chapterGenerationTask(projectId, request);
    return runCreativeAgentTask(event, projectId, task.prompt, task.taskType, task.target);
  });
  ipcMain.handle("agent:review-planning", (event, projectId, request) => runPlanningReview(event, projectId, request));
  ipcMain.handle("agent:resolve-user-input", (_event, requestId, response) => codexAdapter.resolveUserInput(requestId, response));
}

function stopRuntime() {
  if (runtimeProcess && !runtimeProcess.killed) runtimeProcess.kill();
  clearAgentCallbacks();
  codexAdapter.close();
  if (modelBridge) void modelBridge.close();
  modelBridge = null;
}

const hasSingleInstanceLock = app.requestSingleInstanceLock();
if (!hasSingleInstanceLock) {
  app.quit();
} else {
app.on("second-instance", () => {
  const window = BrowserWindow.getAllWindows()[0];
  if (!window) return;
  if (window.isMinimized()) window.restore();
  window.focus();
});

app.whenReady().then(async () => {
  bundledRuntime = app.isPackaged ? resolveRuntimeBinaries({ resourcesPath: process.resourcesPath }) : null;
  const storage = prepareIsolatedStorage(app.getPath("userData"));
  const recoveredBudgets = PersistentTaskBudget.recoverOrphans(storage.budgetDb);
  if (recoveredBudgets.length) console.warn(`[novel-codex] recovered ${recoveredBudgets.length} unfinished model budget(s) as unknown usage`);
  recoveryIndex = new RecoveryIndex(storage.recoveryPath);
  recoveryJournal = new HostRecoveryJournal(recoveryIndex);
  codexAdapter.setCallbacks({ onRawEvent: recordAgentEvent });
  novelSkillPrompt = loadNovelCodexSkills(runtimeAssetRoot);
  const enginePromptPath = app.isPackaged
    ? join(runtimeAssetRoot, "codex-runtime", "engine-prompt.md")
    : join(projectRoot, "../../codex-rs/models-manager/prompt.md");
  engineBaseInstructions = readFileSync(enginePromptPath, "utf8").trim();
  if (!engineBaseInstructions) throw new Error("Codex 上游基础指令为空");
  const codexCli = resolveCodexCli({
    explicitPath: process.env.NOVEL_CODEX_CLI,
    isPackaged: app.isPackaged,
    projectRoot,
    resourcesPath: process.resourcesPath,
  });
  engineQualificationContextDigest = codexCli.path
    ? createQualificationContextDigest({ baseInstructions: engineBaseInstructions, cliPath: codexCli.path })
    : null;
  const evidence = await loadCodexEvidence(storage.evidencePath);
  const selectedConnection = inspectCodexConnection({ model: process.env.NOVEL_CODEX_MODEL, provider: process.env.NOVEL_CODEX_PROVIDER });
  let providerConfig = null;
  let capabilityError = null;
  try {
    if (!engineQualificationContextDigest) throw new Error(codexCli.reason ?? "Novel Codex CLI 来源不可验证");
    providerConfig = resolveCodexProvider({
      credentialScopes: credentialScopesFromEnvironment(process.env, storage.credentialSaltPath),
      evidence,
      model: selectedConnection.model,
      provider: selectedConnection.provider,
      qualificationContextDigest: engineQualificationContextDigest,
    });
  } catch (error) {
    capabilityError = error instanceof Error ? error.message : String(error);
  }
  const modelTransport = providerConfig ? await prepareModelTransport(providerConfig, storage) : { bridge: null, bridgeCredential: null, instance: null };
  modelBridge = modelTransport.instance;
  await startRuntime(storage.runtimeDb);
  await reconcileRecoveryIndex(recoveryIndex, runtimeRequest, (projectId, phase, error) => {
    console.error(`[novel-codex] recovery ${phase} reconciliation failed for project ${projectId}: ${error instanceof Error ? error.message : String(error)}`);
  });
  if (providerConfig) ensureCodexConfig(storage.codexHome, providerConfig, modelTransport.bridge);
  agentStorage = storage;
  agentCli = codexCli;
  agentProviderConfig = providerConfig ?? selectedConnection;
  const configurationError = capabilityError ?? providerConfigurationError(selectedConnection, process.env);
  codexAdapter.configure({
    apiKey: null,
    command: codexCli.path ? [codexCli.path, "app-server"] : null,
    commandSource: codexCli.source,
    configurationError,
    cwd: storage.workspaceHome,
    env: createCodexEnvironment(process.env, { ...storage, credentialRef: modelTransport.bridge ? null : selectedConnection.credentialRef, bridgeCredential: modelTransport.bridgeCredential, runtimeUrl, candidateToken: runtimeCandidateToken }),
    model: selectedConnection.model,
    provider: selectedConnection.provider,
  });
  if (!codexCli.path) console.warn(`[novel-codex] ${codexCli.reason}`);
  if (configurationError) console.warn(`[novel-codex] ${configurationError}`);
  registerIpcHandlers();
  if (app.isPackaged && process.env.NOVEL_CODEX_PACKAGE_SMOKE === "1") {
    const health = await runtimeRequest("/health");
    if (health?.ok !== true) throw new Error("最终安装产物无法访问 Creative Runtime");
    console.log("NOVEL_CODEX_PACKAGE_SMOKE_OK");
    allowWindowClose = true;
    app.quit();
    return;
  }
  createWindow();

  app.on("activate", () => {
    if (BrowserWindow.getAllWindows().length === 0) createWindow();
  });
}).catch((error) => {
  console.error(`[novel-codex] startup failed: ${error instanceof Error ? error.message : String(error)}`);
  app.quit();
});

app.on("before-quit", (event) => {
  if (allowWindowClose) return;
  quitRequested = true;
  event.preventDefault();
  const window = BrowserWindow.getAllWindows()[0];
  if (window) safeSendToWebContents(window.webContents, "app:close-requested", {});
  else {
    allowWindowClose = true;
    app.quit();
  }
});

app.on("will-quit", stopRuntime);

app.on("window-all-closed", () => {
  if (process.platform !== "darwin") app.quit();
});
}
