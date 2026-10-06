import { useLayoutEffect, useRef, useState } from "react";
import type { NovelCodexApi } from "./types";

export type WorkTaskCommand = "pause" | "cancel" | "resume" | "recoverSavedResult";
export type WorkTaskControls = ReturnType<typeof useWorkTaskControls>;

type Scope = { projectId: string };
type Ticket = { targetId: string; superseded: boolean; cancelled: boolean };
type Session = { scope: Scope; active: boolean; resume: Ticket | null; control: Ticket | null; errorTicket: Ticket | null };
type Options = {
  scope: Scope;
  isBlocked: () => boolean;
  refresh: () => Promise<void>;
  onBeforeResume?: (workTaskId: string) => Promise<(result: unknown, error: unknown) => void>;
  onFormalChange: () => Promise<void>;
  onControlApplied?: (workTaskId: string) => void;
  onError: (message: string | null, targetId: string) => void;
  onNotice: (message: string | null, targetId: string) => void;
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
      options.onError(null, targetId);
    }
    options.onNotice(null, targetId);
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
              options.onError(null, targetId);
            }
          }
        }
        if (!ticket.superseded) options.onNotice(notice, targetId);
      } catch (reason) {
        if (!session.active) return;
        if (!ticket.cancelled) {
          session.errorTicket = ticket;
          options.onError(reason instanceof Error ? reason.message : failureMessage, targetId);
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
          options.onError(`操作已返回成功，但作品界面刷新失败：${reason instanceof Error ? reason.message : "请重新读取作品状态"}`, targetId);
        }
      }
    } finally {
      if (session[slot] === ticket) session[slot] = null;
      if (session.active) setState({ session, resuming: !!session.resume, controlling: !!session.control });
    }
  }

  async function control(api: NovelCodexApi, workTaskId: string, command: WorkTaskCommand) {
    await run(command === "recoverSavedResult" ? "resume" : command, workTaskId, async () => {
      const finish = command === "resume" ? await options.onBeforeResume?.(workTaskId) : null;
      let result: unknown, failure: unknown = null;
      try {
        const session = sessionRef.current;
        if (command === "resume" && (!session?.active || session.scope !== scope || session.resume?.cancelled)) {
          result = { status: "cancelled" }; return result;
        }
        result = await (command === "recoverSavedResult" ? api.agent.recoverWorkTask(scope.projectId, workTaskId)
          : command === "resume" ? api.agent.resumeWorkTask(scope.projectId, workTaskId)
          : api.agent.controlWorkTask(scope.projectId, workTaskId, command));
        if ((command === "pause" || command === "cancel") && sessionRef.current?.active && sessionRef.current.scope === scope) options.onControlApplied?.(workTaskId);
        if (command === "resume") {
          const resumed = result as Awaited<ReturnType<NovelCodexApi["agent"]["resumeWorkTask"]>>;
          if (resumed.resumeFailure) {
            if (resumed.id !== workTaskId || resumed.projectId !== scope.projectId || resumed.status !== "paused"
              || resumed.resumeFailure.code !== "WORK_RESUME_NOT_STARTED" || resumed.resumeFailure.workTaskId !== workTaskId
              || typeof resumed.resumeFailure.message !== "string" || !resumed.resumeFailure.message.trim()) {
              throw new Error("原任务恢复结果的来源无法核对，请重新读取任务。");
            }
            throw new Error(`原任务尚未启动：${resumed.resumeFailure.message}`);
          }
        }
        return result;
      } catch (reason) { failure = reason; throw reason; }
      finally {
        const resume = sessionRef.current?.resume;
        if (resume?.targetId === workTaskId && resume.cancelled) { result = { status: "cancelled" }; failure = null; }
        finish?.(result, failure);
      }
    }, "任务状态更新失败",
      command === "cancel" ? "已确认取消任务。" : command === "pause" ? "已确认暂停后续步骤，当前尝试可在安全点结束。"
        : command === "recoverSavedResult" ? "已核验恢复保存的产物，请核对任务状态；后续步骤需另行继续。" : "继续执行已结束，请以任务持久状态和产物为准。");
  }
  const visible = state?.session.active && state.session.scope === scope ? state : null;
  return { run, control, pending: !!visible && (visible.resuming || visible.controlling), resuming: !!visible?.resuming, controlling: !!visible?.controlling,
    hasPendingRequests: () => {
      const session = sessionRef.current;
      return !!session?.active && session.scope === scope && (!!session.resume || !!session.control);
    } };
}
