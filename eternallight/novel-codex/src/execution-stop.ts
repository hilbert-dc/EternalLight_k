import { reduceAgentEvent, type AgentRun } from "./agent-events";

export type StopOwner = { scope: object; projectId: string; threadId: string | null; turnId: string | null };
export type StopTicket = StopOwner & { result: Promise<boolean>; settle: (interrupted: boolean) => void };

export class StoppedExecutionError extends Error {
  constructor() { super("本次执行已停止，已保存内容仍保留。"); this.name = "StoppedExecutionError"; }
}

export function createStopTicket(owner: StopOwner): StopTicket {
  let settle!: (interrupted: boolean) => void;
  const result = new Promise<boolean>(resolve => { settle = resolve; });
  return { ...owner, result, settle };
}

export function ownsStop(ticket: StopTicket | null, owner: StopOwner): ticket is StopTicket {
  return !!ticket && ticket.scope === owner.scope && ticket.projectId === owner.projectId
    && (!ticket.threadId || ticket.threadId === owner.threadId) && (!ticket.turnId || ticket.turnId === owner.turnId);
}

function cancellationMessage(message: string): boolean {
  // This is the current Electron invoke wrapper, not an inferred stop command.
  const wrapped = /^Error invoking remote method 'agent:[a-z-]+': Error: ([\s\S]*)$/.exec(message);
  return ["生成已停止", "规划深审已停止", "规划深审已停止；已保存报告仍会保留"].includes(wrapped?.[1] ?? message);
}

export async function isConfirmedCancellation(message: string, ticket: StopTicket | null, owner: StopOwner, run: AgentRun) {
  if (!cancellationMessage(message)) return false;
  if (run.state === "cancelled") return true;
  return ownsStop(ticket, owner) && await ticket.result;
}

export function stoppedRun(run: AgentRun): AgentRun {
  if (run.state === "completed" || run.state === "cancelled" || (run.state === "error" && !cancellationMessage(run.error ?? ""))) return run;
  // Caller has checked the local stop receipt's execution owner. Native scoped
  // terminal guards remain unchanged; this is a confirmed host control result.
  return reduceAgentEvent(run, { method: "turn/completed", params: { turn: { status: "interrupted" } } });
}

export function failedRun(run: AgentRun, message: string): AgentRun {
  if (run.state === "error" && cancellationMessage(message) && !cancellationMessage(run.error ?? "")) return run;
  return reduceAgentEvent(run, { method: "agent/error", params: { message } });
}

export function confirmsNativeStop(ticket: StopTicket, event: unknown): boolean {
  const message = event as { method?: string; params?: { projectId?: string; threadId?: string; turnId?: string; turn?: { id?: string; status?: string } } } | null;
  const params = message?.params;
  return message?.method === "turn/completed" && params?.turn?.status === "interrupted"
    && !!ticket.threadId && !!ticket.turnId && params.threadId === ticket.threadId
    && params.turn.id === ticket.turnId && (!params.turnId || params.turnId === ticket.turnId)
    && (!params.projectId || params.projectId === ticket.projectId);
}
