// @ts-check
import { recoverUnstartedWorkResume } from "./work-resume-not-started.mjs";
import { listGenerationRunsWithUsage as readGenerationRunsWithUsage } from "./generation-usage-status.mjs";
import { validateAuthorIntent } from "../shared/author-input-budget.mjs";
import { generationPermitForChapters } from "../shared/generation-permit.mjs";
import { assertProviderOperation } from "./codex-runtime.mjs";

// Explicit live capabilities avoid stale configuration snapshots during concurrent execution.
/**
 * @param {{
 *   readonly executionContext: import("./execution-domains.mjs").ExecutionDomains["current"],
 *   readonly runtimeRequest: ReturnType<typeof import("./runtime-service.mjs").createRuntimeService>["runtimeRequest"],
 *   readonly acquireWorkFlowOwner: ReturnType<typeof import("./work-flow-coordinator.mjs").createWorkFlowCoordinator>["acquireWorkFlowOwner"],
 *   readonly currentWorkFlowState: ReturnType<typeof import("./work-flow-coordinator.mjs").createWorkFlowCoordinator>["currentWorkFlowState"],
 *   readonly runAdvisoryAgentTask: ReturnType<typeof import("./advisory-coordinator.mjs").createAdvisoryCoordinator>["runAdvisoryAgentTask"],
 *   readonly runCreativeAgentTask: ReturnType<typeof import("./creative-task-coordinator.mjs").createCreativeTaskCoordinator>["runCreativeAgentTask"],
 *   readonly unstartedCreativeAttempts: ReturnType<typeof import("./creative-task-coordinator.mjs").createCreativeTaskCoordinator>["unstartedCreativeAttempts"],
 *   readonly workControlCommands: import("./work-control-commands.mjs").WorkControlCommands,
 * }} dependencies
 */
export function createGenerationCoordinator(dependencies) {
  async function continueGenerationRun(event, projectId, runId) {
    while (dependencies.executionContext().recoveryRefreshPromise) await dependencies.executionContext().recoveryRefreshPromise;
    if (dependencies.executionContext().activeGenerationRunId) throw new Error("已有自动创作运行正在推进");
    dependencies.executionContext().activeGenerationRunId = runId;
    if (dependencies.executionContext().activeWorkFlow) {
      dependencies.executionContext().activeWorkFlow.runId = runId;
      dependencies.executionContext().activeWorkFlow.id = `work-${runId}`;
    }
    const runBase = `/api/projects/${encodeURIComponent(projectId)}/generation-runs/${encodeURIComponent(runId)}`;
    let dispatchedWork = null;
    try {
      if (dependencies.executionContext().activeWorkFlow) {
        const generationWork = await dependencies.runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/work-tasks/work-${encodeURIComponent(runId)}`);
        // A newly-created generation Work has no root thread before its first
        // real Attempt. Bind that identity at attach, then acquire before send.
        if (generationWork.threadId !== "") await dependencies.acquireWorkFlowOwner(generationWork);
      }
      const dispatched = new Set();
      while (true) {
        if (await dependencies.currentWorkFlowState() === "paused") return (await dependencies.runtimeRequest(runBase));
        const currentWork=await dependencies.runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/work-tasks/${encodeURIComponent(`work-${runId}`)}`);
        if(currentWork.interlude) return dependencies.runtimeRequest(runBase);
        const { run, action } = await dependencies.runtimeRequest(`${runBase}/next`, { method: "POST", body: "{}" });
        if (run.status !== "running" || !action) return run;
        if (action.kind === "awaitTask") {
          if (action.taskStatus !== "completed") return run;
          const charged = await settleGenerationUsage(projectId, run, action.taskId);
          await dependencies.runtimeRequest(`${runBase}/advance-task`, {
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
        dispatchedWork = await dependencies.runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/work-tasks/work-${encodeURIComponent(runId)}`);
        try {
          if (action.operation === "advisory") {
            const work = dispatchedWork;
            const step = work.steps.find(item => item.id === target.resumeStepId && item.operation === "advisory" && item.status === "queued");
            if (!step) throw new Error("自动创作决策步骤不存在");
            await dependencies.runAdvisoryAgentTask(event, projectId, instruction, step, null, null, work.threadId || null);
          } else {
            await dependencies.runCreativeAgentTask(event, projectId, instruction, { operation: action.operation }, target);
          }
        } catch (error) {
          if (error?.code === "TASK_CANCELLED") throw error;
          const provenance = dependencies.unstartedCreativeAttempts.get(error);
          if (provenance) {
            const paused = await recoverUnstartedWorkResume({ runtimeRequest: dependencies.runtimeRequest, controlWork: (work, command) => dependencies.workControlCommands.send(work, command), projectId, workTaskId: `work-${runId}`,
              owner: dispatchedWork, provenance, error });
            const stopped = await dependencies.runtimeRequest(runBase);
            if (stopped.id !== runId || stopped.projectId !== projectId || stopped.status !== "paused") throw error;
            return { ...stopped, resumeFailure: paused.resumeFailure,
              pausedWork: { id: paused.id, projectId: paused.projectId, threadId: paused.threadId, status: paused.status, version: paused.version } };
          }
          // Runtime alone decides whether a settled, known-zero failure can be
          // corrected. Do not replay the failed step or mint another permit.
          const recovery = await dependencies.runtimeRequest(`${runBase}/next`, { method: "POST", body: "{}" }).catch(() => null);
          if (recovery?.run?.status !== "running" || recovery.action?.kind !== "agentTask"
            || recovery.action.operation !== "advisory" || recovery.action.target?.runId !== runId
            || !recovery.action.target.resumeStepId || dispatched.has(recovery.action.target.resumeStepId)) throw error;
        }
      }
    } catch (error) {
      await dependencies.runtimeRequest(`${runBase}/next`, { method: "POST", body: "{}" }).catch(() => undefined);
      throw error;
    } finally {
      dependencies.executionContext().activeGenerationRunId = null;
    }
  }

  async function settleGenerationUsage(projectId, run, taskId) {
    if (run.providerBudget?.charges?.some(charge => charge.taskId === taskId)) return run;
    return dependencies.runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/generation-runs/${encodeURIComponent(run.id)}/provider-usage`, {
      method: "POST", body: JSON.stringify({ taskId }),
    });
  }

  async function reconcileGenerationUsage(projectId, runId) {
    const run = await dependencies.runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/generation-runs/${encodeURIComponent(runId)}`);
    const taskId = run.activeTaskId;
    if (!taskId || run.providerBudget?.charges?.some(charge => charge.taskId === taskId)) return run;
    const task = await dependencies.runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/tasks/${encodeURIComponent(taskId)}`);
    if (!["completed", "failed", "cancelled"].includes(task.status)) return run;
    return settleGenerationUsage(projectId, run, taskId);
  }

  async function listGenerationRunsWithUsage(projectId) {
    return readGenerationRunsWithUsage(dependencies.runtimeRequest, projectId, settleGenerationUsage);
  }

  async function startGenerationRun(event, projectId, authorIntent, targetChapters, expectedProjectRevision, expectedBaseSnapshotId, expectedSourceDigest, revisionPredecessor, generationProposalId = null) {
    if (!dependencies.executionContext().codexAdapter.enabled) throw new Error("Novel Agent Engine 未配置，无法启动自动创作");
    if (typeof projectId !== "string" || !projectId.trim() || /[\r\n]/.test(projectId)) throw new Error("作品 ID 无效");
    validateAuthorIntent(authorIntent);
    const permit = generationPermitForChapters(targetChapters);
    if (!Number.isSafeInteger(expectedProjectRevision) || expectedProjectRevision < 0) throw new Error("创作来源版本无效，请重新读取作品");
    if (typeof expectedBaseSnapshotId !== "string" || !expectedBaseSnapshotId.trim()
      || typeof expectedSourceDigest !== "string" || !/^[a-f0-9]{64}$/.test(expectedSourceDigest)) {
      throw new Error("创作来源快照无效，请重新读取作品");
    }
    if (dependencies.executionContext().activeAgentExecution || dependencies.executionContext().activeGenerationRunId) throw new Error("已有创作引擎任务正在运行");
    const workspace = await dependencies.runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/creative-workspace`);
    if (workspace.revision !== expectedProjectRevision || workspace.headSnapshotId !== expectedBaseSnapshotId
      || workspace.sourceDigest !== expectedSourceDigest) {
      throw new Error("作品已在确认期间变化，请重新核对创作目标与范围");
    }
    assertProviderOperation(dependencies.executionContext().agentProviderConfig, "advisory");
    const run = await dependencies.runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/generation-runs`, {
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
    if (dependencies.executionContext().activeWorkFlow?.cancelled || dependencies.executionContext().activeWorkFlow?.stopRequested) {
      const work = await dependencies.runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/work-tasks/${encodeURIComponent(`work-${run.id}`)}`);
      await dependencies.workControlCommands.send(work, dependencies.executionContext().activeWorkFlow.stopRequested ? "pause" : "cancel");
      throw Object.assign(new Error("生成已停止"), { code: "TASK_CANCELLED" });
    }
    return continueGenerationRun(event, projectId, run.id);
  }

  async function retryGenerationRun(event, projectId, runId) {
    if (!dependencies.executionContext().codexAdapter.enabled) throw new Error("Novel Agent Engine 未配置，无法重试创作");
    if (typeof projectId !== "string" || !projectId.trim() || /[\r\n]/.test(projectId)) throw new Error("作品 ID 无效");
    if (typeof runId !== "string" || !/^generation-[a-f0-9]{32}$/.test(runId)) throw new Error("创作运行 ID 无效");
    if (dependencies.executionContext().activeAgentExecution || dependencies.executionContext().activeGenerationRunId) throw new Error("已有创作引擎任务正在运行");
    const runBase = `/api/projects/${encodeURIComponent(projectId)}/generation-runs/${encodeURIComponent(runId)}`;
    let run = await dependencies.runtimeRequest(runBase);
    if (run.activeTaskId) run = await settleGenerationUsage(projectId, run, run.activeTaskId);
    await dependencies.runtimeRequest(`${runBase}/retry-failed-task`, {
      method: "POST", body: JSON.stringify({ expectedVersion: run.version }),
    });
    return continueGenerationRun(event, projectId, runId);
  }

  async function submitGenerationIntent(projectId, runId, instruction, endpoint) {
    if (typeof projectId !== "string" || !projectId.trim() || /[\r\n]/.test(projectId)) throw new Error("作品 ID 无效");
    if (typeof runId !== "string" || !/^generation-[a-f0-9]{32}$/.test(runId)) throw new Error("创作运行 ID 无效");
    if (typeof instruction !== "string" || !instruction.trim() || Array.from(instruction.trim()).length > 1_000) throw new Error("后续创作要求必须为 1 到 1000 字");
    if ((dependencies.executionContext().activeGenerationRunId && dependencies.executionContext().activeGenerationRunId !== runId)
      || (dependencies.executionContext().activeAgentExecution && dependencies.executionContext().activeGenerationRunId !== runId)) {
      throw new Error("已有其他创作任务正在运行，不能调整此运行的后续要求");
    }
    const runBase = `/api/projects/${encodeURIComponent(projectId)}/generation-runs/${encodeURIComponent(runId)}`;
    const run = await dependencies.runtimeRequest(runBase);
    if (dependencies.executionContext().activeGenerationRunId === runId && !run.activeTaskId) {
      throw new Error("当前创作步骤正在切换，请稍后再调整后续要求");
    }
    return dependencies.runtimeRequest(`${runBase}/${endpoint}`, {
      method: "POST", body: JSON.stringify({ expectedVersion: run.version, instruction: instruction.trim() }),
    });
  }

  return { continueGenerationRun, settleGenerationUsage, reconcileGenerationUsage, listGenerationRunsWithUsage, startGenerationRun, retryGenerationRun, submitGenerationIntent };
}
