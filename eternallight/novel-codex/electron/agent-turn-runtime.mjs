// @ts-check
import { coreTurnFailure } from "./core-turn-failure.mjs";
import { interruptCommittedGenerationPause } from "./committed-generation-control.mjs";
import { AgentEngineHost, CodexSessionRuntime, PersistentTaskBudget, TaskBudget, assertTaskCompletable, createTaskExecution, requestTaskCancellation } from "@eternallight/agent-engine";
import { observeRecoveryEvent } from "./recovery-event-observer.mjs";
import { reconcileRecoveryIndex } from "./recovery-reconciliation.mjs";
import { withTaskOutputCapacity } from "./task-output-capacity.mjs";
import { randomUUID } from "node:crypto";
import { createBoundedOutputSender } from "./bounded-output-correction.mjs";
import { enforceWorkDeliveryProtocol } from "./work-delivery-protocol.mjs";
import { remainingGenerationDurationMs } from "./generation-duration.mjs";
import { maintainExecutionAuthority } from "./execution-authority-renewal.mjs";
import { renewTaskAuthority } from "./task-capability-renewal.mjs";
import { credentialScopesFromEnvironment, loadCodexEvidence, resolveCodexProvider } from "./codex-runtime.mjs";
import { ExecutionEventJournal } from "../shared/execution-event-journal.mjs";
import { safeSendToWebContents } from "./webcontents-safety.mjs";

// Explicit live capabilities avoid stale configuration snapshots during concurrent execution.
/**
 * @param {{
 *   readonly executionContext: import("./execution-domains.mjs").ExecutionDomains["current"],
 *   readonly runtimeRequest: ReturnType<typeof import("./runtime-service.mjs").createRuntimeService>["runtimeRequest"],
 *   readonly executionRequests: import("./execution-requests.mjs").ExecutionRequests,
 *   readonly modelConnectionGate: import("./model-connection-gate.mjs").ModelConnectionGate,
 *   readonly assertCurrentAgentPlan: ReturnType<typeof import("./model-transport.mjs").createModelTransport>["assertCurrentAgentPlan"],
 *   readonly executionDomains: import("./execution-domains.mjs").ExecutionDomains,
 *   readonly recoveryIndex: import("@eternallight/agent-engine").RecoveryIndex | null,
 *   readonly workExecutionOwners: ReturnType<typeof import("./automatic-work-consumers.mjs").createAutomaticWorkConsumers>["workExecutionOwners"],
 *   readonly runtimeCandidateToken: string,
 *   readonly conversationMessages: ReturnType<typeof import("./conversation-delivery.mjs").createConversationDelivery>["conversationMessages"],
 *   readonly engineQualificationContextDigest: string | null,
 *   readonly rendererWebContents: import("electron").WebContents | null,
 *   readonly baseExecutionDomain: import("./execution-domains.mjs").ExecutionDomains["base"],
 * }} dependencies
 */
export function createAgentTurnRuntime(dependencies) {
  function clearAgentCallbacks() {
    dependencies.executionContext().removeAgentWebContentsListener?.();
    dependencies.executionContext().removeAgentWebContentsListener = null;
    dependencies.executionContext().agentWebContents = null;
    dependencies.executionContext().codexAdapter.setCallbacks({ onEvent: () => {}, onUserInputRequest: () => {} });
  }

  function forwardAgentEvent(message) {
    if (message.method === "agent/workflow") dependencies.executionContext().workflow = message.params?.workflow ?? null;
    sendAgentMessage("agent:event", message);
    const execution = dependencies.executionContext().activeAgentExecution;
    if (execution) void interruptCommittedGenerationPause(message, { execution, flow: dependencies.executionContext().activeWorkFlow,
      runtimeRequest: dependencies.runtimeRequest, currentExecution: () => dependencies.executionContext().activeAgentExecution, interrupt: async current => {
        requestTaskCancellation(current);
        requestTaskCancellation(current.task);
        await interruptAgentTurn(current);
      } }).catch(error => { console.error("[novel-codex] committed pause lease check failed:", error.message); });
  }

  function recordAgentEvent(message) {
    void observeRecoveryEvent(dependencies.executionContext().recoveryJournal, message, ({ method, bytes }) => {
      console.error(`[novel-codex] recovery journal failed: method=${method} bytes=${bytes}; 已保留恢复门禁。`);
    }).catch(() => { /* The journal retains this failure for the completion/reconciliation flush gate. */ });
  }

  async function beginRecoveryRun(task, projectId, threadId) {
    if (!dependencies.executionContext().recoveryJournal) return;
    await dependencies.executionContext().recoveryJournal.beginRun({
      runId: task.id,
      planId: dependencies.executionContext().agentProviderConfig?.sessionPlan?.id ?? "unresolved-session-plan",
      sessionPlan: dependencies.executionContext().agentProviderConfig?.sessionPlan ?? null,
      threadId,
      projectId,
      operation: task.operation ?? null,
    });
  }

  async function interruptAgentTurn(execution = dependencies.executionContext().activeAgentExecution) {
    if (execution?.engineHost && execution?.engineRunId) return execution.engineHost.cancelRun(execution.engineRunId, "author-interrupt");
    const bridge = execution?.modelBridge ?? dependencies.executionContext().modelBridge;
    const adapter = execution?.adapter ?? dependencies.executionContext().codexAdapter;
    await bridge?.cancelActiveRequests();
    return adapter.interruptTurn();
  }

  async function beginAgentExecution(execution, operation) {
    dependencies.executionRequests.assertActive();
    while (dependencies.executionContext().recoveryRefreshPromise) await dependencies.executionContext().recoveryRefreshPromise;
    if (dependencies.executionContext().activeAgentExecution && dependencies.executionContext().activeAgentExecution !== execution) throw new Error("已有创作引擎任务正在运行，请先等待或停止当前任务");
    dependencies.modelConnectionGate.assertExecutionAllowed();
    const bridge = dependencies.executionContext().modelBridge;
    const adapter = dependencies.executionContext().codexAdapter;
    const providerConfig = dependencies.executionContext().agentProviderConfig;
    execution.modelBridge = bridge;
    execution.adapter = adapter;
    execution.providerConfig = providerConfig;
    execution.operation = operation;
    const request = dependencies.executionRequests.assertActive();
    dependencies.executionContext().activeAgentExecution = execution;
    if (request) request.execution = execution;
    try {
      execution.sessionPlan = await dependencies.assertCurrentAgentPlan(operation, providerConfig);
    } catch (error) {
      if (dependencies.executionContext().activeAgentExecution === execution) dependencies.executionContext().activeAgentExecution = null;
      throw error;
    }
  }

  const QUIET_RECOVERY_OPERATIONS = new Set(["read_tool_guide", "read_context", "read_commitment", "read_source_content", "read_work_plan", "read_work_status", "read_revision_outcome", "search_story", "search_relations", "query_story", "read_reading_progress", "read_chapter_structure_source", "analyze_story_impact", "read_quality_notebook", "read_quality_review", "read_quality_packet"]);

  async function closeFinishedReads(index) {
    if (!index) return [];
    const report = await index.recoveryReport();
    const visible = [];
    for (const run of report) {
      const manual = run.requests.filter((request) => request.recovery === "manual-reconciliation");
      const finished = ["completed", "failed", "cancelled"].includes(run.runtimeStatus);
      const onlyReads = manual.length > 0 && manual.every((request) => QUIET_RECOVERY_OPERATIONS.has(request.operation));
      if (finished && onlyReads) {
        await index.resolveRun(run.runId, "manual-reviewed");
        continue;
      }
      visible.push(run);
    }
    return visible;
  }

  const readRecoveryOperationEvidence = identity => dependencies.executionContext().codexAdapter.readRecoveryOperationEvidence(identity);

  function refreshRecoveryReport() {
    if (dependencies.executionContext().recoveryRefreshPromise) return dependencies.executionContext().recoveryRefreshPromise;
    // Active executions own their journal and task capability. Fencing them
    // would interrupt the work, so report reads stay passive until the host is idle.
    if (dependencies.executionDomains.active().length || dependencies.executionContext().activeAgentExecution || dependencies.executionContext().activeWorkFlow || dependencies.executionContext().activeGenerationRunId) return closeFinishedReads(dependencies.recoveryIndex);
    dependencies.executionContext().recoveryRefreshPromise = (async () => {
      if (dependencies.recoveryIndex) await reconcileRecoveryIndex(dependencies.recoveryIndex, dependencies.runtimeRequest, (projectId, phase, error) => {
        console.error(`[novel-codex] recovery refresh failed (${projectId}/${phase}): ${error instanceof Error ? error.message : String(error)}`);
      }, readRecoveryOperationEvidence);
      return closeFinishedReads(dependencies.recoveryIndex);
    })().finally(() => { dependencies.executionContext().recoveryRefreshPromise = null; });
    return dependencies.executionContext().recoveryRefreshPromise;
  }

  async function finishAgentExecution(execution) {
    let cleanupError = null;
    try {
      if (execution.engineHost && execution.engineSessionId) await execution.engineHost.closeSession(execution.engineSessionId);
    } catch (error) {
      cleanupError = error;
    }
    if (dependencies.recoveryIndex && !dependencies.executionDomains.active().some(domain => domain !== dependencies.executionContext())) await reconcileRecoveryIndex(dependencies.recoveryIndex, dependencies.runtimeRequest, (projectId, phase, error) => {
      console.error(`[novel-codex] recovery reconciliation failed (${projectId}/${phase}): ${error instanceof Error ? error.message : String(error)}`);
    }, readRecoveryOperationEvidence);
    if (dependencies.executionContext().activeAgentExecution === execution) dependencies.executionContext().activeAgentExecution = null;
    if (cleanupError) throw cleanupError;
  }

  async function sendAgentTurn(execution, input, options) {
    if (!execution.modelBridge || !execution.adapter || !execution.sessionPlan) throw new Error("Agent Engine 执行环境尚未准备完成");
    if (execution.task && !execution.taskBudget) throw new Error("领域任务缺少持久执行额度，拒绝调用模型");
    if (execution.task && dependencies.executionContext().activeWorkFlow?.id) {
      if (dependencies.executionContext().activeWorkFlow.ownerInvalid || !dependencies.executionContext().activeWorkFlow.ownership) throw new Error("当前执行归属失效，不能继续发送模型请求");
      await dependencies.workExecutionOwners.verify(dependencies.executionContext().activeWorkFlow.ownership.record);
      assertTaskCompletable(execution);
    }
    if (execution.task) {
      const turn = withTaskOutputCapacity(input, { task: execution.task,
        sessionPlan: execution.sessionPlan, modelRoute: execution.adapter.modelRoute });
      options = { ...options, additionalContext: turn.additionalContext };
      input = turn.input;
    }
    const runId = execution.task?.id ?? randomUUID();
    const budgetOptions = (id) => ({ id, ...(execution.taskDeadlineAt == null ? {} : {
      maxDurationMs: Math.max(1, execution.taskDeadlineAt * 1000 - Date.now()),
    }) });
    const runtime = new CodexSessionRuntime({
      appServer: execution.adapter,
      bridge: execution.modelBridge,
      bridgeOwnership: "borrowed",
      closeAppServer: (adapter) => adapter.finishTaskOperation(),
      budgetFactory: async ({ lease }) => {
        if (execution.taskBudget) {
          if (lease.runId !== runId || lease.budgetId !== execution.taskBudget.id) throw new Error("任务预算与引擎租约不匹配");
          execution.taskBudgetBound = true;
          return execution.taskBudget;
        }
        return dependencies.executionContext().agentStorage?.budgetDb
          ? new PersistentTaskBudget({ databasePath: dependencies.executionContext().agentStorage.budgetDb, runId: lease.runId, ...budgetOptions(lease.budgetId) })
          : new TaskBudget(budgetOptions(lease.budgetId));
      },
      reconcile: async () => {
        await dependencies.executionContext().recoveryJournal?.flush();
        const report = await dependencies.recoveryIndex?.recoveryReport() ?? [];
        const unresolvedOperations = report.find((entry) => entry.runId === runId)?.requests
          ?.filter((request) => request.recovery === "manual-reconciliation")
          .map((request) => ({ requestId: request.requestId, operation: request.operation })) ?? [];
        return { outcome: unresolvedOperations.length ? "outcome-unknown" : "cancelled", unresolvedOperations };
      },
    });
    const host = new AgentEngineHost({
      connections: execution.providerConfig.connections,
      evidenceProvider: async () => { throw new Error("the product must resolve a trusted SessionPlan before runtime creation"); },
      runtimeFactory: async () => runtime,
    });
    const sessionId = randomUUID();
    execution.engineHost = host;
    execution.engineSessionId = sessionId;
    execution.engineRunId = runId;
    let renewal = null;
    let terminalError = null;
    try {
      await host.startSession({ sessionPlan: execution.sessionPlan, sessionId });
      let turnOptions = execution.task && !options?.dispatch
        ? { ...options, dispatch: adapter => {
          const sendTurn = createBoundedOutputSender(adapter, execution.task, dependencies.runtimeRequest, () => assertTaskCompletable(execution));
          return execution.task.operation === "advisory" && execution.task.inputManifest?.workDecision
            ? enforceWorkDeliveryProtocol({ sendTurn }, input, options, execution.task,
              path => dependencies.runtimeRequest(path, path.includes("/work-decision?") ? { headers: {
                Authorization: `Bearer ${dependencies.runtimeCandidateToken}`, "x-novel-task-capability": execution.capability.secret,
              } } : {}), () => assertTaskCompletable(execution))
            : sendTurn(input, options);
        } }
        : options;
      const dispatch = turnOptions?.dispatch ?? (adapter => adapter.sendTurn(input, options));
      turnOptions = { ...turnOptions, dispatch: async adapter => {
        try { return await dispatch(adapter); }
        catch (error) {
          // Native callback serialization preserves codes but cannot preserve the
          // local admission identity. Retain only this dispatch's matched error.
          if (coreTurnFailure(error)) terminalError = error;
          throw error;
        }
      } };
      await dependencies.conversationMessages.sending(dependencies.executionRequests.current()?.messageDelivery);
      const started = await host.startRun({ sessionId, runId, budgetId: execution.taskBudget?.id ?? randomUUID(), mode: execution.operation, input, turnOptions,
        ...(execution.taskDeadlineAt == null ? {} : { maxDurationMs: remainingGenerationDurationMs(execution.taskDeadlineAt), deadlineAtMs: execution.taskDeadlineAt * 1000 }) });
      renewal = maintainExecutionAuthority({ capability: execution.task ? execution.capability : null,
        sessionPlan: execution.sessionPlan,
        isCurrent: () => dependencies.executionContext().activeAgentExecution === execution && !execution.cancellationRequested && execution.engineRunId === runId,
        renewTask: expectedExpiresAt => renewTaskAuthority({ execution, expectedExpiresAt, runtimeRequest: dependencies.runtimeRequest }),
        renewPlan: async (previous, minimumEvidenceValidityMs) => {
          assertTaskCompletable(execution);
          const evidence = dependencies.executionContext().agentStorage ? await loadCodexEvidence(dependencies.executionContext().agentStorage.evidencePath) : execution.providerConfig.evidence ?? [];
          const next = resolveCodexProvider({ evidence, minimumEvidenceValidityMs,
            credentialScopes: dependencies.executionContext().agentStorage ? credentialScopesFromEnvironment(process.env, dependencies.executionContext().agentStorage.credentialSaltPath) : {},
            model: execution.providerConfig.id, provider: execution.providerConfig.provider, reasoningEffort: execution.providerConfig.reasoning?.effort ?? null,
            operation: execution.operation, qualificationContextDigest: dependencies.engineQualificationContextDigest });
          assertTaskCompletable(execution);
          if (dependencies.executionContext().activeAgentExecution !== execution) throw new Error("执行任务已变化，不能续期");
          await host.renewRun({ runId, sessionPlan: next.sessionPlan });
          assertTaskCompletable(execution);
          if (dependencies.executionContext().activeAgentExecution !== execution) throw new Error("执行任务已变化，不能续期");
          if (dependencies.executionContext().agentProviderConfig === execution.providerConfig) dependencies.executionContext().agentProviderConfig = next;
          execution.providerConfig = next;
          execution.sessionPlan = next.sessionPlan;
          return next.sessionPlan;
        } });
      let result;
      try { result = await Promise.race([started.completion, renewal.failure]); }
      catch (error) {
        if (error.code === "EXECUTION_AUTHORITY_INVALID") await host.cancelRun(runId, "authorization-renewal-failed").catch(() => undefined);
        if (error.code === "CORE_TURN_FAILED" && terminalError) throw terminalError;
        throw error;
      }
      await renewal.stop();
      host.closeRun(runId);
      execution.engineRunId = null;
      await host.closeSession(sessionId);
      execution.engineSessionId = null;
      return result;
    } finally {
      await renewal?.stop();
      if (execution.engineSessionId) await host.closeSession(sessionId).catch(() => undefined);
      execution.engineHost = null;
      execution.engineSessionId = null;
      execution.engineRunId = null;
    }
  }

  async function runCoreTurnWithLease(operation, callback) {
    if (dependencies.executionContext().activeAgentExecution) throw new Error("已有创作引擎任务正在运行，请先等待或停止当前任务");
    const execution = createTaskExecution();
    await beginAgentExecution(execution, operation);
    try {
      assertTaskCompletable(execution);
      const result = await sendAgentTurn(execution, [], { dispatch: callback });
      assertTaskCompletable(execution);
      return result;
    } finally {
      await finishAgentExecution(execution);
    }
  }

  function attachAgentWebContents(webContents) {
    if (dependencies.executionContext().agentWebContents === webContents && !webContents.isDestroyed()) return;
    clearAgentCallbacks();
    if (webContents.isDestroyed()) return;
    const onDestroyed = () => {
      if (dependencies.executionContext().agentWebContents === webContents) clearAgentCallbacks();
    };
    webContents.once("destroyed", onDestroyed);
    dependencies.executionContext().removeAgentWebContentsListener = () => webContents.removeListener("destroyed", onDestroyed);
    dependencies.executionContext().agentWebContents = webContents;
  }

  function sendAgentMessage(channel, payload) {
    const target = dependencies.executionContext().agentWebContents ?? dependencies.rendererWebContents;
    const domain = dependencies.executionContext();
    if (domain !== dependencies.baseExecutionDomain) {
      if (channel === "agent:user-input-request") {
        payload = { ...payload, executionId: domain.id, projectId: domain.projectId };
        sendAgentMessage("agent:event", {method:"agent/user-input-requested",params:{request:payload}});
      }
      if (channel === "agent:event") {
        domain.eventJournal ??= new ExecutionEventJournal({compact:true});
        if(payload.method === "turn/started") domain.eventJournal.startTurn();
        payload = { ...payload, hostExecution: { id: domain.id, requestId: domain.requestId,
          projectId: domain.projectId, threadId: domain.threadId, sequence:domain.eventJournal.cursor + 1 } };
        domain.eventJournal.append(payload);
      }
    }
    if (!safeSendToWebContents(target, channel, payload) && target?.isDestroyed?.()) clearAgentCallbacks();
  }

  return { clearAgentCallbacks, forwardAgentEvent, recordAgentEvent, beginRecoveryRun, interruptAgentTurn, beginAgentExecution, closeFinishedReads, readRecoveryOperationEvidence, refreshRecoveryReport, finishAgentExecution, sendAgentTurn, runCoreTurnWithLease, attachAgentWebContents, sendAgentMessage };
}
