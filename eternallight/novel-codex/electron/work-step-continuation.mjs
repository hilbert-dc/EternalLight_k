// The domain owns readiness and authorization; the host only executes its queued steps.
export async function advanceWorkSteps(event, projectId, workTaskId, {
  runtimeRequest, currentWorkFlowState, runCreativeAgentTask, runAdvisoryAgentTask,
  runCandidateWorkflow, modelChapterKnowledge,
}) {
  const base = `/api/projects/${encodeURIComponent(projectId)}`;
  const path = `${base}/work-tasks/${encodeURIComponent(workTaskId)}`;
  for (let count = 0; count < 16; count += 1) {
    if (!["running", "completed"].includes(await currentWorkFlowState())) return runtimeRequest(path);
    let work = await runtimeRequest(path);
    if (work.status === "completed") work = await runtimeRequest(`${path}/execution-scope/continue`, { method: "POST" });
    if (work.status !== "running") return work;
    const step = work.steps.find((candidate) => candidate.status === "queued"
      && (!candidate.dependsOnStepId || work.steps.some((prior) => prior.id === candidate.dependsOnStepId && prior.status === "completed")));
    if (!step) return work;
    if (step.operation === "modelKnowledge") {
      if (step.target.modelingIntent === true) {
        await runCreativeAgentTask(event, projectId, work.goal, { operation: "modelKnowledge" }, { ...step.target, resumeStepId: step.id });
        continue;
      }
      const { action } = await runtimeRequest(`${base}/chapters/${encodeURIComponent(step.target.chapterId)}/next-step`);
      if (!action) return work;
      await modelChapterKnowledge(event, projectId, action.chapterId, null, action.sourceRevisionId, step.target.sourceTaskId);
    } else if (step.operation === "assessRevisionOutcome") {
      await runCreativeAgentTask(event, projectId, work.goal, { operation: "assessRevisionOutcome" }, { ...step.target, resumeStepId: step.id });
    } else if (step.operation === "advisory") {
      await runAdvisoryAgentTask(event, projectId, work.goal, step);
    } else {
      const target = step.instruction ? { ...step.target, requestedByTaskId: step.sourceTaskId, requestedActionId: step.id }
        : { ...step.target, resumeStepId: step.id };
      await runCandidateWorkflow(event, projectId, work.goal, { operation: step.operation }, target);
    }
  }
  return runtimeRequest(path);
}
