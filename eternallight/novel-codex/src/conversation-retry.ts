import type { AgentRun } from "./agent-events";
import type { WorkTaskSummary } from "./types";
import { attemptContainsRun } from "./work-conversation";

function resumeBlocked(task: WorkTaskSummary) {
  return Boolean(task.resumeBlockReason) && !(task.resumeBlockReason === "unknownUsage"
    && task.resumeMode === "recoverSavedResult");
}

export function isConversationRetryCandidate(task: WorkTaskSummary) {
  return !resumeBlocked(task) && (task.resumeMode === "continueExecution" || task.resumeMode === "recoverSavedResult")
    && (["paused", "blocked"].includes(task.status) || task.status === "completed" && task.pendingContinuation);
}

/** Explicit selection lists existing owned tasks; it never chooses by recency. */
export function conversationRetryCandidates(tasks: WorkTaskSummary[], projectId: string, threadId: string | null) {
  const owned = tasks.filter(task => task.projectId === projectId && (threadId ? task.threadId === threadId : !task.threadId));
  return owned.filter(task => isConversationRetryCandidate(task) && owned.filter(other => other.id === task.id).length === 1);
}

/** A retry is an explicit action on an unambiguous current conversation task. */
export function conversationRetryTask(tasks: WorkTaskSummary[], projectId: string, threadId: string | null, run: AgentRun | null) {
  const owned = tasks.filter(task => task.projectId === projectId && (
    threadId ? task.threadId === threadId : !task.threadId));
  const bound = run ? owned.filter(task => task.steps.some(step => step.attempts.some(attempt => attemptContainsRun(attempt, run)))) : [];
  if (run && (run.taskId || run.turnId) && !bound.length) return null;
  const candidates = (bound.length ? bound : owned).filter(isConversationRetryCandidate);
  return candidates.length === 1 ? candidates[0] : null;
}
