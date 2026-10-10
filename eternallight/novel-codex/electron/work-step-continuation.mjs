import { workStopLabels } from "../shared/work-recovery-presentation.mjs";
import { isDeepStrictEqual } from "node:util";

const stops = new Map(Object.entries(workStopLabels));

const bindingInvalid = () => Object.assign(new Error("原动作与任务授权不一致，已停止恢复；请核对原任务记录。"),
  { code: "WORK_ACTION_BINDING_INVALID" });

// A queued author action is not a resumed Attempt. Resolve its actual Native
// source/permit instead of inferring an action from an ID or copying metadata.
async function queuedActionTarget(runtimeRequest, base, work, step) {
  if (step.status !== "queued" || step.currentAttemptId !== null || !Array.isArray(step.attempts)
    || step.attempts.length || typeof step.sourceTaskId !== "string" || !step.sourceTaskId
    || !Number.isSafeInteger(step.inputRevision) || step.inputRevision < 0) throw bindingInvalid();
  const sourcePath = `${base}/tasks/${encodeURIComponent(step.sourceTaskId)}`;
  const source = await runtimeRequest(sourcePath);
  const owner = await runtimeRequest(`${sourcePath}/work-task`);
  const original = owner.steps?.find(item => item.id === step.id);
  const sourceStep = owner.steps?.find(item => item.attempts?.some(attempt => attempt.id === source.id
    && attempt.threadId === source.threadId && attempt.status === "completed"));
  if (source.id !== step.sourceTaskId || source.projectId !== work.projectId || source.operation !== "advisory"
    || source.status !== "completed" || source.threadId !== work.threadId || source.inputRevision !== step.inputRevision
    || owner.id !== work.id || owner.projectId !== work.projectId || owner.threadId !== work.threadId
    || owner.status !== "running" || !sourceStep || sourceStep.status !== "completed"
    || original?.sourceTaskId !== source.id || original.status !== "queued" || original.currentAttemptId !== null
    || !Array.isArray(original.attempts) || original.attempts.length || original.operation !== step.operation
    || original.inputRevision !== step.inputRevision || original.instruction !== step.instruction
    || !isDeepStrictEqual(original.target, step.target)) throw bindingInvalid();
  const actionPath = `${sourcePath}/actions`;
  const { actions } = await runtimeRequest(actionPath);
  const matches = actions?.filter(item => item.id === step.id);
  const action = matches?.[0];
  if (matches?.length !== 1 || action.sourceTaskId !== source.id || action.status !== "queued"
    || action.resultTaskId !== null || action.operation !== step.operation || action.inputRevision !== step.inputRevision
    || !Number.isSafeInteger(action.intentRevision) || action.intentRevision < 0 || action.instruction !== step.instruction
    || !isDeepStrictEqual(action.target, step.target) || !action.target || typeof action.target !== "object"
    || Array.isArray(action.target) || ["requestedByTaskId", "requestedActionId", "resumeStepId", "sourceTaskId", "runId",
      "workDecision", "workDecisionAction"].some(field => Object.hasOwn(action.target, field))) throw bindingInvalid();
  const { permission } = await runtimeRequest(`${actionPath}/${encodeURIComponent(action.id)}/permission`);
  const evidence = permission?.evidence;
  if (!["exactAction", "finiteScope"].includes(permission?.kind) || permission.actionId !== action.id
    || permission.workTaskId !== work.id || permission.actionStatus !== "queued"
    || (permission.kind === "exactAction" ? permission.scopePermitId !== null : typeof permission.scopePermitId !== "string" || !permission.scopePermitId)
    || evidence?.projectId !== work.projectId || evidence.sourceTaskId !== source.id
    || evidence.inputRevision !== action.inputRevision || evidence.intentRevision !== action.intentRevision
    || evidence.operation !== action.operation || evidence.instruction !== action.instruction
    || !isDeepStrictEqual(evidence.target, action.target) || typeof evidence.authorIntent !== "string" || !evidence.authorIntent.trim()) {
    throw bindingInvalid();
  }
  const target = { ...action.target, requestedByTaskId: source.id, requestedActionId: action.id };
  if (action.operation === "modelKnowledge") {
    const workspace = await runtimeRequest(`${base}/creative-workspace`);
    const source = workspace.modeling?.find(item => item.objectId === `draft-${target.chapterId}`);
    if (workspace.revision !== action.inputRevision || source?.sourceRevisionId !== target.sourceRevisionId) throw bindingInvalid();
    const batch = await runtimeRequest(`${base}/modeling-batches`, { method: "POST", body: JSON.stringify({
      objectId: source.objectId, sourceRevisionId: source.sourceRevisionId, expectedRevision: action.inputRevision,
      extractorVersion: `novel-knowledge-v1:${action.id}`,
    }) });
    if (batch.status === "ready" || typeof batch.id !== "string" || !batch.id) throw bindingInvalid();
    target.batchId = batch.id;
  }
  return target;
}

// Native admits each step and owns the total attempt budget. The Host neither
// chooses follow-up operations nor starts another root Work to bypass a stop.
export async function advanceWorkSteps(event, projectId, workTaskId, {
  runtimeRequest, currentWorkFlowState, runCreativeAgentTask, runAdvisoryAgentTask,
  runQueuedPlanningReview, onResult = () => {},
}) {
  const base = `/api/projects/${encodeURIComponent(projectId)}`;
  const path = `${base}/work-tasks/${encodeURIComponent(workTaskId)}`;
  const dispatched = new Set();
  while (true) {
    if (!["running", "completed", "blocked"].includes(await currentWorkFlowState())) return runtimeRequest(path);
    let work = await runtimeRequest(path);
    if (work.projectId !== projectId || work.id !== workTaskId) throw new Error("执行步骤不属于当前创作任务");
    if (work.interlude || !["running", "completed", "blocked"].includes(work.status)) return work;
    if (work.resumeBlockReason === "unknownUsage") {
      throw Object.assign(new Error(stops.get("unknownUsage")), { code: "CONVERSATION_INCOMPLETE" });
    }
    let step = work.steps.find(candidate => candidate.status === "queued"
      && (!candidate.dependsOnStepId || work.steps.some(prior => prior.id === candidate.dependsOnStepId
        && (prior.status === "completed" || (prior.status === "superseded" && candidate.operation === "advisory")))));
    if (!step) {
      if (work.steps.some(candidate => ["running", "waiting"].includes(candidate.status))) return work;
      // Edition tasks have their own Native completion contract. They must
      // never enter the novel-only workspace/decision API after resumption.
      if (work.steps.length && work.steps.every(candidate => candidate.target?.type === "edition")) {
        if (work.status === "completed" && ["discussion_ended", "ready_for_review"].includes(work.deliveryStatus)
          && work.pendingContinuation === false && work.deliveryReport?.unsettledAttemptIds?.length === 0) return work;
        throw Object.assign(new Error("媒体任务仍有未完成或待核对事项，请查看原任务状态。"), { code: "CONVERSATION_INCOMPLETE" });
      }
      const workspace = await runtimeRequest(`${base}/creative-workspace`);
      const decision = await runtimeRequest(`${path}/decision`, { method: "POST", body: JSON.stringify({
        expectedWorkVersion: work.version, expectedRevision: workspace.revision,
        expectedSourceDigest: workspace.sourceDigest,
      }) });
      if (decision.workTaskId !== work.id || decision.threadId !== work.threadId) {
        throw new Error("执行决策未绑定原任务和原对话");
      }
      if (!decision.step) {
        if (decision.stopReason === "completed") return runtimeRequest(path);
        throw Object.assign(new Error(stops.get(decision.stopReason) ?? "执行决策的停止状态无法确认，请核对原任务。"),
          { code: "CONVERSATION_INCOMPLETE" });
      }
      step = decision.step;
      if (decision.stopReason !== null || step.operation !== "advisory" || step.status !== "queued"
        || step.target?.workDecision?.workTaskId !== work.id || decision.decisionStepId !== step.id) {
        throw new Error("执行决策未返回合法的原任务步骤");
      }
      work = await runtimeRequest(path);
    }
    if (dispatched.has(step.id)) throw new Error("执行步骤未推进，已停止重复调用；请从原任务核对结果。");
    dispatched.add(step.id);
    // Explicit author-approved planning review is a multi-seat primitive. It
    // does not choose any subsequent creative operation.
    if (step.operation === "reviewPlanningSeat") {
      await runQueuedPlanningReview(event, projectId, work, step.target.reviewRunId);
      continue;
    }
    const target = step.sourceTaskId !== null && step.sourceTaskId !== undefined && !step.target.workDecisionAction
      ? await queuedActionTarget(runtimeRequest, base, work, step) : { ...step.target, resumeStepId: step.id };
    if (!["running", "completed", "blocked"].includes(await currentWorkFlowState())) return runtimeRequest(path);
    const instruction = step.instruction || work.goal;
    let result;
    try {
      result = step.operation === "advisory"
        ? await runAdvisoryAgentTask(event, projectId, instruction, step, null, null,
          step.target.workDecision ? work.threadId : null,
          step.target.workDecision ? null : work.threadId)
        : await runCreativeAgentTask(event, projectId, instruction, { operation: step.operation }, target);
    } catch (error) {
      if (error?.code === "TASK_CANCELLED") throw error;
      // Only Native can admit correction after settling the failed attempt.
      // Unknown writes, exhausted permits and repeated failures remain fenced.
      const current = await runtimeRequest(path).catch(() => null);
      if (!current || current.id !== work.id || current.projectId !== projectId
        || !["running", "blocked"].includes(current.status)) throw error;
      const workspace = await runtimeRequest(`${base}/creative-workspace`).catch(() => null);
      if (!workspace) throw error;
      const correction = await runtimeRequest(`${path}/decision`, { method: "POST", body: JSON.stringify({
        expectedWorkVersion: current.version, expectedRevision: workspace.revision,
        expectedSourceDigest: workspace.sourceDigest,
      }) }).catch(() => null);
      if (!correction || correction.workTaskId !== work.id || correction.threadId !== work.threadId
        || correction.stopReason !== null || correction.step?.operation !== "advisory"
        || correction.step.status !== "queued" || correction.decisionStepId !== correction.step.id
        || correction.step.target?.workDecision?.workTaskId !== work.id
        || dispatched.has(correction.step.id)) throw error;
      continue;
    }
    await onResult(result, step);
  }
}
