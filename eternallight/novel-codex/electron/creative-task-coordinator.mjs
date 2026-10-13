// @ts-check
import { coreTurnFailure } from "./core-turn-failure.mjs";
import { assertProviderOperation, createTaskCapability } from "./codex-runtime.mjs";
import { assertTaskCompletable, bindExecutionTask, createTaskExecution } from "@eternallight/agent-engine";
import { grantNewRootBudget } from "./task-budget-grant.mjs";
import { BrowserWindow } from "electron";
import { bindTaskProviderBudget, reconcileGenerationProviderUsage, reconcileWorkProviderUsage } from "./task-provider-budget.mjs";
import { taskExecutionPrompt } from "./workflow-handoff.mjs";
import { bindUnsentStructureTask } from "./unsent-structure-task.mjs";
import { settleFailedTask } from "./task-failure-settlement.mjs";
import { executeConversationPlanningReviews } from "./conversation-planning-reviews.mjs";
import { readPlanningReviewContinuation } from "./planning-review-resumption.mjs";
import { advanceWorkSteps } from "./work-step-continuation.mjs";
import { executeConversationCandidates } from "./conversation-analysis.mjs";
import { executeWorkContinuation, rebindIdleContinuation } from "./semantic-work-continuation.mjs";
import { ensureWorkBudgetAuthorization } from "./work-budget-authorization.mjs";
import { recoverUnstartedWorkResume } from "./work-resume-not-started.mjs";

// Explicit live capabilities avoid stale configuration snapshots during concurrent execution.
/**
 * @param {{
 *   readonly executionContext: import("./execution-domains.mjs").ExecutionDomains["current"],
 *   readonly attachAgentWebContents: ReturnType<typeof import("./agent-turn-runtime.mjs").createAgentTurnRuntime>["attachAgentWebContents"],
 *   readonly executionDomains: import("./execution-domains.mjs").ExecutionDomains,
 *   readonly forwardAgentEvent: ReturnType<typeof import("./agent-turn-runtime.mjs").createAgentTurnRuntime>["forwardAgentEvent"],
 *   readonly sendAgentMessage: ReturnType<typeof import("./agent-turn-runtime.mjs").createAgentTurnRuntime>["sendAgentMessage"],
 *   readonly beginAgentExecution: ReturnType<typeof import("./agent-turn-runtime.mjs").createAgentTurnRuntime>["beginAgentExecution"],
 *   readonly runtimeRequest: ReturnType<typeof import("./runtime-service.mjs").createRuntimeService>["runtimeRequest"],
 *   readonly bindWorkFlowAttempt: ReturnType<typeof import("./work-flow-coordinator.mjs").createWorkFlowCoordinator>["bindWorkFlowAttempt"],
 *   readonly prepareNovelWorkflow: ReturnType<typeof import("./work-flow-coordinator.mjs").createWorkFlowCoordinator>["prepareNovelWorkflow"],
 *   readonly beginRecoveryRun: ReturnType<typeof import("./agent-turn-runtime.mjs").createAgentTurnRuntime>["beginRecoveryRun"],
 *   readonly sendAgentTurn: ReturnType<typeof import("./agent-turn-runtime.mjs").createAgentTurnRuntime>["sendAgentTurn"],
 *   readonly bindCodexTurn: ReturnType<typeof import("./work-flow-coordinator.mjs").createWorkFlowCoordinator>["bindCodexTurn"],
 *   readonly finishAgentExecution: ReturnType<typeof import("./agent-turn-runtime.mjs").createAgentTurnRuntime>["finishAgentExecution"],
 *   readonly currentWorkFlowState: ReturnType<typeof import("./work-flow-coordinator.mjs").createWorkFlowCoordinator>["currentWorkFlowState"],
 *   readonly runPlanningReview: ReturnType<typeof import("./planning-review-coordinator.mjs").createPlanningReviewCoordinator>["runPlanningReview"],
 *   readonly runAdvisoryAgentTask: ReturnType<typeof import("./advisory-coordinator.mjs").createAdvisoryCoordinator>["runAdvisoryAgentTask"],
 *   readonly withWorkFlow: ReturnType<typeof import("./work-flow-coordinator.mjs").createWorkFlowCoordinator>["withWorkFlow"],
 *   readonly acquireWorkFlowOwner: ReturnType<typeof import("./work-flow-coordinator.mjs").createWorkFlowCoordinator>["acquireWorkFlowOwner"],
 *   readonly continueGenerationRun: ReturnType<typeof import("./generation-coordinator.mjs").createGenerationCoordinator>["continueGenerationRun"],
 *   readonly retryGenerationRun: ReturnType<typeof import("./generation-coordinator.mjs").createGenerationCoordinator>["retryGenerationRun"],
 *   readonly workControlCommands: import("./work-control-commands.mjs").WorkControlCommands,
 * }} dependencies
 */
export function createCreativeTaskCoordinator(dependencies) {
  const failedCreativeAttempts = new WeakMap();

  const unstartedCreativeAttempts = new WeakMap();

  async function runCreativeAgentTask(event, projectId, prompt, taskType, target = null, { assertCurrent = () => {} } = {}) {
    assertCurrent();
    if (dependencies.executionContext().activeGenerationRunId && target?.runId !== dependencies.executionContext().activeGenerationRunId) throw new Error("自动创作运行正在推进，不能并行创建其他模型任务");
    if (!dependencies.executionContext().codexAdapter.enabled) throw new Error("Novel Agent Engine 未配置，当前只能手动编辑，不能生成伪造产物");
    if (dependencies.executionContext().activeAgentExecution) throw new Error("已有创作引擎任务正在运行，请先等待或停止当前任务");
    assertProviderOperation(dependencies.executionContext().agentProviderConfig, taskType.operation);
    dependencies.attachAgentWebContents(event.sender);
    dependencies.executionContext().codexAdapter.setCallbacks({
      onEvent: dependencies.executionDomains.bind(dependencies.forwardAgentEvent),
      onUserInputRequest: dependencies.executionDomains.bind(request => dependencies.sendAgentMessage("agent:user-input-request", request)),
    });
    const execution = createTaskExecution();
    await dependencies.beginAgentExecution(execution, taskType.operation);
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
      dependencies.executionContext().activeAgentProjectId = projectId;
      dependencies.executionContext().activeAgentPurpose = taskPurpose;
      dependencies.executionContext().activeAgentTargetKey = targetKey;
      const threadId = await adapter.ensureSession();
      assertTaskCompletable(execution);
      const workspace = await dependencies.runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/creative-workspace`);
      assertTaskCompletable(execution);
      assertCurrent();
      const initialBudget = grantNewRootBudget({ target, execution, databasePath: dependencies.executionContext().agentStorage?.budgetDb,
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
      task = await dependencies.runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/tasks`, {
        method: "POST", body: JSON.stringify(creationRequest),
      });
      task = bindExecutionTask(execution, task);
      dependencies.executionContext().activeRuntimeTask = task;
      if (task.operation === "assessChapterStructure") dependencies.forwardAgentEvent({ method: "agent/structure-started", params: {
        projectId, taskId: task.id, threadId: task.threadId, operation: task.operation,
      } });
      if (target?.runId) {
        generationClaim = await dependencies.runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/generation-runs/${encodeURIComponent(target.runId)}`);
        if (generationClaim.activeTaskId !== task.id || generationClaim.version !== target.runVersion + 1) {
          throw new Error("生成步骤没有绑定到本次任务");
        }
      }
      await bindTaskProviderBudget(execution, { runtimeRequest: dependencies.runtimeRequest, projectId, taskId: task.id,
        generationRun: generationClaim, databasePath: dependencies.executionContext().agentStorage?.budgetDb });
      await dependencies.bindWorkFlowAttempt(projectId, task.id);
      prompt = taskExecutionPrompt(task);
      if (initialBudget && !execution.taskBudget) throw new Error("任务缺少持久额度，不能执行");
      if (execution.cancellationRequested) {
        await dependencies.runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/tasks/${encodeURIComponent(task.id)}/cancel`, { method: "POST", body: "{}" });
        throw Object.assign(new Error("生成已停止"), { code: "TASK_CANCELLED" });
      }
      const prepared = await dependencies.prepareNovelWorkflow(projectId, prompt, task, task.purpose);
      await dependencies.beginRecoveryRun(task, projectId, threadId);
      assertTaskCompletable(execution);
      assertCurrent();
      const result = await dependencies.sendAgentTurn(execution, prepared.prompt, { disableEnvironment: true, threadName: prompt });
      assertTaskCompletable(execution);
      const finalReplyReceipt = await dependencies.bindCodexTurn(projectId, task, threadId, result);
      await dependencies.executionContext().recoveryJournal?.flush();
      const completion = await dependencies.runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/tasks/${encodeURIComponent(task.id)}/complete`, {
        method: "POST",
        body: JSON.stringify({ threadId, summary: Array.from(result.message ?? "").slice(0, 4_000).join("") }),
      });
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
      return { ...result, finalReplyReceipt, projectId, workflow: prepared.workflow, completion, generationRun };
    } catch (error) {
      if (!task && creationRequest?.operation === "assessChapterStructure") {
        try {
          task = bindExecutionTask(execution, await bindUnsentStructureTask(execution, {
            runtimeRequest: dependencies.runtimeRequest, projectId, request: creationRequest, databasePath: dependencies.executionContext().agentStorage?.budgetDb,
          }));
          recoveredUnsentTask = true;
        } catch { /* Unknown or foreign binding must not manufacture a zero-usage receipt. */ }
      }
      let settlementFailure = null;
      if (!taskCompleted) {
        try { await settleFailedTask(dependencies.runtimeRequest, projectId, task, execution, error); }
        catch (failure) { settlementFailure = failure; }
      }
      if (execution.budgetUsagePath && task && dependencies.executionContext().agentStorage?.budgetDb) {
        if (execution.taskBudget && !execution.taskBudgetBound) {
          try { execution.taskBudget.complete(recoveredUnsentTask && execution.cancellationRequested ? "cancelled" : "failed"); } catch { /* A terminal receipt or recovery must decide the outcome. */ }
        }
        try {
          await dependencies.runtimeRequest(execution.budgetUsagePath, {
            method: "POST", body: JSON.stringify({ taskId: task.id }),
          });
        } catch (chargeError) {
          console.warn(`[novel-codex] provider usage remains unsettled for ${task.id}: ${chargeError.message}`);
        }
      }
      if (dependencies.executionContext().activeRuntimeTask?.id === task?.id) dependencies.executionContext().activeRuntimeTask = null;
      if (settlementFailure) throw settlementFailure;
      if (execution.cancellationRequested) throw Object.assign(new Error("生成已停止"), { code: "TASK_CANCELLED" });
      if (!taskCreationAttempted && error && typeof error === "object" && target?.resumeStepId) {
        unstartedCreativeAttempts.set(error, { projectId, workTaskId: dependencies.executionContext().activeWorkFlow?.id, stepId: target.resumeStepId });
      }
      if (task && !taskCompleted && error && typeof error === "object") {
        failedCreativeAttempts.set(error, { id: task.id, projectId: task.projectId,
          threadId: task.threadId, operation: task.operation });
      }
      throw error;
    } finally {
      if (execution.taskBudget && !execution.taskBudgetBound) {
        try { execution.taskBudget.close(); } catch { /* Preserve the original creation failure. */ }
      }
      await dependencies.executionContext().recoveryJournal?.abandonRun().catch(() => undefined);
      await adapter.finishTaskOperation();
      await dependencies.finishAgentExecution(execution);
    }
  }

  function candidateActionDependencies() {
    return { runtimeRequest: dependencies.runtimeRequest, currentWorkFlowState: dependencies.currentWorkFlowState, currentWorkTaskId: () => dependencies.executionContext().activeWorkFlow?.id ?? null, runCandidateWorkflow: runCreativeAgentTask, runCreativeAgentTask,
      continueCreativeWork, continueFailedCreativeWork,
      runConversationReviews: (event, projectId, discussion) => executeConversationPlanningReviews(event, projectId, discussion, { runtimeRequest: dependencies.runtimeRequest, currentWorkFlowState: dependencies.currentWorkFlowState, runPlanningReview: dependencies.runPlanningReview,
        notifyReviewStart: params => dependencies.forwardAgentEvent({ method: "agent/planning-review-started", params }) }),
      notifyCandidateStart: params => dependencies.forwardAgentEvent({ method: "agent/candidate-started", params }),
      assertAvailable: operation => {
        if (!dependencies.executionContext().codexAdapter.enabled) throw new Error("创作引擎未配置，候选建议仍待确认；配置模型后可重试");
        assertProviderOperation(dependencies.executionContext().agentProviderConfig, operation);
      } };
  }

  async function runQueuedPlanningReview(event, projectId, work, runId) {
    const continuation = await readPlanningReviewContinuation(dependencies.runtimeRequest, projectId, work, runId);
    if (continuation?.roles.length) await dependencies.runPlanningReview(event, projectId, continuation.run, continuation);
    else if (continuation) await dependencies.runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/planning-reviews/${encodeURIComponent(runId)}/finalize`,
      { method: "POST", body: "{}" });
  }

  async function modelChapterKnowledge(event, projectId, chapterId, expectedSourceRevisionId) {
    const workspace = await dependencies.runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/creative-workspace`);
    const source = workspace.modeling?.find((item) => item.objectId === `draft-${chapterId}`);
    if (expectedSourceRevisionId && source?.sourceRevisionId !== expectedSourceRevisionId) {
      throw new Error("章节工作稿已变化，知识建模需基于新的版本重新执行");
    }
    if (!source || source.status === "ready") {
      return { status: source?.status ?? "not_applicable" };
    }
    const batch = await dependencies.runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/modeling-batches`, {
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
    const work = await dependencies.runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/tasks/${encodeURIComponent(initial.workflow.task.id)}/work-task`);
    try {
      const finalWork = await advanceWorkSteps(event, projectId, work.id, {
        runtimeRequest: dependencies.runtimeRequest, currentWorkFlowState: dependencies.currentWorkFlowState, runCreativeAgentTask, runAdvisoryAgentTask: dependencies.runAdvisoryAgentTask, runQueuedPlanningReview,
        async onResult(result, step) {
          if (step.operation === "advisory" && step.target.workDecisionAction && !step.target.workDecision) {
            const candidates = await executeConversationCandidates(event, projectId, result, candidateActionDependencies());
            result = { ...result, ...candidates };
            results.push(...(candidates.candidateResults ?? []));
          }
          if (step.operation === "advisory") latest = result;
          else results.push({ taskId: result.workflow.task.id, action: { operation: step.operation, target: step.target } });
        },
      });
      if (finalWork.status !== "completed") throw new Error("本轮处理已暂停，请从原任务继续。");
      return { ...latest, candidateResults: results, workTask: finalWork };
    } catch (error) {
      const cancelled = error?.code === "TASK_CANCELLED";
      const terminal = coreTurnFailure(error);
      return { ...latest, candidateResults: results,
        executionFailure: { message: Array.from(String(error?.message || "后续处理未完成")).slice(0, 400).join(""),
          code: cancelled ? "TASK_CANCELLED" : terminal ? "CORE_TURN_FAILED" : "CONVERSATION_INCOMPLETE",
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
    if (coreTurnFailure(error)) throw error;
    const issued = error?.code === "TASK_CANCELLED" ? null : failedCreativeAttempts.get(error);
    if (!issued || issued.projectId !== projectId) throw error;
    const base = `/api/projects/${encodeURIComponent(projectId)}/tasks/${encodeURIComponent(issued.id)}`;
    let task, work;
    try {
      task = await dependencies.runtimeRequest(base);
      work = await dependencies.runtimeRequest(`${base}/work-task`);
    } catch { throw error; }
    if (task.id !== issued.id || task.projectId !== projectId || task.threadId !== issued.threadId
      || task.operation !== issued.operation || task.status !== "failed"
      || work.projectId !== projectId || work.id !== dependencies.executionContext().activeWorkFlow?.id
      || !["running", "blocked"].includes(work.status)
      || !work.steps.some(step => step.currentAttemptId === task.id && step.operation === task.operation
        && step.status === "blocked")) throw error;
    const delivered = await continueCreativeWork(event, projectId, { projectId, workflow: { task },
      candidateResults: [], message: String(error.message || "本轮尚未完成") });
    return { ...delivered, originalFailedTaskId: task.id };
  }

  async function resumeWorkTask(event, projectId, workTaskId, { origin = "author", requestedMode = "continueExecution", expectedThreadId = null, generationHandoff = null, durableWake = null } = {}) {
    if (typeof projectId !== "string" || !projectId.trim() || typeof workTaskId !== "string" || !/^work-[a-zA-Z0-9-]+$/.test(workTaskId)) throw new Error("创作任务标识无效");
    /** @type {Record<string, unknown> | null} */
    let latestDecision = null;
    let savedResultRecovered = false;
    const resumed = await dependencies.withWorkFlow(projectId, async (flow) => {
      flow.id = workTaskId;
      const path = `/api/projects/${encodeURIComponent(projectId)}/work-tasks/${encodeURIComponent(workTaskId)}`;
      let work = await dependencies.runtimeRequest(path);
      if (expectedThreadId !== null && work.threadId !== expectedThreadId) throw Error("原任务不属于当前对话");
      const resumeOwner = { id: work.id, projectId: work.projectId, threadId: work.threadId, kind: work.kind };
      try {
      if (durableWake) {
        // A durable wake is Host-internal, comes from Native claim, and must
        // borrow that exact lease. It is never inferred from an author word.
        if (origin !== "automatic" || expectedThreadId !== work.threadId || generationHandoff
          || !["running", "completed", "blocked"].includes(work.status)) throw new Error("持久接续不属于当前原任务");
        await dependencies.acquireWorkFlowOwner(work);
        const record = flow.ownership.record;
        const source = work.steps.find(step => step.id === durableWake.sourceStepId
          && step.attempts?.some(attempt => attempt.id === durableWake.sourceAttemptId));
        if (!flow.ownership.borrowed || !record.claim || record.lease.leaseId !== durableWake.leaseId
          || record.lease.hostSessionId !== durableWake.hostSessionId || !source
          || record.claim.eventId !== durableWake.eventId || record.claim.claimId !== durableWake.claimId
          || record.claim.sourceStepId !== durableWake.sourceStepId || record.claim.sourceAttemptId !== durableWake.sourceAttemptId) {
          throw new Error("持久接续的领取与原执行来源不匹配");
        }
        if (work.status === "blocked") {
          const terminal = await dependencies.runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/tasks/${encodeURIComponent(durableWake.sourceAttemptId)}`);
          if (terminal.status !== "failed" || terminal.projectId !== projectId || terminal.id !== durableWake.sourceAttemptId) {
            throw new Error("持久纠错缺少原任务的真实失败证据");
          }
        }
        if (work.kind === "generation") {
          // Native owns both Step/Work and Run admission. A durable generation
          // wake uses only /next, without author resume, retry or input rebind.
          const result = await dependencies.continueGenerationRun(event, projectId, workTaskId.slice("work-".length));
          const refreshed = await dependencies.runtimeRequest(path);
          return result.resumeFailure ? { ...refreshed, resumeFailure: result.resumeFailure } : refreshed;
        }
        if (work.status === "blocked") {
          // Native /decision rechecks current recovery, permit and charged
          // allowance. No author resume transition or new allowance is issued.
          return await advanceWorkSteps(event, projectId, workTaskId, {
            runtimeRequest: dependencies.runtimeRequest, currentWorkFlowState: dependencies.currentWorkFlowState, runCreativeAgentTask, runAdvisoryAgentTask: dependencies.runAdvisoryAgentTask, runQueuedPlanningReview,
            onResult(result, step) { if (step.operation === "advisory") latestDecision = result; },
          });
        }
      }
      if (requestedMode === "recoverSavedResult" && (work.kind === "generation"
        || !["paused", "blocked"].includes(work.status) || work.resumeMode !== "recoverSavedResult")) {
        throw new Error("已保存结果的恢复条件已变化，请重新读取任务；不会改为调用模型。");
      }
      if (origin === "automatic" && !durableWake && work.status !== "running") return work;
      if (work.status === "running" && (work.kind !== "generation" || work.threadId !== "")) await dependencies.acquireWorkFlowOwner(work);
      if (work.kind === "generation") {
        if (!dependencies.executionContext().codexAdapter.enabled) throw new Error("需要可用模型才能继续执行；请先完成模型配置与验证。");
        const runId = workTaskId.slice("work-".length);
        if (generationHandoff) {
          const admission = await dependencies.runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/tasks/${encodeURIComponent(generationHandoff.sourceTaskId)}/generation-handoff/execute`, {
            method: "POST", body: JSON.stringify({ proposalId: generationHandoff.proposalId }),
          });
          if (admission.run?.id !== runId || admission.receipt?.runId !== runId) throw Error("原任务交接回执不匹配");
          if (!admission.shouldExecute) return dependencies.runtimeRequest(path);
          work = await dependencies.runtimeRequest(path);
        }
        let currentRun = await dependencies.runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/generation-runs/${encodeURIComponent(runId)}`);
        currentRun = await reconcileGenerationProviderUsage(dependencies.runtimeRequest, projectId, currentRun);
        if (!generationHandoff && await rebindIdleContinuation(dependencies.runtimeRequest, projectId, runId, work, currentRun)) {
          work = await dependencies.runtimeRequest(path);
          currentRun = await dependencies.runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/generation-runs/${encodeURIComponent(runId)}`);
        }
        if (work.status === "blocked") {
          if (!currentRun.activeTaskId) throw Error("原任务没有可重试的失败步骤，请先核对阻塞原因和保存结果。");
          const result = await dependencies.retryGenerationRun(event, projectId, runId);
          const refreshed = await dependencies.runtimeRequest(path);
          return result.resumeFailure ? { ...refreshed, resumeFailure: result.resumeFailure } : refreshed;
        }
        if (work.status === "paused") {
          await dependencies.workControlCommands.send(work, "resume");
        }
        const result = await dependencies.continueGenerationRun(event, projectId, runId);
        const refreshed = await dependencies.runtimeRequest(path);
        return result.resumeFailure ? { ...refreshed, resumeFailure: result.resumeFailure } : refreshed;
      }
      work = await reconcileWorkProviderUsage(dependencies.runtimeRequest, projectId, work);
      if (requestedMode === "recoverSavedResult" && work.resumeMode !== "recoverSavedResult") {
        throw new Error("已保存结果的恢复条件已变化，请重新读取任务；不会改为调用模型。");
      }
      if (["paused", "blocked"].includes(work.status) && work.resumeMode === "recoverSavedResult") {
        const recovered = await dependencies.workControlCommands.send(work, "recoverSavedResult");
        savedResultRecovered = true;
        return recovered;
      }
      if (work.kind === "conversation" && work.status !== "cancelled") {
        const reviewIds = [...new Set(work.steps.filter(step => step.operation === "reviewPlanningSeat").map(step => step.target.reviewRunId))];
        for (const runId of reviewIds) {
          if (work.steps.filter(step => step.target.reviewRunId === runId).every(step => step.status === "completed")) {
            if (work.status === "paused") work = await dependencies.workControlCommands.send(work, "resume");
            await runQueuedPlanningReview(event, projectId, work, runId);
          }
        }
        if (reviewIds.length) work = await dependencies.runtimeRequest(path);
      }
      const isPlanningReview = work.kind === "direct" && work.steps.some(step => step.operation === "reviewPlanningSeat");
      if (work.status === "cancelled") return work;
      if (isPlanningReview && work.steps.every(step => step.status === "completed")) {
        if (work.status === "paused") work = await dependencies.workControlCommands.send(work, "resume");
        const continuation = await readPlanningReviewContinuation(dependencies.runtimeRequest, projectId, work);
        if (continuation) await dependencies.runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/planning-reviews/${encodeURIComponent(continuation.run.id)}/finalize`,
          { method: "POST", body: "{}" });
        return dependencies.runtimeRequest(path);
      }
      if (!dependencies.executionContext().codexAdapter.enabled) throw new Error("需要可用模型才能继续执行；请先完成模型配置与验证。");
      if (work.status !== "completed") work = await ensureWorkBudgetAuthorization({ runtimeRequest: dependencies.runtimeRequest, projectId, work, origin,
        databasePath: dependencies.executionContext().agentStorage?.budgetDb,
        getWindow: () => BrowserWindow.fromWebContents(event.sender) });
      if (["paused", "blocked"].includes(work.status)) work = await dependencies.workControlCommands.send(work, "resume");
      if (work.status === "running") await dependencies.acquireWorkFlowOwner(work);
      if (isPlanningReview) {
        const continuation = await readPlanningReviewContinuation(dependencies.runtimeRequest, projectId, work);
        if (continuation?.roles.length) await dependencies.runPlanningReview(event, projectId, continuation.run, continuation);
        else if (continuation) await dependencies.runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/planning-reviews/${encodeURIComponent(continuation.run.id)}/finalize`,
          { method: "POST", body: "{}" });
        return dependencies.runtimeRequest(path);
      }
      return await advanceWorkSteps(event, projectId, workTaskId, {
        runtimeRequest: dependencies.runtimeRequest, currentWorkFlowState: dependencies.currentWorkFlowState, runCreativeAgentTask, runAdvisoryAgentTask: dependencies.runAdvisoryAgentTask,
        runQueuedPlanningReview,
        onResult(result, step) { if (step.operation === "advisory") latestDecision = result; },
      });
      } catch (error) {
        return recoverUnstartedWorkResume({ runtimeRequest: dependencies.runtimeRequest, controlWork: (work, command) => dependencies.workControlCommands.send(work, command), projectId, workTaskId, owner: resumeOwner,
          provenance: unstartedCreativeAttempts.get(error), error });
      }
    });
    if (savedResultRecovered || requestedMode === "recoverSavedResult" || resumed.kind === "generation" || resumed.status !== "completed") return resumed;
    const delivered = await executeWorkContinuation(dependencies.runtimeRequest, projectId, { ...latestDecision, workTask: resumed },
      request => resumeWorkTask(event, projectId, request.workTaskId, request));
    return { ...resumed, ...(delivered.workTask ? { continuedWorkTask: delivered.workTask } : {}),
      ...(delivered.executionFailure ? { resumeFailure: delivered.executionFailure } : {}) };
  }

    return { failedCreativeAttempts, unstartedCreativeAttempts, runCreativeAgentTask, candidateActionDependencies, runQueuedPlanningReview, modelChapterKnowledge, continueCreativeWork, runCandidateWorkflow, continueFailedCreativeWork, resumeWorkTask };
}
