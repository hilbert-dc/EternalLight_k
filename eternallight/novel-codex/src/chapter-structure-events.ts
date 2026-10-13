import { createAgentRun, summarizeAgentRun, type AgentRun } from "./agent-events";

const record = (value: unknown): Record<string, unknown> | null => value !== null && typeof value === "object" && !Array.isArray(value) ? value as Record<string, unknown> : null;
const id = (value: unknown): value is string => typeof value === "string" && value.trim() === value && value.length > 0 && value.length <= 160;

export function applyChapterStructureEvent(run: AgentRun, event: unknown, projectId: string): AgentRun {
  const message = record(event), params = record(message?.params);
  if (!params || params.projectId !== projectId || !id(params.taskId)) return run;
  if (message?.method === "structure/error" && run.origin === "chapterStructure" && run.taskId === params.taskId
    && typeof params.message === "string" && ["running", "waiting"].includes(run.state)) {
    return { ...run, state: "error", error: params.message.slice(0, 240), completedAt: Date.now() };
  }
  if (message?.method !== "agent/structure-started" || params.operation !== "assessChapterStructure" || !id(params.threadId)
    || run.taskId === params.taskId || ["running", "waiting"].includes(run.state)) return run;
  const previous = summarizeAgentRun(run);
  return { ...createAgentRun("检查章节删除后的故事衔接", previous ? [previous, ...run.history] : run.history),
    origin: "chapterStructure", taskId: params.taskId, threadId: params.threadId,
    steps: [{ id: "request", kind: "system", title: "自动检查故事衔接", status: "running" }] };
}
