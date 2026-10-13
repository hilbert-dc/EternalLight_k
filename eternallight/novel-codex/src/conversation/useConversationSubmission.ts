import type { RefObject, SetStateAction } from "react";
import { executionFailureMessage } from "../../shared/execution-failure-message.mjs";
import { authorIntentError } from "../../shared/author-intent-validation.mjs";
import { createAgentRun, reduceAgentEvent, summarizeAgentRun, type AgentRun } from "../agent-events";
import { isModelConnectionUnavailable } from "../agent-error-description";
import { chapterWritingMatches } from "../chapter-writing";
import { createConversationExecutionOwner } from "../conversation-execution-events";
import { failedRun, isConfirmedCancellation, stoppedRun } from "../execution-stop";
import { boundGenerationHandoff } from "../generation-handoff";
import type { BoundExecutionRetry, ExecutionRetry } from "../execution-retry";
import type { UserInputResponse } from "../AuthorQuestionCard";
import type { Chapter, ConversationFocus, DiscussionSource, NovelCodexApi, ProposedAction, RevisionPredecessor, ViewId, WorkTaskSummary } from "../types";
import type { useComposerDrafts } from "../useComposerDrafts";
import type { useConversationOutbox } from "../useConversationOutbox";
import type { useSelectedConversation } from "../useSelectedConversation";
import type { usePlanningSubmission } from "../usePlanningSubmission";
import type { useRevisionRepair } from "../useRevisionRepair";
import type { useWorkTaskControls } from "../useWorkTaskControls";
import type { useWorkTaskList } from "../useWorkTaskList";
import { attemptContainsRun } from "../work-conversation";
import { eventRecord } from "./protocol-records";
import type { ConversationExecution } from "./useConversationExecution";
import { executionSession } from "./execution-internals";

export type SubmissionPorts = {
 api?: NovelCodexApi;
 route: { projectId: string; activeProjectId: string; activeView: ViewId; accountOpen: boolean; selectedChapter?: Chapter; agentAvailable: boolean; currentProjectRef: RefObject<string> };
 conversation: { selectedConversation: ReturnType<typeof useSelectedConversation>; reading: boolean; archived: boolean; conversationChanging: boolean; viewingExecution: boolean; executionBusy: boolean; executionConversationId: string | null; refreshThreads: (archived?: boolean, clearError?: boolean) => Promise<void> };
 composer: { composerDrafts: ReturnType<typeof useComposerDrafts>; outbox: ReturnType<typeof useConversationOutbox>; setComposer: (value: SetStateAction<string>) => void; pendingDiscussionSource: DiscussionSource | null };
 preparation: { revisionRepair: ReturnType<typeof useRevisionRepair>; planningSubmission: ReturnType<typeof usePlanningSubmission>; planningSubmissionEligible: boolean; savePlanning: () => (() => Promise<boolean>) | null; writingRouteRef: RefObject<{ projectId: string; view: string; chapterId: string }> };
 workspace: { taskList: ReturnType<typeof useWorkTaskList>; taskControls: ReturnType<typeof useWorkTaskControls>; liveWorkListRef: RefObject<WorkTaskSummary[]>; creativeDirty: boolean; creativeDirtyRef: RefObject<boolean>; discussionSyncRef: RefObject<object | null>; canDiscussSavedChapter: (focus: ConversationFocus | null | undefined) => boolean };
 proposals: { prepare: (projectId: string) => void; receive: (projectId: string, actions: ProposedAction[]) => void };
 readback: (scope: object, current: () => boolean) => Promise<void>;
 onRefresh: () => void;
 onModelUnavailable: (message: string) => void;
};
export function useConversationSubmission(controller: ConversationExecution, input: SubmissionPorts) {
 const session = executionSession(controller);
 const { userInputQueue, setUserInputQueue, chapterWriting, setChapterWriting, chapterWritingRef, chapterWritingPendingRef, chapterWritingPending, setChapterWritingPending, selectChapterWriting, activity, setActivity, agentRun, commitAgentRun, executionConversationRef, conversationRuns, completedExecutionRequests, executionSnapshotBuffer, agentRunRef, promptScopeRef, stopTicketRef, executionRequestRef, executionRetryRef, conversationExecutionRef, setAgentRun, sentRevisionRepair, setSentRevisionRepair, sentRepairHistoryRef, promptSubmissionRef, submittedAuthorDraftRef, submittedRequestScopes, promptPipeline, setPromptPipeline, steerSubmissionRef, generationHandoff, setGenerationHandoff, generationHandoffRef, advisorySteeredRef, successfulSendRef } = session;
 const { api } = input;
 const { projectId, activeProjectId, activeView, accountOpen, selectedChapter, agentAvailable, currentProjectRef } = input.route;
 const { selectedConversation, conversationChanging, viewingExecution, executionBusy, executionConversationId, refreshThreads } = input.conversation;
 const { composerDrafts, outbox, setComposer, pendingDiscussionSource } = input.composer;
 const composer = composerDrafts.value;
 const { revisionRepair, planningSubmission, planningSubmissionEligible, writingRouteRef } = input.preparation;
 const { taskList, taskControls, liveWorkListRef, creativeDirty, creativeDirtyRef, discussionSyncRef, canDiscussSavedChapter } = input.workspace;
 const currentChapterWriting = chapterWritingMatches(chapterWriting, writingRouteRef.current) ? chapterWriting : null;
 const currentSentRepair = sentRepairHistoryRef.current === agentRun.history ? sentRevisionRepair : null;

  function beginExecutionScope(execution: ExecutionRetry = { kind: "taskRecovery", operation: "当前创作执行", view: "chapters" }) {
    const scope = {};
    promptScopeRef.current = scope;
    executionConversationRef.current = selectedConversation.id;
    stopTicketRef.current = null;
    executionRetryRef.current = { projectId, scope, history: null, execution };
    return scope;
  }

  async function reportExecutionFailure(message: string, scope: object, label: string) {
    const current = () => promptScopeRef.current === scope && currentProjectRef.current === projectId;
    if (!current()) return false;
    const run = agentRunRef.current;
    const cancelled = await isConfirmedCancellation(message, stopTicketRef.current,
      { scope, projectId, threadId: run.threadId, turnId: run.turnId }, run);
    if (!current()) return false;
    if ((run.threadId && run.threadId !== agentRunRef.current.threadId) || (run.turnId && run.turnId !== agentRunRef.current.turnId)) return false;
    if (cancelled) { setAgentRun(stoppedRun); return false; }
    const failed = failedRun(agentRunRef.current, message);
    if (failed === agentRunRef.current) return failed.state === "error";
    setAgentRun(failed);
    setActivity(items => [`${label}：${message}`, ...items].slice(0, 4));
    return true;
  }

  function retryExecution(expected: BoundExecutionRetry | null, history: AgentRun["history"]) {
    const run = agentRunRef.current;
    if (currentProjectRef.current !== projectId || projectId !== activeProjectId
      || run.history !== history || run.state !== "error" || promptSubmissionRef.current
      || steerSubmissionRef.current || discussionSyncRef.current
      || creativeDirtyRef.current && !(expected?.execution.kind === "discussion" && (expected.execution.conversationFocus?.readOnly || canDiscussSavedChapter(expected.execution.conversationFocus)))) return;
    if (expected && (executionRetryRef.current !== expected || expected.scope !== promptScopeRef.current
      || expected.projectId !== projectId || expected.history !== history)) return;
    if (expected?.execution.kind === "discussion") {
      void submitPrompt(expected.execution.prompt, expected.execution.conversationFocus ?? null, expected.execution.threadId ?? null);
      return;
    }

  }

  function currentRunCanSteer(run: AgentRun): boolean {
    if (run.state !== "running" || !run.threadId || !run.turnId || currentProjectRef.current !== projectId) return false;
    const step = taskList.tasks.filter(task => task.projectId === projectId)
      .flatMap(task => task.steps).find(step => step.attempts.some(attempt => attemptContainsRun(attempt, run)));
    if (step) return step.operation === "advisory";
    const retry = executionRetryRef.current;
    return Boolean(retry?.projectId === projectId && retry.scope === promptScopeRef.current
      && (retry.execution.kind === "discussion" || retry.execution.operation === "改稿讨论"));
  }

  async function queuePrompt(kind: "afterWork" | "afterTurn" = "afterWork") {
    if (!composerDrafts.excerptsReady) return;
    if (!selectedConversation.id || !viewingExecution || !executionBusy || creativeDirty || currentChapterWriting || revisionRepair.prepared || pendingDiscussionSource || !composer.trim()) return;
    const focus: ConversationFocus = { ...(composerDrafts.source?.focus ?? { view: activeView,
      ...(activeView === "chapters" && selectedChapter ? { chapterId: selectedChapter.id, chapterNumber: selectedChapter.number } : {}) }),
      ...(composerDrafts.excerpts.length ? { excerpts: structuredClone(composerDrafts.excerpts) } : {}),
      ...(composerDrafts.excerpts.some(item => item.state === "unsaved") ? { readOnly: true } : {}) };
    const work = liveWorkListRef.current.find(work => work.projectId === projectId && work.threadId === selectedConversation.id
      && work.steps.some(step => step.attempts.some(attempt => attempt.id === agentRun.taskId)));
    if (!work) { setActivity(items => ["当前任务尚未绑定，暂不能排队发送。", ...items].slice(0, 4)); return; }
    const acceptedDraft = composerDrafts.acceptance();
    if (kind === "afterTurn" && (!agentRun.taskId || !agentRun.turnId || agentRun.threadId !== selectedConversation.id)) return;
    const trigger: import("../types").MessageTrigger = kind === "afterTurn"
      ? {kind, taskId: agentRun.taskId!, turnId: agentRun.turnId!, controlRevision: work.controlRevision}
      : {kind, workTaskId: work.id, controlRevision: work.controlRevision};
    if (await outbox.add(composer, focus, trigger)) acceptedDraft();
  }

  async function submitPrompt(requestedPrompt?: string, requestedFocus?: ConversationFocus | null, requestedThreadId?: string | null) {
    if (!composerDrafts.excerptsReady || conversationChanging || input.conversation.reading || input.conversation.archived || executionBusy && !viewingExecution) return;
    const submittedThreadId = requestedThreadId !== undefined ? requestedThreadId : selectedConversation.id;
    if (taskControls.hasPendingRequests()) return;
    if (liveWorkListRef.current.some(work=>work.projectId===projectId && work.threadId===submittedThreadId && work.interlude)) {
      setActivity(items=>["原任务暂挂中：可再插入一个问题，或先返回原任务。草稿已保留。",...items].slice(0,4));return;
    }
    if (pendingDiscussionSource?.focus.reference.projectId === projectId) { setActivity(items => ["请先选择保留或替换讨论来源，再发送。", ...items].slice(0, 4)); return; }
    if (agentRunRef.current.state === "running" && !currentRunCanSteer(agentRunRef.current)) return;
    if (!projectId || projectId !== activeProjectId) {
      setActivity(items => ["请先创建或打开作品，再开始创作对话。", ...items].slice(0, 4));
      return;
    }
    // React state has not necessarily committed between two Enter events.
    // Keep a synchronous root guard without blocking steer on a running turn.
    if (planningSubmission.isPending() || discussionSyncRef.current || steerSubmissionRef.current || (promptSubmissionRef.current && agentRun.state !== "running")) return;
    if (currentSentRepair && !currentChapterWriting && (requestedPrompt !== undefined || agentRun.state === "error")) {
      setActivity(items => ["本轮改稿讨论不能作为普通聊天重试。请从任务恢复或重新选择修改结果；也可明确取消改稿来源，开始普通讨论。", ...items].slice(0, 4));
      return;
    }
    const prompt = (requestedPrompt ?? composer).trim();
    if (!prompt) return;
    const inputError = authorIntentError(prompt);
    if (inputError) { setActivity((items) => [inputError, ...items].slice(0, 4)); return; }
    if (requestedPrompt === undefined && chapterWritingRef.current !== currentChapterWriting) return;
    const writing = requestedPrompt === undefined && chapterWritingMatches(chapterWritingRef.current, writingRouteRef.current) ? chapterWritingRef.current : null;
    if (!agentAvailable) {
      setActivity((items) => ["当前没有可用模型，请先在右侧“设置”中完成配置。", ...items].slice(0, 4));
      return;
    }
    const selectedExcerpts = requestedPrompt === undefined ? composerDrafts.excerpts : requestedFocus?.excerpts ?? [];
    const excerptDiscussion = selectedExcerpts.length > 0;
    const readOnlyExcerpts = excerptDiscussion && (requestedFocus?.readOnly || creativeDirtyRef.current || selectedExcerpts.some(item => item.state === "unsaved"));
    // Captured local text can be discussed without saving. The frozen focus restricts candidate writes.
    if (excerptDiscussion && agentRun.state === "running") return;
    const requestedChapterFocus = requestedFocus ?? composerDrafts.source?.focus ?? { view: activeView, chapterId: selectedChapter?.id };
    const chapterDraftDiscussion = canDiscussSavedChapter(requestedChapterFocus)
      && !writing && !revisionRepair.prepared && !pendingDiscussionSource;
    if (creativeDirty && !chapterDraftDiscussion && !excerptDiscussion && (!writing || agentRun.state === "running")) {
      if (requestedPrompt !== undefined || requestedFocus !== undefined || !planningSubmissionEligible) {
        setActivity((items) => ["请先保存当前正文、事实表单或审阅决定，再启动或追加 Agent 任务。", ...items].slice(0, 4));
        return;
      }
      const save = input.preparation.savePlanning();
      if (!save) { setActivity(items => ["当前规划暂不能自动保存，请先处理页面中的检查或冲突；创作指令已保留。", ...items].slice(0, 4)); return; }
      setActivity(items => ["正在保存规划修改…", ...items].slice(0, 4));
      try {
        if (!await planningSubmission.prepare(save)) {
          if (currentProjectRef.current === projectId && writingRouteRef.current.view === activeView)
            setActivity(items => ["创作指令尚未发送。请检查保存结果或更改后的输入，再发送。", ...items].slice(0, 4));
          return;
        }
      } catch (error) {
        if (currentProjectRef.current === projectId && writingRouteRef.current.view === activeView)
          setActivity(items => [`规划尚未准备好：${error instanceof Error ? error.message : "请检查保存状态"}。创作指令已保留。`, ...items].slice(0, 4));
        return;
      }
    }
    // Prepared sources are for a new discussion and must never be consumed by steer or retry.
    if (revisionRepair.prepared && (requestedPrompt !== undefined || executionBusy)) return;
    let repairSource: RevisionPredecessor | null = null;
    if (revisionRepair.prepared) {
      repairSource = await revisionRepair.preflight();
      if (!repairSource) return;
      setSentRevisionRepair(revisionRepair.prepared);
      revisionRepair.cancel();
    }
    if (agentRun.state === "running") {
      if (!currentRunCanSteer(agentRunRef.current)) return;
      if (!api || agentRun.mode !== "novel-agent-engine") return;
      const operation = {};
      const scope = promptScopeRef.current;
      const threadId = agentRun.threadId;
      const turnId = agentRun.turnId;
      const isCurrent = () => promptScopeRef.current === scope && currentProjectRef.current === projectId;
      steerSubmissionRef.current = operation;
      advisorySteeredRef.current = true;
      try {
        const result = await api.agent.steer(projectId, prompt, executionConversationId);
        if (!isCurrent()) return;
        if (result.retained && !result.steered) {
          setComposer(value => value === composer ? "" : value);
          setActivity(items => ["追加要求尚未发送，已保留；继续原任务时会一并处理。", ...items].slice(0, 4));
          return;
        }
        if (!result.steered) throw new Error("当前创作引擎轮次尚未准备好接收追加指令");
        setComposer(value => value === composer ? "" : value);
        setAgentRun(run => run.state === "running" && run.threadId === threadId && run.turnId === turnId
          ? reduceAgentEvent(run, { method: "agent/steer", params: { prompt } }) : run);
        setActivity((items) => [`已追加指令：${prompt}`, ...items].slice(0, 4));
      } catch (error) {
        if (!isCurrent()) return;
        setActivity((items) => [`追加指令失败：${error instanceof Error ? error.message : "未知错误"}`, ...items].slice(0, 4));
      } finally {
        if (steerSubmissionRef.current === operation) steerSubmissionRef.current = null;
      }
      return;
    }
    if (agentRun.state === "waiting") return;
    if (requestedPrompt !== undefined) selectChapterWriting(null);
    if (writing) {
      if (chapterWritingPendingRef.current || chapterWritingPending) return;
      const preparation = {};
      const preparedInputVersion = composerDrafts.version();
      promptSubmissionRef.current = preparation;
      chapterWritingPendingRef.current = preparation;
      setChapterWritingPending(true);
      try {
        if (!await writing.prepareDiscussion() || chapterWritingRef.current !== writing
          || !chapterWritingMatches(writing, writingRouteRef.current) || currentProjectRef.current !== writing.projectId
          || composerDrafts.version() !== preparedInputVersion) return;
      } catch (error) {
        if (currentProjectRef.current === writing.projectId) setActivity(items => [
          `本章暂未准备好：${error instanceof Error ? error.message : "请重新读取后再试"}`, ...items,
        ].slice(0, 4));
        return;
      } finally {
        if (promptSubmissionRef.current === preparation) promptSubmissionRef.current = null;
        if (chapterWritingPendingRef.current === preparation) chapterWritingPendingRef.current = null;
        setChapterWritingPending(false);
      }
      selectChapterWriting(null);
    }
    if (!repairSource) setSentRevisionRepair(null);
    let conversationFocus = repairSource ? null : requestedFocus !== undefined ? requestedFocus
      : composerDrafts.source?.focus ?? (accountOpen ? null : { view: activeView,
        ...(activeView === "chapters" && selectedChapter
          ? { chapterId: selectedChapter.id, chapterNumber: selectedChapter.number } : {}) });
    if (excerptDiscussion) conversationFocus = { ...(conversationFocus ?? { view: activeView }),
      excerpts: selectedExcerpts, ...(readOnlyExcerpts ? { readOnly: true } : {}) };
    if (!repairSource && requestedPrompt === undefined && requestedFocus === undefined && !composerDrafts.source && !writing
      && agentRun.origin === "chapterStructure" && agentRun.state === "completed" && agentRun.taskId && agentRun.answer.trim()) {
      conversationFocus = { ...(conversationFocus ?? { view: activeView }), chapterStructureTaskId: agentRun.taskId };
    }
    const promptScope = {};
    successfulSendRef.current = null;
    promptSubmissionRef.current = promptScope;
    planningSubmission.finish();
    promptScopeRef.current = promptScope;
    conversationExecutionRef.current = repairSource ? null : { api, scope: promptScope,
      owner: createConversationExecutionOwner(projectId) };
    setPromptPipeline({ projectId, scope: promptScope });
    executionRetryRef.current = { projectId, scope: promptScope, history: null,
      execution: repairSource ? { kind: "taskRecovery", operation: "改稿讨论", view: "chapters", intent: prompt }
        : { kind: "discussion", prompt, conversationFocus, threadId: submittedThreadId } };
    const promptIsCurrent = () => promptScopeRef.current === promptScope && currentProjectRef.current === projectId;
    setGenerationHandoff(null);
    const previous = summarizeAgentRun(agentRun);
    const history = previous ? [previous, ...agentRun.history] : agentRun.history;
    executionConversationRef.current = submittedThreadId;
    const nextRun = { ...createAgentRun(prompt, history), threadId: submittedThreadId, ...(selectedExcerpts.length ? { excerpts: selectedExcerpts } : {}) };
    sentRepairHistoryRef.current = repairSource ? nextRun.history : null;
    advisorySteeredRef.current = false;
    if (requestedPrompt === undefined) submittedAuthorDraftRef.current.set(promptScope, composerDrafts.acceptance());
    const submittedDraftVersion = composerDrafts.version();
    setUserInputQueue([]);
    input.proposals.prepare(projectId);
    setAgentRun(nextRun);
    setActivity((items) => [`你：${prompt}`, ...items].slice(0, 4));
    let replyReceived = false;
    try {
      if (!api) throw new Error("桌面 Agent 未连接，当前只能手动编辑");
      const result = await api.agent.sendPrompt(projectId, prompt, repairSource, conversationFocus, submittedThreadId);
      replyReceived = true;
      if (!promptIsCurrent()) {
        const saved = submittedThreadId ? conversationRuns.current.get(submittedThreadId) : null;
        if (saved?.scope === promptScope && result.executionFailure) saved.run = failedRun(saved.run, result.executionFailure.message);
        return;
      }
      const latest = await api.agent.listProposedActions(projectId).catch(() => ({ actions: result.proposedActions }));
      if (!promptIsCurrent()) return;
      input.proposals.receive(projectId, latest.actions);
      const sourceTask = eventRecord(result.workflow?.task);
      const proposal = !result.executionFailure && !repairSource && !advisorySteeredRef.current
        ? boundGenerationHandoff(result.generationHandoff, { projectId, authorIntent: prompt,
          taskId: sourceTask?.id, threadId: sourceTask?.threadId }) : null;
      setGenerationHandoff(proposal ? { proposal, scope: promptScope } : null);
      setActivity((items) => [`${result.executionFailure ? "执行状态" : "创作引擎"}：${result.message}`, ...items].slice(0, 4));
      await input.readback(promptScope, promptIsCurrent);
      if (!promptIsCurrent()) return;
      if (result.executionFailure) {
        const recoveryExecution = executionRetryRef.current?.execution;
        executionRetryRef.current = { projectId, scope: promptScope, history: null,
          execution: recoveryExecution?.kind === "taskRecovery" ? recoveryExecution
            : { kind: "taskRecovery", operation: "创作协调", view: "chapters", intent: prompt } };
        await reportExecutionFailure(result.executionFailure.message, promptScope, result.candidateResults?.length
          ? "已有结果已保存，后续处理未完成" : "创作任务未完成");
      }
      if (!result.executionFailure && agentRunRef.current.state === "completed") successfulSendRef.current = promptScope;
      void refreshThreads(false, false);
    } catch (error) {
      if (!promptIsCurrent()) {
        const saved = submittedThreadId ? conversationRuns.current.get(submittedThreadId) : null;
        if (saved?.scope === promptScope) saved.run = failedRun(saved.run, executionFailureMessage(error));
        return;
      }
      const message = executionFailureMessage(error, "未知错误");
      if (isModelConnectionUnavailable(message)) {
        input.onModelUnavailable(message);
        if (requestedPrompt === undefined && !agentRunRef.current.taskId && !agentRunRef.current.turnId
          && composerDrafts.version() === submittedDraftVersion) {
          composerDrafts.setValue(composer);
          composerDrafts.setSource(composerDrafts.source);
          for (const item of selectedExcerpts) await composerDrafts.addExcerpt(item);
        }
        setAgentRun(run => ({ ...run, connectionState: "offline" }));
      }
      await reportExecutionFailure(message, promptScope, "Agent 请求失败");
    } finally {
      const saved = submittedThreadId ? conversationRuns.current.get(submittedThreadId) : null;
      if (saved?.scope === promptScope) { saved.submitting = false; saved.pipeline = null; }
      submittedAuthorDraftRef.current.delete(promptScope);
      for (const [id, request] of submittedRequestScopes.current) if (request.scope === promptScope) submittedRequestScopes.current.delete(id);
      if (conversationExecutionRef.current?.scope === promptScope) conversationExecutionRef.current = null;
      if (promptSubmissionRef.current === promptScope) promptSubmissionRef.current = null;
      setPromptPipeline(current => current?.scope === promptScope ? null : current);
      if (!replyReceived && promptIsCurrent() && !creativeDirtyRef.current) input.onRefresh();
    }
  }

  async function resolveUserInput(response: UserInputResponse): Promise<boolean> { const current = userInputQueue[0]; if (!current || !api) return false; try { await api.agent.resolveUserInput({ projectId: current.projectId, executionId: current.executionId, requestId: current.requestId }, response); setUserInputQueue((queue) => queue.filter((request) => request.executionId !== current.executionId || String(request.requestId) !== String(current.requestId))); setActivity((items) => ["已提交作者回答。", ...items].slice(0, 4)); return true; } catch (error) { setActivity((items) => [`作者回答提交失败：${error instanceof Error ? error.message : "未知错误"}`, ...items].slice(0, 4)); throw error; } }
return { beginExecutionScope, reportExecutionFailure, retryExecution, currentRunCanSteer, queuePrompt, submitPrompt, resolveUserInput };
}
