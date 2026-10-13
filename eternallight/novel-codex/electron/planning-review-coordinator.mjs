// @ts-check
import { validateAuthorIntent } from "../shared/author-input-budget.mjs";
import { assertProviderOperation, composeNovelCodexPrompt, createTaskCapability } from "./codex-runtime.mjs";
import { assertTaskCompletable, bindExecutionTask, createTaskExecution } from "@eternallight/agent-engine";
import { ensureWorkBudgetAuthorization } from "./work-budget-authorization.mjs";
import { BrowserWindow } from "electron";
import { bindTaskProviderBudget, settleTaskProviderBudget } from "./task-provider-budget.mjs";
import { createTaskBoundHandoff, taskMethodText } from "./workflow-handoff.mjs";
import { settleFailedTask } from "./task-failure-settlement.mjs";

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
 *   readonly workControlCommands: import("./work-control-commands.mjs").WorkControlCommands,
 *   readonly acquireWorkFlowOwner: ReturnType<typeof import("./work-flow-coordinator.mjs").createWorkFlowCoordinator>["acquireWorkFlowOwner"],
 *   readonly currentWorkFlowState: ReturnType<typeof import("./work-flow-coordinator.mjs").createWorkFlowCoordinator>["currentWorkFlowState"],
 *   readonly assertCurrentAgentPlan: ReturnType<typeof import("./model-transport.mjs").createModelTransport>["assertCurrentAgentPlan"],
 *   readonly beginRecoveryRun: ReturnType<typeof import("./agent-turn-runtime.mjs").createAgentTurnRuntime>["beginRecoveryRun"],
 *   readonly sendAgentTurn: ReturnType<typeof import("./agent-turn-runtime.mjs").createAgentTurnRuntime>["sendAgentTurn"],
 *   readonly bindCodexTurn: ReturnType<typeof import("./work-flow-coordinator.mjs").createWorkFlowCoordinator>["bindCodexTurn"],
 *   readonly finishAgentExecution: ReturnType<typeof import("./agent-turn-runtime.mjs").createAgentTurnRuntime>["finishAgentExecution"],
 * }} dependencies
 */
export function createPlanningReviewCoordinator(dependencies) {
  async function runPlanningReview(event, projectId, request, continuation = null) {
    if (dependencies.executionContext().activeGenerationRunId) throw new Error("自动创作运行正在推进，不能并行评审");
    if (!dependencies.executionContext().codexAdapter.enabled) throw new Error("Novel Agent Engine 未配置，当前只能手动编辑");
    if (dependencies.executionContext().activeAgentExecution) throw new Error("已有创作引擎任务正在运行，请先等待或停止当前任务");
    if (!request || typeof request !== "object" || !["outline", "plan"].includes(request.artifactKind)) throw new Error("规划评审目标无效");
    if (typeof request.revisionId !== "string" || !request.revisionId.trim()) throw new Error("规划评审缺少明确版本");
    validateAuthorIntent(request.authorIntent);
    const roles = request.roles;
    if (!Array.isArray(roles) || ![1, 2, 3].includes(roles.length) || new Set(roles).size !== roles.length || roles.some((role) => !["structure", "character", "experience"].includes(role))) throw new Error("规划评审角色无效");
    assertProviderOperation(dependencies.executionContext().agentProviderConfig, "reviewPlanningSeat");
    dependencies.attachAgentWebContents(event.sender);
    dependencies.executionContext().codexAdapter.setCallbacks({
      onEvent: dependencies.executionDomains.bind(dependencies.forwardAgentEvent),
      onUserInputRequest: dependencies.executionDomains.bind(request => dependencies.sendAgentMessage("agent:user-input-request", request)),
    });
    const execution = createTaskExecution();
    execution.projectId = projectId;
    await dependencies.beginAgentExecution(execution, "reviewPlanningSeat");
    const adapter = execution.adapter;
    let run = null;
    try {
      assertTaskCompletable(execution);
      run = continuation?.run ?? await dependencies.runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/planning-reviews`, {
        method: "POST",
        body: JSON.stringify(request),
      });
      execution.runId = run.id;
      if (run.projectId !== projectId || typeof run.workTaskId !== "string" || !/^[a-zA-Z0-9-]{1,160}$/.test(run.workTaskId)) throw new Error("规划检查工作任务归属无效");
      const reviewWork = await dependencies.runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/work-tasks/${encodeURIComponent(run.workTaskId)}`);
      if (reviewWork.id !== run.workTaskId || reviewWork.projectId !== projectId || (run.sourceTaskId === null ? reviewWork.kind !== "direct" : reviewWork.kind !== "conversation")) throw new Error("规划检查工作任务归属无效");
      if (dependencies.executionContext().activeWorkFlow) dependencies.executionContext().activeWorkFlow.id = reviewWork.id;
      if (dependencies.executionContext().activeWorkFlow?.stopRequested) {
        if (["running", "blocked"].includes(reviewWork.status)) await dependencies.workControlCommands.send(reviewWork, "pause");
        throw Object.assign(new Error("生成已停止"), { code: "TASK_CANCELLED" });
      }
      if (reviewWork.status !== "completed") await ensureWorkBudgetAuthorization({ runtimeRequest: dependencies.runtimeRequest, projectId, work: reviewWork, origin: run.sourceTaskId === null ? "author" : "automatic",
        databasePath: dependencies.executionContext().agentStorage?.budgetDb,
        getWindow: () => BrowserWindow.fromWebContents(event.sender) });
      if (dependencies.executionContext().activeWorkFlow && reviewWork.status === "running") await dependencies.acquireWorkFlowOwner(
        await dependencies.runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/work-tasks/${encodeURIComponent(run.workTaskId)}`));
      assertTaskCompletable(execution);
      for (const role of continuation?.roles ?? run.roles) {
        if (execution.cancellationRequested) break;
        if (await dependencies.currentWorkFlowState() === "paused") return { projectId,
          run: await dependencies.runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/planning-reviews/${encodeURIComponent(run.id)}`) };
        let task = null;
        let taskCompleted = false;
        try {
          await dependencies.assertCurrentAgentPlan("reviewPlanningSeat");
          assertTaskCompletable(execution);
          const capability = createTaskCapability();
          execution.capability = capability;
          adapter.prepareTaskOperation("reviewPlanningSeat", capability.secret);
          const threadId = await adapter.ensureSession();
          assertTaskCompletable(execution);
          task = await dependencies.runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/planning-reviews/${encodeURIComponent(run.id)}/seats/${encodeURIComponent(role)}/tasks`, {
            method: "POST",
            body: JSON.stringify({ threadId, model: execution.providerConfig?.model ?? null, capabilityHash: capability.hash, capabilityExpiresAt: capability.expiresAt }),
          });
          task = bindExecutionTask(execution, task);
          dependencies.executionContext().activeRuntimeTask = task;
          await bindTaskProviderBudget(execution, { runtimeRequest: dependencies.runtimeRequest, projectId, taskId: task.id,
            databasePath: dependencies.executionContext().agentStorage?.budgetDb });
          if (!execution.taskBudget) throw new Error("规划深审席位缺少持久额度，不能执行");
          if (execution.cancellationRequested) {
            await dependencies.runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/tasks/${encodeURIComponent(task.id)}/cancel`, { method: "POST", body: "{}" });
            throw Object.assign(new Error("规划深审已停止"), { code: "TASK_CANCELLED" });
          }
          const workspace = await dependencies.runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/creative-workspace`);
          const handoff = createTaskBoundHandoff(projectId, workspace, task);
          adapter.emit({ method: "agent/workflow", params: { workflow: { task, planningReviewId: run.id, role, revision: workspace.revision } } });
          const prompt = composeNovelCodexPrompt(taskMethodText(task), task.inputManifest.authorIntent, "你是规划深审中的独立评估任务。先按冻结绑定调用 read_context；不得读取其他席报告，不得修改任何规划、正文或正式事实。按冻结的 checks 和本席角色逐项检查，允许没有问题或无法判断。然后调用 submit_planning_review_report 保存结构化报告，最后调用 complete_task。", { internal: true, binding: handoff, background: { role, candidateInstruction: task.inputManifest.candidateInstruction ?? null } });
          await dependencies.beginRecoveryRun(task, projectId, threadId);
          assertTaskCompletable(execution);
          const result = await dependencies.sendAgentTurn(execution, prompt, { disableEnvironment: true, threadName: `规划深审 · ${role}` });
          assertTaskCompletable(execution);
          await dependencies.bindCodexTurn(projectId, task, threadId, result);
          await dependencies.executionContext().recoveryJournal?.flush();
          await dependencies.runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/tasks/${encodeURIComponent(task.id)}/complete`, { method: "POST", body: JSON.stringify({ threadId, summary: Array.from(result.message ?? "").slice(0, 4_000).join("") }) });
          taskCompleted = true;
          await dependencies.executionContext().recoveryJournal?.completeRun();
          await settleTaskProviderBudget(execution, dependencies.runtimeRequest, task.id);
        } catch (error) {
          let settlementFailure = null;
          if (!taskCompleted) {
            try { await settleFailedTask(dependencies.runtimeRequest, projectId, task, execution, error); }
            catch (failure) { settlementFailure = failure; }
          }
          if (task) {
            try { await settleTaskProviderBudget(execution, dependencies.runtimeRequest, task.id); }
            catch (chargeError) { console.warn(`[novel-codex] review usage remains unsettled for ${task.id}: ${chargeError.message}`); }
          }
          throw settlementFailure ?? error;
        } finally {
          await dependencies.executionContext().recoveryJournal?.abandonRun().catch(() => undefined);
          if (dependencies.executionContext().activeRuntimeTask?.id === task?.id) dependencies.executionContext().activeRuntimeTask = null;
          await adapter.finishTaskOperation();
          execution.task = null;
          execution.taskBudget = null;
          execution.taskBudgetBound = false;
          execution.taskDeadlineAt = null;
          execution.budgetUsagePath = null;
        }
      }
      if (execution.cancellationRequested) {
        if (!dependencies.executionContext().activeWorkFlow?.stopRequested) await dependencies.runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/planning-reviews/${encodeURIComponent(run.id)}/cancel`, { method: "POST", body: "{}" });
        throw Object.assign(new Error("规划深审已停止；已保存报告仍会保留"), { code: "TASK_CANCELLED" });
      }
      const finalized = await dependencies.runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/planning-reviews/${encodeURIComponent(run.id)}/finalize`, { method: "POST", body: "{}" });
      return { projectId, run: finalized };
    } catch (error) {
      if (run && !dependencies.executionContext().activeWorkFlow?.stopRequested && (execution.cancellationRequested || error?.code === "TASK_CANCELLED")) {
        try { await dependencies.runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/planning-reviews/${encodeURIComponent(run.id)}/cancel`, { method: "POST", body: "{}" }); }
        catch (cancelError) { console.warn(`[novel-codex] review cancellation remains pending: ${cancelError.message}`); }
      }
      throw error;
    } finally {
      await dependencies.finishAgentExecution(execution);
    }
  }

  return { runPlanningReview };
}
