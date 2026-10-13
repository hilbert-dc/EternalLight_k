// @ts-check
import { executeCandidateAction, executeConversationCandidates } from "./conversation-analysis.mjs";
import { registerQualityNotebookHandlers } from "./quality-notebook.mjs";
import { chapterGenerationTask, planningGenerationTask } from "./creative-task-contract.mjs";
import { requestTaskCancellation } from "@eternallight/agent-engine";
import { executeWorkContinuation } from "./semantic-work-continuation.mjs";
import { deliverAuthorQuestion } from "./author-question-delivery.mjs";

// Registered only through the trusted, execution-scoped IPC registry.
export function registerCreativeExecutionHandlers(ipcMain, { runtimeRequest, withWorkFlow, candidateActionDependencies, runCreativeAgentTask, runCandidateWorkflow, startGenerationRun, continueGenerationRun, retryGenerationRun, executionContext, resumeWorkTask, workControlCommands, interruptAgentTurn, submitGenerationIntent, runAdvisoryAgentTask, continueCreativeWork, modelChapterKnowledge, runPlanningReview, baseExecutionDomain, sendAgentMessage }) {
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
      if (executionContext().activeAgentExecution || executionContext().activeGenerationRunId) throw new Error("已有创作引擎任务正在运行");
      const runBase = `/api/projects/${encodeURIComponent(projectId)}/generation-runs/${encodeURIComponent(runId)}`;
      const run = await runtimeRequest(runBase);
      await runtimeRequest(`${runBase}/resolve-feedback`, { method: "POST", body: JSON.stringify({
        expectedVersion: run.version, assessmentRevisionId, choice, instruction,
      }) });
      return continueGenerationRun(event, projectId, runId);
    }));
  ipcMain.handle("agent:resume-work-task", (event, projectId, workTaskId) => resumeWorkTask(event, projectId, workTaskId));
  ipcMain.handle("agent:recover-work-task", (event, projectId, workTaskId) => resumeWorkTask(event, projectId, workTaskId, { requestedMode: "recoverSavedResult" }));
  ipcMain.handle("agent:control-work-task", async (_event, projectId, workTaskId, command, expectedControlRevision, expectedThreadId) => {
    if (typeof projectId !== "string" || !projectId || typeof workTaskId !== "string" || !/^work-[a-zA-Z0-9-]+$/.test(workTaskId) || !["pause", "pauseAfterStep", "cancel"].includes(command)
      || !Number.isSafeInteger(expectedControlRevision) || expectedControlRevision < 0 || typeof expectedThreadId !== "string" || !expectedThreadId || expectedThreadId.length > 160 || /[\p{Cc}]/u.test(expectedThreadId)) throw new Error("创作任务控制参数无效");
    const path = `/api/projects/${encodeURIComponent(projectId)}/work-tasks/${encodeURIComponent(workTaskId)}`;
    const work = await runtimeRequest(path);
    if (work.id !== workTaskId || work.projectId !== projectId || work.threadId !== expectedThreadId
      || work.controlRevision !== expectedControlRevision || !(command === "pauseAfterStep" ? ["running"] : command === "pause" ? ["running", "blocked"] : ["running", "paused", "blocked"]).includes(work.status)) {
      throw new Error("所选创作任务的状态或归属已变化，请重新选择；没有控制其他任务。");
    }
    // Bind the local resources before awaiting delivery. A later run of the
    // same Work is not the execution selected by this author action.
    const selectedFlow = executionContext().activeWorkFlow?.id === workTaskId && executionContext().activeWorkFlow.projectId === projectId ? executionContext().activeWorkFlow : null;
    const selectedExecution = selectedFlow ? executionContext().activeAgentExecution : null;
    const selectedTask = selectedExecution?.task ?? (selectedFlow ? executionContext().activeRuntimeTask : null);
    const updated = await workControlCommands.send(work, command);
    if (command === "pauseAfterStep") {
      if (updated.id !== workTaskId || updated.projectId !== projectId || updated.threadId !== expectedThreadId
        || updated.controlRevision !== expectedControlRevision + 1 || !["running", "paused"].includes(updated.status)
        || updated.pauseAfterStep?.controlRevision !== updated.controlRevision
        || !Number.isSafeInteger(updated.pauseAfterStep.remainingAttempts) || updated.pauseAfterStep.remainingAttempts < 0
        || (updated.status === "running") !== (updated.pauseAfterStep.remainingAttempts > 0)) {
        throw new Error("暂停收尾回执无法核对，请重新读取原任务。");
      }
      // Native fences successors; the frozen attempts keep their existing grants.
      // Never interrupt a local turn on delivery or replay of this command.
      return updated;
    }
    const currentFlow = executionContext().activeWorkFlow?.id === workTaskId && executionContext().activeWorkFlow.projectId === projectId ? executionContext().activeWorkFlow : null;
    if (selectedFlow !== currentFlow || (selectedFlow && selectedExecution !== executionContext().activeAgentExecution)) {
      throw new Error("原控制回执已到达，但本地执行已变化；请核对任务当前状态，没有停止后来的执行。");
    }
    if (updated.id !== workTaskId || updated.projectId !== projectId || updated.threadId !== expectedThreadId
      || updated.status !== (command === "pause" ? "paused" : "cancelled") || !Number.isSafeInteger(updated.controlRevision) || updated.controlRevision <= expectedControlRevision) {
      throw new Error("创作任务控制回执无效，请重新读取实际状态；没有继续执行。");
    }
    if (selectedFlow) {
      if (command === "cancel") selectedFlow.cancelled = true;
      else selectedFlow.stopRequested = true;
      const execution = requestTaskCancellation(selectedExecution);
      const task = requestTaskCancellation(selectedTask);
      try {
        await interruptAgentTurn(execution);
      } finally {
        if (command === "cancel" && task?.projectId === projectId) {
          await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/tasks/${encodeURIComponent(task.id)}/cancel`, { method: "POST", body: "{}" });
          if (executionContext().activeRuntimeTask?.id === task.id) executionContext().activeRuntimeTask = null;
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
    const focus = { view: "chapters", chapterId: task.target.chapterId, chapterNumber: task.target.chapterNumber };
    const discussion = await runAdvisoryAgentTask(event, projectId, task.prompt, null, null, focus);
    const coordinated = await executeConversationCandidates(event, projectId, discussion, candidateActionDependencies());
    const delivered = await continueCreativeWork(event, projectId, { ...discussion, ...coordinated });
    return executeWorkContinuation(runtimeRequest, projectId, delivered,
      request => resumeWorkTask(event, projectId, request.workTaskId, request));
  }));
  ipcMain.handle("agent:resume-knowledge", async (event, projectId, chapterId) => {
    if (typeof projectId !== "string" || !projectId.trim() || /[\r\n]/.test(projectId)
      || typeof chapterId !== "string" || !chapterId.trim() || /[\r\n]/.test(chapterId)) throw new Error("章节目标无效");
    if (!executionContext().codexAdapter.enabled) throw new Error("Novel Agent Engine 未配置，无法继续知识建模");
    if (executionContext().activeAgentExecution || executionContext().activeGenerationRunId) throw new Error("已有创作引擎任务正在运行");
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
  ipcMain.handle("agent:resolve-user-input", async (_event, target, response) => {
    const domain = executionContext();
    if (domain === baseExecutionDomain || domain.id !== target?.executionId || domain.projectId !== target.projectId) throw Error("作者问题的原执行已结束");
    const result = await deliverAuthorQuestion(domain, target, response, runtimeRequest);
    sendAgentMessage("agent:event", {method:"serverRequest/resolved",params:{requestId:target.requestId,
      threadId:domain.codexAdapter.threadId,turnId:domain.codexAdapter.turnId}});
    return result;
  });
}
