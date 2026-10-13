// @ts-check
import { ChapterStructureAnalysisConsumer } from "./chapter-structure-analysis.mjs";
import { assertProviderOperation } from "./codex-runtime.mjs";
import { WorkExecutionOwners, WorkExecutionEventConsumer } from "./work-execution-events.mjs";
import { requestTaskCancellation } from "@eternallight/agent-engine";

// Explicit live capabilities avoid stale configuration snapshots during concurrent execution.
/**
 * @param {{
 *   readonly assertTrustedIpcSender: ReturnType<typeof import("./desktop-window.mjs").createDesktopWindow>["assertTrustedIpcSender"],
 *   readonly executionContext: import("./execution-domains.mjs").ExecutionDomains["current"],
 *   readonly desktopLifecycle: import("./desktop-lifecycle.mjs").DesktopLifecycle,
 *   readonly modelConnectionGate: import("./model-connection-gate.mjs").ModelConnectionGate,
 *   readonly runtimeRequest: ReturnType<typeof import("./runtime-service.mjs").createRuntimeService>["runtimeRequest"],
 *   readonly executionRequests: import("./execution-requests.mjs").ExecutionRequests,
 *   readonly executionDomains: import("./execution-domains.mjs").ExecutionDomains,
 *   readonly withWorkFlow: ReturnType<typeof import("./work-flow-coordinator.mjs").createWorkFlowCoordinator>["withWorkFlow"],
 *   readonly runCreativeAgentTask: ReturnType<typeof import("./creative-task-coordinator.mjs").createCreativeTaskCoordinator>["runCreativeAgentTask"],
 *   readonly forwardAgentEvent: ReturnType<typeof import("./agent-turn-runtime.mjs").createAgentTurnRuntime>["forwardAgentEvent"],
 *   readonly workControlCommands: import("./work-control-commands.mjs").WorkControlCommands,
 *   readonly interruptAgentTurn: ReturnType<typeof import("./agent-turn-runtime.mjs").createAgentTurnRuntime>["interruptAgentTurn"],
 *   readonly baseExecutionDomain: import("./execution-domains.mjs").ExecutionDomains["base"],
 *   readonly runtimeError: string | null,
 *   readonly runtimeProcess: import("node:child_process").ChildProcess | null,
 *   readonly rendererWebContents: import("electron").WebContents | null,
 *   readonly assertCurrentAgentPlan: ReturnType<typeof import("./model-transport.mjs").createModelTransport>["assertCurrentAgentPlan"],
 *   readonly prepareWorkExecutionCondition: ReturnType<typeof import("./work-flow-coordinator.mjs").createWorkFlowCoordinator>["prepareWorkExecutionCondition"],
 *   readonly workspaceContext: ReturnType<typeof import("./workspace-edit-context.mjs").workspaceEditContext> | null,
 *   readonly conversationMessages: ReturnType<typeof import("./conversation-delivery.mjs").createConversationDelivery>["conversationMessages"],
 *   readonly assertThreadProject: ReturnType<typeof import("./work-flow-coordinator.mjs").createWorkFlowCoordinator>["assertThreadProject"],
 *   readonly resumeWorkTask: ReturnType<typeof import("./creative-task-coordinator.mjs").createCreativeTaskCoordinator>["resumeWorkTask"],
 * }} dependencies
 */
export function createAutomaticWorkConsumers(dependencies) {
  const chapterStructureConsumer = new ChapterStructureAnalysisConsumer({
    available: ({ event }) => {
      try {
        dependencies.assertTrustedIpcSender(event);
        assertProviderOperation(dependencies.executionContext().agentProviderConfig, "assessChapterStructure");
        return !dependencies.desktopLifecycle.quitRequested && dependencies.executionContext().codexAdapter.enabled && !dependencies.modelConnectionGate.busy
          && !dependencies.executionContext().activeWorkFlow && !dependencies.executionContext().activeAgentExecution && !dependencies.executionContext().activeGenerationRunId;
      } catch { return false; }
    },
    list: (projectId, { cursor, limit, assessmentState }) => {
      const query = new URLSearchParams({ limit: String(limit) });
      if (cursor !== null) query.set("cursor", cursor);
      if (assessmentState !== null) query.set("assessmentState", assessmentState);
      return dependencies.runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/chapter-structure-changes?${query}`);
    },
    read: (projectId, id) => dependencies.runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/chapter-structure-changes/${encodeURIComponent(id)}`),
    run: ({ event, projectId }, prompt, taskType, target, isCurrent) => dependencies.executionRequests.detached(() => dependencies.executionDomains.run({ projectId, threadId: null }, () => dependencies.withWorkFlow(projectId, () =>
      dependencies.runCreativeAgentTask(event, projectId, prompt, taskType, target, { assertCurrent: () => {
        if (!isCurrent()) throw new Error("作品或编辑状态已变化，衔接检查尚未继续。");
      } })))),
    updated: ({ projectId }, change, result) => dependencies.forwardAgentEvent({ method: "agent/structure-updated", params: { projectId, changeId: change.id, completed: result !== null } }),
    failed: async ({ projectId }, change, error) => {
      if (error?.code === "TASK_CANCELLED") {
        dependencies.forwardAgentEvent({ method: "agent/structure-updated", params: { projectId, changeId: change?.id ?? null, completed: false } });
        return;
      }
      // Creation may have committed even when its response was lost. Stop the
      // exact bound Work so it cannot remain visibly "running" without a model.
      if (change?.assessment.state === "bound") {
        try {
          const path = `/api/projects/${encodeURIComponent(projectId)}/work-tasks/${encodeURIComponent(change.assessment.workTaskId)}`;
          const work = await dependencies.runtimeRequest(path);
          if (work.status === "running") await dependencies.workControlCommands.send(work, "pause");
        } catch { /* Keep the original error and bound task visible for recovery. */ }
      }
      dependencies.forwardAgentEvent({ method: "structure/error", params: {
        projectId, changeId: change?.id ?? null, taskId: change?.assessment.taskId ?? null, message: `衔接检查未完成：${error.message}`,
      } });
    },
  });

  const workExecutionOwners = new WorkExecutionOwners({ request: dependencies.runtimeRequest,
    invalidated: async (record) => {
      const domain = dependencies.executionDomains.find({ projectId: record.projectId, workId: record.work.id });
      if (!domain) return;
      return dependencies.executionDomains.enter(domain, async () => {
      const flow = dependencies.executionContext().activeWorkFlow;
      if (flow?.projectId !== record.projectId || flow.id !== record.work.id) return;
      flow.ownerInvalid = true;
      if (dependencies.executionContext().activeAgentExecution) {
        requestTaskCancellation(dependencies.executionContext().activeAgentExecution);
        if (dependencies.executionContext().activeAgentExecution.task) requestTaskCancellation(dependencies.executionContext().activeAgentExecution.task);
        await dependencies.interruptAgentTurn(dependencies.executionContext().activeAgentExecution).catch(() => undefined);
      }
      });
    },
  });

  const workExecutionConsumer = new WorkExecutionEventConsumer({
    maxActive: dependencies.executionDomains.maxActive,
    runInContext: operation => dependencies.executionRequests.detached(() => dependencies.executionDomains.enter(dependencies.baseExecutionDomain,operation)),
    request: dependencies.runtimeRequest, owners: workExecutionOwners,
    available: async () => {
      if (dependencies.desktopLifecycle.quitRequested || dependencies.runtimeError || !dependencies.runtimeProcess || !dependencies.rendererWebContents || dependencies.rendererWebContents.isDestroyed()
        || !dependencies.executionContext().codexAdapter.enabled || dependencies.modelConnectionGate.busy || dependencies.executionContext().activeWorkFlow || dependencies.executionContext().activeAgentExecution
        || dependencies.executionContext().activeGenerationRunId || dependencies.executionContext().recoveryRefreshPromise || chapterStructureConsumer.busy || !dependencies.executionDomains.hasCapacity()) return false;
      try { await dependencies.assertCurrentAgentPlan("advisory"); return true; } catch { return false; }
    },
    prepareConditions: dependencies.prepareWorkExecutionCondition,
    canResume: projectId => !dependencies.workspaceContext?.dirty || dependencies.workspaceContext.projectId !== projectId,
    drainMessages: projectId => dependencies.conversationMessages.drain(projectId, { sender: dependencies.rendererWebContents }),
    readThread: async (projectId, threadId) => {
      const reference = await dependencies.assertThreadProject(projectId, threadId);
      const task = await dependencies.runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/tasks/${encodeURIComponent(reference.id)}`);
      if (task.projectId !== projectId || task.threadId !== threadId) throw new Error("原执行对话不属于当前作品");
      const detail = await dependencies.executionContext().codexAdapter.readThread(threadId);
      if (detail.thread?.id !== threadId) throw new Error("创作引擎的原对话身份不匹配");
      return detail.thread;
    },
    resume: (projectId, workId, request) => dependencies.executionRequests.detached(() => dependencies.executionDomains.run({ projectId, threadId: request.expectedThreadId, workId }, () => dependencies.resumeWorkTask({ sender: dependencies.rendererWebContents }, projectId, workId, request))),
    updated: (projectId, event) => dependencies.forwardAgentEvent({ method: "agent/work-updated", params: {
      projectId, workTaskId: event.workTaskId, threadId: event.rootThreadId,
    } }),
    healthChanged: health => dependencies.forwardAgentEvent({ method: "agent/execution-health", params: { health } }),
    failed: (projectId, event, error) => {
      console.warn(`[novel-codex] original Work wake held (${projectId ?? "directory"}/${event?.eventId ?? "scan"}): ${error.code ?? "invalid-response"} ${error.runtimePath ?? "protocol"} ${String(error.message ?? "").slice(0, 600)}`);
    },
  });

  return { chapterStructureConsumer, workExecutionOwners, workExecutionConsumer };
}
