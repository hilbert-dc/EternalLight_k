// The caller's private provenance proves that this execution never attempted
// task creation. A queued row alone cannot establish a zero-dispatch outcome.
export async function recoverUnstartedWorkResume({ runtimeRequest, controlWork, projectId, workTaskId, owner, provenance, error }) {
  if (!provenance || error?.code === "TASK_CANCELLED" || provenance.projectId !== projectId
    || provenance.workTaskId !== workTaskId || owner.id !== workTaskId || owner.projectId !== projectId
    || typeof owner.threadId !== "string" || (!owner.threadId && owner.kind !== "generation") || !provenance.stepId) throw error;
  const base = `/api/projects/${encodeURIComponent(projectId)}`;
  const path = `${base}/work-tasks/${encodeURIComponent(workTaskId)}`;
  try {
    const fresh = await runtimeRequest(path);
    const step = fresh.steps?.find(item => item.id === provenance.stepId);
    if (fresh.id !== owner.id || fresh.projectId !== projectId || fresh.threadId !== owner.threadId
      || fresh.kind !== owner.kind || fresh.status !== "running" || !Number.isSafeInteger(fresh.controlRevision)
      || !step || step.status !== "queued" || step.currentAttemptId !== null
      || (!owner.threadId && fresh.steps.some(item => item.currentAttemptId !== null))
      || fresh.steps.some(item => ["running", "waiting"].includes(item.status))) throw error;
    const paused = await controlWork(fresh, "pause");
    if (paused.id !== owner.id || paused.projectId !== projectId || paused.threadId !== owner.threadId
      || paused.kind !== owner.kind || paused.status !== "paused") throw error;
    return { ...paused, resumeFailure: { code: "WORK_RESUME_NOT_STARTED",
      message: error.message, workTaskId } };
  } catch { throw error; }
}
