import { app, BrowserWindow, dialog, ipcMain, session } from "electron";
import { spawn } from "node:child_process";
import { randomBytes, randomUUID } from "node:crypto";
import { persistTaskFinalReply } from "./agent-final-reply.mjs";
import { assessQueuedRevision } from "./revision-outcome-followup.mjs";
import { registerRevisionOutcomeHandlers, registerRevisionDecisionHandlers } from "./revision-outcome-handlers.mjs";
import { REVISION_REPAIR_INSTRUCTIONS, advisoryTaskTarget, assertAdvisoryPromptRoute, assertRevisionRepairTaskTarget, preflightRevisionRepair, registerAdvisoryPromptHandler } from "./revision-repair-handlers.mjs";
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
import { authorSteerInput } from "./author-steer.mjs";
import { ModelingConsumer } from "./modeling-consumer.mjs";
import { registerCommitmentHandlers } from "./commitment-handlers.mjs";
import { registerActionPermissionHandlers } from "./action-permission-handlers.mjs";
import { registerExecutionScopeHandlers } from "./execution-scope-handlers.mjs";
import { advanceWorkSteps } from "./work-step-continuation.mjs";
import { bindTaskProviderBudget, reconcileWorkProviderUsage, settleTaskProviderBudget } from "./task-provider-budget.mjs";
import { confirmNewRootBudget } from "./initial-budget-confirmation.mjs";
import { ensureWorkBudgetAuthorization } from "./work-budget-confirmation.mjs";
import { readPlanningReviewContinuation } from "./planning-review-resumption.mjs";
import { registerConditionEvaluationHandlers } from "./condition-evaluation-handlers.mjs";
import { registerGenerationReplanHandlers } from "./generation-replan-handlers.mjs";
import { remainingGenerationDurationMs } from "./generation-duration.mjs";
import { registerChapterRebindHandlers } from "./chapter-rebind-handlers.mjs";
import { assertProviderOperation, buildCodexConfig, buildCodexModelCatalog, composeNovelCodexPrompt, createCodexEnvironment, createQualificationContextDigest, createRuntimeEnvironment, createTaskCapability, credentialScopesFromEnvironment, inspectCodexConnection, installManagedAgentRoles, listCodexConnections, loadCodexEvidence, prepareIsolatedStorage, providerConfigurationError, recordRuntimeQualificationFailure, resolveCodexCli, resolveCodexProvider, resolveRuntimeBinaries, verifyCodexConnection, verifyCodexConnectionEdge } from "./codex-runtime.mjs";
import { chapterGenerationTask, planningGenerationTask } from "./creative-task-contract.mjs";
import { isAllowedRendererNavigation, isTrustedIpcSender } from "./navigation-policy.mjs";
import { reconcileRecoveryIndex } from "./recovery-reconciliation.mjs";
import { safeSendToWebContents } from "./webcontents-safety.mjs";
import { SCOPED_REVISION_INSTRUCTIONS, createTaskBoundHandoff, taskBoundInstructions, taskMethodText, taskExecutionPrompt, STORY_QUERY_INSTRUCTIONS } from "./workflow-handoff.mjs";
import { validateAuthorIntent } from "../shared/author-input-budget.mjs";
import { generationPermitForChapters } from "../shared/generation-permit.mjs";

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
let engineBaseInstructions = "";
let engineQualificationContextDigest = null;
let activeAgentProjectId = null;
let activeAgentPurpose = null;
let activeAgentTargetKey = null;
let activeRuntimeTask = null;
let activeAgentExecution = null;
let advisorySteerInFlight = false;
let advisorySteerCompletion = null;
let activeGenerationRunId = null;
let activeWorkFlow = null;
let recoveryIndex = null;
let recoveryJournal = null;
const advisorySessions = new AdvisorySessions();
let rendererWebContents = null;
let allowWindowClose = false;
let quitRequested = false;
let codexAdapter = new CodexAppServerAdapter({ command: null, cwd: projectRoot });
const modelingConsumer = new ModelingConsumer({
  available: ({ event }) => {
    try { return !quitRequested && !event.sender.isDestroyed() && isTrustedRendererUrl(event.sender.getURL())
      && codexAdapter.enabled && !activeWorkFlow && !activeAgentExecution && !activeGenerationRunId; }
    catch { return false; }
  },
  dispatch: (projectId) => runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/modeling-outbox/dispatch`, { method: "POST", body: "{}" }),
  run: ({ event, projectId }, workTaskId) => resumeWorkTask(event, projectId, workTaskId, { origin: "automatic" }),
  failed: async ({ projectId }, id, error) => {
    if (error?.code === "TASK_CANCELLED") return;
    const path = `/api/projects/${encodeURIComponent(projectId)}/work-tasks/${encodeURIComponent(id)}`;
    const work = await runtimeRequest(path);
    if (work.status === "running") await runtimeRequest(`${path}/control`, { method: "POST", body: JSON.stringify({ expectedVersion: work.version, command: "pause" }) });
    forwardAgentEvent({ method: "agent/error", params: { message: `自动分析已停止：${error.message}` } });
  },
  updated: ({ projectId }) => forwardAgentEvent({ method: "agent/modeling-updated", params: { projectId } }),
});
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
    if (rendererWebContents === window.webContents) { rendererWebContents = null; modelingConsumer.select(null); }
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

function developmentRuntimeBinaries() {
  const suffix = process.platform === "win32" ? ".exe" : "";
  const directory = join(projectRoot, "native", "target", "debug");
  const creativeRuntime = join(directory, `novel-codex-story-runtime${suffix}`);
  const novelWorkspaceMcp = join(directory, `novel-workspace-mcp${suffix}`);
  if (!existsSync(creativeRuntime) || !existsSync(novelWorkspaceMcp)) {
    throw new Error("Rust Story Runtime 尚未构建，请先运行 npm run build:story-runtime");
  }
  return { creativeRuntime, novelWorkspaceMcp };
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

async function startRuntime(runtimeDb, budgetDb) {
  const attempts = configuredRuntimePort === null ? 3 : 1;
  let lastError;
  for (let attempt = 0; attempt < attempts; attempt += 1) {
    runtimePort = configuredRuntimePort ?? await findFreeLoopbackPort();
    runtimeUrl = `http://127.0.0.1:${runtimePort}`;
    runtimeError = null;
    const runtimeCommand = bundledRuntime.creativeRuntime;
    const runtimeArgs = ["--host", "127.0.0.1", "--port", String(runtimePort), "--db", runtimeDb, "--engine-budget-db", budgetDb, "--token-stdin"];
    const child = spawn(runtimeCommand, runtimeArgs, {
      cwd: runtimeAssetRoot,
      env: createRuntimeEnvironment(process.env),
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
    mcpServerCommand: bundledRuntime.novelWorkspaceMcp,
    mcpServerPath: null,
    mcpServerArgs: [],
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

const QUIET_RECOVERY_OPERATIONS = new Set(["read_context", "read_commitment", "read_property_rules", "search_story", "search_relations", "query_story", "read_reading_progress"]);

async function closeFinishedReads(index) {
  if (!index) return [];
  const report = await index.recoveryReport();
  const visible = [];
  for (const run of report) {
    const manual = run.requests.filter((request) => request.recovery === "manual-reconciliation");
    const finished = ["completed", "failed", "cancelled"].includes(run.runtimeStatus);
    const onlyReads = manual.length > 0 && manual.every((request) => QUIET_RECOVERY_OPERATIONS.has(request.operation));
    if (finished && onlyReads) {
      await index.resolveRun(run.runId, "manual-reviewed");
      continue;
    }
    visible.push(run);
  }
  return visible;
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
  if (execution.task && !execution.taskBudget) throw new Error("领域任务缺少持久执行额度，拒绝调用模型");
  const runId = execution.task?.id ?? randomUUID();
  const budgetOptions = (id) => ({ id, ...(execution.taskDeadlineAt == null ? {} : {
    maxDurationMs: Math.max(1, execution.taskDeadlineAt * 1000 - Date.now()),
  }) });
  const runtime = new CodexSessionRuntime({
    appServer: execution.adapter,
    bridge: execution.modelBridge,
    bridgeOwnership: "borrowed",
    closeAppServer: (adapter) => adapter.finishTaskOperation(),
    budgetFactory: async ({ lease }) => {
      if (execution.taskBudget) {
        if (lease.runId !== runId || lease.budgetId !== execution.taskBudget.id) throw new Error("任务预算与引擎租约不匹配");
        execution.taskBudgetBound = true;
        return execution.taskBudget;
      }
      return agentStorage?.budgetDb
        ? new PersistentTaskBudget({ databasePath: agentStorage.budgetDb, runId: lease.runId, ...budgetOptions(lease.budgetId) })
        : new TaskBudget(budgetOptions(lease.budgetId));
    },
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
    const started = await host.startRun({ sessionId, runId, budgetId: execution.taskBudget?.id ?? randomUUID(), mode: execution.operation, input, turnOptions: options,
      ...(execution.taskDeadlineAt == null ? {} : { maxDurationMs: remainingGenerationDurationMs(execution.taskDeadlineAt), deadlineAtMs: execution.taskDeadlineAt * 1000 }) });
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
  if (purpose === "writer") {
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
    prompt: composeNovelCodexPrompt(taskMethodText(task), prompt, taskBoundInstructions(handoff)),
  };
}

async function withWorkFlow(projectId, work) {
  if (activeWorkFlow || activeAgentExecution || activeGenerationRunId) throw new Error("已有创作任务正在推进");
  const flow = { projectId, id: null, runId: null, cancelled: false };
  activeWorkFlow = flow;
  try { return await work(flow); }
  finally { if (activeWorkFlow === flow) activeWorkFlow = null; }
}

async function currentWorkFlowState() {
  const flow = activeWorkFlow;
  if (!flow) return "running";
  if (flow.cancelled) throw Object.assign(new Error("生成已停止"), { code: "TASK_CANCELLED" });
  if (!flow.id) return "running";
  const state = await runtimeRequest(`/api/projects/${encodeURIComponent(flow.projectId)}/work-tasks/${encodeURIComponent(flow.id)}`);
  if (state.status === "cancelled") throw Object.assign(new Error("生成已停止"), { code: "TASK_CANCELLED" });
  return state.status;
}

async function bindWorkFlowAttempt(projectId, taskId) {
  if (!activeWorkFlow) return;
  const work = await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/tasks/${encodeURIComponent(taskId)}/work-task`);
  if (activeWorkFlow.id && activeWorkFlow.id !== work.id) throw new Error("执行尝试不属于当前创作任务");
  activeWorkFlow.id = work.id;
  if (activeWorkFlow.cancelled) {
    await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/work-tasks/${encodeURIComponent(work.id)}/control`, {
      method: "POST", body: JSON.stringify({ expectedVersion: work.version, command: "cancel" }),
    });
    throw Object.assign(new Error("生成已停止"), { code: "TASK_CANCELLED" });
  }
}

async function bindCodexTurn(projectId, task, threadId, result) {
  const turnId = result?.turn?.id ?? result?.turnId;
  if (typeof turnId !== "string" || !turnId || turnId.length > 160) throw new Error("最终回复缺少有效回合绑定");
  await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/tasks/${encodeURIComponent(task.id)}/turn`, {
    method: "POST", body: JSON.stringify({ threadId, turnId }),
  });
  return persistTaskFinalReply(runtimeRequest, projectId, task, result);
}

async function runCreativeAgentTask(event, projectId, prompt, taskType, target = null) {
  if (activeGenerationRunId && target?.runId !== activeGenerationRunId) throw new Error("自动创作运行正在推进，不能并行创建其他模型任务");
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
  let generationClaim = null;
  let taskCompleted = false;
  try {
    assertTaskCompletable(execution);
    const capability = createTaskCapability();
    adapter.prepareTaskOperation(taskType.operation, capability.secret);
    const taskPurpose = taskType.operation === "generateChapter" ? "writer" : ["modelKnowledge", "assessSceneOutcome", "assessRevisionOutcome", "extractCommitments", "reviseCommitment"].includes(taskType.operation) ? "critic" : "planner";
    const targetKey = target?.chapterId ? `${projectId}:${taskPurpose}:${target.chapterId}` : `${projectId}:${taskPurpose}`;
    activeAgentProjectId = projectId;
    activeAgentPurpose = taskPurpose;
    activeAgentTargetKey = targetKey;
    const threadId = await adapter.ensureSession();
    assertTaskCompletable(execution);
    const workspace = await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/creative-workspace`);
    assertTaskCompletable(execution);
    const initialBudget = await confirmNewRootBudget({ target, execution, databasePath: agentStorage?.budgetDb,
      getWindow: () => BrowserWindow.fromWebContents(event.sender),
      showMessageBox: (owner, options) => dialog.showMessageBox(owner, options), workspace, projectId, prompt });
    task = await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/tasks`, {
      method: "POST",
      body: JSON.stringify({
        target,
        initialBudget,
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
    prompt = taskExecutionPrompt(task);
    await bindWorkFlowAttempt(projectId, task.id);
    if (target?.runId) {
      generationClaim = await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/generation-runs/${encodeURIComponent(target.runId)}`);
      if (generationClaim.activeTaskId !== task.id || generationClaim.version !== target.runVersion + 1) {
        throw new Error("生成步骤没有绑定到本次任务");
      }
    }
    await bindTaskProviderBudget(execution, { runtimeRequest, projectId, taskId: task.id,
      generationRun: generationClaim, databasePath: agentStorage?.budgetDb });
    if (initialBudget && !execution.taskBudget) throw new Error("已确认任务缺少持久额度，不能执行");
    if (execution.cancellationRequested) {
      await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/tasks/${encodeURIComponent(task.id)}/cancel`, { method: "POST", body: "{}" });
      throw Object.assign(new Error("生成已停止"), { code: "TASK_CANCELLED" });
    }
    const prepared = await prepareNovelWorkflow(projectId, prompt, task, task.purpose);
    await beginRecoveryRun(task, projectId, threadId);
    assertTaskCompletable(execution);
    const result = await sendAgentTurn(execution, prepared.prompt, { disableEnvironment: true, threadName: prompt });
    assertTaskCompletable(execution);
    const finalReplyReceipt = await bindCodexTurn(projectId, task, threadId, result);
    await recoveryJournal?.flush();
    const completion = await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/tasks/${encodeURIComponent(task.id)}/complete`, {
      method: "POST",
      body: JSON.stringify({ threadId, summary: Array.from(result.message ?? "").slice(0, 4_000).join("") }),
    });
    taskCompleted = true;
    await recoveryJournal?.completeRun();
    if (execution.budgetUsagePath) await runtimeRequest(execution.budgetUsagePath, {
      method: "POST", body: JSON.stringify({ taskId: task.id }),
    });
    const latestRun = generationClaim ? await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/generation-runs/${encodeURIComponent(target.runId)}`) : null;
    const generationRun = latestRun?.status === "running" ? await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/generation-runs/${encodeURIComponent(target.runId)}/advance-task`, {
      method: "POST", body: JSON.stringify({ expectedVersion: latestRun.version, taskId: task.id }),
    }) : latestRun;
    if (activeRuntimeTask?.id === task.id) activeRuntimeTask = null;
    return { ...result, finalReplyReceipt, projectId, workflow: prepared.workflow, completion, generationRun };
  } catch (error) {
    if (!taskCompleted && shouldMarkTaskFailed(task)) {
      try {
        await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/tasks/${encodeURIComponent(task.id)}`, {
          method: "PATCH",
          body: JSON.stringify({ threadId: task.threadId, status: "failed", error: error.message }),
        });
      } catch { /* Preserve the original Agent failure and any saved checkpoints. */ }
    }
    if (execution.budgetUsagePath && task && agentStorage?.budgetDb) {
      if (execution.taskBudget && !execution.taskBudgetBound) {
        try { execution.taskBudget.complete("failed"); } catch { /* A terminal receipt or recovery must decide the outcome. */ }
      }
      try {
        await runtimeRequest(execution.budgetUsagePath, {
          method: "POST", body: JSON.stringify({ taskId: task.id }),
        });
      } catch (chargeError) {
        console.warn(`[novel-codex] provider usage remains unsettled for ${task.id}: ${chargeError.message}`);
      }
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

async function modelChapterKnowledge(event, projectId, chapterId, generationTarget = null, expectedSourceRevisionId = null, sourceTaskId = null) {
  const workspace = await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/creative-workspace`);
  const source = workspace.modeling?.find((item) => item.objectId === `draft-${chapterId}`);
  if (expectedSourceRevisionId && source?.sourceRevisionId !== expectedSourceRevisionId) {
    throw new Error("章节工作稿已变化，知识建模需基于新的版本重新执行");
  }
  if (!source || source.status === "ready") {
    if (source?.status === "ready" && generationTarget?.runId) {
      await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/generation-runs/${encodeURIComponent(generationTarget.runId)}/skip-ready-extract`, {
        method: "POST", body: JSON.stringify({ expectedVersion: generationTarget.runVersion }),
      });
    }
    return { status: source?.status ?? "not_applicable" };
  }
  const batch = await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/modeling-batches`, {
    method: "POST",
    body: JSON.stringify({
      objectId: source.objectId,
      sourceRevisionId: source.sourceRevisionId,
      expectedRevision: workspace.revision,
      extractorVersion: "novel-knowledge-v1",
    }),
  });
  const prompt = "从本次章节工作稿提取可定位到原文的候选故事知识；只处理绑定的源修订，无法确认的内容不输出。";
  await runCreativeAgentTask(event, projectId, prompt, { operation: "modelKnowledge" }, {
    batchId: batch.id,
    ...(sourceTaskId ? { sourceTaskId } : {}),
    ...(generationTarget ? {
      runId: generationTarget.runId,
      stepToken: generationTarget.stepToken,
      runVersion: generationTarget.runVersion,
      intentGeneration: generationTarget.intentGeneration,
      chapterId,
    } : {}),
  });
  return { status: "ready", batchId: batch.id };
}

async function runCandidateWorkflow(event, projectId, prompt, taskType, target = null) {
  const result = await runCreativeAgentTask(event, projectId, prompt, taskType, target);
  try {
    if (await currentWorkFlowState() === "paused") return { ...result, followup: { status: "paused" } };
    const { action } = await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/tasks/${encodeURIComponent(result.workflow.task.id)}/next-step`);
    if (!action) return result;
    if (action.kind !== "modelKnowledge") throw new Error(`未知任务后续步骤：${action.kind}`);
    const modeling = await modelChapterKnowledge(event, projectId, action.chapterId, null, action.sourceRevisionId, action.sourceTaskId);
    if (modeling.status !== "ready") throw new Error("章节知识建模尚未就绪");
    const outcome = await assessQueuedRevision({ event, projectId, writerTask: result.workflow.task,
      runtimeRequest, currentWorkFlowState, runCreativeAgentTask });
    return { ...result, modeling, outcome,
      ...(outcome && ["pending", "paused"].includes(outcome.status) ? { followup: outcome } : {}) };
  } catch (error) {
    if (error?.code === "TASK_CANCELLED") throw error;
    console.warn(`[novel-codex] candidate follow-up remains pending: ${error.message}`);
    const followup = { status: "pending", error: error.message };
    return { ...result, followup, ...(taskType.operation === "generateChapter" ? { modeling: { status: "model_pending", error: error.message } } : {}) };
  }
}

async function resumeWorkTask(event, projectId, workTaskId, { origin = "author" } = {}) {
  if (typeof projectId !== "string" || !projectId.trim() || typeof workTaskId !== "string" || !/^work-[a-zA-Z0-9-]+$/.test(workTaskId)) throw new Error("创作任务标识无效");
  return withWorkFlow(projectId, async (flow) => {
    flow.id = workTaskId;
    const path = `/api/projects/${encodeURIComponent(projectId)}/work-tasks/${encodeURIComponent(workTaskId)}`;
    let work = await runtimeRequest(path);
    if (origin === "automatic" && work.status !== "running") return work;
    if (work.kind === "generation") {
      const runId = workTaskId.slice("work-".length);
      if (work.status === "blocked") {
        await retryGenerationRun(event, projectId, runId);
        return runtimeRequest(path);
      }
      if (work.status === "paused") {
        const run = await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/generation-runs/${encodeURIComponent(runId)}`);
        await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/generation-runs/${encodeURIComponent(runId)}/control`, {
          method: "POST", body: JSON.stringify({ expectedVersion: run.version, command: "resume" }),
        });
      }
      await continueGenerationRun(event, projectId, runId);
      return runtimeRequest(path);
    }
    work = await reconcileWorkProviderUsage(runtimeRequest, projectId, work);
    const isPlanningReview = work.steps.some(step => step.operation === "reviewPlanningSeat");
    if (work.status === "cancelled" || (work.status === "completed" && !isPlanningReview)) return work;
    if (isPlanningReview && work.steps.every(step => step.status === "completed")) {
      if (work.status === "paused") work = await runtimeRequest(`${path}/control`, {
        method: "POST", body: JSON.stringify({ expectedVersion: work.version, command: "resume" }),
      });
      const continuation = await readPlanningReviewContinuation(runtimeRequest, projectId, work);
      if (continuation) await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/planning-reviews/${encodeURIComponent(continuation.run.id)}/finalize`,
        { method: "POST", body: "{}" });
      return runtimeRequest(path);
    }
    if (work.status !== "completed") work = await ensureWorkBudgetAuthorization({ runtimeRequest, projectId, work, origin,
      databasePath: agentStorage?.budgetDb, model: agentProviderConfig?.model,
      getWindow: () => BrowserWindow.fromWebContents(event.sender),
      showMessageBox: (owner, options) => dialog.showMessageBox(owner, options) });
    if (["paused", "blocked"].includes(work.status)) work = await runtimeRequest(`${path}/control`, {
      method: "POST", body: JSON.stringify({ expectedVersion: work.version, command: "resume" }),
    });
    if (isPlanningReview) {
      const continuation = await readPlanningReviewContinuation(runtimeRequest, projectId, work);
      if (continuation?.roles.length) await runPlanningReview(event, projectId, continuation.run, continuation);
      else if (continuation) await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/planning-reviews/${encodeURIComponent(continuation.run.id)}/finalize`,
        { method: "POST", body: "{}" });
      return runtimeRequest(path);
    }
    return advanceWorkSteps(event, projectId, workTaskId, {
      runtimeRequest, currentWorkFlowState, runCreativeAgentTask, runAdvisoryAgentTask,
      runCandidateWorkflow, modelChapterKnowledge,
    });
  });
}

async function continueGenerationRun(event, projectId, runId) {
  if (activeGenerationRunId) throw new Error("已有自动创作运行正在推进");
  activeGenerationRunId = runId;
  if (activeWorkFlow) {
    activeWorkFlow.runId = runId;
    activeWorkFlow.id = `work-${runId}`;
  }
  const runBase = `/api/projects/${encodeURIComponent(projectId)}/generation-runs/${encodeURIComponent(runId)}`;
  try {
    for (let step = 0; step < 40; step += 1) {
      if (await currentWorkFlowState() === "paused") return (await runtimeRequest(runBase));
      const { run, action } = await runtimeRequest(`${runBase}/next`, { method: "POST", body: "{}" });
      if (run.status !== "running" || !action) return run;
      if (action.kind === "awaitTask") {
        if (action.taskStatus !== "completed") return run;
        const charged = await settleGenerationUsage(projectId, run, action.taskId);
        await runtimeRequest(`${runBase}/advance-task`, {
          method: "POST", body: JSON.stringify({ expectedVersion: charged.version, taskId: action.taskId }),
        });
        continue;
      }
      if (action.kind === "materializeChapter") {
        await runtimeRequest(`${runBase}/materialize`, {
          method: "POST", body: JSON.stringify({ expectedVersion: run.version }),
        });
        continue;
      }
      if (action.kind === "checkKnowledge") {
        await runtimeRequest(`${runBase}/check`, {
          method: "POST", body: JSON.stringify({ expectedVersion: run.version }),
        });
        continue;
      }
      if (action.kind !== "agentTask") throw new Error(`未知创作运行步骤：${action.kind}`);
      if (action.operation === "modelKnowledge") {
        const modeled = await modelChapterKnowledge(event, projectId, action.target.chapterId, action.target);
        if (modeled.status !== "ready") throw new Error("章节知识建模未就绪，自动创作已停止");
        continue;
      }
      const instructions = {
        generateBrief: "建立作品创作简报候选",
        generateOutline: "根据创作简报生成全书层级大纲候选",
        generatePlan: `根据本轮作者意图、已完成章节的实际正文、知识检查和场景结果评估，规划第 ${action.target.planFromChapter} 章起的近三章候选。Context Pack 中如有现行 Plan，先核对它与本轮意图和已发生事实是否一致：一致的章节、场景和节拍保留内容及稳定 id，仅改动冲突部分；若方向冲突则重规划，不能因为旧 Plan 结构完整就直接沿用。已有大纲可参考，但不可要求作者先补 Brief 或大纲。每章提供非空 chapterPlanId；每个规划场景与节拍使用稳定的 id；每个场景还需列出非空 expectedEffects，每项包含稳定 id、dimension(world/knowledge/relationship/narrative) 和具体 description、commitmentRefs 数组。未关联承诺时为空；独立引用 {type:commitment,commitmentId,revisionId}，提案引用 {type:proposal,reportRevisionId,proposalId,sourceRevisionId}，从 Context Pack 原样复制，不凭摘要推测 ID。每效果最多4项，全计划最多64项。预期效果是目标，不是已发生事实；未评估的链接不得当作效果已落实`,
        generateChapter: `生成第 ${action.target.chapterNumber} 章完整工作稿。正文 sceneGroups 应使用对应规划场景的 sceneId，便于后续逐场景核对；不得伪造规划来源`,
        extractCommitments: `处理第 ${action.target.chapterNumber} 章完整正文并提取开放叙事承诺提案。先读取 critic 上下文，冻结源正文修订为 ${action.target.sourceRevisionId}；调用 submit_commitment_extraction，payload 包含 chapterId、sourceRevisionId、examinedRanges 和 candidates。examinedRanges 使用 Unicode 字符位置，连续覆盖整章正文，无发现可提交空 candidates，但不得以空处理范围跳过正文。候选只能是 open 状态、open 目标窗口并带本章原文 setupEvidenceRefs，不创建正式承诺，不自动确认或关闭伏笔`,
        assessSceneOutcome: `对第 ${action.target.chapterNumber} 章逐场景、逐 expectedEffect 评估规划与正文实际结果。先读取 critic 上下文中的 scenePlanDraftLinks、boundCommitmentInputs、正文及来源事件证据，再调用 submit_scene_assessment。payload 包含 chapterId、planRevisionId、draftRevisionId、knowledgeBatchId、inputSnapshotId、intentGeneration 和 assessments；来源字段照抄冻结任务目标。每个 sceneId 的评估包含 verdict、rationale、recommendation 和 effectAssessments；effectAssessments 必须覆盖该场景全部预期效果，每项含 effectId、verdict(realized/deviated/insufficientEvidence)、rationale、actualEventRefs: [{modelingBatchId, assertionIndex}]。每个效果还必须显式包含 commitmentAssessments 数组，一一覆盖原规划 commitmentRefs，未绑定时为 []。承诺判断含 reference、referenceState（照抄冻结输入）、outcome(advanced/partiallyFulfilled/fulfilled/contradicted/unchanged/insufficientEvidence)、rationale、actualEventRefs。依据原 fulfillmentCriteria 判断；推进、部分兑现、兑现或矛盾必须有属于该效果的非空事件证据。引用状态不是 current 时只能 insufficientEvidence。realized 至少引用一条该场景内的事件；场景 verdict 是逐项结果的汇总：效果存在 deviated 或承诺存在 contradicted 则 deviated，否则存在 insufficientEvidence 则 insufficientEvidence，否则 realized。不能把“有事件”推断成“效果已实现”。评估只保存候选分析，不得改写 Canon 或正式承诺状态`,
      };
      const instruction = instructions[action.operation];
      if (!instruction) throw new Error(`未知创作任务：${action.operation}`);
      await runCreativeAgentTask(event, projectId, `${instruction}。\n\n作者意图：${action.authorIntent}`, {
        operation: action.operation,
      }, action.target);
    }
    throw new Error("自动创作达到单次推进步数上限；已保存的运行可继续恢复");
  } catch (error) {
    await runtimeRequest(`${runBase}/next`, { method: "POST", body: "{}" }).catch(() => undefined);
    throw error;
  } finally {
    activeGenerationRunId = null;
  }
}

async function settleGenerationUsage(projectId, run, taskId) {
  if (run.providerBudget?.charges?.some(charge => charge.taskId === taskId)) return run;
  return runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/generation-runs/${encodeURIComponent(run.id)}/provider-usage`, {
    method: "POST", body: JSON.stringify({ taskId }),
  });
}

async function reconcileGenerationUsage(projectId, runId) {
  const run = await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/generation-runs/${encodeURIComponent(runId)}`);
  const taskId = run.activeTaskId;
  if (!taskId || run.providerBudget?.charges?.some(charge => charge.taskId === taskId)) return run;
  const task = await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/tasks/${encodeURIComponent(taskId)}`);
  if (!["completed", "failed", "cancelled"].includes(task.status)) return run;
  return settleGenerationUsage(projectId, run, taskId);
}

async function listGenerationRunsWithUsage(projectId) {
  if (typeof projectId !== "string" || !projectId.trim() || projectId.length > 160 || /[\0\r\n]/.test(projectId)) throw new Error("作品 ID 无效");
  const path = `/api/projects/${encodeURIComponent(projectId)}/generation-runs`;
  const { runs } = await runtimeRequest(path);
  const reconciled = [];
  for (const run of runs) {
    if (!["running", "paused", "blocked"].includes(run.status)
      || !run.activeTaskId || run.providerBudget?.charges?.some(charge => charge.taskId === run.activeTaskId)) {
      reconciled.push({ ...run, usageUnsettled: false });
      continue;
    }
    try {
      const task = await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/tasks/${encodeURIComponent(run.activeTaskId)}`);
      if (!["completed", "failed", "cancelled"].includes(task.status)) {
        reconciled.push({ ...run, usageUnsettled: false });
        continue;
      }
      const charged = await settleGenerationUsage(projectId, run, run.activeTaskId);
      reconciled.push({ ...charged, usageUnsettled: false });
    } catch (error) {
      console.warn(`[novel-codex] generation usage remains unsettled for ${run.id}: ${error instanceof Error ? error.message : String(error)}`);
      reconciled.push({ ...run, usageUnsettled: true });
    }
  }
  return { runs: reconciled };
}

async function startGenerationRun(event, projectId, authorIntent, targetChapters, expectedProjectRevision, expectedBaseSnapshotId, expectedSourceDigest, revisionPredecessor) {
  if (!codexAdapter.enabled) throw new Error("Novel Agent Engine 未配置，无法启动自动创作");
  if (typeof projectId !== "string" || !projectId.trim() || /[\r\n]/.test(projectId)) throw new Error("作品 ID 无效");
  validateAuthorIntent(authorIntent);
  const permit = generationPermitForChapters(targetChapters);
  if (!Number.isSafeInteger(expectedProjectRevision) || expectedProjectRevision < 0) throw new Error("创作来源版本无效，请重新读取作品");
  if (typeof expectedBaseSnapshotId !== "string" || !expectedBaseSnapshotId.trim()
    || typeof expectedSourceDigest !== "string" || !/^[a-f0-9]{64}$/.test(expectedSourceDigest)) {
    throw new Error("创作来源快照无效，请重新读取作品");
  }
  if (activeAgentExecution || activeGenerationRunId) throw new Error("已有创作引擎任务正在运行");
  const workspace = await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/creative-workspace`);
  if (workspace.revision !== expectedProjectRevision || workspace.headSnapshotId !== expectedBaseSnapshotId
    || workspace.sourceDigest !== expectedSourceDigest) {
    throw new Error("作品已在确认期间变化，请重新核对创作目标与范围");
  }
  const position = await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/generation-start-position`);
  const firstOperation = {
    brief: "generateBrief",
    outline: "generateOutline",
    rollingPlan: "generatePlan",
  }[position.firstStage];
  if (!firstOperation) throw new Error("创作运行起点无效，请检查作品状态");
  assertProviderOperation(agentProviderConfig, firstOperation);
  const run = await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/generation-runs`, {
    method: "POST", body: JSON.stringify({
      expectedProjectRevision,
      expectedBaseSnapshotId,
      expectedSourceDigest,
      authorIntent,
      revisionPredecessor,
      targetChapters,
      maxTaskAttempts: permit.maxTaskAttempts,
      maxProviderRequests: permit.maxProviderRequests,
      maxProviderTokens: permit.maxProviderTokens,
      deadlineAt: Math.floor(Date.now() / 1000) + permit.maxDurationMinutes * 60,
    }),
  });
  if (activeWorkFlow?.cancelled) {
    const path = `/api/projects/${encodeURIComponent(projectId)}/generation-runs/${encodeURIComponent(run.id)}`;
    await runtimeRequest(`${path}/control`, { method: "POST", body: JSON.stringify({ expectedVersion: run.version, command: "cancel" }) });
    throw Object.assign(new Error("生成已停止"), { code: "TASK_CANCELLED" });
  }
  return continueGenerationRun(event, projectId, run.id);
}

async function retryGenerationRun(event, projectId, runId) {
  if (!codexAdapter.enabled) throw new Error("Novel Agent Engine 未配置，无法重试创作");
  if (typeof projectId !== "string" || !projectId.trim() || /[\r\n]/.test(projectId)) throw new Error("作品 ID 无效");
  if (typeof runId !== "string" || !/^generation-[a-f0-9]{32}$/.test(runId)) throw new Error("创作运行 ID 无效");
  if (activeAgentExecution || activeGenerationRunId) throw new Error("已有创作引擎任务正在运行");
  const runBase = `/api/projects/${encodeURIComponent(projectId)}/generation-runs/${encodeURIComponent(runId)}`;
  let run = await runtimeRequest(runBase);
  if (run.activeTaskId) run = await settleGenerationUsage(projectId, run, run.activeTaskId);
  await runtimeRequest(`${runBase}/retry-failed-task`, {
    method: "POST", body: JSON.stringify({ expectedVersion: run.version }),
  });
  return continueGenerationRun(event, projectId, runId);
}

async function submitGenerationIntent(projectId, runId, instruction, endpoint) {
  if (typeof projectId !== "string" || !projectId.trim() || /[\r\n]/.test(projectId)) throw new Error("作品 ID 无效");
  if (typeof runId !== "string" || !/^generation-[a-f0-9]{32}$/.test(runId)) throw new Error("创作运行 ID 无效");
  if (typeof instruction !== "string" || !instruction.trim() || Array.from(instruction.trim()).length > 1_000) throw new Error("后续创作要求必须为 1 到 1000 字");
  if ((activeGenerationRunId && activeGenerationRunId !== runId)
    || (activeAgentExecution && activeGenerationRunId !== runId)) {
    throw new Error("已有其他创作任务正在运行，不能调整此运行的后续要求");
  }
  const runBase = `/api/projects/${encodeURIComponent(projectId)}/generation-runs/${encodeURIComponent(runId)}`;
  const run = await runtimeRequest(runBase);
  if (activeGenerationRunId === runId && !run.activeTaskId) {
    throw new Error("当前创作步骤正在切换，请稍后再调整后续要求");
  }
  return runtimeRequest(`${runBase}/${endpoint}`, {
    method: "POST", body: JSON.stringify({ expectedVersion: run.version, instruction: instruction.trim() }),
  });
}

async function runPlanningReview(event, projectId, request, continuation = null) {
  if (activeGenerationRunId) throw new Error("自动创作运行正在推进，不能并行评审");
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
    run = continuation?.run ?? await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/planning-reviews`, {
      method: "POST",
      body: JSON.stringify(request),
    });
    execution.runId = run.id;
    const reviewWork = await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/work-tasks/work-${encodeURIComponent(run.id)}`);
    if (activeWorkFlow) activeWorkFlow.id = reviewWork.id;
    if (reviewWork.status !== "completed") await ensureWorkBudgetAuthorization({ runtimeRequest, projectId, work: reviewWork, origin: "author",
      databasePath: agentStorage?.budgetDb, model: execution.providerConfig?.model,
      getWindow: () => BrowserWindow.fromWebContents(event.sender),
      showMessageBox: (owner, options) => dialog.showMessageBox(owner, options) });
    assertTaskCompletable(execution);
    for (const role of continuation?.roles ?? run.roles) {
      if (execution.cancellationRequested) break;
      if (await currentWorkFlowState() === "paused") return { projectId,
        run: await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/planning-reviews/${encodeURIComponent(run.id)}`) };
      let task = null;
      let taskCompleted = false;
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
        await bindTaskProviderBudget(execution, { runtimeRequest, projectId, taskId: task.id,
          databasePath: agentStorage?.budgetDb });
        if (!execution.taskBudget) throw new Error("规划深审席位缺少持久额度，不能执行");
        if (execution.cancellationRequested) {
          await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/tasks/${encodeURIComponent(task.id)}/cancel`, { method: "POST", body: "{}" });
          throw Object.assign(new Error("规划深审已停止"), { code: "TASK_CANCELLED" });
        }
        const workspace = await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/creative-workspace`);
        const handoff = createTaskBoundHandoff(projectId, workspace, task);
        adapter.emit({ method: "agent/workflow", params: { workflow: { task, planningReviewId: run.id, role, revision: workspace.revision } } });
        const prompt = composeNovelCodexPrompt(taskMethodText(task), taskExecutionPrompt(task), `你是规划深审中的独立 ${role} 席。先按以下绑定调用 read_context；不得读取其他席报告，不得修改任何规划、正文或正式事实。按冻结的 checks 逐项检查，允许没有问题或无法判断。然后调用 submit_planning_review_report 保存结构化报告，最后调用 complete_task。\n${JSON.stringify(handoff, null, 2)}`);
        await beginRecoveryRun(task, projectId, threadId);
        assertTaskCompletable(execution);
        const result = await sendAgentTurn(execution, prompt, { disableEnvironment: true, threadName: `规划深审 · ${role}` });
        assertTaskCompletable(execution);
        await bindCodexTurn(projectId, task, threadId, result);
        await recoveryJournal?.flush();
        await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/tasks/${encodeURIComponent(task.id)}/complete`, { method: "POST", body: JSON.stringify({ threadId, summary: Array.from(result.message ?? "").slice(0, 4_000).join("") }) });
        taskCompleted = true;
        await recoveryJournal?.completeRun();
        await settleTaskProviderBudget(execution, runtimeRequest, task.id);
      } catch (error) {
        if (!taskCompleted && shouldMarkTaskFailed(task)) {
          try { await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/tasks/${encodeURIComponent(task.id)}`, { method: "PATCH", body: JSON.stringify({ threadId: task.threadId, status: "failed", error: error.message }) }); } catch { /* Preserve reports from other seats. */ }
        }
        if (task) {
          try { await settleTaskProviderBudget(execution, runtimeRequest, task.id); }
          catch (chargeError) { console.warn(`[novel-codex] review usage remains unsettled for ${task.id}: ${chargeError.message}`); }
        }
        throw error;
      } finally {
        await recoveryJournal?.abandonRun().catch(() => undefined);
        if (activeRuntimeTask?.id === task?.id) activeRuntimeTask = null;
        await adapter.finishTaskOperation();
        execution.task = null;
        execution.taskBudget = null;
        execution.taskBudgetBound = false;
        execution.taskDeadlineAt = null;
        execution.budgetUsagePath = null;
      }
    }
    if (execution.cancellationRequested) {
      await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/planning-reviews/${encodeURIComponent(run.id)}/cancel`, { method: "POST", body: "{}" });
      throw Object.assign(new Error("规划深审已停止；已保存报告仍会保留"), { code: "TASK_CANCELLED" });
    }
    const finalized = await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/planning-reviews/${encodeURIComponent(run.id)}/finalize`, { method: "POST", body: "{}" });
    return { projectId, run: finalized };
  } catch (error) {
    if (run && (execution.cancellationRequested || error?.code === "TASK_CANCELLED")) {
      try { await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/planning-reviews/${encodeURIComponent(run.id)}/cancel`, { method: "POST", body: "{}" }); }
      catch (cancelError) { console.warn(`[novel-codex] review cancellation remains pending: ${cancelError.message}`); }
    }
    throw error;
  } finally {
    await finishAgentExecution(execution);
  }
}

async function runAdvisoryAgentTask(event, projectId, prompt, resumeStep = null, revisionRepairSource = null) {
  const target = advisoryTaskTarget({ resumeStep, revisionRepairSource });
  if (activeGenerationRunId) throw new Error("自动创作运行正在推进，请稍后讨论");
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
  let taskCompleted = false;
  try {
    await preflightRevisionRepair({ runtimeRequest, projectId, source: target.revisionRepairSource ?? null });
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
    const initialBudget = await confirmNewRootBudget({ target,
      execution, databasePath: agentStorage?.budgetDb,
      getWindow: () => BrowserWindow.fromWebContents(event.sender),
      showMessageBox: (owner, options) => dialog.showMessageBox(owner, options), workspace, projectId, prompt });
    task = await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/tasks`, {
      method: "POST",
      body: JSON.stringify({ target, initialBudget, inputRevision: workspace.revision, threadId, model: execution.providerConfig?.model ?? null, operation: "advisory", authorIntent: prompt, capabilityHash: capability.hash, capabilityExpiresAt: capability.expiresAt }),
    });
    assertRevisionRepairTaskTarget(task, projectId, target);
    task = bindExecutionTask(execution, task);
    activeRuntimeTask = task;
    prompt = taskExecutionPrompt(task);
    await bindWorkFlowAttempt(projectId, task.id);
    await bindTaskProviderBudget(execution, { runtimeRequest, projectId, taskId: task.id,
      databasePath: agentStorage?.budgetDb });
    if (initialBudget && !execution.taskBudget) throw new Error("已确认任务缺少持久额度，不能执行");
    if (execution.cancellationRequested) {
      await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/tasks/${encodeURIComponent(task.id)}/cancel`, { method: "POST", body: "{}" });
      throw Object.assign(new Error("生成已停止"), { code: "TASK_CANCELLED" });
    }
    const handoff = createTaskBoundHandoff(projectId, workspace, task);
    adapter.emit({ method: "agent/workflow", params: { workflow: { task, revision: workspace.revision,
      headSnapshotId: workspace.headSnapshotId, sourceDigest: workspace.sourceDigest } } });
    const extractionInstructions = "若作者明确要求提取并保存叙事承诺候选，可调用 request_candidate_task，operation=extractCommitments，target 包含 chapterId、chapterNumber、当前 sourceRevisionId。先以检索返回的源修订核对正文，不能使用已被工作稿替换的版本。提取批次只是开放承诺提案，不会确认或关闭已有伏笔。若明确要求调整已有承诺，先 search_story(kind=commitment) 定位，再 read_commitment 读取完整对象，request_candidate_task(operation=reviseCommitment) 的 target 包含 commitmentId 和读取到的 revisionId。名字歧义先询问作者，不猜目标；只能生成这个承诺的候选修订，不创建、确认或自动关闭承诺。若作者明确要求生成或调整属性结构及事件条件规则，request_candidate_task(operation=generatePropertyRules,target={type:artifact}) 登记有限规则候选任务；模型不能直接采用、确认事实或执行规则说明/例外，合法的条件表达式也不证明条件成立。仅讨论规则不登记修改。";
    const repairInstructions = target.revisionRepairSource ? REVISION_REPAIR_INSTRUCTIONS : "";
    const advisoryPrompt = composeNovelCodexPrompt(taskMethodText(task), prompt, `${STORY_QUERY_INSTRUCTIONS}\n\n${target.revisionRepairSource ? "" : extractionInstructions}\n\n${SCOPED_REVISION_INSTRUCTIONS}\n\n作者意图代次为 0；若调用 request_candidate_task，必须显式传 expectedIntentRevision=0。收到追加指令后，只能使用其注明的最新代次，不能沿用旧代次。\n\n先通过 novel_workspace.read_context 读取本次任务的冻结上下文，调用时原样传入以下绑定中的 taskId、projectId、threadId、expectedRevision，并使用 purpose=planner、budget=24、byteBudget=60000：\n${JSON.stringify(handoff, null, 2)}\n\n根据作者的问题自主规划证据检索：query_story 可在同一版本联合检索正文、精确名称身份候选、时间状态和关系；search_story 可补充正文分页。查询计划只使用工具支持的有限操作，名称歧义先澄清再选择身份，不能把不完整空结果解释为不存在。区分已确认事实、工作稿和推测，并引用来源。分析和预测只作为对话回复。${target.revisionRepairSource ? repairInstructions : "若作者明确要求修改作品，可调用 request_candidate_task 指定规划或已有章节的精确目标、操作和作者要求；它只登记后续候选任务，不会直接修改作品。"}若作者要求续写尚不存在的下一章，不要用 request_candidate_task(generateChapter) 虚构章节 ID 或创建占位；说明作者可在章节区将本轮原始指令带入续写许可确认，核对目标、范围和预算后由 GenerationRun 建章。一次对话最多登记三个候选任务。不得把提问、假设或预测当成修改指令。解释证据与变更目标；不要声称候选已采用。`);
    await beginRecoveryRun(task, projectId, threadId);
    assertTaskCompletable(execution);
    const result = await sendAgentTurn(execution, advisoryPrompt, { disableEnvironment: true, threadName: prompt });
    if (advisorySteerCompletion) await advisorySteerCompletion;
    assertTaskCompletable(execution);
    const finalReplyReceipt = await bindCodexTurn(projectId, task, threadId, result);
    await recoveryJournal?.flush();
    const completion = await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/tasks/${encodeURIComponent(task.id)}/complete`, { method: "POST", body: JSON.stringify({ threadId, summary: Array.from(result.message ?? "").slice(0, 4_000).join("") }) });
    taskCompleted = true;
    await recoveryJournal?.completeRun();
    if (execution.budgetUsagePath) await runtimeRequest(execution.budgetUsagePath, {
      method: "POST", body: JSON.stringify({ taskId: task.id }),
    });
    if (activeRuntimeTask?.id === task.id) activeRuntimeTask = null;
    return { ...result, finalReplyReceipt, projectId, workflow: { task, revision: workspace.revision,
      headSnapshotId: workspace.headSnapshotId, sourceDigest: workspace.sourceDigest }, completion };
  } catch (error) {
    if (!taskCompleted && shouldMarkTaskFailed(task)) {
      try { await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/tasks/${encodeURIComponent(task.id)}`, { method: "PATCH", body: JSON.stringify({ threadId: task.threadId, status: "failed", error: error.message }) }); } catch { /* Preserve the original advisory failure. */ }
    }
    if (execution.budgetUsagePath && task && agentStorage?.budgetDb) {
      if (execution.taskBudget && !execution.taskBudgetBound) {
        try { execution.taskBudget.complete("failed"); } catch { /* Recovery must reconcile an existing receipt. */ }
      }
      try {
        await runtimeRequest(execution.budgetUsagePath, { method: "POST", body: JSON.stringify({ taskId: task.id }) });
      } catch (chargeError) {
        console.warn(`[novel-codex] provider usage remains unsettled for ${task.id}: ${chargeError.message}`);
      }
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
  registerActionPermissionHandlers(ipcMain, runtimeRequest);
  registerRevisionOutcomeHandlers(ipcMain, runtimeRequest);
  registerRevisionDecisionHandlers(ipcMain, runtimeRequest);
  registerExecutionScopeHandlers(ipcMain, {
    runtimeRequest, withWorkFlow,
    assertOperationsAvailable: (operations) => {
      if (!codexAdapter.enabled) throw new Error("创作引擎未配置，请先选择可用模型；范围仍待确认");
      for (const operation of operations) assertProviderOperation(agentProviderConfig, operation);
    },
    advance: (event, projectId, workTaskId) => advanceWorkSteps(event, projectId, workTaskId, {
      runtimeRequest, currentWorkFlowState, runCreativeAgentTask, runAdvisoryAgentTask,
      runCandidateWorkflow, modelChapterKnowledge,
    }),
  });
  registerConditionEvaluationHandlers(ipcMain, runtimeRequest);
  registerGenerationReplanHandlers(ipcMain, runtimeRequest, reconcileGenerationUsage);
  registerChapterRebindHandlers(ipcMain, runtimeRequest);
  registerCommitmentHandlers(ipcMain, runtimeRequest);
  ipcMain.handle("agent:set-modeling-context", (event, context) => {
    if (!context || typeof context.dirty !== "boolean" || (context.projectId !== null && (typeof context.projectId !== "string" || !context.projectId.trim() || context.projectId.length > 160))) throw new Error("自动分析上下文无效");
    modelingConsumer.select(context.projectId === null ? null : { ...context, event });
    return { selected: context.projectId };
  });
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
  ipcMain.handle("runtime:list-proposal-sets", (_event, projectId, cursor = null) => {
    if (typeof projectId !== "string" || !projectId.trim() || (cursor !== null && (typeof cursor !== "string" || !/^[1-9][0-9]{0,18}$/.test(cursor)))) throw new Error("提案列表范围无效");
    const suffix = cursor === null ? "" : `?cursor=${encodeURIComponent(cursor)}`;
    return runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/proposal-sets${suffix}`);
  });
  ipcMain.handle("runtime:create-proposal-set", (_event, projectId, request) => {
    if (typeof projectId !== "string" || !projectId.trim() || !request || typeof request !== "object"
      || typeof request.baseSnapshotId !== "string" || !request.baseSnapshotId.trim()
      || typeof request.intent !== "string" || !request.intent.trim() || request.intent.length > 1000
      || !Array.isArray(request.operations) || request.operations.length < 2 || request.operations.length > 20
      || typeof request.idempotencyKey !== "string" || !request.idempotencyKey.trim()) throw new Error("跨对象提案参数无效");
    return runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/proposal-sets`, { method: "POST", body: JSON.stringify(request) });
  });
  ipcMain.handle("runtime:get-proposal-set", (_event, projectId, proposalSetId) => {
    if (typeof projectId !== "string" || !projectId.trim() || typeof proposalSetId !== "string" || !/^proposal-set-[a-f0-9]{32}$/.test(proposalSetId)) throw new Error("提案编号无效");
    return runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/proposal-sets/${proposalSetId}`);
  });
  ipcMain.handle("runtime:preview-proposal-set", (_event, projectId, proposalSetId, selectedOperationIds, selectionsByOperationId) => {
    if (typeof projectId !== "string" || !projectId.trim() || typeof proposalSetId !== "string" || !/^proposal-set-[a-f0-9]{32}$/.test(proposalSetId)
      || !Array.isArray(selectedOperationIds) || selectedOperationIds.length > 20 || selectedOperationIds.some((id) => typeof id !== "string" || !id.trim())
      || !selectionsByOperationId || typeof selectionsByOperationId !== "object" || Array.isArray(selectionsByOperationId)
      || JSON.stringify(selectionsByOperationId).length > 32_000) throw new Error("提案选择无效");
    return runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/proposal-sets/${proposalSetId}/preview`, { method: "POST", body: JSON.stringify({ selectedOperationIds, selectionsByOperationId }) });
  });
  ipcMain.handle("runtime:apply-proposal-set", (_event, projectId, proposalSetId, request) => {
    if (typeof projectId !== "string" || !projectId.trim() || typeof proposalSetId !== "string" || !/^proposal-set-[a-f0-9]{32}$/.test(proposalSetId)
      || !request || typeof request !== "object" || !Array.isArray(request.selectedOperationIds) || request.selectedOperationIds.length < 1 || request.selectedOperationIds.length > 20
      || !request.selectionsByOperationId || typeof request.selectionsByOperationId !== "object" || Array.isArray(request.selectionsByOperationId)
      || typeof request.idempotencyKey !== "string" || !request.idempotencyKey.trim() || JSON.stringify(request).length > 32_000) throw new Error("整组采用参数无效");
    return runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/proposal-sets/${proposalSetId}/apply`, { method: "POST", body: JSON.stringify(request) });
  });
  ipcMain.handle("runtime:undo-proposal-set", (_event, projectId, applicationId, expectedProjectRevision, idempotencyKey) => {
    if (typeof projectId !== "string" || !projectId.trim() || typeof applicationId !== "string" || !/^proposal-application-[a-f0-9]{32}$/.test(applicationId)
      || !Number.isInteger(expectedProjectRevision) || expectedProjectRevision < 0 || typeof idempotencyKey !== "string" || !idempotencyKey.trim()) throw new Error("整组撤销参数无效");
    return runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/proposal-applications/${applicationId}/undo`, { method: "POST", body: JSON.stringify({ expectedProjectRevision, idempotencyKey }) });
  });
  ipcMain.handle("runtime:get-impact-analysis", (_event, projectId, options) => {
    if (typeof projectId !== "string" || !projectId.trim() || !options || typeof options !== "object") throw new Error("影响分析目标无效");
    const { entityId, fromRevisionId = null, toRevisionId = null, maxHops = 3, limit = 25 } = options;
    if (typeof entityId !== "string" || !entityId.trim() || entityId.length > 160) throw new Error("影响分析实体无效");
    if ((fromRevisionId === null) !== (toRevisionId === null)
      || [fromRevisionId, toRevisionId].some((id) => id !== null && (typeof id !== "string" || !id.trim() || id.length > 160))) throw new Error("修订版本必须成对提供");
    if (!Number.isInteger(maxHops) || maxHops < 1 || maxHops > 3 || !Number.isInteger(limit) || limit < 1 || limit > 50) throw new Error("影响分析范围无效");
    const params = new URLSearchParams({ entityId: entityId.trim(), maxHops: String(maxHops), limit: String(limit) });
    if (fromRevisionId !== null) {
      params.set("fromRevisionId", fromRevisionId);
      params.set("toRevisionId", toRevisionId);
    }
    return runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/impact-analysis?${params}`);
  });
  ipcMain.handle("runtime:get-historical-knowledge-state", (_event, projectId, snapshotId) => {
    if (typeof projectId !== "string" || !projectId.trim() || typeof snapshotId !== "string" || !snapshotId.trim() || snapshotId.length > 160) throw new Error("历史知识范围无效");
    return runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/knowledge/state?perspective=author&snapshotId=${encodeURIComponent(snapshotId)}`);
  });
  ipcMain.handle("runtime:create-hypothesis", (_event, projectId, request) => {
    if (typeof projectId !== "string" || !projectId.trim() || !request || typeof request !== "object"
      || typeof request.baseSnapshotId !== "string" || !request.baseSnapshotId.trim()
      || typeof request.intent !== "string" || !request.intent.trim() || request.intent.length > 1000
      || !Array.isArray(request.assumptions) || request.assumptions.length < 1 || request.assumptions.length > 25
      || typeof request.idempotencyKey !== "string" || !request.idempotencyKey.trim()) throw new Error("假设条件无效");
    return runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/hypotheses`, { method: "POST", body: JSON.stringify(request) });
  });
  ipcMain.handle("runtime:get-hypothesis", (_event, projectId, hypothesisId) => {
    if (typeof projectId !== "string" || !projectId.trim() || typeof hypothesisId !== "string" || !/^hypothesis-[a-f0-9]{32}$/.test(hypothesisId)) throw new Error("假设编号无效");
    return runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/hypotheses/${encodeURIComponent(hypothesisId)}`);
  });
  ipcMain.handle("runtime:get-identity-catalog", (_event, projectId) => runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/knowledge/entities`));
  ipcMain.handle("runtime:save-identity-decisions", (_event, projectId, decisions, expectedRevision, expectedCandidateRevisionId = null) => {
    if (!Array.isArray(decisions) || decisions.length > 256 || !Number.isInteger(expectedRevision) || expectedRevision < 0) throw new Error("身份决议参数无效");
    if (expectedCandidateRevisionId !== null && (typeof expectedCandidateRevisionId !== "string" || !expectedCandidateRevisionId.trim())) throw new Error("身份候选版本无效");
    return runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/artifacts/identity`, { method: "POST", body: JSON.stringify({ payload: { decisions }, expectedRevision, expectedCandidateRevisionId, idempotencyKey: `desktop-identity-${randomUUID()}` }) });
  });
  ipcMain.handle("runtime:save-property-rules", (_event, projectId, request) => {
    if (typeof projectId !== "string" || !projectId.trim() || !request || !Array.isArray(request.payload?.definitions)
      || !Array.isArray(request.payload.eventRules) || request.payload.eventRules.length > 16 || Object.keys(request.payload).length !== 2
      || request.payload.definitions.length > 64 || Buffer.byteLength(JSON.stringify(request.payload)) > 16 * 1024
      || !Number.isSafeInteger(request.expectedRevision) || request.expectedRevision < 0
      || typeof request.idempotencyKey !== "string" || !request.idempotencyKey.trim() || request.idempotencyKey.length > 160
      || (request.expectedCandidateRevisionId !== null && (typeof request.expectedCandidateRevisionId !== "string" || !request.expectedCandidateRevisionId.trim()))) throw new Error("属性规则参数无效");
    return runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/artifacts/propertyRules`, { method: "POST", body: JSON.stringify(request) });
  });
  ipcMain.handle("runtime:list-generation-runs", (_event, projectId) => listGenerationRunsWithUsage(projectId));
  ipcMain.handle("runtime:get-scene-assessment", (_event, projectId, chapterId) => {
    if (typeof projectId !== "string" || !projectId.trim() || typeof chapterId !== "string" || !/^chapter-[a-f0-9]{32}$/.test(chapterId)) throw new Error("场景评估目标无效");
    return runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/chapters/${encodeURIComponent(chapterId)}/scene-assessments`);
  });
  ipcMain.handle("runtime:list-work-tasks", (_event, projectId, cursor = null) => {
    if (typeof projectId !== "string" || !projectId.trim()) throw new Error("作品标识无效");
    if (cursor !== null && (typeof cursor !== "string" || !/^[01]:[1-9][0-9]{0,18}:[0-9]{1,19}:[0-9]{1,19}$/.test(cursor))) throw new Error("任务分页标识无效");
    const page = cursor === null ? "" : `?cursor=${encodeURIComponent(cursor)}`;
    return runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/work-tasks${page}`);
  });
  ipcMain.handle("runtime:get-search-audit", (_event, projectId) => {
    if (typeof projectId !== "string" || !projectId.trim()) throw new Error("作品标识无效");
    return runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/search-audits`);
  });
  ipcMain.handle("runtime:start-search-audit", (_event, projectId) => {
    if (typeof projectId !== "string" || !projectId.trim()) throw new Error("作品标识无效");
    return runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/search-audits`, { method: "POST", body: "{}" });
  });
  ipcMain.handle("runtime:control-search-audit", (_event, projectId, jobId, command, expectedVersion) => {
    if (typeof projectId !== "string" || !projectId.trim() || typeof jobId !== "string" || !/^search-audit-[a-f0-9]{32}$/.test(jobId) || !["pause", "resume", "cancel"].includes(command) || !Number.isInteger(expectedVersion) || expectedVersion < 0) throw new Error("索引校验控制参数无效");
    return runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/search-audits/${encodeURIComponent(jobId)}/control`, { method: "POST", body: JSON.stringify({ command, expectedVersion }) });
  });
  ipcMain.handle("runtime:get-entity-audit", (_event, projectId) => {
    if (typeof projectId !== "string" || !projectId.trim()) throw new Error("作品标识无效");
    return runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/entity-audits`);
  });
  ipcMain.handle("runtime:start-entity-audit", (_event, projectId) => {
    if (typeof projectId !== "string" || !projectId.trim()) throw new Error("作品标识无效");
    return runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/entity-audits`, { method: "POST", body: "{}" });
  });
  ipcMain.handle("runtime:control-entity-audit", (_event, projectId, jobId, command, expectedVersion) => {
    if (typeof projectId !== "string" || !projectId.trim() || typeof jobId !== "string" || !/^entity-audit-[a-f0-9]{32}$/.test(jobId) || !["pause", "resume", "cancel"].includes(command) || !Number.isInteger(expectedVersion) || expectedVersion < 0) throw new Error("实体索引校验控制参数无效");
    return runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/entity-audits/${encodeURIComponent(jobId)}/control`, { method: "POST", body: JSON.stringify({ command, expectedVersion }) });
  });
  ipcMain.handle("runtime:get-relation-audit", (_event, projectId) => {
    if (typeof projectId !== "string" || !projectId.trim()) throw new Error("作品标识无效");
    return runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/relation-audits`);
  });
  ipcMain.handle("runtime:start-relation-audit", (_event, projectId) => {
    if (typeof projectId !== "string" || !projectId.trim()) throw new Error("作品标识无效");
    return runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/relation-audits`, { method: "POST", body: "{}" });
  });
  ipcMain.handle("runtime:control-relation-audit", (_event, projectId, jobId, command, expectedVersion) => {
    if (typeof projectId !== "string" || !projectId.trim() || typeof jobId !== "string" || !/^relation-audit-[a-f0-9]{32}$/.test(jobId) || !["pause", "resume", "cancel"].includes(command) || !Number.isInteger(expectedVersion) || expectedVersion < 0) throw new Error("关系索引校验控制参数无效");
    return runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/relation-audits/${encodeURIComponent(jobId)}/control`, { method: "POST", body: JSON.stringify({ command, expectedVersion }) });
  });
  ipcMain.handle("runtime:get-knowledge-audit", (_event, projectId) => {
    if (typeof projectId !== "string" || !projectId.trim()) throw new Error("作品标识无效");
    return runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/knowledge-audits`);
  });
  ipcMain.handle("runtime:start-knowledge-audit", (_event, projectId) => {
    if (typeof projectId !== "string" || !projectId.trim()) throw new Error("作品标识无效");
    return runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/knowledge-audits`, { method: "POST", body: "{}" });
  });
  ipcMain.handle("runtime:control-knowledge-audit", (_event, projectId, jobId, command, expectedVersion) => {
    if (typeof projectId !== "string" || !projectId.trim() || typeof jobId !== "string" || !/^knowledge-audit-[a-f0-9]{32}$/.test(jobId) || !["pause", "resume", "cancel"].includes(command) || !Number.isInteger(expectedVersion) || expectedVersion < 0) throw new Error("知识索引校验控制参数无效");
    return runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/knowledge-audits/${encodeURIComponent(jobId)}/control`, { method: "POST", body: JSON.stringify({ command, expectedVersion }) });
  });
  ipcMain.handle("runtime:get-snapshot-knowledge-audit", (_event, projectId) => {
    if (typeof projectId !== "string" || !projectId.trim()) throw new Error("作品标识无效");
    return runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/snapshot-knowledge-audits`);
  });
  ipcMain.handle("runtime:start-snapshot-knowledge-audit", (_event, projectId) => {
    if (typeof projectId !== "string" || !projectId.trim()) throw new Error("作品标识无效");
    return runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/snapshot-knowledge-audits`, { method: "POST", body: "{}" });
  });
  ipcMain.handle("runtime:control-snapshot-knowledge-audit", (_event, projectId, jobId, command, expectedVersion) => {
    if (typeof projectId !== "string" || !projectId.trim() || typeof jobId !== "string" || !/^snapshot-knowledge-audit-[a-f0-9]{32}$/.test(jobId) || !["pause", "resume", "cancel"].includes(command) || !Number.isInteger(expectedVersion) || expectedVersion < 0) throw new Error("历史知识校验控制参数无效");
    return runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/snapshot-knowledge-audits/${encodeURIComponent(jobId)}/control`, { method: "POST", body: JSON.stringify({ command, expectedVersion }) });
  });
  ipcMain.handle("runtime:get-fact-audit", (_event, projectId) => {
    if (typeof projectId !== "string" || !projectId.trim()) throw new Error("作品标识无效");
    return runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/fact-audits`);
  });
  ipcMain.handle("runtime:start-fact-audit", (_event, projectId) => {
    if (typeof projectId !== "string" || !projectId.trim()) throw new Error("作品标识无效");
    return runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/fact-audits`, { method: "POST", body: "{}" });
  });
  ipcMain.handle("runtime:control-fact-audit", (_event, projectId, jobId, command, expectedVersion) => {
    if (typeof projectId !== "string" || !projectId.trim() || typeof jobId !== "string" || !/^fact-audit-[a-f0-9]{32}$/.test(jobId) || !["pause", "resume", "cancel"].includes(command) || !Number.isInteger(expectedVersion) || expectedVersion < 0) throw new Error("确认事实校验控制参数无效");
    return runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/fact-audits/${encodeURIComponent(jobId)}/control`, { method: "POST", body: JSON.stringify({ command, expectedVersion }) });
  });
  ipcMain.handle("runtime:get-work-task", (_event, projectId, workTaskId) => runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/work-tasks/${encodeURIComponent(workTaskId)}`));
  ipcMain.handle("runtime:get-task-final-reply", (_event, projectId, taskId) => {
    if (typeof projectId !== "string" || !projectId.trim() || projectId.length > 160
      || typeof taskId !== "string" || !taskId.trim() || taskId.length > 160) throw new Error("最终回复标识无效");
    return runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/tasks/${encodeURIComponent(taskId)}/final-reply`);
  });
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
    const params = new URLSearchParams({ purpose: "writer", projectRevision: String(projectRevision), readingChapter: String(readingChapter), sceneOrder: String(sceneOrder), budget: "24", byteBudget: "60000" });
    if (povCharacterId !== null) params.set("povCharacterId", povCharacterId);
    if (storyTime !== null) params.set("storyTime", String(storyTime));
    return runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/context?${params}`);
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
  ipcMain.handle("runtime:create-review", (_event, projectId, artifactKind, revisionId, proposals = [], selections = {}, modelingBatchId = null) => {
    if (!["brief", "outline", "plan", "draft", "identity", "propertyRules"].includes(artifactKind)) throw new Error("审阅产物类型无效");
    if (typeof revisionId !== "string" || !revisionId.trim()) throw new Error("修订 ID 无效");
    if (!Array.isArray(proposals) || proposals.length > 200 || selections === null || typeof selections !== "object" || Array.isArray(selections)) throw new Error("审阅内容无效");
    if (modelingBatchId !== null && (artifactKind !== "draft" || typeof modelingBatchId !== "string" || !modelingBatchId.trim() || proposals.length > 0)) throw new Error("知识建模审阅参数无效");
    if (artifactKind === "identity" && (proposals.length > 0 || Object.keys(selections).length > 0)) throw new Error("身份决议审阅不能附带事实或选择");
    if (artifactKind === "propertyRules" && (proposals.length > 0 || Object.keys(selections).length > 0 || modelingBatchId !== null)) throw new Error("属性规则审阅不能附带事实或选择");
    return runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/reviews`, { method: "POST", body: JSON.stringify({ artifactKind, revisionId, proposals, selections, modelingBatchId, idempotencyKey: `desktop-review-${randomUUID()}` }) });
  });
  ipcMain.handle("runtime:reject-identity-review", (_event, projectId, reviewId, expectedRevision) => {
    if (typeof reviewId !== "string" || !reviewId.trim() || !Number.isInteger(expectedRevision) || expectedRevision < 0) throw new Error("身份决议审阅参数无效");
    return runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/reviews/${encodeURIComponent(reviewId)}/reject-identity`, { method: "POST", body: JSON.stringify({ expectedRevision }) });
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
  ipcMain.handle("runtime:search", (_event, projectId, query, limit = 20, kind = null, cursor = null) => {
    if (typeof projectId !== "string" || !projectId.trim()) throw new Error("作品 ID 无效");
    if (typeof query !== "string" || query.length > 240) throw new Error("搜索内容无效");
    if (!Number.isInteger(limit) || limit < 1 || limit > 50) throw new Error("搜索数量无效");
    if (kind !== null && typeof kind !== "string") throw new Error("搜索类型无效");
    if (cursor !== null && (typeof cursor !== "string" || cursor.length > 256)) throw new Error("搜索游标无效");
    const params = new URLSearchParams({ q: query, limit: String(limit) });
    if (kind) params.set("kind", kind);
    if (cursor) params.set("cursor", cursor);
    return runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/search?${params}`);
  });
  ipcMain.handle("agent:status", () => codexAdapter.status());
  ipcMain.handle("agent:recovery-report", () => closeFinishedReads(recoveryIndex));
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
    const flow = activeWorkFlow;
    if (flow) {
      flow.cancelled = true;
      if (flow.runId) {
        const path = `/api/projects/${encodeURIComponent(flow.projectId)}/generation-runs/${encodeURIComponent(flow.runId)}`;
        const run = await runtimeRequest(path);
        if (["running", "paused", "blocked"].includes(run.status)) await runtimeRequest(`${path}/control`, {
          method: "POST", body: JSON.stringify({ expectedVersion: run.version, command: "cancel" }),
        });
      } else if (flow.id) {
        const path = `/api/projects/${encodeURIComponent(flow.projectId)}/work-tasks/${encodeURIComponent(flow.id)}`;
        const task = await runtimeRequest(path);
        if (["running", "paused", "blocked"].includes(task.status)) await runtimeRequest(`${path}/control`, {
          method: "POST", body: JSON.stringify({ expectedVersion: task.version, command: "cancel" }),
        });
      }
    }
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
    return { ...interrupted, interrupted: interrupted.interrupted || Boolean(execution) || Boolean(flow) };
  });
  ipcMain.handle("agent:steer", async (_event, projectId, prompt, revisionRepairSource = null) => {
    assertAdvisoryPromptRoute({ steer: true, revisionRepairSource });
    validateAuthorIntent(prompt);
    if (typeof projectId !== "string" || !projectId.trim()) throw new Error("作品 ID 无效");
    const execution = activeAgentExecution;
    const task = activeRuntimeTask;
    if (!execution || !task || execution.task?.id !== task.id || execution.operation !== "advisory"
      || execution.cancellationRequested || task.operation !== "advisory"
      || projectId !== task.projectId || projectId !== activeAgentProjectId || advisorySteerInFlight
      || !execution.adapter?.turnWaiter || !execution.adapter?.turnId) {
      throw new Error("当前作品没有可追加指令的对话任务");
    }
    const path = `/api/projects/${encodeURIComponent(projectId)}/tasks/${encodeURIComponent(task.id)}/intents`;
    // Validate the entire model message before creating a pending author intent.
    authorSteerInput(task, { taskId: task.id, authorMessageId: "x".repeat(160), intentRevision: 16, status: "pending", authorText: prompt });
    advisorySteerInFlight = true;
    let finishSteer;
    advisorySteerCompletion = new Promise((resolve) => { finishSteer = resolve; });
    let pending = null;
    try {
      const { intents } = await runtimeRequest(path);
      if (activeAgentExecution !== execution || activeRuntimeTask?.id !== task.id || execution.cancellationRequested) throw new Error("对话任务已变化");
      const expectedIntentRevision = intents.at(-1)?.intentRevision ?? 0;
      pending = await runtimeRequest(path, { method: "POST", body: JSON.stringify({ expectedIntentRevision, authorText: prompt }) });
      if (pending.intentRevision !== expectedIntentRevision + 1 || pending.authorText !== prompt) throw new Error("作者追加消息回执不匹配");
      const result = await execution.adapter.steerTurn(authorSteerInput(task, pending));
      if (!result.steered) throw new Error("当前轮次尚未准备好接收追加指令");
      const delivered = await runtimeRequest(`${path}/${pending.intentRevision}/delivered`, { method: "POST", body: "{}" });
      if (delivered.authorMessageId !== pending.authorMessageId || delivered.taskId !== task.id
        || delivered.intentRevision !== pending.intentRevision || delivered.authorText !== pending.authorText
        || delivered.status !== "delivered") throw new Error("作者追加消息交付回执不匹配");
      return result;
    } catch (error) {
      if (pending) {
        requestTaskCancellation(execution);
        requestTaskCancellation(task);
        try { await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/tasks/${encodeURIComponent(task.id)}/cancel`, { method: "POST", body: "{}" }); } catch { /* Pending intent remains unwritable. */ }
        try { await interruptAgentTurn(execution); } catch { /* Revoked task remains unwritable. */ }
      }
      throw error;
    } finally {
      advisorySteerInFlight = false;
      advisorySteerCompletion = null;
      finishSteer();
    }
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
  registerAdvisoryPromptHandler(ipcMain, { withWorkFlow, runtimeRequest, runAdvisoryAgentTask,
    routingState: () => ({ activeWorkFlow, activeAgentExecution, activeGenerationRunId }) });
  ipcMain.handle("agent:list-proposed-actions", async (_event, projectId) => {
    if (typeof projectId !== "string" || !projectId.trim() || /[\r\n]/.test(projectId)) throw new Error("作品 ID 无效");
    return runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/task-actions/proposed`);
  });
  ipcMain.handle("agent:approve-action", (event, projectId, sourceTaskId, actionId) => withWorkFlow(projectId, async () => {
    if (typeof projectId !== "string" || !projectId.trim() || /[\r\n]/.test(projectId)
      || typeof sourceTaskId !== "string" || !/^task-[a-f0-9]{32}$/.test(sourceTaskId)
      || typeof actionId !== "string" || !/^action-[a-f0-9]{32}$/.test(actionId)) throw new Error("候选动作身份无效");
    const path = `/api/projects/${encodeURIComponent(projectId)}/tasks/${encodeURIComponent(sourceTaskId)}/actions`;
    const { actions } = await runtimeRequest(path);
    const proposed = actions.find((action) => action.id === actionId && action.sourceTaskId === sourceTaskId && action.status === "proposed");
    if (!proposed) throw new Error("这项候选动作已失效或已处理，请刷新后重试");
    if (!codexAdapter.enabled) throw new Error("创作引擎未配置，候选建议仍待确认；配置模型后可重试");
    assertProviderOperation(agentProviderConfig, proposed.operation);
    const action = await runtimeRequest(`${path}/${encodeURIComponent(actionId)}/approve`, {
      method: "POST", body: JSON.stringify({ expectedIntentRevision: proposed.intentRevision }),
    });
    try {
      const target = { ...action.target, requestedByTaskId: sourceTaskId, requestedActionId: action.id };
      const result = await runCandidateWorkflow(event, projectId, action.instruction, { operation: action.operation }, target);
      await runtimeRequest(`${path}/${encodeURIComponent(action.id)}/finish`, {
        method: "POST", body: JSON.stringify({ resultTaskId: result.workflow.task.id }),
      });
      return { action, taskId: result.workflow.task.id, modeling: result.modeling ?? null,
        outcome: result.outcome ?? null, followup: result.followup ?? null };
    } catch (error) {
      await runtimeRequest(`${path}/${encodeURIComponent(action.id)}/finish`, {
        method: "POST", body: JSON.stringify({ error: String(error?.message ?? "候选任务失败").slice(0, 400) }),
      }).catch(() => undefined);
      throw error;
    }
  }));
  ipcMain.handle("agent:decline-action", async (_event, projectId, sourceTaskId, actionId) => {
    if (typeof projectId !== "string" || !projectId.trim() || /[\r\n]/.test(projectId)
      || typeof sourceTaskId !== "string" || !/^task-[a-f0-9]{32}$/.test(sourceTaskId)
      || typeof actionId !== "string" || !/^action-[a-f0-9]{32}$/.test(actionId)) throw new Error("候选动作身份无效");
    const path = `/api/projects/${encodeURIComponent(projectId)}/tasks/${encodeURIComponent(sourceTaskId)}/actions`;
    const { actions } = await runtimeRequest(path);
    const proposed = actions.find((action) => action.id === actionId && action.sourceTaskId === sourceTaskId && action.status === "proposed");
    if (!proposed) throw new Error("这项候选动作已失效或已处理，请刷新后重试");
    return runtimeRequest(`${path}/${encodeURIComponent(actionId)}/decline`, {
      method: "POST", body: JSON.stringify({ expectedIntentRevision: proposed.intentRevision }),
    });
  });
  ipcMain.handle("agent:generate-planning", (event, projectId, operation, intent) => withWorkFlow(projectId, async () => {
    if (typeof projectId !== "string" || !projectId.trim() || /[\r\n]/.test(projectId)) throw new Error("作品 ID 无效");
    const task = planningGenerationTask(operation, intent);
    return runCandidateWorkflow(event, projectId, task.prompt, task.taskType, task.target);
  }));
  ipcMain.handle("agent:generate-work", (event, projectId, intent, targetChapters, expectedProjectRevision, expectedBaseSnapshotId, expectedSourceDigest, revisionPredecessor) =>
    withWorkFlow(projectId, () => startGenerationRun(event, projectId, intent, targetChapters,
      expectedProjectRevision, expectedBaseSnapshotId, expectedSourceDigest, revisionPredecessor)));
  ipcMain.handle("agent:continue-work", (event, projectId, runId) =>
    withWorkFlow(projectId, () => continueGenerationRun(event, projectId, runId)));
  ipcMain.handle("agent:retry-work", (event, projectId, runId) =>
    withWorkFlow(projectId, () => retryGenerationRun(event, projectId, runId)));
  ipcMain.handle("agent:resolve-generation-feedback", (event, projectId, runId, assessmentRevisionId, choice, instruction = "") =>
    withWorkFlow(projectId, async () => {
      if (typeof projectId !== "string" || !projectId.trim() || typeof runId !== "string" || !/^generation-[a-f0-9]{32}$/.test(runId)
        || typeof assessmentRevisionId !== "string" || !assessmentRevisionId.trim()
        || !["continue", "repairDraft"].includes(choice) || typeof instruction !== "string" || Array.from(instruction).length > 1000) throw new Error("场景反馈处理参数无效");
      if (activeAgentExecution || activeGenerationRunId) throw new Error("已有创作引擎任务正在运行");
      const runBase = `/api/projects/${encodeURIComponent(projectId)}/generation-runs/${encodeURIComponent(runId)}`;
      const run = await runtimeRequest(runBase);
      await runtimeRequest(`${runBase}/resolve-feedback`, { method: "POST", body: JSON.stringify({
        expectedVersion: run.version, assessmentRevisionId, choice, instruction,
      }) });
      return continueGenerationRun(event, projectId, runId);
    }));
  ipcMain.handle("agent:resume-work-task", (event, projectId, workTaskId) => resumeWorkTask(event, projectId, workTaskId));
  ipcMain.handle("agent:control-work-task", async (_event, projectId, workTaskId, command) => {
    if (typeof projectId !== "string" || typeof workTaskId !== "string" || !/^work-[a-zA-Z0-9-]+$/.test(workTaskId) || !["pause", "cancel"].includes(command)) throw new Error("创作任务控制参数无效");
    const path = `/api/projects/${encodeURIComponent(projectId)}/work-tasks/${encodeURIComponent(workTaskId)}`;
    const work = await runtimeRequest(path);
    let updated;
    if (work.kind === "generation") {
      const runId = workTaskId.slice("work-".length);
      const runPath = `/api/projects/${encodeURIComponent(projectId)}/generation-runs/${encodeURIComponent(runId)}`;
      const run = await runtimeRequest(runPath);
      await runtimeRequest(`${runPath}/control`, { method: "POST", body: JSON.stringify({ expectedVersion: run.version, command }) });
      updated = await runtimeRequest(path);
    } else {
      updated = await runtimeRequest(`${path}/control`, { method: "POST", body: JSON.stringify({ expectedVersion: work.version, command }) });
    }
    if (command === "cancel" && activeWorkFlow?.id === workTaskId && activeWorkFlow.projectId === projectId) {
      activeWorkFlow.cancelled = true;
      const execution = requestTaskCancellation(activeAgentExecution);
      const task = requestTaskCancellation(execution?.task ?? activeRuntimeTask);
      try {
        await interruptAgentTurn(execution);
      } finally {
        if (task?.projectId === projectId) {
          await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/tasks/${encodeURIComponent(task.id)}/cancel`, { method: "POST", body: "{}" });
          if (activeRuntimeTask?.id === task.id) activeRuntimeTask = null;
        }
      }
    }
    return updated;
  });
  ipcMain.handle("agent:add-future-intent", (_event, projectId, runId, instruction) =>
    submitGenerationIntent(projectId, runId, instruction, "future-intents"));
  ipcMain.handle("agent:revise-current-chapter", (_event, projectId, runId, instruction) =>
    submitGenerationIntent(projectId, runId, instruction, "current-chapter-revision"));
  ipcMain.handle("agent:generate-chapter", (event, projectId, request) => withWorkFlow(projectId, async () => {
    const task = chapterGenerationTask(projectId, request);
    return runCandidateWorkflow(event, projectId, task.prompt, task.taskType, task.target);
  }));
  ipcMain.handle("agent:resume-knowledge", async (event, projectId, chapterId) => {
    if (typeof projectId !== "string" || !projectId.trim() || /[\r\n]/.test(projectId)
      || typeof chapterId !== "string" || !chapterId.trim() || /[\r\n]/.test(chapterId)) throw new Error("章节目标无效");
    if (!codexAdapter.enabled) throw new Error("Novel Agent Engine 未配置，无法继续知识建模");
    if (activeAgentExecution || activeGenerationRunId) throw new Error("已有创作引擎任务正在运行");
    const { action } = await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/chapters/${encodeURIComponent(chapterId)}/next-step`);
    if (!action) return { status: "ready", skipped: true };
    if (action.kind !== "modelKnowledge" || action.chapterId !== chapterId) throw new Error("章节后续步骤无效");
    if (action.sourceTaskId) {
      const work = await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/tasks/${encodeURIComponent(action.sourceTaskId)}/work-task`);
      if (work.status === "blocked" && work.steps.some((step) => step.operation === "modelKnowledge" && step.target?.sourceTaskId === action.sourceTaskId && step.status === "blocked")) {
        return resumeWorkTask(event, projectId, work.id);
      }
    }
    return withWorkFlow(projectId, () => modelChapterKnowledge(event, projectId, chapterId, null, action.sourceRevisionId, action.sourceTaskId));
  });
  ipcMain.handle("agent:review-planning", (event, projectId, request) =>
    withWorkFlow(projectId, () => runPlanningReview(event, projectId, request)));
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
  bundledRuntime = app.isPackaged ? resolveRuntimeBinaries({ resourcesPath: process.resourcesPath }) : developmentRuntimeBinaries();
  const storage = prepareIsolatedStorage(app.getPath("userData"));
  const recoveredBudgets = PersistentTaskBudget.recoverOrphans(storage.budgetDb);
  if (recoveredBudgets.length) console.warn(`[novel-codex] recovered ${recoveredBudgets.length} unfinished model budget(s) as unknown usage`);
  recoveryIndex = new RecoveryIndex(storage.recoveryPath);
  recoveryJournal = new HostRecoveryJournal(recoveryIndex);
  codexAdapter.setCallbacks({ onRawEvent: recordAgentEvent });
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
  await startRuntime(storage.runtimeDb, storage.budgetDb);
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
  modelingConsumer.start();

  app.on("activate", () => {
    if (BrowserWindow.getAllWindows().length === 0) createWindow();
  });
}).catch((error) => {
  console.error(`[novel-codex] startup failed: ${error instanceof Error ? error.message : String(error)}`);
  app.quit();
});

app.on("before-quit", (event) => {
  if (allowWindowClose) { modelingConsumer.stop(); return; }
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
