// The caller's private provenance proves that this execution never attempted
// task creation. A queued row alone cannot establish a zero-dispatch outcome.
export async function recoverUnstartedWorkResume({ runtimeRequest, projectId, workTaskId, owner, provenance, error }) {
  if (!provenance || error?.code === "TASK_CANCELLED" || provenance.projectId !== projectId
    || provenance.workTaskId !== workTaskId || owner.id !== workTaskId || owner.projectId !== projectId
    || typeof owner.threadId !== "string" || (!owner.threadId && owner.kind !== "generation") || !provenance.stepId) throw error;
  const base = `/api/projects/${encodeURIComponent(projectId)}`;
  const path = `${base}/work-tasks/${encodeURIComponent(workTaskId)}`;
  try {
    const fresh = await runtimeRequest(path);
    const step = fresh.steps?.find(item => item.id === provenance.stepId);
    if (fresh.id !== owner.id || fresh.projectId !== projectId || fresh.threadId !== owner.threadId
      || fresh.kind !== owner.kind || fresh.status !== "running" || !Number.isInteger(fresh.version)
      || !step || step.status !== "queued" || step.currentAttemptId !== null
      || (!owner.threadId && fresh.steps.some(item => item.currentAttemptId !== null))
      || fresh.steps.some(item => ["running", "waiting"].includes(item.status))) throw error;
    let paused;
    if (fresh.kind === "generation") {
      const runId = workTaskId.slice("work-".length);
      const runPath = `${base}/generation-runs/${encodeURIComponent(runId)}`;
      const run = await runtimeRequest(runPath);
      if (run.id !== runId || run.projectId !== projectId || run.status !== "running"
        || run.activeTaskId !== null || !Number.isInteger(run.version)) throw error;
      await runtimeRequest(`${runPath}/control`, {
        method: "POST", body: JSON.stringify({ expectedVersion: run.version, command: "pause" }),
      });
      paused = await runtimeRequest(path);
    } else {
      paused = await runtimeRequest(`${path}/control`, {
        method: "POST", body: JSON.stringify({ expectedVersion: fresh.version, command: "pause" }),
      });
    }
    if (paused.id !== owner.id || paused.projectId !== projectId || paused.threadId !== owner.threadId
      || paused.kind !== owner.kind || paused.status !== "paused") throw error;
    return { ...paused, resumeFailure: { code: "WORK_RESUME_NOT_STARTED",
      message: error.message, workTaskId } };
  } catch { throw error; }
}
