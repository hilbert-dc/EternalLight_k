// Resolve only persisted review work. A renderer/model suggestion cannot choose seats.
export async function readPlanningReviewContinuation(runtimeRequest, projectId, work) {
  if (work?.projectId !== projectId || work.kind !== "direct" || typeof work.id !== "string" || !Array.isArray(work.steps)
    || work.steps.length < 2 || work.steps.length > 3 || work.status === "cancelled") {
    throw new Error("规划检查恢复归属无效");
  }
  const runId = work.id.slice("work-".length);
  if (work.id !== `work-${runId}` || !/^planning-review-[a-zA-Z0-9-]+$/.test(runId)) {
    throw new Error("规划检查运行标识无效");
  }
  const run = await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/planning-reviews/${encodeURIComponent(runId)}`);
  if (run?.id !== runId || run.projectId !== projectId || !Array.isArray(run.roles)
    || typeof run.authorMessageId !== "string" || !/^[a-zA-Z0-9-]{1,160}$/.test(run.authorMessageId)
    || typeof run.authorIntent !== "string" || !run.authorIntent.trim() || run.authorIntent !== work.goal
    || run.roles.length !== work.steps.length || new Set(run.roles).size !== run.roles.length
    || !Array.isArray(run.seats) || run.seats.length !== run.roles.length) {
    throw new Error("规划检查运行响应无效");
  }
  if (["completed", "partial", "cancelled"].includes(run.status)) return null;
  if (run.status !== "running") throw new Error("规划检查运行状态无效");
  const roles = [];
  for (const role of run.roles) {
    const matches = work.steps.filter(step => step.id === `review-seat-${runId}-${role}`);
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
