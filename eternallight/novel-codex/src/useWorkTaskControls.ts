import { useLayoutEffect, useRef, useState } from "react";

type Scope = { projectId: string };
type Ticket = { targetId: string; superseded: boolean; cancelled: boolean };
type Session = { scope: Scope; active: boolean; resume: Ticket | null; control: Ticket | null; errorTicket: Ticket | null };
type Options = {
  scope: Scope;
  isBlocked: () => boolean;
  refresh: () => Promise<void>;
  onFormalChange: () => Promise<void>;
  onError: (message: string | null) => void;
  onNotice: (message: string | null) => void;
};

// Owns in-flight UI requests only. Runtime remains the authority for task status.
export function useWorkTaskControls(options: Options) {
  const { scope } = options;
  const sessionRef = useRef<Session | null>(null);
  const [state, setState] = useState<{ session: Session; resuming: boolean; controlling: boolean } | null>(null);
  useLayoutEffect(() => {
    const session: Session = { scope, active: true, resume: null, control: null, errorTicket: null };
    sessionRef.current = session;
    return () => {
      session.active = false;
      if (sessionRef.current === session) sessionRef.current = null;
    };
  }, [scope]);

  async function run(kind: "resume" | "pause" | "cancel" | "undo", targetId: string, action: () => Promise<unknown>, failureMessage: string, notice: string) {
    const session = sessionRef.current;
    if (!session?.active || session.scope !== scope || options.isBlocked() || session.control
      || (session.resume && (kind === "resume" || kind === "undo"))) return;
    const ticket: Ticket = { targetId, superseded: false, cancelled: false };
    const controlledResume = session.resume?.targetId === targetId ? session.resume : null;
    const slot = kind === "resume" ? "resume" : "control";
    session[slot] = ticket;
    setState({ session, resuming: !!session.resume, controlling: !!session.control });
    if (!session.errorTicket || session.errorTicket.targetId === targetId) {
      session.errorTicket = null;
      options.onError(null);
    }
    options.onNotice(null);
    let succeeded = false;
    try {
      try {
        await action();
        succeeded = true;
        if (!session.active) return;
        if ((kind === "pause" || kind === "cancel") && controlledResume) {
          controlledResume.superseded = true;
          if (kind === "cancel") {
            controlledResume.cancelled = true;
            if (session.errorTicket === controlledResume) {
              session.errorTicket = null;
              options.onError(null);
            }
          }
        }
        if (!ticket.superseded) options.onNotice(notice);
      } catch (reason) {
        if (!session.active) return;
        if (!ticket.cancelled) {
          session.errorTicket = ticket;
          options.onError(reason instanceof Error ? reason.message : failureMessage);
        }
      }
      if (!session.active) return;
      // Even failed execution can have durable partial output. Read, never replay.
      try {
        await options.refresh();
        if (!session.active) return;
        await options.onFormalChange();
      } catch (reason) {
        if (session.active && succeeded && !ticket.superseded) {
          session.errorTicket = ticket;
          options.onError(`操作已返回成功，但作品界面刷新失败：${reason instanceof Error ? reason.message : "请重新读取作品状态"}`);
        }
      }
    } finally {
      if (session[slot] === ticket) session[slot] = null;
      if (session.active) setState({ session, resuming: !!session.resume, controlling: !!session.control });
    }
  }

  const visible = state?.session.active && state.session.scope === scope ? state : null;
  return { run, pending: !!visible && (visible.resuming || visible.controlling), controlling: !!visible?.controlling,
    hasPendingRequests: () => {
      const session = sessionRef.current;
      return !!session?.active && session.scope === scope && (!!session.resume || !!session.control);
    } };
}
