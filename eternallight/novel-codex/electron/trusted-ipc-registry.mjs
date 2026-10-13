// @ts-check
import { app, ipcMain } from "electron";
import { NativeRuntimeError } from "./runtime-request-error.mjs";
import { coreTurnFailure } from "./core-turn-failure.mjs";
import { EXECUTION_CHANNELS } from "./execution-requests.mjs";
import { registerWorkspaceObservationHandlers } from "./workspace-observation-handlers.mjs";
import { registerModelConfigurationHandlers } from "./model-configuration-handlers.mjs";
import { registerSessionControlHandlers } from "./session-control-handlers.mjs";
import { registerCreativeExecutionHandlers } from "./creative-execution-handlers.mjs";

// Explicit live capabilities avoid stale configuration snapshots during concurrent execution.
/**
 * @param {{
 *   readonly assertTrustedIpcSender: ReturnType<typeof import("./desktop-window.mjs").createDesktopWindow>["assertTrustedIpcSender"],
 *   businessIpcRegistered: boolean,
 *   readonly executionIdentity: ReturnType<typeof import("./work-flow-coordinator.mjs").createWorkFlowCoordinator>["executionIdentity"],
 *   readonly executionDomains: import("./execution-domains.mjs").ExecutionDomains,
 *   readonly controlDomain: ReturnType<typeof import("./work-flow-coordinator.mjs").createWorkFlowCoordinator>["controlDomain"],
 *   messageSendPrompt: (event: import("electron").IpcMainInvokeEvent, ...args: unknown[]) => Promise<unknown>,
 *   readonly executionRequests: import("./execution-requests.mjs").ExecutionRequests,
 *   readonly conversationMessages: ReturnType<typeof import("./conversation-delivery.mjs").createConversationDelivery>["conversationMessages"],
 *   readonly workInterludes: import("./work-interludes.mjs").WorkInterludes,
 *   readonly runtimeRequest: ReturnType<typeof import("./runtime-service.mjs").createRuntimeService>["runtimeRequest"],
 *   readonly withWorkFlow: ReturnType<typeof import("./work-flow-coordinator.mjs").createWorkFlowCoordinator>["withWorkFlow"],
 *   readonly executionContext: import("./execution-domains.mjs").ExecutionDomains["current"],
 *   readonly currentWorkFlowState: ReturnType<typeof import("./work-flow-coordinator.mjs").createWorkFlowCoordinator>["currentWorkFlowState"],
 *   readonly runCreativeAgentTask: ReturnType<typeof import("./creative-task-coordinator.mjs").createCreativeTaskCoordinator>["runCreativeAgentTask"],
 *   readonly runAdvisoryAgentTask: ReturnType<typeof import("./advisory-coordinator.mjs").createAdvisoryCoordinator>["runAdvisoryAgentTask"],
 *   readonly runQueuedPlanningReview: ReturnType<typeof import("./creative-task-coordinator.mjs").createCreativeTaskCoordinator>["runQueuedPlanningReview"],
 *   readonly reconcileGenerationUsage: ReturnType<typeof import("./generation-coordinator.mjs").createGenerationCoordinator>["reconcileGenerationUsage"],
 *   readonly chapterStructureConsumer: ReturnType<typeof import("./automatic-work-consumers.mjs").createAutomaticWorkConsumers>["chapterStructureConsumer"],
 *   readonly workExecutionConsumer: ReturnType<typeof import("./automatic-work-consumers.mjs").createAutomaticWorkConsumers>["workExecutionConsumer"],
 *   readonly listGenerationRunsWithUsage: ReturnType<typeof import("./generation-coordinator.mjs").createGenerationCoordinator>["listGenerationRunsWithUsage"],
 *   readonly workControlCommands: import("./work-control-commands.mjs").WorkControlCommands,
 *   readonly interruptAgentTurn: ReturnType<typeof import("./agent-turn-runtime.mjs").createAgentTurnRuntime>["interruptAgentTurn"],
 *   multimediaHost: import("./media-host.mjs").MultimediaHost,
 *   workspaceContext: ReturnType<typeof import("./workspace-edit-context.mjs").workspaceEditContext> | null,
 *   readonly assertCurrentAgentPlan: ReturnType<typeof import("./model-transport.mjs").createModelTransport>["assertCurrentAgentPlan"],
 *   readonly refreshRecoveryReport: ReturnType<typeof import("./agent-turn-runtime.mjs").createAgentTurnRuntime>["refreshRecoveryReport"],
 *   readonly modelConnectionGate: import("./model-connection-gate.mjs").ModelConnectionGate,
 *   readonly prepareModelTransport: ReturnType<typeof import("./model-transport.mjs").createModelTransport>["prepareModelTransport"],
 *   readonly projectRoot: string,
 *   readonly runtimeCandidateToken: string,
 *   readonly ensureCodexConfig: ReturnType<typeof import("./model-transport.mjs").createModelTransport>["ensureCodexConfig"],
 *   engineQualificationContextDigest: string | null,
 *   agentCli: ReturnType<typeof import("./codex-runtime.mjs").resolveCodexCli> | null,
 *   engineBaseInstructions: string,
 *   configurationRequestEpoch: number,
 *   runtimeUrl: string,
 *   readonly forwardAgentEvent: ReturnType<typeof import("./agent-turn-runtime.mjs").createAgentTurnRuntime>["forwardAgentEvent"],
 *   readonly assertThreadProject: ReturnType<typeof import("./work-flow-coordinator.mjs").createWorkFlowCoordinator>["assertThreadProject"],
 *   readonly runCoreTurnWithLease: ReturnType<typeof import("./agent-turn-runtime.mjs").createAgentTurnRuntime>["runCoreTurnWithLease"],
 *   readonly candidateActionDependencies: ReturnType<typeof import("./creative-task-coordinator.mjs").createCreativeTaskCoordinator>["candidateActionDependencies"],
 *   readonly resumeWorkTask: ReturnType<typeof import("./creative-task-coordinator.mjs").createCreativeTaskCoordinator>["resumeWorkTask"],
 *   readonly runCandidateWorkflow: ReturnType<typeof import("./creative-task-coordinator.mjs").createCreativeTaskCoordinator>["runCandidateWorkflow"],
 *   readonly startGenerationRun: ReturnType<typeof import("./generation-coordinator.mjs").createGenerationCoordinator>["startGenerationRun"],
 *   readonly continueGenerationRun: ReturnType<typeof import("./generation-coordinator.mjs").createGenerationCoordinator>["continueGenerationRun"],
 *   readonly retryGenerationRun: ReturnType<typeof import("./generation-coordinator.mjs").createGenerationCoordinator>["retryGenerationRun"],
 *   readonly submitGenerationIntent: ReturnType<typeof import("./generation-coordinator.mjs").createGenerationCoordinator>["submitGenerationIntent"],
 *   readonly continueCreativeWork: ReturnType<typeof import("./creative-task-coordinator.mjs").createCreativeTaskCoordinator>["continueCreativeWork"],
 *   readonly modelChapterKnowledge: ReturnType<typeof import("./creative-task-coordinator.mjs").createCreativeTaskCoordinator>["modelChapterKnowledge"],
 *   readonly runPlanningReview: ReturnType<typeof import("./planning-review-coordinator.mjs").createPlanningReviewCoordinator>["runPlanningReview"],
 *   readonly baseExecutionDomain: import("./execution-domains.mjs").ExecutionDomains["base"],
 *   readonly sendAgentMessage: ReturnType<typeof import("./agent-turn-runtime.mjs").createAgentTurnRuntime>["sendAgentMessage"],
 * }} dependencies
 */
export function createTrustedIpcRegistry(dependencies) {
  const electronIpcMain = ipcMain;

  const trustedIpcMain = {
    handle(channel, listener) {
      electronIpcMain.handle(channel, async (event, ...args) => {
        dependencies.assertTrustedIpcSender(event);
        try { return await listener(event, ...args); }
        catch (error) {
          if (error?.code === "TASK_CANCELLED") return { executionRequestFailure: { code: "TASK_CANCELLED", message: "生成已停止" } };
          const failure = coreTurnFailure(error);
          if (failure) return { coreTurnFailure: failure };
          if (error instanceof NativeRuntimeError) return { nativeRequestFailure: {
            issuer: error.issuer, phase: error.phase, code: error.code, message: error.message, details: error.details,
          } };
          throw error;
        }
      });
    },
  };

  function registerIpcHandlers(registry = trustedIpcMain) {
    if (dependencies.businessIpcRegistered) return;
    const registered = [];
    const ipcMain = { handle(channel, listener) {
      const scoped = (event, ...args) => {
        // Saved-result recovery only reconciles durable Native facts. It must not
        // prepare a provider domain or turn into paid execution when state changes.
        if (EXECUTION_CHANNELS.has(channel) && channel !== "agent:recover-work-task") return dependencies.executionIdentity(channel, args).then(identity => dependencies.executionDomains.run(identity, () => listener(event, ...args)));
        const domain = dependencies.controlDomain(channel, args);
        return domain ? dependencies.executionDomains.enter(domain, () => listener(event, ...args)) : listener(event, ...args);
      };
      if (channel === "agent:send-prompt") dependencies.messageSendPrompt = scoped;
      registry.handle(channel, dependencies.executionRequests.wrap(channel, scoped));
      registered.push(channel);
    } };
    try {
    registerWorkspaceObservationHandlers(ipcMain, {
      app,
      conversationMessages: dependencies.conversationMessages,
      workInterludes: dependencies.workInterludes,
      executionDomains: dependencies.executionDomains,
      runtimeRequest: dependencies.runtimeRequest,
      withWorkFlow: dependencies.withWorkFlow,
      executionContext: dependencies.executionContext,
      currentWorkFlowState: dependencies.currentWorkFlowState,
      runCreativeAgentTask: dependencies.runCreativeAgentTask,
      runAdvisoryAgentTask: dependencies.runAdvisoryAgentTask,
      runQueuedPlanningReview: dependencies.runQueuedPlanningReview,
      reconcileGenerationUsage: dependencies.reconcileGenerationUsage,
      chapterStructureConsumer: dependencies.chapterStructureConsumer,
      workExecutionConsumer: dependencies.workExecutionConsumer,
      listGenerationRunsWithUsage: dependencies.listGenerationRunsWithUsage,
      workControlCommands: dependencies.workControlCommands,
      interruptAgentTurn: dependencies.interruptAgentTurn,
      state: { get multimediaHost() { return dependencies.multimediaHost; }, set multimediaHost(value) { dependencies.multimediaHost = value; }, get workspaceContext() { return dependencies.workspaceContext; }, set workspaceContext(value) { dependencies.workspaceContext = value; } }
    });
    registerModelConfigurationHandlers(ipcMain, {
      app,
      executionContext: dependencies.executionContext,
      assertCurrentAgentPlan: dependencies.assertCurrentAgentPlan,
      workExecutionConsumer: dependencies.workExecutionConsumer,
      refreshRecoveryReport: dependencies.refreshRecoveryReport,
      modelConnectionGate: dependencies.modelConnectionGate,
      prepareModelTransport: dependencies.prepareModelTransport,
      projectRoot: dependencies.projectRoot,
      runtimeCandidateToken: dependencies.runtimeCandidateToken,
      ensureCodexConfig: dependencies.ensureCodexConfig,
      state: { get engineQualificationContextDigest() { return dependencies.engineQualificationContextDigest; }, set engineQualificationContextDigest(value) { dependencies.engineQualificationContextDigest = value; }, get agentCli() { return dependencies.agentCli; }, set agentCli(value) { dependencies.agentCli = value; }, get engineBaseInstructions() { return dependencies.engineBaseInstructions; }, set engineBaseInstructions(value) { dependencies.engineBaseInstructions = value; }, get configurationRequestEpoch() { return dependencies.configurationRequestEpoch; }, set configurationRequestEpoch(value) { dependencies.configurationRequestEpoch = value; }, get runtimeUrl() { return dependencies.runtimeUrl; }, set runtimeUrl(value) { dependencies.runtimeUrl = value; } }
    });
    registerSessionControlHandlers(ipcMain, {
      executionRequests: dependencies.executionRequests,
      runtimeRequest: dependencies.runtimeRequest,
      workControlCommands: dependencies.workControlCommands,
      executionContext: dependencies.executionContext,
      interruptAgentTurn: dependencies.interruptAgentTurn,
      forwardAgentEvent: dependencies.forwardAgentEvent,
      modelConnectionGate: dependencies.modelConnectionGate,
      assertThreadProject: dependencies.assertThreadProject,
      runCoreTurnWithLease: dependencies.runCoreTurnWithLease,
      withWorkFlow: dependencies.withWorkFlow,
      runAdvisoryAgentTask: dependencies.runAdvisoryAgentTask,
      candidateActionDependencies: dependencies.candidateActionDependencies,
      resumeWorkTask: dependencies.resumeWorkTask,
      state: { get multimediaHost() { return dependencies.multimediaHost; }, set multimediaHost(value) { dependencies.multimediaHost = value; } }
    });
    registerCreativeExecutionHandlers(ipcMain, {
      runtimeRequest: dependencies.runtimeRequest,
      withWorkFlow: dependencies.withWorkFlow,
      candidateActionDependencies: dependencies.candidateActionDependencies,
      runCreativeAgentTask: dependencies.runCreativeAgentTask,
      runCandidateWorkflow: dependencies.runCandidateWorkflow,
      startGenerationRun: dependencies.startGenerationRun,
      continueGenerationRun: dependencies.continueGenerationRun,
      retryGenerationRun: dependencies.retryGenerationRun,
      executionContext: dependencies.executionContext,
      resumeWorkTask: dependencies.resumeWorkTask,
      workControlCommands: dependencies.workControlCommands,
      interruptAgentTurn: dependencies.interruptAgentTurn,
      submitGenerationIntent: dependencies.submitGenerationIntent,
      runAdvisoryAgentTask: dependencies.runAdvisoryAgentTask,
      continueCreativeWork: dependencies.continueCreativeWork,
      modelChapterKnowledge: dependencies.modelChapterKnowledge,
      runPlanningReview: dependencies.runPlanningReview,
      baseExecutionDomain: dependencies.baseExecutionDomain,
      sendAgentMessage: dependencies.sendAgentMessage
    });
    } catch (error) {
      for (const channel of registered) electronIpcMain.removeHandler(channel);
      throw error;
    }
    dependencies.businessIpcRegistered = true;
  }

  return { trustedIpcMain, registerIpcHandlers };
}
