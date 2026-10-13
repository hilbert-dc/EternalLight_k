// @ts-check
import { registerConversationMessageHandlers } from "./conversation-messages.mjs";
import { validateConversationId } from "./conversation-handlers.mjs";
import { ExecutionEventJournal } from "../shared/execution-event-journal.mjs";
import { registerMultimediaHandlers } from "./media-host.mjs";
import { AuthorSessionStore, registerAuthorSessionHandlers } from "./author-session-store.mjs";
import { join } from "node:path";
import { registerActionPermissionHandlers } from "./action-permission-handlers.mjs";
import { registerRevisionDecisionHandlers, registerRevisionOutcomeHandlers } from "./revision-outcome-handlers.mjs";
import { registerExecutionScopeHandlers } from "./execution-scope-handlers.mjs";
import { assertProviderOperation } from "./codex-runtime.mjs";
import { advanceWorkSteps } from "./work-step-continuation.mjs";
import { registerConditionEvaluationHandlers } from "./condition-evaluation-handlers.mjs";
import { registerGenerationReplanHandlers } from "./generation-replan-handlers.mjs";
import { registerChapterRebindHandlers } from "./chapter-rebind-handlers.mjs";
import { registerCommitmentHandlers } from "./commitment-handlers.mjs";
import { registerChapterStructureReadHandlers } from "./chapter-structure-analysis.mjs";
import { workspaceEditContext } from "./workspace-edit-context.mjs";
import { registerProjectArtifactHandlers } from "./project-artifact-handlers.mjs";
import { registerKnowledgeProposalHandlers } from "./knowledge-proposal-handlers.mjs";
import { registerIndexAuditHandlers } from "./index-audit-handlers.mjs";
import { registerPlanningReviewReadHandler } from "./conversation-planning-reviews.mjs";
import { registerWorkCatalog } from "./work-catalog.mjs";
import { registerChapterDeletion } from "./chapter-deletion.mjs";
import { requestTaskCancellation } from "@eternallight/agent-engine";

// Registered only through the trusted, execution-scoped IPC registry.
export function registerWorkspaceObservationHandlers(ipcMain, { app, conversationMessages, workInterludes, executionDomains, runtimeRequest, withWorkFlow, executionContext, currentWorkFlowState, runCreativeAgentTask, runAdvisoryAgentTask, runQueuedPlanningReview, reconcileGenerationUsage, chapterStructureConsumer, workExecutionConsumer, listGenerationRunsWithUsage, workControlCommands, interruptAgentTurn, state }) {
  registerConversationMessageHandlers(ipcMain, conversationMessages);
  ipcMain.handle("agent:begin-interlude",(_event,project,work,input)=>workInterludes.begin(project,work,input));
  ipcMain.handle("agent:return-interlude",(_event,project,work,input)=>workInterludes.return(project,work,input));
  ipcMain.handle("agent:list-executions", async (_event, projectId) => {
    validateConversationId(projectId);
    return executionDomains.active().filter(domain => domain.projectId === projectId && !domain.settled && !domain.closed).map(domain => {
      const adapter = domain.codexAdapter;
      const threadId = adapter?.threadId;
      return { executionId: domain.id, projectId, threadId: domain.threadId, requestId: domain.requestId,
        workflow: domain.workflow ?? null, ...(domain.eventJournal ?? new ExecutionEventJournal()).snapshot(),
        taskId: domain.activeRuntimeTask?.id ?? null, executionThreadId: threadId ?? null, turnId: adapter?.turnId ?? null,
        prompt: domain.activeRuntimeTask?.inputManifest?.authorIntent ?? "",
        questions: [...(adapter?.userInputs.values() ?? [])].map(request => ({projectId, executionId: domain.id,
          requestId: request.id, method: request.method, params: request.params ?? {},
          threadId: request.params?.threadId ?? threadId, turnId: request.params?.turnId ?? adapter.turnId})) };
    });
  });
  registerMultimediaHandlers(ipcMain, state.multimediaHost);
  registerAuthorSessionHandlers(ipcMain, new AuthorSessionStore(join(app.getPath("userData"), "author-session.json")));
  registerActionPermissionHandlers(ipcMain, runtimeRequest);
  registerRevisionOutcomeHandlers(ipcMain, runtimeRequest);
  registerRevisionDecisionHandlers(ipcMain, runtimeRequest);
  registerExecutionScopeHandlers(ipcMain, {
    runtimeRequest, withWorkFlow,
    assertOperationsAvailable: (operations) => {
      if (!executionContext().codexAdapter.enabled) throw new Error("创作引擎未配置，请先选择可用模型；范围仍待确认");
      for (const operation of operations) assertProviderOperation(executionContext().agentProviderConfig, operation);
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
    state.workspaceContext = workspaceEditContext(context);
    chapterStructureConsumer.select(context.projectId === null ? null : { ...context, event });
    if (!context.dirty) workExecutionConsumer.poke();
    return { selected: context.projectId };
  });
  registerProjectArtifactHandlers(ipcMain, runtimeRequest);
  registerKnowledgeProposalHandlers(ipcMain, runtimeRequest);
  ipcMain.handle("runtime:list-generation-runs", (_event, projectId) => listGenerationRunsWithUsage(projectId));
  ipcMain.handle("runtime:list-work-tasks", async (_event, projectId, cursor = null) => {
    if (typeof projectId !== "string" || !projectId.trim()) throw new Error("作品标识无效");
    if (cursor !== null && (typeof cursor !== "string" || !/^[01]:[1-9][0-9]{0,18}:[0-9]{1,19}:[0-9]{1,19}$/.test(cursor))) throw new Error("任务分页标识无效");
    const page = cursor === null ? "" : `?cursor=${encodeURIComponent(cursor)}`;
    await workControlCommands.reconcile(projectId);
    const result = await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/work-tasks${page}`);
    return { ...result, tasks: await Promise.all(result.tasks.map(work => workControlCommands.project(work))) };
  });
  registerIndexAuditHandlers(ipcMain, runtimeRequest);
  ipcMain.handle("runtime:get-work-task", async (_event, projectId, workTaskId) => {
    await workControlCommands.reconcile(projectId);
    return workControlCommands.project(await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/work-tasks/${encodeURIComponent(workTaskId)}`));
  });
  registerPlanningReviewReadHandler(ipcMain, runtimeRequest);
  registerWorkCatalog(ipcMain, runtimeRequest);
  registerChapterDeletion(ipcMain, runtimeRequest);
  ipcMain.handle("runtime:cancel-planning-review", async (_event, projectId, runId) => {
    if (typeof projectId !== "string" || !projectId.trim() || typeof runId !== "string" || !runId.trim()) throw new Error("规划评审标识无效");
    if (executionContext().activeAgentExecution?.projectId === projectId && executionContext().activeAgentExecution?.runId === runId) {
      requestTaskCancellation(executionContext().activeAgentExecution);
      requestTaskCancellation(executionContext().activeAgentExecution.task ?? executionContext().activeRuntimeTask);
      await interruptAgentTurn().catch(() => undefined);
    }
    return runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/planning-reviews/${encodeURIComponent(runId)}/cancel`, { method: "POST", body: "{}" });
  });
}
