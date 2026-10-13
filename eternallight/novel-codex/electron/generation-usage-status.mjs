// @ts-check
// A failed read is not a receipt. Keep the failed stage in the Host projection.
export async function listGenerationRunsWithUsage(runtimeRequest, projectId, settleUsage) {
  if (typeof projectId !== "string" || !projectId.trim() || projectId.length > 160 || /[\0\r\n]/.test(projectId)) throw new Error("作品 ID 无效");
  const { runs } = await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/generation-runs`);
  const reconciled = [];
  for (const run of runs) {
    const current = value => ({ ...value, usageCheck: { state: "current" } });
    if (!["running", "paused", "blocked"].includes(run.status) || !run.activeTaskId
      || run.providerBudget?.charges?.some(charge => charge.taskId === run.activeTaskId)) {
      reconciled.push(current(run)); continue;
    }
    let phase = "taskRead";
    try {
      const task = await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/tasks/${encodeURIComponent(run.activeTaskId)}`);
      if (!["completed", "failed", "cancelled"].includes(task.status)) { reconciled.push(current(run)); continue; }
      phase = "usageSettlement";
      reconciled.push(current(await settleUsage(projectId, run, run.activeTaskId)));
    } catch (error) {
      reconciled.push({ ...run, usageCheck: { state: "unavailable", phase,
        code: typeof error?.code === "string" && /^[a-z][a-z0-9_]{0,79}$/.test(error.code) ? error.code : null } });
    }
  }
  return { runs: reconciled };
}
