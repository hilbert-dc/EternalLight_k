import type { Dispatch, SetStateAction } from "react";
import { useRef, useState, type RefObject } from "react";
import { reduceAgentEvent, type AgentRun } from "./agent-events";
import type { UserInputRequest } from './app-workspace-types';
import { createStopTicket, ownsStop, stoppedRun, type StopTicket } from "./execution-stop";
import type { NovelCodexApi, WorkTaskSummary } from "./types";
import { useSelectedConversation } from "./useSelectedConversation";
import { useWorkTaskControls } from "./useWorkTaskControls";
import { useWorkTaskList } from "./useWorkTaskList";

type Input = {
  agentRunRef: RefObject<AgentRun>;
  api: NovelCodexApi | undefined;
  taskControls: ReturnType<typeof useWorkTaskControls>;
  selectedConversation: ReturnType<typeof useSelectedConversation>;
  taskList: ReturnType<typeof useWorkTaskList>;
  currentProjectRef: RefObject<string>;
  projectId: string;
  taskControlScopeRef: RefObject<{ api: NovelCodexApi | undefined; projectId: string; enabled: boolean; }>;
  setTaskControlMessage: Dispatch<SetStateAction<{ scope: object; error: { targetId: string; message: string; } | null; notice: { targetId: string; message: string; } | null; } | null>>;
  promptPipelineBusy: boolean;
  promptScopeRef: RefObject<{}>;
  stopTicketRef: RefObject<StopTicket | null>;
  executionRequestRef: RefObject<{ requestId: string; projectId: string; scope: object; } | null>;
  liveWorkListRef: RefObject<WorkTaskSummary[]>;
  setUserInputQueue: Dispatch<SetStateAction<UserInputRequest[]>>;
  setAgentRun: (update: AgentRun | ((run: AgentRun) => AgentRun)) => void;
  setActivity: Dispatch<SetStateAction<string[]>>;
  reportExecutionFailure: (message: string, scope: object, label: string) => Promise<boolean>;
  setThreadAction: (message: string | null) => void;
  refreshThreads: (archived?: boolean, clearAction?: boolean) => Promise<void>;
};

export function useExecutionRecovery({ agentRunRef, api, taskControls, selectedConversation, taskList, currentProjectRef, projectId, taskControlScopeRef, setTaskControlMessage, promptPipelineBusy, promptScopeRef, stopTicketRef, executionRequestRef, liveWorkListRef, setUserInputQueue, setAgentRun, setActivity, reportExecutionFailure, setThreadAction, refreshThreads }: Input) {
  const reconnectRef = useRef<object | null>(null);
  const [reconnectNotice, setReconnectNotice] = useState<{ scope: object; message: string } | null>(null);

  async function interruptAgent() {
    const run = agentRunRef.current;
    if (!api) return;
    if (taskControls.resumingId && !["running", "waiting"].includes(run.state)) {
      const targetId = taskControls.resumingId, selectedOwner = selectedConversation.current.current, scope = taskList.scope;
      const current = () => currentProjectRef.current === projectId && taskControlScopeRef.current === scope
        && selectedConversation.current.current.scope === selectedOwner.scope && selectedConversation.current.current.id === selectedOwner.id;
      try {
        const fresh = await api.runtime.getWorkTask(projectId, targetId);
        if (!current()) return;
        if (fresh.id !== targetId || fresh.projectId !== projectId || fresh.threadId !== (selectedOwner.id ?? "") || fresh.status !== "running") {
          throw new Error("原任务状态已变化，请重新读取任务；尚未停止。");
        }
        await taskControls.control(api, targetId, "pause", current, { threadId: fresh.threadId, controlRevision: fresh.controlRevision });
      } catch (reason) {
        if (current()) setTaskControlMessage(previous => ({ scope, error: { targetId,
          message: reason instanceof Error ? reason.message : "读取任务失败，尚未停止。" }, notice: previous?.scope === scope ? previous.notice : null }));
      }
      return;
    }
    if (!promptPipelineBusy && !["running", "waiting"].includes(run.state)) return;
    const stoppingContinuation = promptPipelineBusy && !["running", "waiting"].includes(run.state);
    // A pipeline stop addresses the host-owned Work, never an already completed advisory turn.
    const owner = { scope: promptScopeRef.current, projectId, threadId: stoppingContinuation ? null : run.threadId, turnId: stoppingContinuation ? null : run.turnId };
    if (ownsStop(stopTicketRef.current, owner)) return;
    const ticket = createStopTicket(owner);
    stopTicketRef.current = ticket;
    const current = () => ownsStop(ticket, { scope: promptScopeRef.current, projectId: currentProjectRef.current,
      threadId: agentRunRef.current.threadId, turnId: agentRunRef.current.turnId });
    try {
      const request = executionRequestRef.current;
      let result: { interrupted: boolean };
      if (request && request.scope === owner.scope && request.projectId === projectId) {
        result = await api.agent.interrupt({ requestId: request.requestId, projectId: request.projectId });
      } else {
        const owned = liveWorkListRef.current.find(work => work.projectId === projectId
          && work.steps.some(step => step.attempts.some(attempt => attempt.id === run.taskId)));
        if (!owned) throw new Error("原执行身份尚未就绪，请从创作任务中暂停原任务。");
        const fresh = await api.runtime.getWorkTask(projectId, owned.id);
        if (!current()) { ticket.settle(false); if (stopTicketRef.current === ticket) stopTicketRef.current = null; return; }
        if (fresh.id !== owned.id || fresh.projectId !== projectId || fresh.threadId !== owned.threadId) throw new Error("原任务归属已变化，尚未停止。");
        const paused = await api.agent.controlWorkTask(projectId, fresh.id, "pause", fresh.controlRevision, fresh.threadId);
        result = { interrupted: paused.status === "paused" };
      }
      ticket.settle(result.interrupted);
      if (!result.interrupted && stopTicketRef.current === ticket) stopTicketRef.current = null;
      if (result.interrupted && current()) {
        setUserInputQueue([]);
        if (!stoppingContinuation) setAgentRun(stoppedRun);
        setActivity(items => ["当前执行已停止，已保存结果仍保留。", ...items].slice(0, 4));
      }
    } catch (error) {
      ticket.settle(false);
      if (stopTicketRef.current === ticket) stopTicketRef.current = null;
      if (current()) await reportExecutionFailure(error instanceof Error ? error.message : "未知错误", owner.scope, "停止 Agent 失败");
    }
  }
  async function reconnectAgent() {
    const current = agentRunRef.current;
    if (!api || !current.threadId || reconnectRef.current) return;
    const owner = {}, scope = promptScopeRef.current, threadId = current.threadId, turnId = current.turnId;
    const isCurrent = () => scope === promptScopeRef.current && currentProjectRef.current === projectId
      && agentRunRef.current.threadId === threadId && agentRunRef.current.turnId === turnId;
    reconnectRef.current = owner;
    setReconnectNotice({ scope, message: "正在核对原任务连接，不会重新生成。" });
    setAgentRun((run) => reduceAgentEvent(run, { method: "agent/connection", params: { state: "reconnecting" } }));
    setActivity((items) => ["正在恢复创作引擎任务连接…", ...items].slice(0, 4));
    try {
      const result = await api.agent.reconnectThread(projectId, threadId, turnId);
      if (!isCurrent()) return;
      if (result.thread.id !== threadId || (result.recovery && result.recovery.turnId !== turnId)) throw new Error("恢复回执不属于原回合");
      setAgentRun(run => {
        const effectiveState = ["completed", "cancelled"].includes(run.state) ? run.state : result.recovery?.state ?? run.state;
        return { reconnectNotice, ...run, state: effectiveState, connectionState: "ready", error: result.recovery?.state === "error" ? run.error : null,
          completedAt: effectiveState === "running" || effectiveState === "waiting" ? null : run.completedAt ?? Date.now() };
      });
      setReconnectNotice(null);
      setThreadAction(null);
      setActivity((items) => [result.recovery?.state === "completed" ? "原任务已完成，请核对候选结果；后续修改从新任务开始。"
        : result.recovery ? "已核对原创作回合，请以原任务交付状态为准。" : "创作引擎任务连接已恢复。", ...items].slice(0, 4));
      void refreshThreads(false, false);
    } catch (error) {
      if (!isCurrent()) return;
      const message = error instanceof Error ? error.message : "未知错误";
      // A failed status read does not end the original generation promise.
      const stillActive = ["running", "waiting"].includes(agentRunRef.current.state);
      setAgentRun(run => ["running", "waiting"].includes(run.state) ? run : { ...run, connectionState: "error" });
      setReconnectNotice({ scope, message: `连接核对未完成：${message}。${stillActive
        ? "原任务仍保留，可等待或停止；不会重新生成。"
        : "请在创作任务中核对交付状态并继续；此次核对没有重新生成。"}` });
    } finally { if (reconnectRef.current === owner) reconnectRef.current = null; }
  }

  return { reconnectNotice, reconnectAgent, interruptAgent };
}
