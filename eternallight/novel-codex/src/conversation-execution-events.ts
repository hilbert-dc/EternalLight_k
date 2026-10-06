import type { WorkTaskSummary } from "./types";
import { createAgentRun, summarizeAgentRun, type AgentRun } from "./agent-events";

export interface ConversationExecutionOwner {
  projectId: string;
  originalThreadId: string | null;
  workTaskId: string | null;
  originTaskId: string | null;
  unstartedGeneration: boolean;
  parents: Set<string>;
  decisions: Map<string, string>;
  workers: Map<string, { operation: string }>;
}

export function createConversationExecutionOwner(projectId: string, source: { taskId: string; threadId: string } | null = null): ConversationExecutionOwner {
  return { projectId, originalThreadId: source?.threadId ?? null, workTaskId: null, originTaskId: source?.taskId ?? null, unstartedGeneration: false,
    parents: new Set(source ? [source.taskId] : []), decisions: new Map(), workers: new Map() };
}

const record = (value: unknown): Record<string, unknown> | null => value !== null && typeof value === "object" && !Array.isArray(value) ? value as Record<string, unknown> : null;
const id = (value: unknown): value is string => typeof value === "string" && value.trim() === value && value.length > 0
  && new TextEncoder().encode(value).length <= 160 && !/[\p{Cc}]/u.test(value);
const nullableId = (value: unknown) => value === null || id(value);
const initialOperations = new Set(["generateBrief", "generateOutline", "generatePlan", "generateChapter", "modelKnowledge", "extractCommitments", "generatePropertyRules", "reviseCommitment"]);

export function createRecoveredExecutionOwner(work: WorkTaskSummary): ConversationExecutionOwner | null {
  const unstartedGeneration = work.kind === "generation" && work.threadId === "" && work.steps.every(step => !step.attempts.length && step.currentAttemptId === null);
  if (!id(work.id) || !id(work.projectId) || (!id(work.threadId) && !unstartedGeneration)) return null;
  const decisions = work.steps.filter(step => record(step.target.workDecision));
  const origins = decisions.map(step => record(step.target.workDecision)?.originTaskId);
  if (decisions.some(step => record(step.target.workDecision)?.workTaskId !== work.id)
    || origins.some(origin => !nullableId(origin)) || new Set(origins).size > 1) return null;
  const first = work.steps[0]?.attempts[0];
  const origin = origins.length ? origins[0] : work.kind === "generation" ? null : first?.id;
  if (!nullableId(origin)) return null;
  const owner = createConversationExecutionOwner(work.projectId);
  owner.workTaskId = work.id; owner.originalThreadId = unstartedGeneration ? null : work.threadId; owner.originTaskId = origin as string | null;
  owner.unstartedGeneration = unstartedGeneration;
  for (const step of work.steps) {
    const declared = record(step.target.workDecisionAction);
    for (const attempt of step.attempts) {
      if (!id(attempt.id) || !id(attempt.threadId)) return null;
      if (step.operation === "advisory") {
        owner.parents.add(attempt.id);
        if (record(step.target.workDecision)) owner.decisions.set(attempt.id, step.id);
      } else {
        if (declared && declared.workTaskId !== work.id) return null;
        owner.workers.set(attempt.id, { operation: step.operation });
      }
    }
  }
  return owner;
}


// The App supplies only the active send's owner. A different workflow may start
// a new display run only through its declared parent, never through thread reuse.
export function applyConversationExecutionEvent(run: AgentRun, event: unknown, owner: ConversationExecutionOwner): AgentRun {
  const message = record(event), params = record(message?.params), workflow = record(params?.workflow);
  const task = record(workflow?.task), target = record(task?.target);
  if (message?.method !== "agent/workflow" || task?.projectId !== owner.projectId || run.state === "cancelled"
    || !id(task.id) || !id(task.threadId)) return run;
  const manifest = record(task.inputManifest), binding = record(workflow?.executionBinding);
  const decision = record(manifest?.workDecision), action = record(manifest?.workDecisionAction);
  if (decision || action || binding) {
    const frozen = action ?? decision;
    const declared = record(target?.[action ? "workDecisionAction" : "workDecision"]);
    if (!binding || !frozen || (decision && action) || !declared
      || !id(binding.workTaskId) || !id(binding.rootThreadId) || !id(binding.stepId)
      || binding.taskId !== task.id || !id(binding.decisionStepId) || !id(binding.decisionTaskId)
      || binding.workTaskId !== frozen.workTaskId || binding.rootThreadId !== frozen.rootThreadId
      || binding.decisionStepId !== frozen.decisionStepId || binding.decisionTaskId !== frozen.decisionTaskId
      || binding.originTaskId !== frozen.originTaskId || binding.originTaskId !== owner.originTaskId
      || declared.workTaskId !== binding.workTaskId
      || (owner.workTaskId !== null && owner.workTaskId !== binding.workTaskId)
      || (owner.originalThreadId !== binding.rootThreadId && !(owner.unstartedGeneration && owner.originalThreadId === null && decision))) return run;
    if (action) {
      if (!id(binding.actionKey) || binding.actionKey !== action.actionKey || task.operation !== action.operation
        || !nullableId(action.sourceTaskId) || !nullableId(action.sourceRevisionId)
        || declared.actionKey !== binding.actionKey || declared.decisionStepId !== binding.decisionStepId
        || declared.decisionTaskId !== binding.decisionTaskId
        || owner.decisions.get(binding.decisionTaskId) !== binding.decisionStepId) return run;
    } else if (task.operation !== "advisory" || task.threadId !== binding.rootThreadId || binding.actionKey !== null
      || binding.stepId !== binding.decisionStepId || binding.decisionTaskId !== task.id
      || declared.originTaskId !== binding.originTaskId || !nullableId(decision?.sourceTaskId)
      || declared.sourceTaskId !== decision?.sourceTaskId) return run;
    const known = owner.parents.has(task.id) || owner.workers.has(task.id);
    if ((known && run.taskId !== null) || (run.taskId && !["completed", "error"].includes(run.state))) return run;
    owner.workTaskId = binding.workTaskId;
    owner.originalThreadId = binding.rootThreadId; owner.unstartedGeneration = false;
    if (action) owner.workers.set(task.id, { operation: task.operation as string });
    else { owner.parents.add(task.id); owner.decisions.set(task.id, binding.decisionStepId); }
    return startExecution(run, task.id, task.threadId, !action);
  }
  if (!owner.parents.size) {
    if (owner.workTaskId !== null || task.operation !== "advisory" || target?.conversationContinuation || target?.requestedByTaskId
      || (run.taskId && run.taskId !== task.id)) return run;
    owner.originalThreadId = task.threadId;
    owner.originTaskId = task.id;
    owner.parents.add(task.id);
    return run;
  }
  if (owner.parents.has(task.id) || owner.workers.has(task.id) || !["completed", "error"].includes(run.state)) return run;
  // The original author advisory's first candidate still uses its persisted
  // author action. Subsequent choices require Runtime's decision admission.
  if (owner.workTaskId !== null || typeof task.operation !== "string" || !initialOperations.has(task.operation)
    || target?.requestedByTaskId !== owner.originTaskId || !id(target.requestedActionId)) return run;
  owner.workers.set(task.id, { operation: task.operation });
  return startExecution(run, task.id, task.threadId, false);
}

function startExecution(run: AgentRun, taskId: string, threadId: string, coordinating: boolean): AgentRun {
  const previous = summarizeAgentRun(run);
  return { ...createAgentRun(run.prompt, previous ? [previous, ...run.history] : run.history),
    taskId, threadId, model: run.model, provider: run.provider,
    steps: [{ id: "request", kind: "system", title: coordinating ? "正在分析并调整创作步骤" : "正在执行创作任务", status: "running" }] };
}
