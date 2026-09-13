import { app, BrowserWindow, ipcMain, session } from "electron";
import { spawn } from "node:child_process";
import { randomBytes, randomUUID } from "node:crypto";
import { existsSync, readFileSync, renameSync, writeFileSync } from "node:fs";
import { createServer } from "node:net";
import { isAbsolute, join } from "node:path";
import { fileURLToPath } from "node:url";

import { CodexAppServerAdapter } from "./codex-adapter.mjs";
import { applyAgentConfiguration } from "./agent-configuration.mjs";
import { AdvisorySessions } from "./advisory-sessions.mjs";
import { assertProviderOperation, buildCodexConfig, composeNovelCodexPrompt, createCodexEnvironment, createRuntimeEnvironment, createTaskCapability, loadNovelCodexSkills, prepareIsolatedStorage, providerConfigurationError, resolveCodexCli, resolveCodexProvider } from "./codex-runtime.mjs";
import { chapterGenerationTask, planningGenerationTask } from "./creative-task-contract.mjs";
import { isAllowedRendererNavigation, isTrustedIpcSender } from "./navigation-policy.mjs";
import { safeSendToWebContents } from "./webcontents-safety.mjs";
import { assertTaskCompletable, bindExecutionTask, createTaskExecution, requestTaskCancellation, shouldMarkTaskFailed } from "./task-lifecycle.mjs";
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
let runtimeError = null;
let agentWebContents = null;
let removeAgentWebContentsListener = null;
let agentStorage = null;
let agentCli = null;
let agentProviderConfig = null;
let novelSkillPrompt = "";
let activeAgentProjectId = null;
let activeAgentPurpose = null;
let activeAgentTargetKey = null;
let activeRuntimeTask = null;
let activeAgentExecution = null;
const advisorySessions = new AdvisorySessions();
let rendererWebContents = null;
let allowWindowClose = false;
let quitRequested = false;
const codexAdapter = new CodexAppServerAdapter({ command: null, cwd: projectRoot });
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
    const child = spawn(pythonCommand(), [join(runtimeAssetRoot, "runtime", "server.py"), "--host", "127.0.0.1", "--port", String(runtimePort), "--db", runtimeDb, "--token-stdin"], {
      cwd: projectRoot,
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

function ensureCodexConfig(codexHome, providerConfig) {
  const configPath = join(codexHome, "config.toml");
  const modelCatalogPath = join(codexHome, "model-catalog.json");
  const catalogTemporaryPath = `${modelCatalogPath}.${process.pid}.tmp`;
  const configTemporaryPath = `${configPath}.${process.pid}.tmp`;
  writeFileSync(catalogTemporaryPath, readFileSync(join(runtimeAssetRoot, "codex-runtime", "codex-model-catalog.json")), { mode: 0o600 });
  renameSync(catalogTemporaryPath, modelCatalogPath);
  writeFileSync(configTemporaryPath, buildCodexConfig({ ...providerConfig, mcpServerCommand: pythonCommand(), mcpServerPath: join(runtimeAssetRoot, "runtime", "mcp_server.py"), modelCatalogPath }), { encoding: "utf8", mode: 0o600 });
  renameSync(configTemporaryPath, configPath);
}

function clearAgentCallbacks() {
  removeAgentWebContentsListener?.();
  removeAgentWebContentsListener = null;
  agentWebContents = null;
  codexAdapter.setCallbacks({ onEvent: () => {}, onApprovalRequest: () => {} });
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
  if (!codexAdapter.enabled) throw new Error("Codex App Server 未配置，当前只能手动编辑，不能生成伪造产物");
  if (activeAgentExecution) throw new Error("已有 Codex 任务正在运行，请先等待或停止当前任务");
  assertProviderOperation(agentProviderConfig, taskType.operation);
  attachAgentWebContents(event.sender);
  codexAdapter.setCallbacks({
    onEvent: (message) => sendAgentMessage("agent:event", message),
    onApprovalRequest: (request) => sendAgentMessage("agent:approval-request", request),
  });
  const execution = createTaskExecution();
  activeAgentExecution = execution;
  let task = null;
  try {
    const capability = createTaskCapability();
    codexAdapter.prepareTaskOperation(taskType.operation, capability.secret);
    const taskPurpose = taskType.operation === "generateChapter" ? "writer" : "planner";
    const targetKey = target?.chapterId ? `${projectId}:${taskPurpose}:${target.chapterId}` : `${projectId}:${taskPurpose}`;
    activeAgentProjectId = projectId;
    activeAgentPurpose = taskPurpose;
    activeAgentTargetKey = targetKey;
    const threadId = await codexAdapter.ensureSession();
    const workspace = await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/creative-workspace`);
    task = await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/tasks`, {
      method: "POST",
      body: JSON.stringify({
        target,
        inputRevision: workspace.revision,
        threadId,
        model: agentProviderConfig?.model ?? null,
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
    const result = await codexAdapter.sendTurn(prepared.prompt, { disableEnvironment: true, threadName: prompt });
    assertTaskCompletable(execution);
    const completion = await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/tasks/${encodeURIComponent(task.id)}/complete`, {
      method: "POST",
      body: JSON.stringify({ threadId, summary: Array.from(result.message ?? "").slice(0, 4_000).join("") }),
    });
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
    await codexAdapter.finishTaskOperation();
    if (activeAgentExecution === execution) activeAgentExecution = null;
  }
}

async function runAdvisoryAgentTask(event, projectId, prompt) {
  if (!codexAdapter.enabled) throw new Error("Codex App Server 未配置，当前只能手动编辑");
  if (activeAgentExecution) throw new Error("已有 Codex 任务正在运行，请先等待或停止当前任务");
  assertProviderOperation(agentProviderConfig, "advisory");
  attachAgentWebContents(event.sender);
  codexAdapter.setCallbacks({
    onEvent: (message) => sendAgentMessage("agent:event", message),
    onApprovalRequest: (request) => sendAgentMessage("agent:approval-request", request),
  });
  const execution = createTaskExecution();
  activeAgentExecution = execution;
  let task = null;
  try {
    const capability = createTaskCapability();
    const advisoryThreadId = advisorySessions.preferredThread(projectId);
    codexAdapter.prepareAdvisorySession(capability.secret);
    if (codexAdapter.threadId && !advisorySessions.canReuseCurrent(projectId, codexAdapter.threadId, activeAgentProjectId, activeAgentPurpose)) {
      codexAdapter.detachThread();
    }
    if (!codexAdapter.threadId && advisoryThreadId) await codexAdapter.resumeThread(advisoryThreadId);
    activeAgentProjectId = projectId;
    activeAgentPurpose = "advisory";
    activeAgentTargetKey = `${projectId}:advisory`;
    const threadId = await codexAdapter.ensureSession();
    advisorySessions.remember(projectId, threadId);
    const workspace = await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/creative-workspace`);
    task = await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/tasks`, {
      method: "POST",
      body: JSON.stringify({ target: { type: "project" }, inputRevision: workspace.revision, threadId, model: agentProviderConfig?.model ?? null, operation: "advisory", authorIntent: prompt, capabilityHash: capability.hash, capabilityExpiresAt: capability.expiresAt }),
    });
    task = bindExecutionTask(execution, task);
    activeRuntimeTask = task;
    if (execution.cancellationRequested) {
      await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/tasks/${encodeURIComponent(task.id)}/cancel`, { method: "POST", body: "{}" });
      throw Object.assign(new Error("生成已停止"), { code: "TASK_CANCELLED" });
    }
    const handoff = createTaskBoundHandoff(projectId, workspace, task);
    codexAdapter.emit({ method: "agent/workflow", params: { workflow: { task, revision: workspace.revision } } });
    const advisoryPrompt = composeNovelCodexPrompt(novelSkillPrompt, prompt, `只通过 novel_workspace.read_context 读取本次任务的冻结上下文，调用时原样传入以下绑定中的 taskId、projectId、threadId、expectedRevision，并使用 purpose=planner、budget=24、byteBudget=60000：\n${JSON.stringify(handoff, null, 2)}\n\n这是自由讨论任务：只提供分析、问题和建议，不得保存或采用领域产物。讨论结论会由界面带入创作简报、大纲或章纲的专用生成入口。`);
    const result = await codexAdapter.sendTurn(advisoryPrompt, { disableEnvironment: true, threadName: prompt });
    assertTaskCompletable(execution);
    const completion = await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/tasks/${encodeURIComponent(task.id)}/complete`, { method: "POST", body: JSON.stringify({ threadId, summary: Array.from(result.message ?? "").slice(0, 4_000).join("") }) });
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
    await codexAdapter.finishTaskOperation();
    if (activeAgentExecution === execution) activeAgentExecution = null;
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
  ipcMain.handle("agent:configure", (_event, provider, model) => {
    if (activeAgentExecution) throw new Error("已有 Codex 任务正在运行，请先等待或停止当前任务");
    if (typeof provider !== "string" || provider.length > 40 || /[\r\n]/.test(provider)) throw new Error("Provider 无效");
    if (typeof model !== "string" || model.length > 120 || /[\r\n]/.test(model)) throw new Error("Model 无效");
    if (!agentStorage || !agentCli || !agentProviderConfig) throw new Error("Novel Codex 配置尚未就绪");
    const next = resolveCodexProvider({ provider, model });
    const configurationError = providerConfigurationError(next, process.env);
    if (configurationError) throw new Error(configurationError);
    const status = applyAgentConfiguration({
      adapter: codexAdapter,
      configuration: {
        apiKey: next.provider === "openai" ? process.env[next.apiKeyEnv] : null,
        command: agentCli.path ? [agentCli.path, "app-server"] : null,
        commandSource: agentCli.source,
        configurationError,
        cwd: projectRoot,
        env: createCodexEnvironment(process.env, { ...agentStorage, provider: next.provider, runtimeUrl, candidateToken: runtimeCandidateToken }),
        model: next.model,
        provider: next.provider,
      },
      persist: () => ensureCodexConfig(agentStorage.codexHome, next),
    });
    agentProviderConfig = next;
    return status;
  });
  ipcMain.handle("agent:interrupt", async () => {
    const execution = requestTaskCancellation(activeAgentExecution);
    const task = requestTaskCancellation(execution?.task ?? activeRuntimeTask);
    let interrupted = { interrupted: false };
    try {
      interrupted = await codexAdapter.interruptTurn();
    } finally {
      try {
        if (task) {
          await runtimeRequest(`/api/projects/${encodeURIComponent(task.projectId)}/tasks/${encodeURIComponent(task.id)}/cancel`, { method: "POST", body: "{}" });
          if (activeRuntimeTask?.id === task.id) activeRuntimeTask = null;
        }
      } finally {
        await codexAdapter.finishTaskOperation();
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
      if (activeAgentExecution) throw new Error("已有 Codex 任务正在运行，请先等待或停止当前任务");
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
  ipcMain.handle("agent:review-thread", async (_event, projectId, threadId) => { await assertThreadProject(projectId, threadId); return codexAdapter.reviewThread(threadId); });
  ipcMain.handle("agent:compact-thread", async (_event, projectId, threadId) => { await assertThreadProject(projectId, threadId); return codexAdapter.compactThread(threadId); });
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
  ipcMain.handle("agent:resolve-approval", (_event, requestId, decision) => codexAdapter.resolveApproval(requestId, decision));
}

function stopRuntime() {
  if (runtimeProcess && !runtimeProcess.killed) runtimeProcess.kill();
  clearAgentCallbacks();
  codexAdapter.close();
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
  const storage = prepareIsolatedStorage(app.getPath("userData"));
  novelSkillPrompt = loadNovelCodexSkills(projectRoot);
  const providerConfig = resolveCodexProvider({ model: process.env.NOVEL_CODEX_MODEL, provider: process.env.NOVEL_CODEX_PROVIDER });
  await startRuntime(storage.runtimeDb);
  ensureCodexConfig(storage.codexHome, providerConfig);
  const codexCli = resolveCodexCli({
    explicitPath: process.env.NOVEL_CODEX_CLI,
    isPackaged: app.isPackaged,
    projectRoot,
    resourcesPath: process.resourcesPath,
  });
  agentStorage = storage;
  agentCli = codexCli;
  agentProviderConfig = providerConfig;
  const configurationError = providerConfigurationError(providerConfig, process.env);
  codexAdapter.configure({
    apiKey: providerConfig.provider === "openai" ? process.env[providerConfig.apiKeyEnv] : null,
    command: codexCli.path ? [codexCli.path, "app-server"] : null,
    commandSource: codexCli.source,
    configurationError,
    cwd: projectRoot,
    env: createCodexEnvironment(process.env, { ...storage, provider: providerConfig.provider, runtimeUrl, candidateToken: runtimeCandidateToken }),
    model: providerConfig.model,
    provider: providerConfig.provider,
  });
  if (!codexCli.path) console.warn(`[novel-codex] ${codexCli.reason}`);
  if (configurationError) console.warn(`[novel-codex] ${configurationError}`);
  registerIpcHandlers();
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
