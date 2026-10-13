// @ts-check
import { createTaskExecution, requestTaskCancellation } from "@eternallight/agent-engine";
import { assertAdvisoryPromptRoute, registerAdvisoryPromptHandler } from "./revision-repair-handlers.mjs";
import { validateAuthorIntent } from "../shared/author-input-budget.mjs";
import { conversationSummary, readConversation, registerConversationHandlers } from "./conversation-handlers.mjs";
import { authorSteerInput } from "./author-steer.mjs";
import { enrichAuthorHistoryImages, presentAuthorHistory } from "./thread-history-presentation.mjs";

// Registered only through the trusted, execution-scoped IPC registry.
export function registerSessionControlHandlers(ipcMain, { executionRequests, runtimeRequest, workControlCommands, executionContext, interruptAgentTurn, forwardAgentEvent, modelConnectionGate, assertThreadProject, runCoreTurnWithLease, withWorkFlow, runAdvisoryAgentTask, candidateActionDependencies, resumeWorkTask, state }) {
  ipcMain.handle("agent:interrupt", (event, target) => executionRequests.stop(event, target, async request => {
    const flow = request.flow;
    const execution = request.execution;
    // Capture only this request's resources. Never fall back to a global adapter.
    if (flow) flow.stopRequested = true;
    const task = execution?.task;
    let controlFailure = null;
    try {
      let workId = flow?.id;
      if (!workId && task) {
        const work = await runtimeRequest(`/api/projects/${encodeURIComponent(request.projectId)}/tasks/${encodeURIComponent(task.id)}/work-task`);
        workId = work.id;
      }
      if (workId) {
        const path = `/api/projects/${encodeURIComponent(request.projectId)}/work-tasks/${encodeURIComponent(workId)}`;
        const work = await runtimeRequest(path);
        if (work.id !== workId || work.projectId !== request.projectId) throw new Error("停止目标归属已变化");
        if (["running", "blocked"].includes(work.status)) {
          const paused = await workControlCommands.send(work, "pause");
          if (paused.id !== workId || paused.status !== "paused") throw new Error("暂停回执无效，请核对原任务");
        }
      }
    } catch (error) { controlFailure = error; }
    // A failed Native receipt must not leave this exact local turn generating.
    // Keep the failure visible: local cancellation is not a persisted Work pause.
    requestTaskCancellation(execution);
    requestTaskCancellation(task);
    let interruptFailure = null;
    if (execution && executionContext().activeAgentExecution === execution) {
      try {
        try { await interruptAgentTurn(execution); }
        finally { await execution.adapter.finishTaskOperation(); }
      } catch (error) { interruptFailure = error; }
    }
    if (controlFailure || interruptFailure) {
      const details = [controlFailure && `原任务暂停尚未确认：${controlFailure.message}`,
        interruptFailure && `本机执行中断尚未确认：${interruptFailure.message}`].filter(Boolean).join("；");
      throw new Error(details, { cause: controlFailure ?? interruptFailure });
    }
    return { interrupted: true };
  }));
  ipcMain.handle("agent:steer", async (_event, projectId, prompt, threadId = null) => {
    assertAdvisoryPromptRoute({ steer: true });
    validateAuthorIntent(prompt);
    if (typeof projectId !== "string" || !projectId.trim()) throw new Error("作品 ID 无效");
    const execution = executionContext().activeAgentExecution;
    const task = executionContext().activeRuntimeTask;
    if (threadId !== null) {
      await readConversation(runtimeRequest, projectId, threadId, true);
      if (threadId !== task?.threadId) throw new Error("只能追加当前执行对话的指令");
    }
    if (!execution || !task || execution.task?.id !== task.id || execution.operation !== "advisory"
      || execution.cancellationRequested || task.operation !== "advisory"
      || projectId !== task.projectId || projectId !== executionContext().activeAgentProjectId || executionContext().advisorySteerInFlight
      || !execution.adapter?.turnWaiter || !execution.adapter?.turnId) {
      throw new Error("当前作品没有可追加指令的对话任务");
    }
    const expected = execution.engineHost.runStatus(execution.engineRunId);
    const expectedTurnId = execution.adapter.turnId;
    const path = `/api/projects/${encodeURIComponent(projectId)}/tasks/${encodeURIComponent(task.id)}/intents`;
    // Validate the entire model message before creating a pending author intent.
    authorSteerInput(task, { taskId: task.id, authorMessageId: "x".repeat(160), intentRevision: 16, status: "pending", authorText: prompt });
    executionContext().advisorySteerInFlight = true;
    let finishSteer = () => {};
    executionContext().advisorySteerCompletion = new Promise((resolve) => { finishSteer = () => resolve(); });
    let pending = null;
    try {
      const { intents } = await runtimeRequest(path);
      if (executionContext().activeAgentExecution !== execution || executionContext().activeRuntimeTask?.id !== task.id || execution.cancellationRequested) throw new Error("对话任务已变化");
      const expectedIntentRevision = intents.at(-1)?.intentRevision ?? 0;
      pending = await runtimeRequest(path, { method: "POST", body: JSON.stringify({ expectedIntentRevision, authorText: prompt }) });
      if (pending.intentRevision !== expectedIntentRevision + 1 || pending.authorText !== prompt) throw new Error("作者追加消息回执不匹配");
      forwardAgentEvent({method:"agent/message-updated",params:{projectId,threadId:task.threadId,messageId:pending.authorMessageId,status:"sending"}});
      const steer = authorSteerInput(task, pending);
      const result = await execution.engineHost.steerRun({runId: execution.engineRunId, sessionId: execution.engineSessionId,
        threadId: task.threadId, turnId: expectedTurnId, leaseId: expected.leaseId, epoch: expected.epoch,
        input: steer.input, additionalContext: steer.additionalContext });
      if (result.steered === false && result.delivery === "notSent") {
        const retained = await runtimeRequest(`${path}/${pending.intentRevision}/not-sent`, { method: "POST", body: "{}" });
        if (retained.status !== "notSent" || retained.taskId !== task.id || retained.authorMessageId !== pending.authorMessageId
          || retained.intentRevision !== pending.intentRevision || retained.authorText !== pending.authorText) throw Error("未投递追加要求回执不匹配");
        forwardAgentEvent({method:"agent/message-updated",params:{projectId,threadId:task.threadId,messageId:retained.authorMessageId,status:"failed"}});
        requestTaskCancellation(execution);
        requestTaskCancellation(task);
        await interruptAgentTurn(execution);
        return { steered: false, retained: true, messageId: retained.authorMessageId };
      }
      if (!result.steered) throw new Error("当前轮次尚未准备好接收追加指令");
      const delivered = await runtimeRequest(`${path}/${pending.intentRevision}/delivered`, { method: "POST", body: "{}" });
      if (delivered.authorMessageId !== pending.authorMessageId || delivered.taskId !== task.id
        || delivered.intentRevision !== pending.intentRevision || delivered.authorText !== pending.authorText
        || delivered.status !== "delivered") throw new Error("作者追加消息交付回执不匹配");
      forwardAgentEvent({method:"agent/message-updated",params:{projectId,threadId:task.threadId,messageId:delivered.authorMessageId,status:"delivered"}});
      return {...result,messageId:delivered.authorMessageId};
    } catch (error) {
      if (pending) {
        requestTaskCancellation(execution);
        requestTaskCancellation(task);
        try { await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/tasks/${encodeURIComponent(task.id)}/cancel`, { method: "POST", body: "{}" }); } catch { /* Pending intent remains unwritable. */ }
        try { await interruptAgentTurn(execution); } catch { /* Revoked task remains unwritable. */ }
      }
      throw error;
    } finally {
      executionContext().advisorySteerInFlight = false;
      executionContext().advisorySteerCompletion = null;
      finishSteer();
    }
  });
  registerConversationHandlers(ipcMain, {
    runtimeRequest, adapter: () => executionContext().codexAdapter,
    isBusy: () => Boolean(executionContext().activeAgentExecution || executionContext().activeWorkFlow || executionContext().activeGenerationRunId),
    exclusive: async (callback, { modelRequired = true } = {}) => {
      if (executionContext().activeAgentExecution || executionContext().activeWorkFlow || executionContext().activeGenerationRunId) throw new Error("当前任务正在执行，完成后可新建对话");
      if (modelRequired) modelConnectionGate.assertExecutionAllowed();
      const operation = createTaskExecution(); executionContext().activeAgentExecution = operation;
      try { return await callback(); } finally { if (executionContext().activeAgentExecution === operation) executionContext().activeAgentExecution = null; }
    },
    readHistory: async (projectId, threadId) => {
      const detail = await executionContext().codexAdapter.readThread(threadId);
      const authored = await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/threads/${encodeURIComponent(threadId)}/author-messages`, {
        method: "POST", body: JSON.stringify({ turnIds: detail.turns.map(turn => turn.id) }),
      });
      const withImages = await enrichAuthorHistoryImages(authored, { projectId,
        readAsset: revisionId => runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/media-assets/${encodeURIComponent(revisionId)}`),
        rememberAsset: asset => state.multimediaHost.rememberAsset(asset) });
      return presentAuthorHistory(detail, withImages);
    },
  });
  for (const [channel, restore] of /** @type {const} */ ([
    ["agent:resume-thread", (threadId) => executionContext().codexAdapter.resumeThread(threadId)],
    ["agent:reconnect-thread", (threadId) => executionContext().codexAdapter.reconnectThread(threadId)],
  ])) {
    ipcMain.handle(channel, async (_event, projectId, threadId, expectedTurnId) => {
      while (executionContext().recoveryRefreshPromise) await executionContext().recoveryRefreshPromise;
      modelConnectionGate.assertExecutionAllowed();
      if (channel === "agent:reconnect-thread" && expectedTurnId !== null && (typeof expectedTurnId !== "string" || !expectedTurnId.trim())) throw new Error("恢复回合 ID 无效");
      if (channel === "agent:reconnect-thread" && executionContext().activeAgentExecution) {
        const execution = executionContext().activeAgentExecution, adapter = execution.adapter;
        const isCurrent = () => executionContext().activeAgentExecution === execution && executionContext().codexAdapter === adapter
          && execution.task?.projectId === projectId && execution.task?.threadId === threadId;
        if (!isCurrent() || !expectedTurnId) throw new Error("只能核对当前作品正在执行的原回合");
        const recovery = await adapter.reconnectActiveTurn(threadId, expectedTurnId, isCurrent, () => requestTaskCancellation(execution));
        return { thread: { id: threadId }, recovery };
      }
      if (executionContext().activeAgentExecution) throw new Error("已有创作引擎任务正在运行，请先等待或停止当前任务");
      const execution = createTaskExecution();
      executionContext().activeAgentExecution = execution;
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
        executionContext().activeAgentProjectId = projectId;
        executionContext().activeAgentPurpose = purpose;
        executionContext().activeAgentTargetKey = task.target?.chapterId
          ? `${projectId}:${purpose}:${task.target.chapterId}`
          : `${projectId}:${purpose}`;
        return channel === "agent:reconnect-thread" ? { ...result, recovery: null } : { ...result, thread: { ...result.thread, ...conversationSummary(conversation) } };
      } finally {
        if (executionContext().activeAgentExecution === execution) executionContext().activeAgentExecution = null;
      }
    });
  }
  ipcMain.handle("agent:read-thread", async (_event, projectId, threadId) => {
    await assertThreadProject(projectId, threadId);
    const detail = await executionContext().codexAdapter.readThread(threadId);
    const authored = await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/threads/${encodeURIComponent(threadId)}/author-messages`, {
      method: "POST", body: JSON.stringify({ turnIds: detail.turns.map(turn => turn.id) }),
    });
    const withImages = await enrichAuthorHistoryImages(authored, { projectId,
      readAsset: revisionId => runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/media-assets/${encodeURIComponent(revisionId)}`),
      rememberAsset: asset => state.multimediaHost.rememberAsset(asset) });
    return presentAuthorHistory(detail, withImages);
  });
  ipcMain.handle("agent:update-thread-read-state", async (_event, projectId, threadId, request) => {
    await readConversation(runtimeRequest, projectId, threadId);
    if (!request || typeof request !== "object" || Array.isArray(request)) throw new Error("对话已读请求无效");
    return executionContext().codexAdapter.updateThreadReadState(threadId, request);
  });
  ipcMain.handle("agent:review-thread", async (_event, projectId, threadId) => { await assertThreadProject(projectId, threadId); return runCoreTurnWithLease("advisory", () => executionContext().codexAdapter.reviewThread(threadId)); });
  ipcMain.handle("agent:compact-thread", async (_event, projectId, threadId) => { await assertThreadProject(projectId, threadId); return runCoreTurnWithLease("advisory", () => executionContext().codexAdapter.compactThread(threadId)); });
  ipcMain.handle("agent:list-background-terminals", async (_event, projectId, threadId) => { await assertThreadProject(projectId, threadId); return executionContext().codexAdapter.listBackgroundTerminals(threadId); });
  ipcMain.handle("agent:terminate-background-terminal", async (_event, projectId, threadId, processId) => { await assertThreadProject(projectId, threadId); return executionContext().codexAdapter.terminateBackgroundTerminal(threadId, processId); });
  registerAdvisoryPromptHandler(ipcMain, { withWorkFlow, runAdvisoryAgentTask, analysisDependencies: candidateActionDependencies(),
    resumeConversationWork: (event, projectId, request) => resumeWorkTask(event, projectId, request.workTaskId, request),
    routingState: () => ({ activeWorkFlow: executionContext().activeWorkFlow, activeAgentExecution: executionContext().activeAgentExecution, activeGenerationRunId: executionContext().activeGenerationRunId}) });
}
