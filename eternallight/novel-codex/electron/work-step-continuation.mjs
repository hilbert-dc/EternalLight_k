const stops = new Map([
  ["workNotReady", "任务尚未就绪，请查看原任务状态。"],
  ["pendingRecovery", "保存结果还需要核对，核对前不会重复执行。"],
  ["budgetExhausted", "本次执行额度已用完，后续处理已暂停。"],
  ["budgetExpired", "本次执行授权已到期，后续处理已暂停。"],
  ["executionBlocked", "本次执行遇到无法安全自动处理的问题，已保留当前结果与错误说明。"],
  ["recoveryStalled", "自动纠错未能继续，已暂停并保留结果；可以在对话中补充信息后继续。"],
  ["noReadyActions", "当前没有可以安全执行的下一步，请查看未满足的条件。"],
]);

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
    if (!["running", "completed", "blocked"].includes(work.status)) return work;
    let step = work.steps.find(candidate => candidate.status === "queued"
      && (!candidate.dependsOnStepId || work.steps.some(prior => prior.id === candidate.dependsOnStepId
        && (prior.status === "completed" || (prior.status === "superseded" && candidate.operation === "advisory")))));
    if (!step) {
      if (work.steps.some(candidate => ["running", "waiting"].includes(candidate.status))) return work;
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
    const target = { ...step.target, resumeStepId: step.id };
    const instruction = step.instruction || work.goal;
    let result;
    try {
      result = step.operation === "advisory"
        ? await runAdvisoryAgentTask(event, projectId, instruction, step, null, null,
          step.target.workDecision ? work.threadId : null)
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
