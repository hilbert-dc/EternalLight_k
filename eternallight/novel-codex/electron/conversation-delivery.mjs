// @ts-check
import { ConversationMessages } from "./conversation-messages.mjs";
import { messageBlockedByEdits } from "./workspace-edit-context.mjs";

// Explicit live capabilities avoid stale configuration snapshots during concurrent execution.
/**
 * @param {{
 *   readonly executionDomains: import("./execution-domains.mjs").ExecutionDomains,
 *   readonly runtimeRequest: ReturnType<typeof import("./runtime-service.mjs").createRuntimeService>["runtimeRequest"],
 *   readonly executionContext: import("./execution-domains.mjs").ExecutionDomains["current"],
 *   readonly desktopLifecycle: import("./desktop-lifecycle.mjs").DesktopLifecycle,
 *   readonly runtimeError: string | null,
 *   readonly modelConnectionGate: import("./model-connection-gate.mjs").ModelConnectionGate,
 *   readonly workspaceContext: ReturnType<typeof import("./workspace-edit-context.mjs").workspaceEditContext> | null,
 *   readonly forwardAgentEvent: ReturnType<typeof import("./agent-turn-runtime.mjs").createAgentTurnRuntime>["forwardAgentEvent"],
 *   readonly executionRequests: import("./execution-requests.mjs").ExecutionRequests,
 *   readonly withWorkFlow: ReturnType<typeof import("./work-flow-coordinator.mjs").createWorkFlowCoordinator>["withWorkFlow"],
 *   readonly acquireWorkFlowOwner: ReturnType<typeof import("./work-flow-coordinator.mjs").createWorkFlowCoordinator>["acquireWorkFlowOwner"],
 *   readonly runAdvisoryAgentTask: ReturnType<typeof import("./advisory-coordinator.mjs").createAdvisoryCoordinator>["runAdvisoryAgentTask"],
 *   readonly workInterludes: import("./work-interludes.mjs").WorkInterludes,
 *   readonly workExecutionConsumer: ReturnType<typeof import("./automatic-work-consumers.mjs").createAutomaticWorkConsumers>["workExecutionConsumer"],
 *   readonly messageSendPrompt: (event: import("electron").IpcMainInvokeEvent, ...args: unknown[]) => Promise<unknown>,
 * }} dependencies
 */
export function createConversationDelivery(dependencies) {
  const conversationMessages = new ConversationMessages({
    maxActive: dependencies.executionDomains.maxActive,
    bindTrigger: async (projectId, threadId, trigger) => {
      if (trigger?.kind !== "afterTurn") return;
      const domain = dependencies.executionDomains.find({projectId, threadId});
      const task = domain?.activeRuntimeTask, adapter = domain?.codexAdapter;
      if (!task || task.id !== trigger.taskId || task.threadId !== threadId || adapter?.turnId !== trigger.turnId) throw Error("原对话回合已变化，请重新选择等待条件");
      await dependencies.runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/tasks/${encodeURIComponent(task.id)}/turn`, {
        method: "POST", body: JSON.stringify({threadId, turnId: trigger.turnId})});
    },
    request: dependencies.runtimeRequest,
    connectionId: () => dependencies.executionContext().agentProviderConfig?.id ?? null,
    available: message => !dependencies.desktopLifecycle.quitRequested && !dependencies.runtimeError && dependencies.executionContext().codexAdapter.enabled && !dependencies.modelConnectionGate.busy
      && !dependencies.executionContext().activeAgentExecution && !dependencies.executionContext().activeWorkFlow && !dependencies.executionContext().activeGenerationRunId
      && !dependencies.executionDomains.find({ projectId: message.projectId, threadId: message.threadId })
      && dependencies.executionDomains.hasCapacity()
      && !messageBlockedByEdits(dependencies.workspaceContext, message)
      && (!message.input.connectionId || message.input.connectionId === dependencies.executionContext().agentProviderConfig?.id),
    started: message => dependencies.forwardAgentEvent({ method: "agent/message-started", params: { message } }),
    ended: (message, error, result) => dependencies.forwardAgentEvent({ method: "agent/message-ended", params: {
      projectId: message.projectId, threadId: message.threadId, messageId: message.id,
      error: error ?? result?.executionFailure?.message ?? null,
      generationHandoff: !error && !result?.executionFailure && message.status === "delivered"
        && result?.generationHandoff?.projectId === message.projectId
        && result.generationHandoff.threadId === message.threadId
        && result.generationHandoff.sourceTaskId === message.taskId
        && result.generationHandoff.authorIntent === message.input.text ? result.generationHandoff : null,
    } }),
    changed: message => dependencies.forwardAgentEvent({ method: "agent/message-updated", params: { projectId: message.projectId, threadId: message.threadId, messageId: message.id, status: message.status } }),
    execute: (event, delivery) => dependencies.executionRequests.detached(() => dependencies.executionRequests.wrap("agent:send-prompt", async (sender, ...args) => {
      dependencies.executionRequests.current().messageDelivery = delivery;
      if (delivery.message.trigger.kind === "interlude") {
        const message=delivery.message, workId=message.trigger.workTaskId;
        const result=await dependencies.executionDomains.run({projectId:message.projectId,threadId:message.threadId,workId,requestId:message.id},
          () => dependencies.withWorkFlow(message.projectId,async flow => {
            const work=await dependencies.runtimeRequest(`/api/projects/${encodeURIComponent(message.projectId)}/work-tasks/${encodeURIComponent(workId)}`);
            const step=work.steps.find(step=>step.id===work.interlude?.stepId && step.target.interludeId===message.trigger.interludeId);
            if(!step || work.interlude.messageId!==message.id || work.threadId!==message.threadId || work.status!=="running") throw Error("临时提问的原任务已变化");
            flow.id=workId; await dependencies.acquireWorkFlowOwner(work);
            return dependencies.runAdvisoryAgentTask(sender,message.projectId,message.input.text,step,null,message.input.focus,null,message.threadId);
          }));
        await dependencies.workInterludes.afterAnswer(message.projectId,workId);
        dependencies.workExecutionConsumer.poke(); return result;
      }
      return dependencies.messageSendPrompt(sender, ...args);
    })(event, { requestId: delivery.message.id, args: [delivery.message.projectId, delivery.message.input.text, null, delivery.message.input.focus, delivery.message.threadId, delivery.message.input.media ?? null] })),
  });

  return { conversationMessages };
}
