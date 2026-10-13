// @ts-check
// Execute only the roles persisted in this review; a later UI suggestion cannot change its scope.
export async function readPlanningReviewContinuation(runtimeRequest, projectId, work, requestedRunId = null) {
  if (work?.projectId !== projectId || !["direct", "conversation"].includes(work.kind) || typeof work.id !== "string" || !/^[a-zA-Z0-9-]{1,160}$/.test(work.id) || !Array.isArray(work.steps)
    || work.steps.length < 1 || work.status === "cancelled") {
    throw new Error("规划检查恢复归属无效");
  }
  const runId = requestedRunId ?? work.steps[0]?.target?.reviewRunId;
  const reviewSteps = work.steps.filter(step => step.operation === "reviewPlanningSeat" && step.target?.reviewRunId === runId);
  if (reviewSteps.length < 1 || reviewSteps.length > 3 || (work.kind === "direct" && reviewSteps.length !== work.steps.length)) {
    throw new Error("规划检查席位集合无效");
  }
  if (typeof runId !== "string" || !/^planning-review-[a-zA-Z0-9-]{1,120}$/.test(runId)) {
    throw new Error("规划检查运行标识无效");
  }
  const run = await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/planning-reviews/${encodeURIComponent(runId)}`);
  if (run?.id !== runId || run.projectId !== projectId || run.workTaskId !== work.id || !Array.isArray(run.roles)
    || typeof run.authorMessageId !== "string" || !/^[a-zA-Z0-9-]{1,160}$/.test(run.authorMessageId)
    || typeof run.authorIntent !== "string" || !run.authorIntent.trim() || (work.kind === "direct" && run.authorIntent !== work.goal)
    || (work.kind === "direct" ? run.sourceTaskId !== null || run.sourceIntentRevision !== null
      : typeof run.sourceTaskId !== "string" || !/^[A-Za-z0-9_-]{1,160}$/.test(run.sourceTaskId)
        || !Number.isSafeInteger(run.sourceIntentRevision) || run.sourceIntentRevision < 0)
    || run.roles.length !== reviewSteps.length || new Set(run.roles).size !== run.roles.length
    || !Array.isArray(run.seats) || run.seats.length !== run.roles.length) {
    throw new Error("规划检查运行响应无效");
  }
  if (work.kind === "conversation" && (!requestedRunId || typeof run.sourceTaskId !== "string"
    || !work.steps.some(step => step.operation === "advisory" && step.currentAttemptId === run.sourceTaskId && step.status === "completed"))) {
    throw new Error("规划检查原对话归属无效");
  }
  if (["completed", "partial", "cancelled"].includes(run.status)) return null;
  if (run.status !== "running") throw new Error("规划检查运行状态无效");
  const roles = [];
  for (const role of run.roles) {
    const matches = reviewSteps.filter(step => step.id === `review-seat-${runId}-${role}`);
    const seats = run.seats.filter(seat => seat.role === role);
    const step = matches[0];
    const target = step?.target;
    if (matches.length !== 1 || seats.length !== 1 || step.operation !== "reviewPlanningSeat"
      || target?.type !== "artifact" || target.reviewRunId !== runId || target.role !== role || target.artifactKind !== run.artifactKind
      || target.revisionId !== run.revisionId || step.inputRevision !== run.inputRevision) {
      throw new Error("规划检查席位绑定无效");
    }
    if (step.status === "queued") roles.push(role);
    else if (step.status !== "completed" || step.currentAttemptId !== seats[0].taskId || seats[0].status !== "completed") {
      throw new Error("规划检查仍需恢复核账或作者决定，不能重跑席位");
    }
  }
  if (roles.length && work.status !== "running") throw new Error("规划检查工作任务尚未恢复");
  return { run, roles };
}
