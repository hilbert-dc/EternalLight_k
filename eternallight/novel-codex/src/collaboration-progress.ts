import type { AgentStep } from "./agent-events";
import type { TimelineRun } from "./conversation-process";

export interface Collaborator {
  id: string;
  task?: string;
  status: string;
  result?: string;
}

export function collaborationProgress(run: TimelineRun): Collaborator[] {
  const agents = new Map<string, Collaborator>();
  for (const step of run.steps) {
    for (const agent of step.collaborators ?? []) {
      const previous = agents.get(agent.id);
      agents.set(agent.id, { ...previous, ...agent, task: previous?.task ?? agent.task,
        result: ["running", "pendingInit"].includes(agent.status) ? undefined : agent.result ?? previous?.result,
        status: agent.status === "unknown" && previous ? previous.status : agent.status });
    }
  }
  return [...agents.values()].map(agent => {
    if (!["pendingInit", "running", "unknown"].includes(agent.status)) return agent;
    if (run.state === "cancelled") return { ...agent, status: "turnStopped" };
    if (run.state === "completed" || run.state === "error") return { ...agent, status: "unknown" };
    return agent;
  });
}

export function collaborationFromItem(item: Record<string, unknown>): NonNullable<AgentStep["collaborators"]> {
  const states = item.agentsStates && typeof item.agentsStates === "object" ? item.agentsStates as Record<string, unknown> : {};
  const ids = new Set([...(Array.isArray(item.receiverThreadIds) ? item.receiverThreadIds : []), ...Object.keys(states)]);
  return [...ids].filter((id): id is string => typeof id === "string" && id.length > 0 && id.length <= 160).slice(0, 64).map(id => {
    const state = states[id] && typeof states[id] === "object" ? states[id] as Record<string, unknown> : {};
    const status = typeof state.status === "string" && ["pendingInit", "running", "completed", "errored", "interrupted", "shutdown", "notFound"].includes(state.status) ? state.status : "unknown";
    return { id, status,
      ...(item.tool === "spawnAgent" && typeof item.prompt === "string" && item.prompt.trim() ? { task: item.prompt.trim().slice(0, 1200) } : {}),
      ...(["completed", "errored"].includes(status) && typeof state.message === "string" ? { result: state.message.slice(0, 1600) } : {}),
    };
  });
}
