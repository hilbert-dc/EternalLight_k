import { executorValidatedTools } from "./executor-validation-policy.mjs";
import { enforceWorkDeliveryProtocol } from "./work-delivery-protocol.mjs";
import { registerWorkCatalog } from "./work-catalog.mjs";
import { registerChapterDeletion } from "./chapter-deletion.mjs";
import { ChapterStructureAnalysisConsumer, registerChapterStructureReadHandlers } from "./chapter-structure-analysis.mjs";
import { registerQualityNotebookHandlers } from "./quality-notebook.mjs";
import { bindUnsentStructureTask } from "./unsent-structure-task.mjs";
import { composeAdvisoryPrompt } from "./advisory-prompt.mjs";
import { structureDiscussionThread } from "./structure-discussion.mjs";
import { verifyDiscussionReference } from "./discussion-reference.mjs";
import { isDeepStrictEqual } from "node:util";
import { app, BrowserWindow, dialog, ipcMain, session } from "electron";
import { spawn } from "node:child_process";
import { randomBytes, randomUUID } from "node:crypto";
import { persistTaskFinalReply } from "./agent-final-reply.mjs";
import { bindConversationFocus, validateConversationFocus, conversationFocusInstructions, executeCandidateAction } from "./conversation-analysis.mjs";
import { registerRevisionOutcomeHandlers, registerRevisionDecisionHandlers } from "./revision-outcome-handlers.mjs";
import { advisoryTaskTarget, assertAdvisoryPromptRoute, assertRevisionRepairTaskTarget, preflightRevisionRepair, registerAdvisoryPromptHandler } from "./revision-repair-handlers.mjs";
import { existsSync, readFileSync, renameSync, writeFileSync } from "node:fs";
import { createServer } from "node:net";
import { isAbsolute, join } from "node:path";
import { fileURLToPath } from "node:url";

import { AgentEngineHost, CodexSessionRuntime, HostRecoveryJournal, ModelBridgeProcess, PersistentTaskBudget, RecoveryIndex, TaskBudget, assertTaskCompletable, bindExecutionTask, createTaskExecution, requestTaskCancellation, shouldMarkTaskFailed } from "@eternallight/agent-engine";
import { CodexAppServerAdapter } from "./codex-adapter.mjs";
import { terminateEngineProcess } from "./engine-process-lifecycle.mjs";
import { waitForRuntimeReady } from "./runtime-readiness.mjs";
import { CodexConformanceRunner } from "./codex-conformance-runner.mjs";
import { CodexCoreIntegrationRunner } from "./codex-core-integration-runner.mjs";
import { applyAgentConfiguration } from "./agent-configuration.mjs";
import { ModelConnectionGate } from "./model-connection-gate.mjs";
import { assertAgentAvailability, readAgentStatus } from "./agent-availability.mjs";
import { registerConversationHandlers, readConversation, conversationSummary } from "./conversation-handlers.mjs";
import { NOVEL_ROOT_DEVELOPER_INSTRUCTIONS } from "./creative-mission.mjs";
import { authorSteerInput } from "./author-steer.mjs";
import { listModelConnections, prepareEvidenceReverification, readModelEvidenceCache } from "./model-evidence-cache.mjs";
import { registerCommitmentHandlers } from "./commitment-handlers.mjs";
import { registerActionPermissionHandlers } from "./action-permission-handlers.mjs";
import { registerExecutionScopeHandlers } from "./execution-scope-handlers.mjs";
import { advanceWorkSteps } from "./work-step-continuation.mjs";
import { recoverUnstartedWorkResume } from "./work-resume-not-started.mjs";
import { readWorkExecutionBinding } from "./work-execution-binding.mjs";
import { bindTaskProviderBudget, reconcileWorkProviderUsage, settleTaskProviderBudget } from "./task-provider-budget.mjs";
import { grantNewRootBudget } from "./task-budget-grant.mjs";
import { ensureWorkBudgetAuthorization } from "./work-budget-authorization.mjs";
import { executeConversationPlanningReviews, registerPlanningReviewReadHandler } from "./conversation-planning-reviews.mjs";
import { readPlanningReviewContinuation } from "./planning-review-resumption.mjs";
import { registerConditionEvaluationHandlers } from "./condition-evaluation-handlers.mjs";
import { registerGenerationReplanHandlers } from "./generation-replan-handlers.mjs";
import { remainingGenerationDurationMs } from "./generation-duration.mjs";
import { maintainExecutionAuthority } from "./execution-authority-renewal.mjs";
import { renewTaskAuthority } from "./task-capability-renewal.mjs";
import { presentAuthorHistory } from "./thread-history-presentation.mjs";
import { registerChapterRebindHandlers } from "./chapter-rebind-handlers.mjs";
import { NOVEL_SUBAGENT_DEVELOPER_INSTRUCTIONS, assertProviderOperation, buildCodexConfig, buildCodexModelCatalog, composeNovelCodexPrompt, createCodexEnvironment, createQualificationContextDigest, createRuntimeEnvironment, createTaskCapability, credentialScopesFromEnvironment, inspectCodexConnection, installManagedAgentRoles, listCodexConnections, loadCodexEvidence, prepareIsolatedStorage, providerConfigurationError, recordRuntimeQualificationFailure, resolveCodexCli, resolveCodexProvider, resolveRuntimeBinaries, verifyCodexConnection, verifyCodexConnectionEdge } from "./codex-runtime.mjs";
import { chapterGenerationTask, planningGenerationTask } from "./creative-task-contract.mjs";
import { isAllowedRendererNavigation, isTrustedIpcSender } from "./navigation-policy.mjs";
import { reconcileRecoveryIndex } from "./recovery-reconciliation.mjs";
import { safeSendToWebContents } from "./webcontents-safety.mjs";
import { createTaskBoundHandoff, taskBoundInstructions, taskMethodText, taskExecutionPrompt } from "./workflow-handoff.mjs";
import { validateAuthorIntent } from "../shared/author-input-budget.mjs";
import { generationPermitForChapters } from "../shared/generation-permit.mjs";
import { AuthorSessionStore, registerAuthorSessionHandlers } from "./author-session-store.mjs";

const __dirname = fileURLToPath(new URL(".", import.meta.url));
app.setName("小说工坊");
const configuredUserDataDir = process.env.NOVEL_CODEX_USER_DATA_DIR ?? null;
if (configuredUserDataDir !== null) {
  if (!isAbsolute(configuredUserDataDir) || configuredUserDataDir.includes("\0")) throw new Error("NOVEL_CODEX_USER_DATA_DIR 必须是绝对路径");
  app.setPath("userData", configuredUserDataDir);
} else {
  app.setPath("userData", join(app.getPath("appData"), app.getName()));
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
let recoveryRefreshPromise = null;
const modelConnectionGate = new ModelConnectionGate(() => Boolean(activeAgentExecution || activeWorkFlow || activeGenerationRunId));
let advisorySteerInFlight = false;
let advisorySteerCompletion = null;
let activeGenerationRunId = null;
let activeWorkFlow = null;
let recoveryIndex = null;
let recoveryJournal = null;
let rendererWebContents = null;
let allowWindowClose = false;
let quitRequested = false;
let codexAdapter = new CodexAppServerAdapter({ command: null, cwd: projectRoot });
const chapterStructureConsumer = new ChapterStructureAnalysisConsumer({
  available: ({ event }) => {
    try {
      assertTrustedIpcSender(event);
      assertProviderOperation(agentProviderConfig, "assessChapterStructure");
      return !quitRequested && codexAdapter.enabled && !modelConnectionGate.busy
        && !activeWorkFlow && !activeAgentExecution && !activeGenerationRunId;
    } catch { return false; }
  },
  list: (projectId, { cursor, limit, assessmentState }) => {
    const query = new URLSearchParams({ limit: String(limit) });
    if (cursor !== null) query.set("cursor", cursor);
    if (assessmentState !== null) query.set("assessmentState", assessmentState);
    return runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/chapter-structure-changes?${query}`);
  },
  read: (projectId, id) => runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/chapter-structure-changes/${encodeURIComponent(id)}`),
  run: ({ event, projectId }, prompt, taskType, target, isCurrent) => withWorkFlow(projectId, () =>
    runCreativeAgentTask(event, projectId, prompt, taskType, target, { assertCurrent: () => {
      if (!isCurrent()) throw new Error("作品或编辑状态已变化，衔接检查尚未继续。");
    } })),
  updated: ({ projectId }, change, result) => forwardAgentEvent({ method: "agent/structure-updated", params: { projectId, changeId: change.id, completed: result !== null } }),
  failed: async ({ projectId }, change, error) => {
    if (error?.code === "TASK_CANCELLED") {
      forwardAgentEvent({ method: "agent/structure-updated", params: { projectId, changeId: change?.id ?? null, completed: false } });
      return;
    }
    // Creation may have committed even when its response was lost. Stop the
    // exact bound Work so it cannot remain visibly "running" without a model.
    if (change?.assessment.state === "bound") {
      try {
        const path = `/api/projects/${encodeURIComponent(projectId)}/work-tasks/${encodeURIComponent(change.assessment.workTaskId)}`;
        const work = await runtimeRequest(path);
        if (work.status === "running") await runtimeRequest(`${path}/control`, {
          method: "POST", body: JSON.stringify({ expectedVersion: work.version, command: "pause" }),
        });
      } catch { /* Keep the original error and bound task visible for recovery. */ }
    }
    forwardAgentEvent({ method: "structure/error", params: {
      projectId, changeId: change?.id ?? null, taskId: change?.assessment.taskId ?? null, message: `衔接检查未完成：${error.message}`,
    } });
  },
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
    title: app.getName(),
    webPreferences: {
      preload: join(__dirname, "preload.cjs"),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
    },
  });

  const webContents = window.webContents;
  rendererWebContents = webContents;
  window.on("closed", () => {
    if (!quitRequested) allowWindowClose = false;
  });
  window.on("close", (event) => {
    if (allowWindowClose || window.webContents.isDestroyed()) return;
    event.preventDefault();
    safeSendToWebContents(window.webContents, "app:close-requested", {});
  });
  webContents.once("destroyed", () => {
    if (rendererWebContents === webContents) { rendererWebContents = null; chapterStructureConsumer.select(null); }
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
      await terminateEngineProcess(child);
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
    executorValidatedTools,
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
  return assertAgentAvailability(async () => {
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
  });
}

async function beginAgentExecution(execution, operation) {
  while (recoveryRefreshPromise) await recoveryRefreshPromise;
  if (activeAgentExecution && activeAgentExecution !== execution) throw new Error("已有创作引擎任务正在运行，请先等待或停止当前任务");
  modelConnectionGate.assertExecutionAllowed();
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

const QUIET_RECOVERY_OPERATIONS = new Set(["read_context", "read_commitment", "read_task_source", "read_work_status", "search_story", "search_relations", "query_story", "read_reading_progress", "read_chapter_structure_source", "analyze_story_impact", "read_quality_notebook", "read_quality_review", "read_quality_packet"]);

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

function refreshRecoveryReport() {
  if (recoveryRefreshPromise) return recoveryRefreshPromise;
  // Active executions own their journal and task capability. Fencing them
  // would interrupt the work, so report reads stay passive until the host is idle.
  if (activeAgentExecution || activeWorkFlow || activeGenerationRunId) return closeFinishedReads(recoveryIndex);
  recoveryRefreshPromise = (async () => {
    if (recoveryIndex) await reconcileRecoveryIndex(recoveryIndex, runtimeRequest, (projectId, phase, error) => {
      console.error(`[novel-codex] recovery refresh failed (${projectId}/${phase}): ${error instanceof Error ? error.message : String(error)}`);
    });
    return closeFinishedReads(recoveryIndex);
  })().finally(() => { recoveryRefreshPromise = null; });
  return recoveryRefreshPromise;
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
  let renewal = null;
  try {
    await host.startSession({ sessionPlan: execution.sessionPlan, sessionId });
    const turnOptions = execution.task?.inputManifest?.workDecision && !options?.dispatch
      ? { ...options, dispatch: adapter => enforceWorkDeliveryProtocol(adapter, input, options, execution.task,
        path => runtimeRequest(path, path.includes("/work-decision?") ? { headers: {
          Authorization: `Bearer ${runtimeCandidateToken}`, "x-novel-task-capability": execution.capability.secret,
        } } : {}), () => assertTaskCompletable(execution)) }
      : options;
    const started = await host.startRun({ sessionId, runId, budgetId: execution.taskBudget?.id ?? randomUUID(), mode: execution.operation, input, turnOptions,
      ...(execution.taskDeadlineAt == null ? {} : { maxDurationMs: remainingGenerationDurationMs(execution.taskDeadlineAt), deadlineAtMs: execution.taskDeadlineAt * 1000 }) });
    renewal = maintainExecutionAuthority({ capability: execution.task ? execution.capability : null,
      sessionPlan: execution.sessionPlan,
      isCurrent: () => activeAgentExecution === execution && !execution.cancellationRequested && execution.engineRunId === runId,
      renewTask: expectedExpiresAt => renewTaskAuthority({ execution, expectedExpiresAt, runtimeRequest }),
      renewPlan: async (previous, minimumEvidenceValidityMs) => {
        assertTaskCompletable(execution);
        const evidence = agentStorage ? await loadCodexEvidence(agentStorage.evidencePath) : execution.providerConfig.evidence ?? [];
        const next = resolveCodexProvider({ evidence, minimumEvidenceValidityMs,
          credentialScopes: agentStorage ? credentialScopesFromEnvironment(process.env, agentStorage.credentialSaltPath) : {},
          model: execution.providerConfig.model, provider: execution.providerConfig.provider,
          operation: execution.operation, qualificationContextDigest: engineQualificationContextDigest });
        assertTaskCompletable(execution);
        if (activeAgentExecution !== execution) throw new Error("执行任务已变化，不能续期");
        await host.renewRun({ runId, sessionPlan: next.sessionPlan });
        assertTaskCompletable(execution);
        if (activeAgentExecution !== execution) throw new Error("执行任务已变化，不能续期");
        if (agentProviderConfig === execution.providerConfig) agentProviderConfig = next;
        execution.providerConfig = next;
        execution.sessionPlan = next.sessionPlan;
        return next.sessionPlan;
      } });
    let result;
    try { result = await Promise.race([started.completion, renewal.failure]); }
    catch (error) {
      if (error.code === "EXECUTION_AUTHORITY_INVALID") await host.cancelRun(runId, "authorization-renewal-failed").catch(() => undefined);
      throw error;
    }
    await renewal.stop();
    host.closeRun(runId);
    execution.engineRunId = null;
    await host.closeSession(sessionId);
    execution.engineSessionId = null;
    return result;
  } finally {
    await renewal?.stop();
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
  const executionBinding = await readWorkExecutionBinding(runtimeRequest, projectId, task);
  codexAdapter.emit({
    method: "agent/workflow",
    params: { workflow: { task, target, revision: workspace.revision, executionBinding } },
  });
  return {
    workflow: { task, target, revision: workspace.revision, executionBinding },
    prompt: composeNovelCodexPrompt(task.operation === "reviewQuality" ? "" : taskMethodText(task), prompt, taskBoundInstructions(handoff)),
  };
}

async function withWorkFlow(projectId, work) {
  while (recoveryRefreshPromise) await recoveryRefreshPromise;
  modelConnectionGate.assertExecutionAllowed();
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

const failedCreativeAttempts = new WeakMap();
const unstartedCreativeAttempts = new WeakMap();

async function runCreativeAgentTask(event, projectId, prompt, taskType, target = null, { assertCurrent = () => {} } = {}) {
  assertCurrent();
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
  let creationRequest = null;
  let taskCreationAttempted = false;
  let recoveredUnsentTask = false;
  try {
    assertTaskCompletable(execution);
    const capability = createTaskCapability();
    execution.capability = capability;
    adapter.prepareTaskOperation(taskType.operation, capability.secret);
    const taskPurpose = taskType.operation === "generateChapter" ? "writer" : ["modelKnowledge", "assessSceneOutcome", "assessRevisionOutcome", "assessChapterStructure", "extractCommitments", "reviseCommitment", "reviewQuality"].includes(taskType.operation) ? "critic" : "planner";
    const targetKey = target?.chapterId ? `${projectId}:${taskPurpose}:${target.chapterId}` : `${projectId}:${taskPurpose}`;
    activeAgentProjectId = projectId;
    activeAgentPurpose = taskPurpose;
    activeAgentTargetKey = targetKey;
    const threadId = await adapter.ensureSession();
    assertTaskCompletable(execution);
    const workspace = await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/creative-workspace`);
    assertTaskCompletable(execution);
    assertCurrent();
    const initialBudget = grantNewRootBudget({ target, execution, databasePath: agentStorage?.budgetDb,
      getWindow: () => BrowserWindow.fromWebContents(event.sender), workspace });
    creationRequest = {
        target,
        initialBudget,
        inputRevision: workspace.revision,
        threadId,
        model: execution.providerConfig?.model ?? null,
        operation: taskType.operation,
        authorIntent: prompt,
        capabilityHash: capability.hash,
        capabilityExpiresAt: capability.expiresAt,
    };
    taskCreationAttempted = true;
    task = await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/tasks`, {
      method: "POST", body: JSON.stringify(creationRequest),
    });
    task = bindExecutionTask(execution, task);
    activeRuntimeTask = task;
    prompt = taskExecutionPrompt(task);
    if (task.operation === "assessChapterStructure") forwardAgentEvent({ method: "agent/structure-started", params: {
      projectId, taskId: task.id, threadId: task.threadId, operation: task.operation,
    } });
    await bindWorkFlowAttempt(projectId, task.id);
    if (target?.runId) {
      generationClaim = await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/generation-runs/${encodeURIComponent(target.runId)}`);
      if (generationClaim.activeTaskId !== task.id || generationClaim.version !== target.runVersion + 1) {
        throw new Error("生成步骤没有绑定到本次任务");
      }
    }
    await bindTaskProviderBudget(execution, { runtimeRequest, projectId, taskId: task.id,
      generationRun: generationClaim, databasePath: agentStorage?.budgetDb });
    if (initialBudget && !execution.taskBudget) throw new Error("任务缺少持久额度，不能执行");
    if (execution.cancellationRequested) {
      await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/tasks/${encodeURIComponent(task.id)}/cancel`, { method: "POST", body: "{}" });
      throw Object.assign(new Error("生成已停止"), { code: "TASK_CANCELLED" });
    }
    const prepared = await prepareNovelWorkflow(projectId, prompt, task, task.purpose);
    await beginRecoveryRun(task, projectId, threadId);
    assertTaskCompletable(execution);
    assertCurrent();
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
    if (!task && creationRequest?.operation === "assessChapterStructure") {
      try {
        task = bindExecutionTask(execution, await bindUnsentStructureTask(execution, {
          runtimeRequest, projectId, request: creationRequest, databasePath: agentStorage?.budgetDb,
        }));
        recoveredUnsentTask = true;
      } catch { /* Unknown or foreign binding must not manufacture a zero-usage receipt. */ }
    }
    if (!taskCompleted && (recoveredUnsentTask || shouldMarkTaskFailed(task))) {
      try {
        await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/tasks/${encodeURIComponent(task.id)}`, {
          method: "PATCH",
          body: JSON.stringify({ threadId: task.threadId, status: recoveredUnsentTask && execution.cancellationRequested ? "cancelled" : "failed", error: error.message }),
        });
      } catch { /* Preserve the original Agent failure and any saved checkpoints. */ }
    }
    if (execution.budgetUsagePath && task && agentStorage?.budgetDb) {
      if (execution.taskBudget && !execution.taskBudgetBound) {
        try { execution.taskBudget.complete(recoveredUnsentTask && execution.cancellationRequested ? "cancelled" : "failed"); } catch { /* A terminal receipt or recovery must decide the outcome. */ }
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
    if (!taskCreationAttempted && error && typeof error === "object" && target?.resumeStepId) {
      unstartedCreativeAttempts.set(error, { projectId, workTaskId: activeWorkFlow?.id, stepId: target.resumeStepId });
    }
    if (task && !taskCompleted && error && typeof error === "object") {
      failedCreativeAttempts.set(error, { id: task.id, projectId: task.projectId,
        threadId: task.threadId, operation: task.operation });
    }
    throw error;
  } finally {
    if (recoveredUnsentTask && execution.taskBudget && !execution.taskBudgetBound) {
      try { execution.taskBudget.close(); } catch { /* Preserve the original creation failure. */ }
    }
    await recoveryJournal?.abandonRun().catch(() => undefined);
    await adapter.finishTaskOperation();
    await finishAgentExecution(execution);
  }
}

function candidateActionDependencies() {
  return { runtimeRequest, currentWorkFlowState, currentWorkTaskId: () => activeWorkFlow?.id ?? null, runCandidateWorkflow: runCreativeAgentTask, runCreativeAgentTask,
    continueCreativeWork, continueFailedCreativeWork,
    runConversationReviews: (event, projectId, discussion) => executeConversationPlanningReviews(event, projectId, discussion, { runtimeRequest, currentWorkFlowState, runPlanningReview,
      notifyReviewStart: params => forwardAgentEvent({ method: "agent/planning-review-started", params }) }),
    notifyCandidateStart: params => forwardAgentEvent({ method: "agent/candidate-started", params }),
    assertAvailable: operation => {
      if (!codexAdapter.enabled) throw new Error("创作引擎未配置，候选建议仍待确认；配置模型后可重试");
      assertProviderOperation(agentProviderConfig, operation);
    } };
}

async function runQueuedPlanningReview(event, projectId, work, runId) {
  const continuation = await readPlanningReviewContinuation(runtimeRequest, projectId, work, runId);
  if (continuation?.roles.length) await runPlanningReview(event, projectId, continuation.run, continuation);
  else if (continuation) await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/planning-reviews/${encodeURIComponent(runId)}/finalize`,
    { method: "POST", body: "{}" });
}

async function modelChapterKnowledge(event, projectId, chapterId, expectedSourceRevisionId) {
  const workspace = await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/creative-workspace`);
  const source = workspace.modeling?.find((item) => item.objectId === `draft-${chapterId}`);
  if (expectedSourceRevisionId && source?.sourceRevisionId !== expectedSourceRevisionId) {
    throw new Error("章节工作稿已变化，知识建模需基于新的版本重新执行");
  }
  if (!source || source.status === "ready") {
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

  });
  return { status: "ready", batchId: batch.id };
}

async function continueCreativeWork(event, projectId, initial) {
  const results = [...(initial.candidateResults ?? [])];
  let latest = initial;
  const work = await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/tasks/${encodeURIComponent(initial.workflow.task.id)}/work-task`);
  try {
    const finalWork = await advanceWorkSteps(event, projectId, work.id, {
      runtimeRequest, currentWorkFlowState, runCreativeAgentTask, runAdvisoryAgentTask, runQueuedPlanningReview,
      onResult(result, step) {
        if (step.operation === "advisory") latest = result;
        else results.push({ taskId: result.workflow.task.id, action: { operation: step.operation, target: step.target } });
      },
    });
    if (finalWork.status !== "completed") throw new Error("本轮处理已暂停，请从原任务继续。");
    return { ...latest, candidateResults: results, workTask: finalWork };
  } catch (error) {
    const cancelled = error?.code === "TASK_CANCELLED";
    return { ...latest, candidateResults: results,
      executionFailure: { message: Array.from(String(error?.message || "后续处理未完成")).slice(0, 400).join(""),
        code: cancelled ? "TASK_CANCELLED" : "CONVERSATION_INCOMPLETE",
        sourceTaskId: latest.workflow.task.id, workTaskId: work.id, stepId: null },
      message: `${results.length ? "已有保存结果保留。" : ""}${cancelled ? "本轮已停止。" : "本轮尚未完成，请从原任务继续或核对执行结果。"}` };
  }
}

async function runCandidateWorkflow(event, projectId, prompt, taskType, target = null) {
  let result;
  try {
    result = await runCreativeAgentTask(event, projectId, prompt, taskType, target);
  } catch (error) {
    const recovered = await continueFailedCreativeWork(event, projectId, error);
    delete recovered.originalFailedTaskId;
    return recovered;
  }
  return continueCreativeWork(event, projectId, result);
}

async function continueFailedCreativeWork(event, projectId, error) {
  const issued = error?.code === "TASK_CANCELLED" ? null : failedCreativeAttempts.get(error);
  if (!issued || issued.projectId !== projectId) throw error;
  const base = `/api/projects/${encodeURIComponent(projectId)}/tasks/${encodeURIComponent(issued.id)}`;
  let task, work;
  try {
    task = await runtimeRequest(base);
    work = await runtimeRequest(`${base}/work-task`);
  } catch { throw error; }
  if (task.id !== issued.id || task.projectId !== projectId || task.threadId !== issued.threadId
    || task.operation !== issued.operation || task.status !== "failed"
    || work.projectId !== projectId || work.id !== activeWorkFlow?.id
    || !["running", "blocked"].includes(work.status)
    || !work.steps.some(step => step.currentAttemptId === task.id && step.operation === task.operation
      && step.status === "blocked")) throw error;
  const delivered = await continueCreativeWork(event, projectId, { projectId, workflow: { task },
    candidateResults: [], message: String(error.message || "本轮尚未完成") });
  return { ...delivered, originalFailedTaskId: task.id };
}

async function resumeWorkTask(event, projectId, workTaskId, { origin = "author", requestedMode = "continueExecution" } = {}) {
  if (typeof projectId !== "string" || !projectId.trim() || typeof workTaskId !== "string" || !/^work-[a-zA-Z0-9-]+$/.test(workTaskId)) throw new Error("创作任务标识无效");
  return withWorkFlow(projectId, async (flow) => {
    flow.id = workTaskId;
    const path = `/api/projects/${encodeURIComponent(projectId)}/work-tasks/${encodeURIComponent(workTaskId)}`;
    let work = await runtimeRequest(path);
    const resumeOwner = { id: work.id, projectId: work.projectId, threadId: work.threadId, kind: work.kind };
    try {
    if (requestedMode === "recoverSavedResult" && (work.kind === "generation"
      || !["paused", "blocked"].includes(work.status) || work.resumeMode !== "recoverSavedResult")) {
      throw new Error("已保存结果的恢复条件已变化，请重新读取任务；不会改为调用模型。");
    }
    if (origin === "automatic" && work.status !== "running") return work;
    if (work.kind === "generation") {
      if (!codexAdapter.enabled) throw new Error("需要可用模型才能继续执行；请先完成模型配置与验证。");
      const runId = workTaskId.slice("work-".length);
      if (work.status === "blocked") {
        const result = await retryGenerationRun(event, projectId, runId);
        const refreshed = await runtimeRequest(path);
        return result.resumeFailure ? { ...refreshed, resumeFailure: result.resumeFailure } : refreshed;
      }
      if (work.status === "paused") {
        const run = await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/generation-runs/${encodeURIComponent(runId)}`);
        await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/generation-runs/${encodeURIComponent(runId)}/control`, {
          method: "POST", body: JSON.stringify({ expectedVersion: run.version, command: "resume" }),
        });
      }
      const result = await continueGenerationRun(event, projectId, runId);
      const refreshed = await runtimeRequest(path);
      return result.resumeFailure ? { ...refreshed, resumeFailure: result.resumeFailure } : refreshed;
    }
    work = await reconcileWorkProviderUsage(runtimeRequest, projectId, work);
    if (requestedMode === "recoverSavedResult" && work.resumeMode !== "recoverSavedResult") {
      throw new Error("已保存结果的恢复条件已变化，请重新读取任务；不会改为调用模型。");
    }
    if (work.kind === "conversation" && work.status !== "cancelled") {
      const reviewIds = [...new Set(work.steps.filter(step => step.operation === "reviewPlanningSeat").map(step => step.target.reviewRunId))];
      for (const runId of reviewIds) {
        if (work.steps.filter(step => step.target.reviewRunId === runId).every(step => step.status === "completed")) {
          if (work.status === "paused") work = await runtimeRequest(`${path}/control`, {
            method: "POST", body: JSON.stringify({ expectedVersion: work.version, command: "resume" }),
          });
          await runQueuedPlanningReview(event, projectId, work, runId);
        }
      }
      if (reviewIds.length) work = await runtimeRequest(path);
    }
    const isPlanningReview = work.kind === "direct" && work.steps.some(step => step.operation === "reviewPlanningSeat");
    if (work.status === "cancelled") return work;
    if (["paused", "blocked"].includes(work.status) && work.resumeMode === "recoverSavedResult") {
      return runtimeRequest(`${path}/control`, {
        method: "POST", body: JSON.stringify({ expectedVersion: work.version, command: "recoverSavedResult" }),
      });
    }
    if (isPlanningReview && work.steps.every(step => step.status === "completed")) {
      if (work.status === "paused") work = await runtimeRequest(`${path}/control`, {
        method: "POST", body: JSON.stringify({ expectedVersion: work.version, command: "resume" }),
      });
      const continuation = await readPlanningReviewContinuation(runtimeRequest, projectId, work);
      if (continuation) await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/planning-reviews/${encodeURIComponent(continuation.run.id)}/finalize`,
        { method: "POST", body: "{}" });
      return runtimeRequest(path);
    }
    if (!codexAdapter.enabled) throw new Error("需要可用模型才能继续执行；请先完成模型配置与验证。");
    if (work.status !== "completed") work = await ensureWorkBudgetAuthorization({ runtimeRequest, projectId, work, origin,
      databasePath: agentStorage?.budgetDb,
      getWindow: () => BrowserWindow.fromWebContents(event.sender) });
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
    return await advanceWorkSteps(event, projectId, workTaskId, {
      runtimeRequest, currentWorkFlowState, runCreativeAgentTask, runAdvisoryAgentTask,
      runQueuedPlanningReview,
    });
    } catch (error) {
      return recoverUnstartedWorkResume({ runtimeRequest, projectId, workTaskId, owner: resumeOwner,
        provenance: unstartedCreativeAttempts.get(error), error });
    }
  });
}

async function continueGenerationRun(event, projectId, runId) {
  while (recoveryRefreshPromise) await recoveryRefreshPromise;
  if (activeGenerationRunId) throw new Error("已有自动创作运行正在推进");
  activeGenerationRunId = runId;
  if (activeWorkFlow) {
    activeWorkFlow.runId = runId;
    activeWorkFlow.id = `work-${runId}`;
  }
  const runBase = `/api/projects/${encodeURIComponent(projectId)}/generation-runs/${encodeURIComponent(runId)}`;
  let dispatchedWork = null;
  try {
    const dispatched = new Set();
    while (true) {
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
      if (action.kind !== "agentTask") throw new Error(`未知创作运行步骤：${action.kind}`);
      const target = action.target;
      if (!target?.resumeStepId || target.runId !== runId || dispatched.has(target.resumeStepId)) {
        throw new Error("自动创作未返回新的已授权执行步骤，已停止重复调用。");
      }
      dispatched.add(target.resumeStepId);
      const instruction = action.instruction || action.authorIntent || run.authorIntent;
      if (typeof instruction !== "string" || !instruction.trim()) throw new Error("创作步骤缺少原目标");
      dispatchedWork = await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/work-tasks/work-${encodeURIComponent(runId)}`);
      try {
        if (action.operation === "advisory") {
          const work = dispatchedWork;
          const step = work.steps.find(item => item.id === target.resumeStepId && item.operation === "advisory" && item.status === "queued");
          if (!step) throw new Error("自动创作决策步骤不存在");
          await runAdvisoryAgentTask(event, projectId, instruction, step, null, null, work.threadId || null);
        } else {
          await runCreativeAgentTask(event, projectId, instruction, { operation: action.operation }, target);
        }
      } catch (error) {
        if (error?.code === "TASK_CANCELLED") throw error;
        const provenance = unstartedCreativeAttempts.get(error);
        if (provenance) {
          const paused = await recoverUnstartedWorkResume({ runtimeRequest, projectId, workTaskId: `work-${runId}`,
            owner: dispatchedWork, provenance, error });
          const stopped = await runtimeRequest(runBase);
          if (stopped.id !== runId || stopped.projectId !== projectId || stopped.status !== "paused") throw error;
          return { ...stopped, resumeFailure: paused.resumeFailure,
            pausedWork: { id: paused.id, projectId: paused.projectId, threadId: paused.threadId, status: paused.status, version: paused.version } };
        }
        // Runtime alone decides whether a settled, known-zero failure can be
        // corrected. Do not replay the failed step or mint another permit.
        const recovery = await runtimeRequest(`${runBase}/next`, { method: "POST", body: "{}" }).catch(() => null);
        if (recovery?.run?.status !== "running" || recovery.action?.kind !== "agentTask"
          || recovery.action.operation !== "advisory" || recovery.action.target?.runId !== runId
          || !recovery.action.target.resumeStepId || dispatched.has(recovery.action.target.resumeStepId)) throw error;
      }
    }
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

async function startGenerationRun(event, projectId, authorIntent, targetChapters, expectedProjectRevision, expectedBaseSnapshotId, expectedSourceDigest, revisionPredecessor, generationProposalId = null) {
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
  assertProviderOperation(agentProviderConfig, "advisory");
  const run = await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/generation-runs`, {
    method: "POST", body: JSON.stringify({
      expectedProjectRevision,
      expectedBaseSnapshotId,
      expectedSourceDigest,
      authorIntent,
      revisionPredecessor,
      generationProposalId,
      targetChapters,
      maxTaskAttempts: permit.maxTaskAttempts,
      maxProviderRequests: null,
      maxProviderTokens: null,
      deadlineAt: null,
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
  if (!Array.isArray(roles) || ![1, 2, 3].includes(roles.length) || new Set(roles).size !== roles.length || roles.some((role) => !["structure", "character", "experience"].includes(role))) throw new Error("规划评审角色无效");
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
    if (run.projectId !== projectId || typeof run.workTaskId !== "string" || !/^[a-zA-Z0-9-]{1,160}$/.test(run.workTaskId)) throw new Error("规划检查工作任务归属无效");
    const reviewWork = await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/work-tasks/${encodeURIComponent(run.workTaskId)}`);
    if (reviewWork.id !== run.workTaskId || reviewWork.projectId !== projectId || (run.sourceTaskId === null ? reviewWork.kind !== "direct" : reviewWork.kind !== "conversation")) throw new Error("规划检查工作任务归属无效");
    if (activeWorkFlow) activeWorkFlow.id = reviewWork.id;
    if (reviewWork.status !== "completed") await ensureWorkBudgetAuthorization({ runtimeRequest, projectId, work: reviewWork, origin: run.sourceTaskId === null ? "author" : "automatic",
      databasePath: agentStorage?.budgetDb,
      getWindow: () => BrowserWindow.fromWebContents(event.sender) });
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
        execution.capability = capability;
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

async function runAdvisoryAgentTask(event, projectId, prompt, resumeStep = null, revisionRepairSource = null, conversationFocus = null, continuationThreadId = null, selectedThreadId = null) {
  let target = advisoryTaskTarget({ resumeStep, revisionRepairSource });
  if (selectedThreadId !== null) await readConversation(runtimeRequest, projectId, selectedThreadId, true);
  if (continuationThreadId !== null && (!target.workDecision
    || typeof continuationThreadId !== "string" || !continuationThreadId || continuationThreadId.length > 160)) {
    throw new Error("创作协调缺少原对话绑定");
  }
  if (activeGenerationRunId && target.runId !== activeGenerationRunId) throw new Error("自动创作运行正在推进，请稍后讨论");
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
  let taskCreationAttempted = false;
  try {
    await preflightRevisionRepair({ runtimeRequest, projectId, source: target.revisionRepairSource ?? null });
    assertTaskCompletable(execution);
    const capability = createTaskCapability();
    execution.capability = capability;
    const selectedFocus = conversationFocus ?? target.conversationFocus ?? null;
    // Validate before opening any thread; a missing report never falls back to another session.
    const continuationFocus = validateConversationFocus(selectedFocus);
    const reportThreadId = await structureDiscussionThread({ projectId, focus: continuationFocus,
      repairSource: target.revisionRepairSource, runtimeRequest });
    assertTaskCompletable(execution);
    const advisoryThreadId = continuationThreadId ?? selectedThreadId ?? reportThreadId;
    adapter.prepareAdvisorySession(capability.secret);
    if (adapter.threadId && (!advisoryThreadId || adapter.threadId !== advisoryThreadId
      || activeAgentProjectId !== projectId || activeAgentPurpose !== "advisory")) {
      adapter.detachThread();
    }
    if (!adapter.threadId && advisoryThreadId) await adapter.resumeThread(advisoryThreadId);
    activeAgentProjectId = projectId;
    activeAgentPurpose = "advisory";
    activeAgentTargetKey = `${projectId}:advisory`;
    const threadId = await adapter.ensureSession();
    assertTaskCompletable(execution);
    if (!continuationThreadId && !reportThreadId) {
      if (selectedThreadId !== null && threadId !== selectedThreadId) throw new Error("引擎未使用所选对话，已阻止发送");
      if (selectedThreadId === null) await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/conversations`, {
        method: "POST", body: JSON.stringify({ threadId, title: Array.from(prompt).slice(0, 40).join("").replace(/[\p{Cc}]/gu, " ").trim() || "新对话", forkedFromId: null, lastTurnId: null }),
      });
      else {
        const record = await readConversation(runtimeRequest, projectId, threadId, true);
        await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/conversations/${encodeURIComponent(threadId)}`, {
          method: "PATCH", body: JSON.stringify({ expectedVersion: record.version,
            title: record.version === 0 && record.title === "新对话" ? Array.from(prompt).slice(0, 40).join("").replace(/[\p{Cc}]/gu, " ").trim() || "新对话" : record.title }),
        });
      }
    }
    const workspace = await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/creative-workspace`);
    assertTaskCompletable(execution);
    const focus = bindConversationFocus(workspace, conversationFocus ?? target.conversationFocus ?? null);
    if (focus?.reference || focus?.chapterStructureTaskId || focus?.excerpts) conversationFocusInstructions(focus);
    await verifyDiscussionReference({ projectId, workspace, reference: focus?.reference, runtimeRequest });
    assertTaskCompletable(execution);
    if (focus) target = Object.freeze({ ...target, conversationFocus: focus });
    const initialBudget = grantNewRootBudget({ target,
      execution, databasePath: agentStorage?.budgetDb,
      getWindow: () => BrowserWindow.fromWebContents(event.sender), workspace });
    taskCreationAttempted = true;
    task = await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/tasks`, {
      method: "POST",
      body: JSON.stringify({ target, initialBudget, inputRevision: workspace.revision, threadId, model: execution.providerConfig?.model ?? null, operation: "advisory", authorIntent: prompt, capabilityHash: capability.hash, capabilityExpiresAt: capability.expiresAt }),
    });
    if ((focus?.reference || focus?.chapterStructureTaskId || focus?.excerpts) && (task.projectId !== projectId || task.operation !== "advisory"
      || !isDeepStrictEqual(bindConversationFocus(workspace, task.target?.conversationFocus ?? null), focus))) {
      throw new Error("Runtime 未保留精确的讨论来源引用");
    }
    assertRevisionRepairTaskTarget(task, projectId, target);
    task = bindExecutionTask(execution, task);
    activeRuntimeTask = task;
    prompt = taskExecutionPrompt(task);
    await bindWorkFlowAttempt(projectId, task.id);
    const generationClaim = target.runId ? await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/generation-runs/${encodeURIComponent(target.runId)}`) : null;
    if (generationClaim && (generationClaim.activeTaskId !== task.id || generationClaim.version !== target.runVersion + 1)) {
      throw new Error("生成决策没有绑定到本次任务");
    }
    await bindTaskProviderBudget(execution, { runtimeRequest, projectId, taskId: task.id,
      generationRun: generationClaim, databasePath: agentStorage?.budgetDb });
    if (initialBudget && !execution.taskBudget) throw new Error("任务缺少持久额度，不能执行");
    if (execution.cancellationRequested) {
      await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/tasks/${encodeURIComponent(task.id)}/cancel`, { method: "POST", body: "{}" });
      throw Object.assign(new Error("生成已停止"), { code: "TASK_CANCELLED" });
    }
    const handoff = createTaskBoundHandoff(projectId, workspace, task);
    const executionBinding = await readWorkExecutionBinding(runtimeRequest, projectId, task);
    adapter.emit({ method: "agent/workflow", params: { workflow: { task, revision: workspace.revision,
      headSnapshotId: workspace.headSnapshotId, sourceDigest: workspace.sourceDigest, executionBinding } } });
    const advisoryPrompt = composeAdvisoryPrompt(task, prompt, handoff, focus);
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
    const latestRun = generationClaim ? await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/generation-runs/${encodeURIComponent(target.runId)}`) : null;
    const generationRun = latestRun?.status === "running" ? await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/generation-runs/${encodeURIComponent(target.runId)}/advance-task`, {
      method: "POST", body: JSON.stringify({ expectedVersion: latestRun.version, taskId: task.id }),
    }) : latestRun;
    if (activeRuntimeTask?.id === task.id) activeRuntimeTask = null;
    const { proposal: generationHandoff } = await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/tasks/${encodeURIComponent(task.id)}/generation-handoff`);
    return { ...result, finalReplyReceipt, projectId, generationRun, generationHandoff, workflow: { task, revision: workspace.revision,
      headSnapshotId: workspace.headSnapshotId, sourceDigest: workspace.sourceDigest, executionBinding }, completion };
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
    if (!taskCreationAttempted && error && typeof error === "object" && target?.resumeStepId) {
      unstartedCreativeAttempts.set(error, { projectId, workTaskId: activeWorkFlow?.id, stepId: target.resumeStepId });
    }
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
  registerAuthorSessionHandlers(ipcMain, new AuthorSessionStore(join(app.getPath("userData"), "author-session.json")));
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
      runQueuedPlanningReview,
    }),
  });
  registerConditionEvaluationHandlers(ipcMain, runtimeRequest);
  registerGenerationReplanHandlers(ipcMain, runtimeRequest, reconcileGenerationUsage);
  registerChapterRebindHandlers(ipcMain, runtimeRequest);
  registerCommitmentHandlers(ipcMain, runtimeRequest);
  registerChapterStructureReadHandlers(ipcMain, runtimeRequest);
  ipcMain.handle("agent:retry-chapter-structure-check", (_event, projectId, changeId) => chapterStructureConsumer.retry(projectId, changeId));
  ipcMain.handle("agent:set-workspace-context", (event, context) => {
    if (!context || typeof context.dirty !== "boolean" || (context.projectId !== null && (typeof context.projectId !== "string" || !context.projectId.trim() || context.projectId.length > 160))) throw new Error("自动分析上下文无效");
    chapterStructureConsumer.select(context.projectId === null ? null : { ...context, event });
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
  ipcMain.handle("runtime:get-character-knowledge", (_event, projectId, entityId) => {
    const validId = value => typeof value === "string" && value.trim() === value && value.length > 0
      && Buffer.byteLength(value, "utf8") <= 160 && !/[\p{Cc}]/u.test(value);
    if (!validId(projectId) || !validId(entityId)) throw new Error("人物资料读取范围无效");
    const query = new URLSearchParams({ perspective: "author", entityId });
    return runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/knowledge/state?${query}`);
  });
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
  registerPlanningReviewReadHandler(ipcMain, runtimeRequest);
  registerWorkCatalog(ipcMain, runtimeRequest);
  registerChapterDeletion(ipcMain, runtimeRequest);
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
  ipcMain.handle("runtime:materialize-standalone-chapter", (_event, projectId, title, expectedRevision, volumeId = null) => runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/chapters/materialize-standalone`, { method: "POST", body: JSON.stringify({ title, expectedRevision, volumeId, idempotencyKey: `desktop-standalone-${expectedRevision}-${volumeId ?? "unfiled"}-${String(title).slice(0, 40)}` }) }));
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
  ipcMain.handle("agent:status", () => readAgentStatus({
    readStatus: () => codexAdapter.status(),
    isActive: () => Boolean(activeAgentExecution || activeWorkFlow || activeGenerationRunId),
    getOwner: () => agentProviderConfig,
    check: () => assertCurrentAgentPlan(null),
  }));
  ipcMain.handle("agent:recovery-report", refreshRecoveryReport);
  ipcMain.handle("agent:list-connections", async () => ({
    connections: await listModelConnections({
      credentialScopes: agentStorage ? credentialScopesFromEnvironment(process.env, agentStorage.credentialSaltPath) : {},
      evidencePath: agentStorage.evidencePath,
      qualificationContextDigest: engineQualificationContextDigest ?? "unavailable",
    }),
  }));
  ipcMain.handle("agent:verify-connection", async (_event, provider, model) => modelConnectionGate.run(async () => {
    if (activeAgentExecution) throw new Error("已有创作引擎任务正在运行，不能验证连接");
    if (!agentStorage || !agentCli?.path) throw new Error("创作引擎尚未就绪，无法执行真实能力认证");
    const connection = inspectCodexConnection({ provider, model });
    const credentialError = providerConfigurationError(connection, process.env);
    if (credentialError) throw new Error(credentialError);
    await prepareEvidenceReverification(agentStorage.evidencePath);
    const conformanceRunner = new CodexConformanceRunner({
      baseEnvironment: process.env,
      baseInstructions: engineBaseInstructions,
      developerInstructions: NOVEL_ROOT_DEVELOPER_INSTRUCTIONS,
      subagentDeveloperInstructions: NOVEL_SUBAGENT_DEVELOPER_INSTRUCTIONS,
      command: [agentCli.path, "app-server"],
      cwd: agentStorage.workspaceHome,
      nodeCommand: process.execPath,
    });
    const coreRunner = new CodexCoreIntegrationRunner({
      baseEnvironment: process.env,
      baseInstructions: engineBaseInstructions,
      developerInstructions: NOVEL_ROOT_DEVELOPER_INSTRUCTIONS,
      subagentDeveloperInstructions: NOVEL_SUBAGENT_DEVELOPER_INSTRUCTIONS,
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
  }));
  ipcMain.handle("agent:verify-connection-edge", async (_event, parentProvider, parentModel, childProvider, childModel) => modelConnectionGate.run(async () => {
    if (activeAgentExecution) throw new Error("已有创作引擎任务正在运行，不能验证连接");
    if (!agentStorage || !agentCli?.path) throw new Error("创作引擎尚未就绪，无法执行真实能力认证");
    for (const [provider, model] of [[parentProvider, parentModel], [childProvider, childModel]]) {
      const credentialError = providerConfigurationError(inspectCodexConnection({ provider, model }), process.env);
      if (credentialError) throw new Error(credentialError);
    }
    await prepareEvidenceReverification(agentStorage.evidencePath);
    const conformanceRunner = new CodexConformanceRunner({
      baseEnvironment: process.env,
      baseInstructions: engineBaseInstructions,
      developerInstructions: NOVEL_ROOT_DEVELOPER_INSTRUCTIONS,
      subagentDeveloperInstructions: NOVEL_SUBAGENT_DEVELOPER_INSTRUCTIONS,
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
  }));
  ipcMain.handle("agent:configure", async (_event, provider, model) => modelConnectionGate.run(async () => {
    if (activeAgentExecution) throw new Error("已有创作引擎任务正在运行，请先等待或停止当前任务");
    if (typeof provider !== "string" || provider.length > 40 || /[\r\n]/.test(provider)) throw new Error("Provider 无效");
    if (typeof model !== "string" || model.length > 120 || /[\r\n]/.test(model)) throw new Error("Model 无效");
    if (!agentStorage || !agentCli) throw new Error("创作配置尚未就绪");
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
          modelRoute: next.id,
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
  }));
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
  ipcMain.handle("agent:steer", async (_event, projectId, prompt, threadId = null) => {
    assertAdvisoryPromptRoute({ steer: true });
    validateAuthorIntent(prompt);
    if (typeof projectId !== "string" || !projectId.trim()) throw new Error("作品 ID 无效");
    const execution = activeAgentExecution;
    const task = activeRuntimeTask;
    if (threadId !== null) {
      await readConversation(runtimeRequest, projectId, threadId, true);
      if (threadId !== task?.threadId) throw new Error("只能追加当前执行对话的指令");
    }
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
  registerConversationHandlers(ipcMain, {
    runtimeRequest, adapter: () => codexAdapter,
    isBusy: () => Boolean(activeAgentExecution || activeWorkFlow || activeGenerationRunId),
    exclusive: async (callback, { modelRequired = true } = {}) => {
      if (activeAgentExecution || activeWorkFlow || activeGenerationRunId) throw new Error("当前任务正在执行，完成后可新建对话");
      if (modelRequired) modelConnectionGate.assertExecutionAllowed();
      const operation = createTaskExecution(); activeAgentExecution = operation;
      try { return await callback(); } finally { if (activeAgentExecution === operation) activeAgentExecution = null; }
    },
    readHistory: async (projectId, threadId) => {
      const detail = await codexAdapter.readThread(threadId);
      const authored = await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/threads/${encodeURIComponent(threadId)}/author-messages`, {
        method: "POST", body: JSON.stringify({ turnIds: detail.turns.map(turn => turn.id) }),
      });
      return presentAuthorHistory(detail, authored);
    },
  });
  for (const [channel, restore] of [
    ["agent:resume-thread", (threadId) => codexAdapter.resumeThread(threadId)],
    ["agent:reconnect-thread", (threadId) => codexAdapter.reconnectThread(threadId)],
  ]) {
    ipcMain.handle(channel, async (_event, projectId, threadId, expectedTurnId) => {
      while (recoveryRefreshPromise) await recoveryRefreshPromise;
      modelConnectionGate.assertExecutionAllowed();
      if (channel === "agent:reconnect-thread" && expectedTurnId !== null && (typeof expectedTurnId !== "string" || !expectedTurnId.trim())) throw new Error("恢复回合 ID 无效");
      if (channel === "agent:reconnect-thread" && activeAgentExecution) {
        const execution = activeAgentExecution, adapter = execution.adapter;
        const isCurrent = () => activeAgentExecution === execution && codexAdapter === adapter
          && execution.task?.projectId === projectId && execution.task?.threadId === threadId;
        if (!isCurrent() || !expectedTurnId) throw new Error("只能核对当前作品正在执行的原回合");
        const recovery = await adapter.reconnectActiveTurn(threadId, expectedTurnId, isCurrent, () => requestTaskCancellation(execution));
        return { thread: { id: threadId }, recovery };
      }
      if (activeAgentExecution) throw new Error("已有创作引擎任务正在运行，请先等待或停止当前任务");
      const execution = createTaskExecution();
      activeAgentExecution = execution;
      try {
        const conversation = channel === "agent:resume-thread" ? await readConversation(runtimeRequest, projectId, threadId, true) : null;
        const task = conversation ? { purpose: "advisory", target: {} } : await assertThreadProject(projectId, threadId);
        const purpose = task.purpose;
        if (!["advisory", "planner", "writer", "critic"].includes(purpose)) throw new Error("任务缺少有效的上下文用途");
        if (channel === "agent:reconnect-thread" && purpose !== "advisory") {
          const persisted = await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/tasks/${encodeURIComponent(task.id)}`);
          if (persisted.threadId !== threadId || (expectedTurnId && persisted.turnId !== expectedTurnId)
            || !["completed", "failed", "cancelled"].includes(persisted.status)) throw new Error("原生成任务需要在创作任务中核对恢复，不能恢复为普通对话");
          return { thread: { id: threadId }, recovery: { turnId: persisted.turnId,
            state: persisted.status === "completed" ? "completed" : persisted.status === "cancelled" ? "cancelled" : "error" } };
        }
        const result = await restore(threadId);
        activeAgentProjectId = projectId;
        activeAgentPurpose = purpose;
        activeAgentTargetKey = task.target?.chapterId
          ? `${projectId}:${purpose}:${task.target.chapterId}`
          : `${projectId}:${purpose}`;
        return channel === "agent:reconnect-thread" ? { ...result, recovery: null } : { ...result, thread: { ...result.thread, ...conversationSummary(conversation) } };
      } finally {
        if (activeAgentExecution === execution) activeAgentExecution = null;
      }
    });
  }
  ipcMain.handle("agent:read-thread", async (_event, projectId, threadId) => {
    await assertThreadProject(projectId, threadId);
    const detail = await codexAdapter.readThread(threadId);
    const authored = await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/threads/${encodeURIComponent(threadId)}/author-messages`, {
      method: "POST", body: JSON.stringify({ turnIds: detail.turns.map(turn => turn.id) }),
    });
    return presentAuthorHistory(detail, authored);
  });
  ipcMain.handle("agent:review-thread", async (_event, projectId, threadId) => { await assertThreadProject(projectId, threadId); return runCoreTurnWithLease("advisory", () => codexAdapter.reviewThread(threadId)); });
  ipcMain.handle("agent:compact-thread", async (_event, projectId, threadId) => { await assertThreadProject(projectId, threadId); return runCoreTurnWithLease("advisory", () => codexAdapter.compactThread(threadId)); });
  ipcMain.handle("agent:list-background-terminals", async (_event, projectId, threadId) => { await assertThreadProject(projectId, threadId); return codexAdapter.listBackgroundTerminals(threadId); });
  ipcMain.handle("agent:terminate-background-terminal", async (_event, projectId, threadId, processId) => { await assertThreadProject(projectId, threadId); return codexAdapter.terminateBackgroundTerminal(threadId, processId); });
  registerAdvisoryPromptHandler(ipcMain, { withWorkFlow, runAdvisoryAgentTask, analysisDependencies: candidateActionDependencies(),
    routingState: () => ({ activeWorkFlow, activeAgentExecution, activeGenerationRunId }) });
  ipcMain.handle("agent:list-proposed-actions", async (_event, projectId) => {
    if (typeof projectId !== "string" || !projectId.trim() || /[\r\n]/.test(projectId)) throw new Error("作品 ID 无效");
    return runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/task-actions/proposed`);
  });
  ipcMain.handle("agent:approve-action", (event, projectId, sourceTaskId, actionId) => withWorkFlow(projectId,
    () => executeCandidateAction(event, projectId, sourceTaskId, actionId, candidateActionDependencies(), { coordinate: true })));
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
  registerQualityNotebookHandlers(ipcMain, { runtimeRequest,
    runReader: (event, projectId, prompt, type, target) => withWorkFlow(projectId,
      () => runCreativeAgentTask(event, projectId, prompt, type, target)) });
  ipcMain.handle("agent:generate-planning", (event, projectId, operation, intent) => withWorkFlow(projectId, async () => {
    if (typeof projectId !== "string" || !projectId.trim() || /[\r\n]/.test(projectId)) throw new Error("作品 ID 无效");
    const task = planningGenerationTask(operation, intent);
    return runCandidateWorkflow(event, projectId, task.prompt, task.taskType, task.target);
  }));
  ipcMain.handle("agent:generate-work", (event, projectId, intent, targetChapters, expectedProjectRevision, expectedBaseSnapshotId, expectedSourceDigest, revisionPredecessor, generationProposalId = null) =>
    withWorkFlow(projectId, () => startGenerationRun(event, projectId, intent, targetChapters,
      expectedProjectRevision, expectedBaseSnapshotId, expectedSourceDigest, revisionPredecessor, generationProposalId)));
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
  ipcMain.handle("agent:recover-work-task", (event, projectId, workTaskId) => resumeWorkTask(event, projectId, workTaskId, { requestedMode: "recoverSavedResult" }));
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
      return resumeWorkTask(event, projectId, work.id);
    }
    return withWorkFlow(projectId, () => modelChapterKnowledge(event, projectId, chapterId, action.sourceRevisionId));
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
  if (!engineBaseInstructions) throw new Error("创作引擎基础指令为空");
  const codexCli = resolveCodexCli({
    explicitPath: process.env.NOVEL_CODEX_CLI,
    isPackaged: app.isPackaged,
    projectRoot,
    resourcesPath: process.resourcesPath,
  });
  engineQualificationContextDigest = codexCli.path
    ? createQualificationContextDigest({ baseInstructions: engineBaseInstructions, cliPath: codexCli.path })
    : null;
  const evidenceSnapshot = await readModelEvidenceCache(storage.evidencePath);
  const selectedConnection = inspectCodexConnection({ model: process.env.NOVEL_CODEX_MODEL, provider: process.env.NOVEL_CODEX_PROVIDER });
  let providerConfig = null;
  let capabilityError = null;
  try {
    if (evidenceSnapshot.error) throw new Error(evidenceSnapshot.error);
    if (!engineQualificationContextDigest) throw new Error(codexCli.reason ?? "内置创作引擎来源不可验证");
    providerConfig = resolveCodexProvider({
      credentialScopes: credentialScopesFromEnvironment(process.env, storage.credentialSaltPath),
      evidence: evidenceSnapshot.records,
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
    modelRoute: selectedConnection.id,
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
  chapterStructureConsumer.start();

  app.on("activate", () => {
    if (BrowserWindow.getAllWindows().length === 0) createWindow();
  });
}).catch((error) => {
  console.error(`[novel-codex] startup failed: ${error instanceof Error ? error.message : String(error)}`);
  app.quit();
});

app.on("before-quit", (event) => {
  if (allowWindowClose) { chapterStructureConsumer.stop(); return; }
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
