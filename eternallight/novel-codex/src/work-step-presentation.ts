import type { RecoveryRun, WorkTaskSummary } from "./types";
import { compactTaskName, taskOperationName, taskOperationPurpose, taskTargetChapter } from "./task-operation-presentation";

type WorkStep = WorkTaskSummary["steps"][number];
type RecordValue = Record<string, unknown>;
const object = (value: unknown): RecordValue | null => value !== null && typeof value === "object" && !Array.isArray(value) ? value as RecordValue : null;

/** Names describe registered work, never infer actions or success from reply prose. */
export function workStepName(task: WorkTaskSummary, step: WorkStep): { label: string; fullLabel: string } {
  let fullLabel: string;
  if (step.operation !== "advisory") {
    fullLabel = taskOperationName(step.operation, step.target);
  } else {
    const decision = object(step.target.workDecision);
    if (decision?.workTaskId === task.id) {
      const selected = step.currentAttemptId ? task.steps.filter(candidate => {
        const action = object(candidate.target.workDecisionAction);
        return action?.workTaskId === task.id && action.decisionStepId === step.id && action.decisionTaskId === step.currentAttemptId
          && taskOperationPurpose(candidate.operation, candidate.target) !== null;
      }) : [];
      const actions = [...new Set(selected.map(action => taskOperationPurpose(action.operation, action.target)!))];
      const sourceId = typeof decision.sourceTaskId === "string" ? decision.sourceTaskId : null;
      const source = sourceId ? task.steps.find(candidate => candidate.id !== step.id && candidate.attempts.some(attempt => attempt.id === sourceId)) : null;
      const sourcePurpose = source ? taskOperationPurpose(source.operation, source.target) : null;
      fullLabel = actions.length ? `安排${actions.slice(0, 2).join("、")}${actions.length > 2 ? "等后续动作" : ""}`
        : sourcePurpose ? `核对${sourcePurpose}结果与后续安排`
        : "核对任务进展与后续安排";
    } else {
      const instruction = (step.instruction?.trim() || task.goal.trim()).replace(/\s+/g, " ");
      const number = taskTargetChapter(step.target);
      fullLabel = instruction ? `${number ? `第 ${number} 章 · ` : ""}讨论：${instruction}` : "处理创作要求";
    }
  }
  return compactTaskName(fullLabel);
}

/** Recovery only borrows a name from one loaded, current, identically owned attempt. */
export function recoveryRunName(run: RecoveryRun, tasks: WorkTaskSummary[]) {
  const matches = tasks.filter(task => task.projectId === run.projectId && task.threadId === run.threadId).flatMap(task =>
    task.steps.filter(step => step.operation === run.operation && step.currentAttemptId === run.runId
      && step.attempts.filter(attempt => attempt.id === run.runId && attempt.threadId === run.threadId).length === 1)
      .map(step => ({ task, step })));
  return matches.length === 1 ? workStepName(matches[0].task, matches[0].step) : compactTaskName(taskOperationName(run.operation));
}
