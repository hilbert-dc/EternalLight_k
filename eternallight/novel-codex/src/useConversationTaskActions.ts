import type { Dispatch, SetStateAction } from "react";
import { useRef, useState, type RefObject } from "react";
import { type AgentRun } from "./agent-events";
import { conversationRetryCandidates, conversationRetryTask, isConversationRetryCandidate } from "./conversation-retry";
import type { ConversationFocus, NovelCodexApi, WorkTaskSummary } from "./types";
import { useSelectedConversation } from "./useSelectedConversation";
import { useWorkTaskControls } from "./useWorkTaskControls";
import type { useWorkTaskList } from "./useWorkTaskList";

import { type BoundExecutionRetry } from "./execution-retry";
import type { ThreadDetail, ThreadSummary } from "./types";

type Input = {
  taskList: ReturnType<typeof useWorkTaskList>;
  projectId: string;
  selectedConversation: ReturnType<typeof useSelectedConversation>;
  conversationProjectReady: boolean;
  api: NovelCodexApi | undefined;
  conversationChanging: boolean;
  taskControls: ReturnType<typeof useWorkTaskControls>;
  currentProjectRef: RefObject<string>;
  taskControlScopeRef: RefObject<{ api: NovelCodexApi | undefined; projectId: string; enabled: boolean; }>;
  setTaskControlMessage: Dispatch<SetStateAction<{ scope: object; error: { targetId: string; message: string; } | null; notice: { targetId: string; message: string; } | null; } | null>>;
  viewingExecution: boolean;
  agentRun: AgentRun;
  renderedRetry: BoundExecutionRetry | null;
  canDiscussSavedChapter: (focus: ConversationFocus | null | undefined) => boolean;
  executionBusy: boolean;
  creativeDirty: boolean;
  activeConversationDetail: { projectId: string; threadId: string; detail: ThreadDetail | null; loading: boolean; error: string | null; } | null;
  selectedThread: ThreadSummary | null | undefined;
  agentAvailable: boolean;
  creativeDirtyRef: RefObject<boolean>;
  promptSubmissionRef: RefObject<object | null>;
  agentRunRef: RefObject<AgentRun>;
  retryExecution: (expected: BoundExecutionRetry | null, history: AgentRun["history"]) => void;
};

export function useConversationTaskActions({ taskList, projectId, selectedConversation, conversationProjectReady, api, conversationChanging, taskControls, currentProjectRef, taskControlScopeRef, setTaskControlMessage, viewingExecution, agentRun, renderedRetry, canDiscussSavedChapter, executionBusy, creativeDirty, activeConversationDetail, selectedThread, agentAvailable, creativeDirtyRef, promptSubmissionRef, agentRunRef, retryExecution }: Input) {
  const retryContextRef = useRef<{ api: NovelCodexApi | undefined; projectId: string; scope: object;
    owner: { scope: string; id: string | null }; choices: WorkTaskSummary[]; blocked: boolean; agentAvailable: boolean } | null>(null);
  const retryRequestRef = useRef<{ scope: object; ownerScope: string; threadId: string | null } | null>(null);
  const [retryPending, setRetryPending] = useState<{ scope: object; ownerScope: string; threadId: string | null } | null>(null);
  const pauseContextRef = useRef<{ api: NovelCodexApi | undefined; projectId: string; scope: object;
    owner: { scope: string; id: string | null }; choices: WorkTaskSummary[]; blocked: boolean } | null>(null);
  const pauseRequestRef = useRef<{ scope: object; ownerScope: string; threadId: string | null } | null>(null);
  const [pausePending, setPausePending] = useState<{ scope: object; ownerScope: string; threadId: string | null } | null>(null);

  const retryChoices = conversationRetryCandidates(taskList.tasks, projectId, selectedConversation.id);
  const pauseChoices = taskList.tasks.filter(task => task.projectId === projectId && selectedConversation.id
    && task.threadId === selectedConversation.id && task.status === "running");
  const pauseContext = { api, projectId, scope: taskList.scope, owner: selectedConversation.current.current, choices: pauseChoices,
    blocked: !conversationProjectReady || !api || !taskList.loaded || !!taskList.error || conversationChanging };
  pauseContextRef.current = pauseContext;
  const pausePreparing = pausePending?.scope === taskList.scope && pausePending.ownerScope === pauseContext.owner.scope && pausePending.threadId === pauseContext.owner.id;
  const pauseTaskDisabled = pauseContext.blocked || pausePreparing || taskControls.controlling;
  const pauseSelectedTask = async (selected: WorkTaskSummary, command: "pause" | "pauseAfterStep" = "pause") => {
    const context = pauseContextRef.current;
    const pending = pauseRequestRef.current;
    if (!context?.api || context.blocked || taskControls.hasPendingControlRequests()
      || pending?.scope === context.scope && pending.ownerScope === context.owner.scope && pending.threadId === context.owner.id) return;
    const owned = context.choices.find(task => task.id === selected.id && task.projectId === selected.projectId && task.threadId === selected.threadId
      && task.controlRevision === selected.controlRevision && task.status === "running");
    if (!owned || currentProjectRef.current !== context.projectId || taskControlScopeRef.current !== context.scope
      || selectedConversation.current.current.scope !== context.owner.scope || selectedConversation.current.current.id !== context.owner.id) return;
    const ticket = { scope: context.scope, ownerScope: context.owner.scope, threadId: context.owner.id };
    pauseRequestRef.current = ticket; setPausePending(ticket);
    const current = () => {
      const latest = pauseContextRef.current;
      if (!latest) return false;
      return pauseRequestRef.current === ticket && latest.api === context.api && latest.scope === ticket.scope
        && latest.projectId === context.projectId && latest.owner.scope === ticket.ownerScope && latest.owner.id === ticket.threadId
        && taskControlScopeRef.current === ticket.scope && currentProjectRef.current === context.projectId
        && selectedConversation.current.current.scope === ticket.ownerScope && selectedConversation.current.current.id === ticket.threadId;
    };
    try {
      const fresh = await context.api.runtime.getWorkTask(context.projectId, owned.id);
      if (!current()) return;
      const latest = pauseContextRef.current;
      if (!latest || latest.blocked || taskControls.hasPendingControlRequests()) return;
      if (fresh.id !== owned.id || fresh.projectId !== context.projectId || fresh.threadId !== ticket.threadId) {
        throw new Error("任务来源已变化，尚未暂停。请重新读取原任务。");
      }
      if (fresh.controlRevision !== owned.controlRevision || fresh.status !== "running"
        || !latest.choices.some(task => task.id === owned.id && task.controlRevision === owned.controlRevision && task.status === "running")) {
        throw new Error("原任务状态已变化，请重新读取后选择；尚未暂停。");
      }
      await taskControls.control(context.api, owned.id, command, current, { threadId: fresh.threadId, controlRevision: fresh.controlRevision });
    } catch (reason) {
      if (!current()) return;
      setTaskControlMessage(previous => ({ scope: context.scope, error: { targetId: owned.id,
        message: reason instanceof Error ? reason.message : "原任务读取失败，尚未暂停。" }, notice: previous?.scope === context.scope ? previous.notice : null }));
      await taskList.refresh();
    } finally {
      if (pauseRequestRef.current === ticket) { pauseRequestRef.current = null; setPausePending(previous => previous === ticket ? null : previous); }
    }
  };
  const implicitRetry = conversationRetryTask(taskList.tasks, projectId, selectedConversation.id, viewingExecution ? agentRun : null);
  const retryTask = retryChoices.length === 1 && retryChoices[0].id === implicitRetry?.id ? implicitRetry : null;
  const retryDiscussion = viewingExecution && agentRun.state === "error" && renderedRetry?.execution.kind === "discussion";
  const retryReadOnlyDiscussion = retryDiscussion && retryChoices.length === 0
    && renderedRetry?.execution.kind === "discussion" && (renderedRetry.execution.conversationFocus?.readOnly || canDiscussSavedChapter(renderedRetry.execution.conversationFocus));
  const retryContext = { api, projectId, scope: taskList.scope, owner: selectedConversation.current.current, choices: retryChoices,
    blocked: executionBusy || creativeDirty && !retryReadOnlyDiscussion || !taskList.loaded || !!taskList.error || conversationChanging
      || !!activeConversationDetail?.loading || !!activeConversationDetail?.error || !!selectedThread?.isArchived,
    agentAvailable };
  retryContextRef.current = retryContext;
  const retryPreparing = retryPending?.scope === taskList.scope && retryPending.ownerScope === retryContext.owner.scope && retryPending.threadId === retryContext.owner.id;
  const retryTaskDisabled = retryContext.blocked || retryPreparing
    || !(retryChoices.some(task => task.resumeMode === "recoverSavedResult" || agentAvailable) || retryDiscussion && agentAvailable);
  const retrySelectedTask = async (selected: WorkTaskSummary) => {
    const context = retryContextRef.current;
    if (!context) return;
    const owned = context.choices.find(task => task.id === selected.id && task.version === selected.version && task.resumeMode === selected.resumeMode);
    const pending = retryRequestRef.current;
    if (!context.api || context.blocked || !owned || taskControls.hasPendingRequests() || creativeDirtyRef.current
      || promptSubmissionRef.current || ["running", "waiting"].includes(agentRunRef.current.state)
      || currentProjectRef.current !== context.projectId || taskControlScopeRef.current !== context.scope
      || selectedConversation.current.current.scope !== context.owner.scope || selectedConversation.current.current.id !== context.owner.id
      || owned.resumeMode !== "recoverSavedResult" && !context.agentAvailable
      || pending?.scope === context.scope && pending.ownerScope === context.owner.scope && pending.threadId === context.owner.id) return;
    const ticket = { scope: context.scope, ownerScope: context.owner.scope, threadId: context.owner.id };
    retryRequestRef.current = ticket; setRetryPending(ticket);
    const current = () => {
      const latest = retryContextRef.current;
      if (!latest) return false;
      return retryRequestRef.current === ticket && latest.api === context.api && latest.scope === ticket.scope
        && latest.projectId === context.projectId && latest.owner.scope === ticket.ownerScope && latest.owner.id === ticket.threadId
        && taskControlScopeRef.current === ticket.scope && currentProjectRef.current === context.projectId
        && selectedConversation.current.current.scope === ticket.ownerScope && selectedConversation.current.current.id === ticket.threadId;
    };
    try {
      const fresh = await context.api.runtime.getWorkTask(context.projectId, owned.id);
      if (!current()) return;
      const latest = retryContextRef.current;
      if (!latest) return;
      if (latest.blocked || taskControls.hasPendingRequests() || creativeDirtyRef.current || promptSubmissionRef.current
        || ["running", "waiting"].includes(agentRunRef.current.state) || owned.resumeMode !== "recoverSavedResult" && !latest.agentAvailable) return;
      if (fresh.id !== owned.id || fresh.projectId !== context.projectId || fresh.threadId !== (ticket.threadId ?? "")) {
        throw new Error("任务来源已变化，尚未继续执行。请重新读取原任务。");
      }
      if (fresh.version !== owned.version || fresh.resumeMode !== owned.resumeMode || !isConversationRetryCandidate(fresh)
        || !latest.choices.some(task => task.id === owned.id && task.version === owned.version && task.resumeMode === owned.resumeMode)) {
        throw new Error("原任务状态已变化，请重新读取后选择；尚未继续执行。");
      }
      await taskControls.control(context.api, owned.id, owned.resumeMode === "recoverSavedResult" ? "recoverSavedResult" : "resume");
    } catch (reason) {
      if (!current()) return;
      setTaskControlMessage(previous => ({ scope: context.scope, error: { targetId: owned.id,
        message: reason instanceof Error ? reason.message : "原任务读取失败，尚未继续执行。" }, notice: previous?.scope === context.scope ? previous.notice : null }));
      await taskList.refresh();
    } finally {
      if (retryRequestRef.current === ticket) { retryRequestRef.current = null; setRetryPending(previous => previous === ticket ? null : previous); }
    }
  };
  const retryConversation = () => {
    if (!api || retryTaskDisabled || selectedConversation.current.current.id !== selectedConversation.id) return;
    if (retryTask) void retrySelectedTask(retryTask);
    else if (retryDiscussion) retryExecution(renderedRetry, agentRun.history);
  };
  const composerRetry = retryChoices.length || retryDiscussion ? retryConversation : undefined;

  return { pauseChoices, pauseTaskDisabled, pauseSelectedTask, retryChoices, retryTask, retrySelectedTask, composerRetry, retryTaskDisabled };
}
