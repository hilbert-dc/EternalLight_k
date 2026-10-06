import type { AgentRun, AgentRunSummary, AgentStep } from "./agent-events";

export type TimelineRun = Pick<AgentRun, "steps" | "state" | "answer" | "startedAt" | "completedAt" | "threadId" | "turnId">;
export type DisplayExecution = Pick<TimelineRun, "steps" | "startedAt" | "completedAt"> & { totalPublicSteps: number; detailsTruncated: boolean };

export function isPublicExecutionStep(step: AgentStep) {
  return step.sourceType !== "reasoning" && step.kind !== "assistant" && step.id !== "agent-message"
    && step.id !== "request" && !step.id.startsWith("turn:") && !step.id.startsWith("stderr:")
    && !step.id.startsWith("user-input-resolved:") && !["运行 Hook", "运行时提示", "作者回答已提交"].includes(step.title);
}

export function snapshotExecution(run: TimelineRun): DisplayExecution {
  const publicSteps = run.steps.filter(isPublicExecutionStep);
  const snapshot: DisplayExecution = { steps: [], startedAt: run.startedAt, completedAt: run.completedAt,
    totalPublicSteps: publicSteps.length, detailsTruncated: false };
  const clip = (value: string, limit: number) => {
    if (value.length <= limit) return value;
    snapshot.detailsTruncated = true;
    return `${value.slice(0, limit - 1)}…`;
  };
  for (const step of publicSteps) {
    if (snapshot.steps.length === 96) break;
    if (step.id.length > 160) continue; // Never truncate an identity into another step's key.
    const projected: AgentStep = { id: step.id, kind: step.kind, status: step.status, title: clip(step.title, 160) };
    if (step.text) projected.text = clip(step.text, 512);
    if (step.detail) projected.detail = clip(step.detail, 1024);
    if (step.messagePhase) projected.messagePhase = step.messagePhase;
    if (step.collaborators) projected.collaborators = step.collaborators.map(agent => ({ ...agent,
      ...(agent.task ? { task: clip(agent.task, 512) } : {}), ...(agent.result ? { result: clip(agent.result, 1024) } : {}) }));
    snapshot.steps.push(projected);
    if (new TextEncoder().encode(JSON.stringify(snapshot)).length > 24 * 1024) { snapshot.steps.pop(); break; }
  }
  return snapshot;
}

export function retainRecentExecutions(history: AgentRunSummary[]): AgentRunSummary[] {
  return history.map((entry, index) => index < 10 || !entry.displayExecution ? entry : { ...entry, displayExecution: null });
}
