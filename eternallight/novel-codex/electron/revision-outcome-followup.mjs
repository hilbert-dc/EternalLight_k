// The repository queues assessments; the host never synthesizes an authorized target.
export async function assessQueuedRevision({ event, projectId, writerTask, runtimeRequest,
  currentWorkFlowState, runCreativeAgentTask }) {
  if (!writerTask.target?.desiredOutcomes?.length) return null;
  try {
    const state = await currentWorkFlowState();
    if (state !== "running") return { status: state === "paused" ? "paused" : "pending" };
    const base = `/api/projects/${encodeURIComponent(projectId)}`;
    const work = await runtimeRequest(`${base}/tasks/${encodeURIComponent(writerTask.id)}/work-task`);
    if (work.projectId !== projectId || work.status !== "running") return { status: work.status === "paused" ? "paused" : "pending" };
    const step = work.steps.find(candidate => candidate.operation === "assessRevisionOutcome"
      && candidate.target?.writerTaskId === writerTask.id && candidate.status === "queued"
      && work.steps.some(prior => prior.id === candidate.dependsOnStepId && prior.status === "completed"));
    if (!step) return { status: "pending" };
    const result = await runCreativeAgentTask(event, projectId, "核对作者已批准的局部改稿预期结果，只提交有实际原文证据的候选判断。",
      { operation: "assessRevisionOutcome" }, { ...step.target, resumeStepId: step.id });
    return { status: "recorded", taskId: result.workflow.task.id };
  } catch (error) {
    if (error?.code === "TASK_CANCELLED") throw error;
    return { status: "pending", error: String(error?.message ?? "结果核验尚未完成") };
  }
}
