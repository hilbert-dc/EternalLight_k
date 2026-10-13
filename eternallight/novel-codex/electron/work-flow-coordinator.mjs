// @ts-check
import { createTaskBoundHandoff, taskBoundInstructions, taskMethodText } from "./workflow-handoff.mjs";
import { readWorkExecutionBinding } from "./work-execution-binding.mjs";
import { composeNovelCodexPrompt } from "./codex-runtime.mjs";
import { executionConditionDigest } from "./execution-condition.mjs";
import { requestTaskCancellation } from "@eternallight/agent-engine";
import { persistTaskFinalReply } from "./agent-final-reply.mjs";
import { randomUUID } from "node:crypto";

// Explicit live capabilities avoid stale configuration snapshots during concurrent execution.
/**
 * @param {{
 *   readonly runtimeRequest: ReturnType<typeof import("./runtime-service.mjs").createRuntimeService>["runtimeRequest"],
 *   readonly executionContext: import("./execution-domains.mjs").ExecutionDomains["current"],
 *   readonly executionRequests: import("./execution-requests.mjs").ExecutionRequests,
 *   readonly modelConnectionGate: import("./model-connection-gate.mjs").ModelConnectionGate,
 *   readonly workExecutionOwners: ReturnType<typeof import("./automatic-work-consumers.mjs").createAutomaticWorkConsumers>["workExecutionOwners"],
 *   readonly workExecutionConsumer: ReturnType<typeof import("./automatic-work-consumers.mjs").createAutomaticWorkConsumers>["workExecutionConsumer"],
 *   readonly assertCurrentAgentPlan: ReturnType<typeof import("./model-transport.mjs").createModelTransport>["assertCurrentAgentPlan"],
 *   readonly engineQualificationContextDigest: string | null,
 *   readonly baseExecutionDomain: import("./execution-domains.mjs").ExecutionDomains["base"],
 *   readonly workControlCommands: import("./work-control-commands.mjs").WorkControlCommands,
 *   readonly executionDomains: import("./execution-domains.mjs").ExecutionDomains,
 * }} dependencies
 */
export function createWorkFlowCoordinator(dependencies) {
  async function assertThreadProject(projectId, threadId) {
    if (typeof projectId !== "string" || !projectId.trim() || typeof threadId !== "string" || !threadId.trim()) throw new Error("作品或任务 ID 无效");
    const { tasks } = await dependencies.runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/tasks/lookup`, {
      method: "POST", body: JSON.stringify({ threadIds: [threadId] }),
    });
    if (!tasks.length) throw new Error("该任务不属于当前作品");
    return tasks[0];
  }

  async function prepareNovelWorkflow(projectId, prompt, task, purpose) {
    const workspace = await dependencies.runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/creative-workspace`);
    const target = task.target ?? {};
    if (purpose === "writer") {
      const currentChapter = workspace.chapters?.find((chapter) => chapter.id === target.chapterId);
      if (!currentChapter || currentChapter.number !== target.chapterNumber || !Number.isInteger(target.sceneOrder)) {
        throw new Error("任务缺少精确的章节与场景范围，不能启动生成");
      }
    }
    const handoff = createTaskBoundHandoff(projectId, workspace, task);
    const executionBinding = await readWorkExecutionBinding(dependencies.runtimeRequest, projectId, task);
    dependencies.executionContext().codexAdapter.emit({
      method: "agent/workflow",
      params: { workflow: { task, target, revision: workspace.revision, executionBinding } },
    });
    return {
      workflow: { task, target, revision: workspace.revision, executionBinding },
      prompt: composeNovelCodexPrompt(task.operation === "reviewQuality" ? "" : taskMethodText(task), task.inputManifest.authorIntent, taskBoundInstructions(handoff),
        { internal: true, binding: task.operation === "reviewQuality" ? {task:{taskId:task.id,projectId,threadId:task.threadId,expectedRevision:task.inputRevision,operation:task.operation,target:{type:"project",qualityPacketId:task.target.qualityPacketId}}} : handoff, authorBackground: !["assessRevisionOutcome", "reviewQuality"].includes(task.operation), background: task.operation === "reviewQuality" ? null : { candidateInstruction: task.inputManifest.candidateInstruction ?? null } }),
    };
  }

  async function withWorkFlow(projectId, work) {
    dependencies.executionRequests.assertActive();
    while (dependencies.executionContext().recoveryRefreshPromise) await dependencies.executionContext().recoveryRefreshPromise;
    dependencies.modelConnectionGate.assertExecutionAllowed();
    if (dependencies.executionContext().activeWorkFlow || dependencies.executionContext().activeAgentExecution || dependencies.executionContext().activeGenerationRunId) throw new Error("已有创作任务正在推进");
    const flow = { projectId, id: dependencies.executionContext().workId ?? null, runId: null, cancelled: false, ownership: null, ownerInvalid: false };
    const request = dependencies.executionRequests.assertActive();
    if (request) request.flow = flow;
    dependencies.executionContext().activeWorkFlow = flow;
    try { return await work(flow); }
    finally {
      if (flow.ownership && !flow.ownership.borrowed) {
        await dependencies.workExecutionOwners.release(flow.ownership.record).catch(error => {
          console.warn(`[novel-codex] Work owner release held: ${error.code ?? "invalid-response"}`);
        });
      }
      if (dependencies.executionContext().activeWorkFlow === flow) dependencies.executionContext().activeWorkFlow = null;
      dependencies.workExecutionConsumer.poke();
    }
  }

  async function prepareWorkExecutionCondition(projectId, workId) {
    const plan = await dependencies.assertCurrentAgentPlan(null);
    const digest = executionConditionDigest(plan, dependencies.engineQualificationContextDigest);
    await dependencies.runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/work-tasks/${encodeURIComponent(workId)}/execution-condition`, { method: "POST", body: JSON.stringify({ digest }) });
  }

  async function acquireWorkFlowOwner(work) {
    const flow = dependencies.executionContext().activeWorkFlow;
    if (!flow) return;
    if (flow.projectId !== work.projectId || flow.id && flow.id !== work.id) {
      throw new Error("执行归属不属于当前创作任务");
    }
    if (flow.ownerInvalid) {
      // Local owner loss is an interrupted Attempt, not a foreign identity or a
      // new author cancellation. Native still decides recovery after settlement.
      const execution = dependencies.executionContext().activeAgentExecution;
      if (execution) {
        requestTaskCancellation(execution);
        if (execution.task) requestTaskCancellation(execution.task);
      }
      throw Object.assign(new Error("原任务执行归属已失效，已停止后续发送"), { code: "WORK_EXECUTION_OWNER_INVALID" });
    }
    flow.id = work.id;
    await prepareWorkExecutionCondition(flow.projectId, work.id);
    if (flow.ownership) {
      if (flow.ownership.record.work.id !== work.id) throw new Error("创作任务执行归属已变化");
      await dependencies.workExecutionOwners.verify(flow.ownership.record);
    } else {
      flow.ownership = await dependencies.workExecutionOwners.acquire(flow.projectId, work);
    }
  }

  async function currentWorkFlowState() {
    const flow = dependencies.executionContext().activeWorkFlow;
    if (!flow) return "running";
    if (flow.cancelled || flow.stopRequested) throw Object.assign(new Error("生成已停止"), { code: "TASK_CANCELLED" });
    if (flow.ownerInvalid) throw Object.assign(new Error("原任务执行归属已失效，已停止后续发送"), { code: "WORK_EXECUTION_OWNER_INVALID" });
    if (!flow.id) return "running";
    const state = await dependencies.runtimeRequest(`/api/projects/${encodeURIComponent(flow.projectId)}/work-tasks/${encodeURIComponent(flow.id)}`);
    if (state.status === "cancelled") throw Object.assign(new Error("生成已停止"), { code: "TASK_CANCELLED" });
    return state.status;
  }

  async function bindWorkFlowAttempt(projectId, taskId) {
    if (!dependencies.executionContext().activeWorkFlow) return;
    const work = await dependencies.runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/tasks/${encodeURIComponent(taskId)}/work-task`);
    if (dependencies.executionContext().activeWorkFlow.id && dependencies.executionContext().activeWorkFlow.id !== work.id) throw new Error("执行尝试不属于当前创作任务");
    dependencies.executionContext().activeWorkFlow.id = work.id;
    if (dependencies.executionContext() !== dependencies.baseExecutionDomain) dependencies.executionContext().threadId = work.threadId;
    if (["paused", "cancelled"].includes(work.status)) {
      // Control can commit after Native creates the attempt but before the Host
      // knows its Work id. Keep that committed state and never start its lease.
      if (dependencies.executionContext().activeAgentExecution?.task?.id === taskId) {
        requestTaskCancellation(dependencies.executionContext().activeAgentExecution);
        requestTaskCancellation(dependencies.executionContext().activeAgentExecution.task);
      }
      throw Object.assign(new Error(work.status === "paused" ? "创作任务已暂停" : "生成已停止"), { code: "TASK_CANCELLED" });
    }
    if (dependencies.executionContext().activeWorkFlow.cancelled || dependencies.executionContext().activeWorkFlow.stopRequested) {
      await dependencies.workControlCommands.send(work, dependencies.executionContext().activeWorkFlow.stopRequested ? "pause" : "cancel");
      throw Object.assign(new Error("生成已停止"), { code: "TASK_CANCELLED" });
    }
    await acquireWorkFlowOwner(work);
  }

  async function bindCodexTurn(projectId, task, threadId, result) {
    const turnId = result?.turn?.id ?? result?.turnId;
    if (typeof turnId !== "string" || !turnId || turnId.length > 160) throw new Error("最终回复缺少有效回合绑定");
    await dependencies.runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/tasks/${encodeURIComponent(task.id)}/turn`, {
      method: "POST", body: JSON.stringify({ threadId, turnId }),
    });
    return persistTaskFinalReply(dependencies.runtimeRequest, projectId, task, result);
  }

  async function executionIdentity(channel, args) {
    dependencies.modelConnectionGate.assertExecutionAllowed();
    if (!dependencies.baseExecutionDomain.codexAdapter.enabled) throw Error("创作引擎未配置");
    const projectId = args[0];
    let threadId = null, workId = null;
    if (channel === "agent:send-prompt") threadId = args[4] ?? null;
    else if (["agent:review-thread", "agent:compact-thread"].includes(channel)) threadId = args[1];
    else if (["agent:resume-work-task", "agent:recover-work-task"].includes(channel)) workId = args[1];
    else if (["agent:continue-work", "agent:retry-work", "agent:resolve-generation-feedback"].includes(channel)) workId = `work-${args[1]}`;
    else if (["agent:approve-action", "agent:confirm-execution-scope"].includes(channel)) {
      const work = await dependencies.runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/tasks/${encodeURIComponent(args[1])}/work-task`);
      if (work.projectId !== projectId || typeof work.id !== "string" || !work.id
        || typeof work.threadId !== "string" || !work.threadId) throw Error("执行归属不匹配");
      workId = work.id; threadId = work.threadId;
    }
    if (workId && !threadId) {
      const work = await dependencies.runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/work-tasks/${encodeURIComponent(workId)}`);
      if (work.id !== workId || work.projectId !== projectId) throw Error("执行归属不匹配");
      threadId = work.threadId || null;
    }
  return { projectId, threadId, workId, requestId: dependencies.executionRequests.current()?.requestId ?? randomUUID() };
  }

  function controlDomain(channel, args) {
    if (channel === "agent:interrupt") return dependencies.executionDomains.find(args[0] ?? {});
    if (channel === "agent:control-work-task") return dependencies.executionDomains.find({ projectId: args[0], workId: args[1] });
    if (channel === "agent:steer") return dependencies.executionDomains.find({ projectId: args[0], threadId: args[2] });
    if (["agent:reconnect-thread", "agent:list-background-terminals", "agent:terminate-background-terminal"].includes(channel)) return dependencies.executionDomains.find({ projectId: args[0], threadId: args[1] });
    if (["agent:add-future-intent", "agent:revise-current-chapter"].includes(channel)) return dependencies.executionDomains.find({ projectId: args[0], workId: `work-${args[1]}` });
    if (channel === "agent:resolve-user-input") return dependencies.executionDomains.active().find(domain => domain.id === args[0]?.executionId && domain.projectId === args[0]?.projectId) ?? null;
    return null;
  }

    return { assertThreadProject, prepareNovelWorkflow, withWorkFlow, prepareWorkExecutionCondition, acquireWorkFlowOwner, currentWorkFlowState, bindWorkFlowAttempt, bindCodexTurn, executionIdentity, controlDomain };
}
