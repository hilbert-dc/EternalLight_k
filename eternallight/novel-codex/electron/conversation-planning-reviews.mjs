import { readPlanningReviewContinuation } from "./planning-review-resumption.mjs";

const identifier = value => typeof value === "string" && /^[A-Za-z0-9_-]{1,160}$/.test(value);

export function registerPlanningReviewReadHandler(ipc, runtimeRequest) {
  ipc.handle("runtime:get-planning-review", (_event, projectId, runId) => {
    if (!identifier(projectId) || !identifier(runId)) throw new Error("规划报告标识无效");
    return runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/planning-reviews/${encodeURIComponent(runId)}`);
  });
}

// A registered run already owns its source and seats. Reading this receipt never
// creates another run or grants another allowance after an unknown outcome.
export async function executeConversationPlanningReviews(event, projectId, discussion, dependencies) {
  const source = discussion.workflow?.task;
  if (!identifier(source?.id) || source.projectId !== projectId || source.operation !== "advisory") {
    throw new Error("规划检查不属于本次对话");
  }
  const base = `/api/projects/${encodeURIComponent(projectId)}`;
  const { runs } = await dependencies.runtimeRequest(`${base}/tasks/${encodeURIComponent(source.id)}/planning-reviews`);
  if (!Array.isArray(runs) || runs.length > 3 || new Set(runs.map(run => run.id)).size !== runs.length) {
    throw new Error("对话规划检查记录无效");
  }
  const completed = [];
  for (const run of runs) {
    if (!identifier(run.id) || !identifier(run.workTaskId) || run.projectId !== projectId
      || run.sourceTaskId !== source.id || run.inputRevision !== source.inputRevision) {
      throw new Error("规划检查来源与本次对话不一致");
    }
    if (!["running", "completed"].includes(await dependencies.currentWorkFlowState())) break;
    const work = await dependencies.runtimeRequest(`${base}/work-tasks/${encodeURIComponent(run.workTaskId)}`);
    const parent = work.steps?.find(step => step.operation === "advisory"
      && step.currentAttemptId === source.id && step.status === "completed");
    if (work.kind !== "conversation" || !parent) throw new Error("规划检查未绑定已结束的原对话任务");
    const continuation = await readPlanningReviewContinuation(dependencies.runtimeRequest, projectId, work, run.id);
    if (!continuation) continue;
    dependencies.notifyReviewStart({ projectId, sourceTaskId: source.id, runId: run.id, artifactKind: run.artifactKind });
    const result = continuation.roles.length
      ? await dependencies.runPlanningReview(event, projectId, continuation.run, continuation)
      : { run: await dependencies.runtimeRequest(`${base}/planning-reviews/${encodeURIComponent(run.id)}/finalize`, { method: "POST", body: "{}" }) };
    completed.push(result.run);
  }
  return { planningReviews: completed, ...(completed.length ? { message: `${discussion.message}\n\n${completed.map(run =>
    `规划检查已保存 ${run.reports.length}/${run.roles.length} 个视角的报告${run.status === "completed" ? "。" : "，仍有未完成部分。"}`).join("\n")}规划内容未自动修改或采用。` } : {}) };
}
