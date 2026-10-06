import type { ReactNode } from "react";
import type { AgentRun, AgentRunSummary } from "./agent-events";
import type { WorkTaskSummary } from "./types";
import type { TaskResultDestination } from "./task-results";
import { isPublicExecutionStep, type TimelineRun } from "./conversation-process";

export type WorkConversationContent = {
  threadId: string | null;
  run: AgentRun;
  renderCurrent: (options: { taskBound: boolean }) => ReactNode;
  renderHistory: (entry: AgentRunSummary, options: { taskBound: boolean }) => ReactNode;
};

type BoundTurn = { threadId: string | null; turnId: string | null; taskId?: string | null };

export function conversationWorkSteps(run: Pick<TimelineRun, "steps">) {
  return run.steps.filter(isPublicExecutionStep);
}

export function conversationActivity(run: Pick<TimelineRun, "steps" | "answer">) {
  const steps = conversationWorkSteps(run);
  const current = [...run.steps].reverse().find(step => ["running", "waiting"].includes(step.status)
    && (step.sourceType === "reasoning" || steps.some(visible => visible.id === step.id)));
  const commentary = [...steps].reverse().find(step => step.messagePhase === "commentary" && step.text?.trim())?.text ?? "";
  return { title: current?.sourceType === "reasoning" ? "正在分析" : current?.messagePhase === "commentary" ? "正在处理" : current?.title ?? (run.answer.trim() ? "正在写回复" : "正在处理"),
    commentary: commentary.length > 280 ? `${commentary.slice(0, 280)}…` : commentary };
}

export function sameConversationFailure(left: string | null, right: string | null) {
  const normalize = (value: string) => value.trim().replace(/^Error invoking remote method '[^']+':\s*/, "").replace(/^Error:\s*/, "");
  return Boolean(left && right && normalize(left) === normalize(right));
}
export function attemptContainsRun(attempt: Pick<WorkTaskSummary["steps"][number]["attempts"][number], "id" | "threadId" | "turnId">, turn: BoundTurn): boolean {
  // A current host workflow knows its attempt even before the terminal turn
  // receipt is persisted. Native history instead carries exact turn identity.
  if (turn.taskId) return Boolean(turn.threadId && attempt.id === turn.taskId && attempt.threadId === turn.threadId
    && (!attempt.turnId || !turn.turnId || attempt.turnId === turn.turnId));
  return Boolean(turn.threadId && turn.turnId && attempt.threadId === turn.threadId && attempt.turnId === turn.turnId);
}

export function taskContainsTurn(task: WorkTaskSummary, turn: BoundTurn): boolean {
  return task.steps.some(step => step.attempts.some(attempt => attemptContainsRun(attempt, turn)));
}

/** A separately funded continuation still has the exact original author message. */
export function hasOriginalAuthorMessage(task: WorkTaskSummary, tasks: WorkTaskSummary[]): boolean {
  if (task.kind !== "generation") return false;
  const origins = task.steps.flatMap(step => {
    const value = step.target.workDecision;
    const binding = value && typeof value === "object" && !Array.isArray(value) ? value as Record<string, unknown> : null;
    return binding && typeof binding === "object" && !Array.isArray(binding)
      && binding.workTaskId === task.id && typeof binding.originTaskId === "string" && binding.originTaskId
      ? [binding.originTaskId] : [];
  });
  if (!origins.length || new Set(origins).size !== 1) return false;
  const matches = tasks.filter(other => other.id !== task.id && other.projectId === task.projectId
    && other.threadId === task.threadId && other.steps.some(step => step.operation === "advisory"
      && step.attempts.some(attempt => attempt.id === origins[0] && attempt.threadId === task.threadId)));
  return matches.length === 1;
}

export function primaryReply(task: WorkTaskSummary) {
  const finished = task.steps.filter(step => step.attempts.some(attempt => ["completed", "failed", "cancelled"].includes(attempt.status)));
  const last = finished.at(-1);
  const binding = last?.target?.workDecision;
  const finalAttempt = last?.attempts.filter(attempt => ["completed", "failed", "cancelled"].includes(attempt.status)).at(-1);
  const finalDecision = last?.operation === "advisory" && binding && typeof binding === "object" && !Array.isArray(binding)
    && (binding as Record<string, unknown>).workTaskId === task.id && finalAttempt
    && last.currentAttemptId === finalAttempt.id && finalAttempt.threadId === task.threadId ? last : null;
  const step = finalDecision ?? [...finished].reverse().find(item => ["generateChapter", "generateBrief", "generateOutline", "generatePlan", "generatePropertyRules"].includes(item.operation))
    ?? [...finished].reverse().find(item => ["modelKnowledge", "extractCommitments"].includes(item.operation))
    ?? [...finished].reverse().find(item => item.operation === "advisory") ?? finished.at(-1);
  const attempt = step?.attempts.filter(item => ["completed", "failed", "cancelled"].includes(item.status)).at(-1);
  return attempt ? { attempt, operation: step!.operation } : null;
}

/** Internal replies belong to their execution step once later work is admitted.
 * Failures only fold when Native explicitly supersedes their step for recovery. */
export function intermediateReplyStep(task: WorkTaskSummary,
  turn: BoundTurn & { state: AgentRun["state"]; error?: string | null }) {
  if (!["completed", "error", "cancelled"].includes(turn.state)) return null;
  const matches = task.steps.flatMap((step, index) => step.attempts
    .filter(attempt => attemptContainsRun(attempt, turn)).map(attempt => ({ step, index, attempt })));
  if (matches.length !== 1) return null;
  const { step, index, attempt } = matches[0];
  const cleanCompletion = turn.state === "completed" && !turn.error && attempt.status === "completed" && !attempt.error;
  const supersededFailure = step.status === "superseded" && ["failed", "cancelled"].includes(attempt.status);
  if (!cleanCompletion && !supersededFailure) return null;
  const laterAttempt = step.attempts.some((item, i) => i > step.attempts.indexOf(attempt) && item.status !== "cancelled");
  const laterStep = task.steps.slice(index + 1).some(item => !["cancelled", "superseded"].includes(item.status));
  return laterAttempt || laterStep ? { stepId: step.id, attemptId: attempt.id } : null;
}

function completedDelivery(task: WorkTaskSummary) {
  const evidence = task.deliveryReport.evidence;
  const outputs = evidence.filter(item => item.kind !== "completionReport");
  return task.status === "completed" && ["ready_for_review", "partial"].includes(task.deliveryStatus) && !task.error
    && task.steps.every(step => ["completed", "superseded"].includes(step.status))
    && outputs.length > 0 && outputs.every(item => Boolean(item.referenceId))
    && evidence.every(item => ["recorded", "superseded"].includes(item.state))
    && (task.deliveryStatus !== "partial" || outputs.some(item => item.state === "superseded"))
    && !task.deliveryReport.pendingActionIds.length && !task.deliveryReport.unsettledAttemptIds.length;
}

/** Historical delivery is a display fact, never a destination to the newer revision. */
export function completedDeliveryPresentation(task: WorkTaskSummary): "current" | "updated" | null {
  if (!completedDelivery(task) || task.steps.some(step => step.target.revisionRepairSource != null
    || step.attempts.some(attempt => attempt.error || ["failed", "cancelled"].includes(attempt.status)))) return null;
  const evidence = task.deliveryReport.evidence;
  if (task.steps.some(step => step.status === "completed"
    && evidence.filter(item => item.stepId === step.id
      && item.kind === (step.operation === "reviewPlanningSeat" ? "planningReview" : step.expectedOutputKind)).length !== 1)) return null;
  if (!evidence.every(item => {
    const steps = task.steps.filter(step => step.id === item.stepId);
    if (steps.length !== 1) return false;
    const step = steps[0];
    const attempts = step.attempts.filter(attempt => attempt.id === item.attemptId);
    const kind = step.operation === "reviewPlanningSeat" ? "planningReview" : step.expectedOutputKind;
    return step.status === "completed" && item.attemptId === step.currentAttemptId
      && attempts.length === 1 && attempts[0].status === "completed" && item.kind === kind
      && typeof item.referenceId === "string" && item.referenceId.trim() === item.referenceId
      && item.referenceId.length > 0 && new TextEncoder().encode(item.referenceId).length <= 160
      && !/[\p{Cc}]/u.test(item.referenceId)
      && (kind !== "completionReport" || item.referenceId === item.attemptId)
      && evidence.filter(other => other.stepId === item.stepId && other.kind === item.kind).length === 1;
  })) return null;
  return task.deliveryReport.evidence.some(item => item.kind !== "completionReport" && item.state === "superseded")
    ? "updated" : "current";
}

export function generatedReplyDescription(task: WorkTaskSummary, results: TaskResultDestination[]) {
  const reply = primaryReply(task);
  const presentation = completedDeliveryPresentation(task);
  const kinds: Record<string, TaskResultDestination["kind"]> = { generateBrief: "brief", generateOutline: "outline",
    generatePlan: "plan", generateChapter: "chapter", generatePropertyRules: "propertyRules" };
  if (!presentation || !reply || !Object.hasOwn(kinds, reply.operation)) return null;
  const steps = task.steps.filter(step => step.operation === reply.operation && step.status === "completed"
    && step.currentAttemptId === reply.attempt.id && step.attempts.filter(attempt => attempt.id === reply.attempt.id).length === 1);
  if (steps.length !== 1 || reply.attempt.status !== "completed") return null;
  const step = steps[0];
  const evidence = task.deliveryReport.evidence.filter(item => item.stepId === step.id && item.kind !== "completionReport");
  const kind = kinds[reply.operation];
  if (evidence.length !== 1 || evidence[0].attemptId !== reply.attempt.id
    || evidence[0].kind !== (kind === "chapter" ? "draft" : kind) || !evidence[0].referenceId) return null;
  const binding = { stepId: step.id, attemptId: reply.attempt.id };
  if (presentation === "updated") return binding;
  return results.some(result => result.projectId === task.projectId && result.workTaskId === task.id
    && result.stepId === step.id && result.attemptId === reply.attempt.id
    && result.kind === kind && evidence[0].state === "recorded" && evidence[0].referenceId === result.referenceId)
    ? binding : null;
}

export function conversationItems(tasks: WorkTaskSummary[], conversation: WorkConversationContent | null) {
  const run = conversation?.run;
  const history = run?.history ?? [];
  const items: ({ kind: "task"; task: WorkTaskSummary; current: boolean; history: AgentRunSummary[]; time: number }
    | { kind: "history"; entry: AgentRunSummary; time: number }
    | { kind: "current"; time: number })[] = [];
  for (const task of [...tasks].reverse()) {
    const matches = history.filter(entry => taskContainsTurn(task, entry));
    const attempts = task.steps.flatMap(step => step.attempts);
    const positions = new Map(matches.map(entry => {
      const found = attempts.flatMap((attempt, index) => attemptContainsRun(attempt, entry) ? [index] : []);
      return [entry, found.length === 1 ? found[0] : null] as const;
    }));
    const ordered = matches.filter(entry => positions.get(entry) !== null)
      .sort((left, right) => positions.get(left)! - positions.get(right)!);
    // Ambiguous identity stays in its original slot; source order is used only
    // for entries that bind one exact persisted attempt.
    const orderedHistory = matches.map(entry => positions.get(entry) === null ? entry : ordered.shift()!);
    const times = task.steps.flatMap(step => step.attempts.map(attempt => attempt.startedAt * 1000)).filter(time => Number.isFinite(time) && time > 0);
    items.push({ kind: "task", task, current: Boolean(run && run.state !== "idle" && taskContainsTurn(task, run)),
      history: orderedHistory, time: times.length ? Math.min(...times) : matches.at(-1)?.createdAt ?? 0 });
  }
  for (const entry of [...history].reverse()) {
    if (!tasks.some(task => taskContainsTurn(task, entry))) items.push({ kind: "history", entry, time: entry.createdAt });
  }
  if (run && run.state !== "idle" && !tasks.some(task => taskContainsTurn(task, run))) {
    items.push({ kind: "current", time: run.startedAt ?? Number.MAX_SAFE_INTEGER });
  }
  return items.sort((a, b) => a.time - b.time);
}
