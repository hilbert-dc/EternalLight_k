// @ts-check
import { defaultMediaBudget, prepareAdvisoryMedia, assertMediaImageRoute, composeMediaPrompt } from "./media-conversation.mjs";
import { advisoryTaskTarget, assertRevisionRepairTaskTarget, preflightRevisionRepair } from "./revision-repair-handlers.mjs";
import { readConversation } from "./conversation-handlers.mjs";
import { assertProviderOperation, createTaskCapability } from "./codex-runtime.mjs";
import { assertTaskCompletable, bindExecutionTask, createTaskExecution } from "@eternallight/agent-engine";
import { bindConversationFocus, validateConversationFocus, conversationFocusInstructions } from "./conversation-analysis.mjs";
import { structureDiscussionThread } from "./structure-discussion.mjs";
import { verifyDiscussionReference } from "./discussion-reference.mjs";
import { grantNewRootBudget } from "./task-budget-grant.mjs";
import { BrowserWindow } from "electron";
import { isDeepStrictEqual } from "node:util";
import { createTaskBoundHandoff, taskExecutionPrompt } from "./workflow-handoff.mjs";
import { bindTaskProviderBudget } from "./task-provider-budget.mjs";
import { readWorkExecutionBinding } from "./work-execution-binding.mjs";
import { composeAdvisoryPrompt } from "./advisory-prompt.mjs";
import { settleFailedTask } from "./task-failure-settlement.mjs";

// Explicit live capabilities avoid stale configuration snapshots during concurrent execution.
/**
 * @param {{
 *   readonly multimediaHost: import("./media-host.mjs").MultimediaHost,
 *   readonly runtimeRequest: ReturnType<typeof import("./runtime-service.mjs").createRuntimeService>["runtimeRequest"],
 *   readonly executionContext: import("./execution-domains.mjs").ExecutionDomains["current"],
 *   readonly attachAgentWebContents: ReturnType<typeof import("./agent-turn-runtime.mjs").createAgentTurnRuntime>["attachAgentWebContents"],
 *   readonly executionDomains: import("./execution-domains.mjs").ExecutionDomains,
 *   readonly forwardAgentEvent: ReturnType<typeof import("./agent-turn-runtime.mjs").createAgentTurnRuntime>["forwardAgentEvent"],
 *   readonly sendAgentMessage: ReturnType<typeof import("./agent-turn-runtime.mjs").createAgentTurnRuntime>["sendAgentMessage"],
 *   readonly beginAgentExecution: ReturnType<typeof import("./agent-turn-runtime.mjs").createAgentTurnRuntime>["beginAgentExecution"],
 *   readonly executionRequests: import("./execution-requests.mjs").ExecutionRequests,
 *   readonly conversationMessages: ReturnType<typeof import("./conversation-delivery.mjs").createConversationDelivery>["conversationMessages"],
 *   readonly bindWorkFlowAttempt: ReturnType<typeof import("./work-flow-coordinator.mjs").createWorkFlowCoordinator>["bindWorkFlowAttempt"],
 *   readonly beginRecoveryRun: ReturnType<typeof import("./agent-turn-runtime.mjs").createAgentTurnRuntime>["beginRecoveryRun"],
 *   readonly sendAgentTurn: ReturnType<typeof import("./agent-turn-runtime.mjs").createAgentTurnRuntime>["sendAgentTurn"],
 *   readonly bindCodexTurn: ReturnType<typeof import("./work-flow-coordinator.mjs").createWorkFlowCoordinator>["bindCodexTurn"],
 *   readonly unstartedCreativeAttempts: ReturnType<typeof import("./creative-task-coordinator.mjs").createCreativeTaskCoordinator>["unstartedCreativeAttempts"],
 *   readonly finishAgentExecution: ReturnType<typeof import("./agent-turn-runtime.mjs").createAgentTurnRuntime>["finishAgentExecution"],
 * }} dependencies
 */
export function createAdvisoryCoordinator(dependencies) {
  async function runAdvisoryAgentTask(event, projectId, prompt, resumeStep = null, revisionRepairSource = null, conversationFocus = null, continuationThreadId = null, selectedThreadId = null, mediaInput = null) {
    const media = await prepareAdvisoryMedia(dependencies.multimediaHost, projectId, mediaInput, resumeStep, prompt);
    let target = advisoryTaskTarget({ resumeStep, revisionRepairSource });
    if (media) target = Object.freeze(media.target);
    else if (!resumeStep && !revisionRepairSource) target = Object.freeze({ ...target, mediaBudget: defaultMediaBudget() });
    if (selectedThreadId !== null) {
      const selected = await readConversation(dependencies.runtimeRequest, projectId, selectedThreadId, true);
      if (!resumeStep?.target?.adaptation && (selected.scopeId ?? null) !== (media?.workspace.edition.scopeId ?? null)) throw new Error("所选对话不属于当前作品版本");
    }
    if (continuationThreadId !== null && (!target.workDecision
      || typeof continuationThreadId !== "string" || !continuationThreadId || continuationThreadId.length > 160)) {
      throw new Error("创作协调缺少原对话绑定");
    }
    if (dependencies.executionContext().activeGenerationRunId && target.runId !== dependencies.executionContext().activeGenerationRunId) throw new Error("自动创作运行正在推进，请稍后讨论");
    if (!dependencies.executionContext().codexAdapter.enabled) throw new Error("Novel Agent Engine 未配置，当前只能手动编辑");
    if (dependencies.executionContext().activeAgentExecution) throw new Error("已有创作引擎任务正在运行，请先等待或停止当前任务");
    assertProviderOperation(dependencies.executionContext().agentProviderConfig, "advisory");
    dependencies.attachAgentWebContents(event.sender);
    dependencies.executionContext().codexAdapter.setCallbacks({
      onEvent: dependencies.executionDomains.bind(dependencies.forwardAgentEvent),
      onUserInputRequest: dependencies.executionDomains.bind(request => dependencies.sendAgentMessage("agent:user-input-request", request)),
    });
    const execution = createTaskExecution();
    await dependencies.beginAgentExecution(execution, "advisory");
    const adapter = execution.adapter;
    let task = null;
    let taskCompleted = false;
    let taskCreationAttempted = false;
    try {
      if (media) assertMediaImageRoute(execution.sessionPlan, media.images);
      await preflightRevisionRepair({ runtimeRequest: dependencies.runtimeRequest, projectId, source: target.revisionRepairSource ?? null });
      assertTaskCompletable(execution);
      const capability = createTaskCapability();
      execution.capability = capability;
      const selectedFocus = conversationFocus ?? target.conversationFocus ?? null;
      // Validate before opening any thread; a missing report never falls back to another session.
      const continuationFocus = validateConversationFocus(selectedFocus);
      const reportThreadId = await structureDiscussionThread({ projectId, focus: continuationFocus,
        repairSource: target.revisionRepairSource, runtimeRequest: dependencies.runtimeRequest });
      assertTaskCompletable(execution);
      const advisoryThreadId = continuationThreadId ?? selectedThreadId ?? reportThreadId;
      adapter.prepareAdvisorySession(capability.secret);
      if (adapter.threadId && (!advisoryThreadId || adapter.threadId !== advisoryThreadId
        || dependencies.executionContext().activeAgentProjectId !== projectId || dependencies.executionContext().activeAgentPurpose !== "advisory")) {
        adapter.detachThread();
      }
      if (!adapter.threadId && advisoryThreadId) await adapter.resumeThread(advisoryThreadId);
      dependencies.executionContext().activeAgentProjectId = projectId;
      dependencies.executionContext().activeAgentPurpose = "advisory";
      dependencies.executionContext().activeAgentTargetKey = `${projectId}:advisory`;
      const threadId = await adapter.ensureSession();
      assertTaskCompletable(execution);
      if (!continuationThreadId && !reportThreadId) {
        if (selectedThreadId !== null && threadId !== selectedThreadId) throw new Error("引擎未使用所选对话，已阻止发送");
        if (selectedThreadId === null) await dependencies.runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/conversations`, {
          method: "POST", body: JSON.stringify({ threadId, title: Array.from(prompt).slice(0, 40).join("").replace(/[\p{Cc}]/gu, " ").trim() || "新对话", forkedFromId: null, lastTurnId: null, scopeId: media?.workspace.edition.scopeId ?? null }),
        });
        else {
          const record = await readConversation(dependencies.runtimeRequest, projectId, threadId, true);
          await dependencies.runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/conversations/${encodeURIComponent(threadId)}`, {
            method: "PATCH", body: JSON.stringify({ expectedVersion: record.version,
              title: record.version === 0 && record.title === "新对话" ? Array.from(prompt).slice(0, 40).join("").replace(/[\p{Cc}]/gu, " ").trim() || "新对话" : record.title }),
          });
        }
      }
      const workspace = media?.workspace ?? await dependencies.runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/creative-workspace`);
      assertTaskCompletable(execution);
      const focus = bindConversationFocus(workspace, conversationFocus ?? target.conversationFocus ?? null);
      if (focus?.reference || focus?.chapterStructureTaskId || focus?.excerpts) conversationFocusInstructions(focus);
      await verifyDiscussionReference({ projectId, workspace, reference: focus?.reference, runtimeRequest: dependencies.runtimeRequest });
      assertTaskCompletable(execution);
      if (focus) target = Object.freeze({ ...target, conversationFocus: focus });
      const initialBudget = grantNewRootBudget({ target,
        execution, databasePath: dependencies.executionContext().agentStorage?.budgetDb,
        getWindow: () => BrowserWindow.fromWebContents(event.sender), workspace });
      const authorRequest = dependencies.executionRequests.current();
      if (authorRequest?.channel === "agent:send-prompt" && !authorRequest.messageDelivery && !target.workDecision && !target.runId && !target.resumeStepId) {
        const accepted = await dependencies.conversationMessages.submit(projectId, threadId, { messageId: authorRequest.requestId,
          input: { text: prompt, focus: conversationFocus, connectionId: dependencies.executionContext().agentProviderConfig?.id ?? null, ...(media?{media:mediaInput}:{}) }, trigger: { kind: "immediate" } });
        authorRequest.messageDelivery = { message: await dependencies.conversationMessages.change(accepted, "claim") };
      }
      const submission = dependencies.executionRequests.current()?.messageDelivery?.message;
      if (submission && !submission.taskId) target = Object.freeze({ ...target, messageDispatch: { messageId: submission.id, sessionId: submission.sessionId, revision: submission.revision } });
      taskCreationAttempted = true;
      task = await dependencies.runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/tasks`, {
        method: "POST",
        body: JSON.stringify({ target, initialBudget, inputRevision: workspace.revision, threadId, model: execution.providerConfig?.model ?? null, operation: "advisory", authorIntent: prompt, capabilityHash: capability.hash, capabilityExpiresAt: capability.expiresAt }),
      });
      if ((focus?.reference || focus?.chapterStructureTaskId || focus?.excerpts) && (task.projectId !== projectId || task.operation !== "advisory"
        || !isDeepStrictEqual(bindConversationFocus(workspace, task.target?.conversationFocus ?? null), focus))) {
        throw new Error("Runtime 未保留精确的讨论来源引用");
      }
      assertRevisionRepairTaskTarget(task, projectId, target);
      task = bindExecutionTask(execution, task);
      dependencies.executionContext().activeRuntimeTask = task;
      await dependencies.conversationMessages.bind(dependencies.executionRequests.current()?.messageDelivery, task);
      prompt = taskExecutionPrompt(task);
      const generationClaim = target.runId ? await dependencies.runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/generation-runs/${encodeURIComponent(target.runId)}`) : null;
      if (generationClaim && (generationClaim.activeTaskId !== task.id || generationClaim.version !== target.runVersion + 1)) {
        throw new Error("生成决策没有绑定到本次任务");
      }
      await bindTaskProviderBudget(execution, { runtimeRequest: dependencies.runtimeRequest, projectId, taskId: task.id,
        generationRun: generationClaim, databasePath: dependencies.executionContext().agentStorage?.budgetDb });
      await dependencies.bindWorkFlowAttempt(projectId, task.id);
      if (initialBudget && !execution.taskBudget) throw new Error("任务缺少持久额度，不能执行");
      if (execution.cancellationRequested) {
        await dependencies.runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/tasks/${encodeURIComponent(task.id)}/cancel`, { method: "POST", body: "{}" });
        throw Object.assign(new Error("生成已停止"), { code: "TASK_CANCELLED" });
      }
      const handoff = createTaskBoundHandoff(projectId, workspace, task);
      const executionBinding = await readWorkExecutionBinding(dependencies.runtimeRequest, projectId, task);
      adapter.emit({ method: "agent/workflow", params: { workflow: { task, revision: workspace.revision,
        headSnapshotId: workspace.headSnapshotId, sourceDigest: workspace.sourceDigest, executionBinding } } });
      const advisoryPrompt = media ? composeMediaPrompt(task, handoff, await dependencies.multimediaHost.connections.list()) : composeAdvisoryPrompt(task, prompt, handoff, focus);
      if (media) advisoryPrompt.input.push(...media.images);
      await dependencies.beginRecoveryRun(task, projectId, threadId);
      assertTaskCompletable(execution);
      const result = await dependencies.sendAgentTurn(execution, advisoryPrompt, { disableEnvironment: true, threadName: prompt });
      if (dependencies.executionContext().advisorySteerCompletion) await dependencies.executionContext().advisorySteerCompletion;
      assertTaskCompletable(execution);
      const finalReplyReceipt = await dependencies.bindCodexTurn(projectId, task, threadId, result);
      await dependencies.conversationMessages.delivered(dependencies.executionRequests.current()?.messageDelivery, result.turn?.id ?? result.turnId);
      await dependencies.executionContext().recoveryJournal?.flush();
      const completion = await dependencies.runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/tasks/${encodeURIComponent(task.id)}/complete`, { method: "POST", body: JSON.stringify({ threadId, summary: Array.from(result.message ?? "").slice(0, 4_000).join("") }) });
      taskCompleted = true;
      await dependencies.executionContext().recoveryJournal?.completeRun();
      if (execution.budgetUsagePath) await dependencies.runtimeRequest(execution.budgetUsagePath, {
        method: "POST", body: JSON.stringify({ taskId: task.id }),
      });
      const latestRun = generationClaim ? await dependencies.runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/generation-runs/${encodeURIComponent(target.runId)}`) : null;
      const generationRun = latestRun?.status === "running" ? await dependencies.runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/generation-runs/${encodeURIComponent(target.runId)}/advance-task`, {
        method: "POST", body: JSON.stringify({ expectedVersion: latestRun.version, taskId: task.id }),
      }) : latestRun;
      if (dependencies.executionContext().activeRuntimeTask?.id === task.id) dependencies.executionContext().activeRuntimeTask = null;
      const { proposal: generationHandoff } = await dependencies.runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/tasks/${encodeURIComponent(task.id)}/generation-handoff`);
      return { ...result, finalReplyReceipt, projectId, generationRun, generationHandoff, workflow: { task, revision: workspace.revision,
        headSnapshotId: workspace.headSnapshotId, sourceDigest: workspace.sourceDigest, executionBinding }, completion };
    } catch (error) {
      let settlementFailure = null;
      if (!taskCompleted) {
        try { await settleFailedTask(dependencies.runtimeRequest, projectId, task, execution, error); }
        catch (failure) { settlementFailure = failure; }
      }
      if (execution.budgetUsagePath && task && dependencies.executionContext().agentStorage?.budgetDb) {
        if (execution.taskBudget && !execution.taskBudgetBound) {
          try { execution.taskBudget.complete("failed"); } catch { /* Recovery must reconcile an existing receipt. */ }
        }
        try {
          await dependencies.runtimeRequest(execution.budgetUsagePath, { method: "POST", body: JSON.stringify({ taskId: task.id }) });
        } catch (chargeError) {
          console.warn(`[novel-codex] provider usage remains unsettled for ${task.id}: ${chargeError.message}`);
        }
      }
      if (dependencies.executionContext().activeRuntimeTask?.id === task?.id) dependencies.executionContext().activeRuntimeTask = null;
      if (settlementFailure) throw settlementFailure;
      if (execution.cancellationRequested) throw Object.assign(new Error("生成已停止"), { code: "TASK_CANCELLED" });
      if (!taskCreationAttempted && error && typeof error === "object" && target?.resumeStepId) {
        dependencies.unstartedCreativeAttempts.set(error, { projectId, workTaskId: dependencies.executionContext().activeWorkFlow?.id, stepId: target.resumeStepId });
      }
      throw error;
    } finally {
      await dependencies.executionContext().recoveryJournal?.abandonRun().catch(() => undefined);
      await adapter.finishTaskOperation();
      await dependencies.finishAgentExecution(execution);
    }
  }

  return { runAdvisoryAgentTask };
}
