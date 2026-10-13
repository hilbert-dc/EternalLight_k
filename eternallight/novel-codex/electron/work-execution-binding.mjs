// @ts-check
// The renderer receives provenance only after the author-side Host has checked
// the task against Runtime's persisted Work, step and attempt membership.
export async function readWorkExecutionBinding(runtimeRequest, projectId, task) {
  const decision = task.inputManifest?.workDecision;
  const action = task.inputManifest?.workDecisionAction;
  if (!decision && !action) return null;
  if (decision && action) throw new Error("任务包含冲突的执行来源");
  const binding = action ?? decision;
  const work = await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/tasks/${encodeURIComponent(task.id)}/work-task`);
  const step = work.steps?.find(item => item.currentAttemptId === task.id
    && item.attempts?.some(attempt => attempt.id === task.id && attempt.threadId === task.threadId));
  const declared = task.target?.[action ? "workDecisionAction" : "workDecision"];
  if (task.projectId !== projectId || work.projectId !== projectId || work.id !== binding.workTaskId
    || work.threadId !== binding.rootThreadId || !step || step.operation !== task.operation || !declared
    || declared.workTaskId !== work.id
    || (action && (declared.decisionStepId !== binding.decisionStepId
      || declared.decisionTaskId !== binding.decisionTaskId || declared.actionKey !== binding.actionKey))
    || (!action && (declared.originTaskId !== binding.originTaskId || declared.sourceTaskId !== binding.sourceTaskId))) {
    throw new Error("任务的工作、步骤或对话来源不一致");
  }
  const parent = action ? work.steps.find(item => item.id === binding.decisionStepId
    && item.attempts?.some(attempt => attempt.id === binding.decisionTaskId
      && attempt.threadId === work.threadId)) : step;
  if (!parent || parent.operation !== "advisory" || (!action && (step.id !== binding.decisionStepId
    || task.threadId !== work.threadId))) throw new Error("任务缺少原对话中的决策记录");
  return Object.freeze({ workTaskId: work.id, rootThreadId: work.threadId, stepId: step.id, taskId: task.id,
    decisionStepId: parent.id, decisionTaskId: action ? binding.decisionTaskId : task.id,
    actionKey: action ? binding.actionKey : null, originTaskId: binding.originTaskId ?? null });
}
