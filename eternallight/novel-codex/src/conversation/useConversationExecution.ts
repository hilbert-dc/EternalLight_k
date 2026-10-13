import { useRef } from 'react';
import { createAgentRun, summarizeAgentRun } from '../agent-events';
import { chapterWritingMatches, type ChapterWritingRequest } from '../chapter-writing';
import { createRecoveredExecutionOwner } from '../conversation-execution-events';
import { failedRun, stoppedRun } from '../execution-stop';
import type { NovelCodexApi, WorkTaskSummary } from '../types';
import type { AgentStatus } from '../app-workspace-types';
import { eventRecord } from './protocol-records';
import { bindExecution, executionSession } from './execution-internals';
import { useExecutionSession } from './useExecutionSession';

/** Public execution boundary: read projections and issue lifecycle commands. */
export function useConversationExecution(api: NovelCodexApi | undefined) {
  const session = useExecutionSession(api);
  const controller = useRef<ReturnType<typeof createController> | null>(null);
  if (!controller.current) controller.current = createController();
  bindExecution(controller.current, session);
  return controller.current;
}
function createController() {
 const controller = {
  get run() { return read().agentRun; },
  get submittingConversationIds() { return [...read().conversationRuns.current].filter(([, entry]) => entry.submitting).map(([id]) => id); },
  get activity() { return read().activity; },
  get questions() { return read().userInputQueue; },
  get writing() { return read().chapterWriting; },
  get writingPending() { return read().chapterWritingPending; },
  get sentRepair() { const s = read(); return s.sentRepairHistoryRef.current === s.agentRun.history ? s.sentRevisionRepair : null; },
  get handoff() { return read().generationHandoff; },
  get retry() { return read().executionRetryRef.current; },
  get scope() { return read().promptScopeRef.current; },
  get owner() { return read().conversationExecutionRef.current; },
  get conversationId() { const s = read(); return s.executionConversationRef.current ?? s.agentRun.threadId; },
  isSubmitting() { return !!read().promptSubmissionRef.current; },
  isRunning() { return ['running', 'waiting'].includes(read().agentRunRef.current.state); },
  isConversationRunning(id: string) { const saved = read().conversationRuns.current.get(id); return Boolean(saved?.submitting || ['running', 'waiting'].includes(saved?.run.state ?? '')); },
  isPipelineBusy(projectId: string) { const s = read(); return s.promptPipeline?.projectId === projectId && s.promptPipeline.scope === s.promptScopeRef.current && s.promptSubmissionRef.current === s.promptPipeline.scope; },
  isViewing(projectId: string, selectedId: string | null) {
    const s = read(), id = controller.conversationId;
    return selectedId === id || !selectedId && (!id || Boolean(s.promptSubmissionRef.current && s.conversationExecutionRef.current?.scope === s.promptScopeRef.current)
      || Boolean(!s.executionConversationRef.current && s.agentRun.prompt && s.executionRetryRef.current?.projectId === projectId && s.executionRetryRef.current.scope === s.promptScopeRef.current));
  },
  notice(message: string) { read().setActivity(items => [message, ...items].slice(0, 4)); },
  dismissRepair() { read().setSentRevisionRepair(null); },
  dismissHandoff() { read().setGenerationHandoff(null); },
  selectWriting(request: ChapterWritingRequest | null) { read().selectChapterWriting(request); },
  validateWritingRoute(route: Parameters<typeof chapterWritingMatches>[1]) { const s = read(); if (!chapterWritingMatches(s.chapterWritingRef.current, route)) s.chapterWritingRef.current = null; },
  canPrepareWriting() { const s = read(); return !controller.isRunning() && !s.chapterWritingPendingRef.current && !s.promptSubmissionRef.current; },
  activateConversation(projectId: string, threadId: string | null, fresh = false) { read().activateConversation(projectId, threadId, fresh); },
  leaveProject() { read().leaveProject(); },
  applyConnection(status: Pick<AgentStatus, 'provider' | 'model' | 'configured'>, initial = false) {
    read().setAgentRun(run => initial ? { ...run, model: status.model ?? run.model, provider: status.provider ?? run.provider, connectionState: status.configured ? run.connectionState === 'error' ? 'error' : 'ready' : 'offline' }
      : ['running', 'waiting'].includes(run.state) ? run : { ...run, provider: status.provider ?? null, model: status.model ?? null, connectionState: status.configured ? 'ready' : 'offline', error: null });
  },
  startCandidate(prompt: string) { const s = read(), previous = summarizeAgentRun(s.agentRun); s.setAgentRun(createAgentRun(prompt, previous ? [previous, ...s.agentRun.history] : s.agentRun.history)); },
  completeCandidate(outcome: string) { read().setAgentRun(run => ['running', 'waiting'].includes(run.state) ? { ...run, state: 'completed', completedAt: Date.now(), summary: outcome, error: null } : run); },
  finishScope(status: string | undefined, outcome: string) { read().setAgentRun(run => !['running', 'waiting'].includes(run.state) ? run : { ...run, state: status === 'completed' ? 'completed' : ['paused', 'cancelled'].includes(status ?? '') ? 'cancelled' : 'error', summary: outcome, completedAt: Date.now(), error: ['completed', 'paused', 'cancelled'].includes(status ?? '') ? null : outcome }); },
  controlApplied(api: NovelCodexApi | undefined, workTaskId: string, projectId: string) {
    const s = read(), pipeline = s.conversationExecutionRef.current;
    if (pipeline?.pendingStart && pipeline.api === api && pipeline.scope === s.promptScopeRef.current && pipeline.owner.workTaskId === workTaskId && pipeline.owner.projectId === projectId) {
      if (s.promptSubmissionRef.current === pipeline.scope) s.promptSubmissionRef.current = null;
      s.setPromptPipeline(current => current?.scope === pipeline.scope ? null : current);
      s.conversationExecutionRef.current = null;
    }
  },
  resumeWork(api: NovelCodexApi | undefined, work: WorkTaskSummary, projectId: string, isCurrent: () => boolean, beginScope: () => object) {
    const { promptSubmissionRef, setPromptPipeline, agentRunRef, conversationExecutionRef, promptScopeRef, setAgentRun } = read();
      const owner = createRecoveredExecutionOwner(work);
      if (!owner) throw new Error("原任务的对话来源未能核对，尚未继续执行。");

      const scope = beginScope();
      promptSubmissionRef.current = scope;
      setPromptPipeline({ projectId, scope });
      const originalRun = agentRunRef.current;
      conversationExecutionRef.current = { api, scope, owner, pendingStart: { goal: work.goal, originalRun } };
      return (result: unknown, error?: unknown) => {
        const returned = eventRecord(result), notStarted = eventRecord(returned?.resumeFailure);
        const didNotStart = promptScopeRef.current === scope && isCurrent()
          && returned?.id === work.id && returned.projectId === projectId && returned.status === "paused"
          && notStarted?.code === "WORK_RESUME_NOT_STARTED" && notStarted.workTaskId === work.id
          && typeof notStarted.message === "string" && Boolean(notStarted.message.trim())
          && conversationExecutionRef.current?.scope === scope && Boolean(conversationExecutionRef.current.pendingStart);
        if (didNotStart) {
          // A Host-proven pre-task failure is an operation error on the original
          // Work, not another author turn or a fabricated failed model attempt.
          setAgentRun(originalRun);
        }
        if (promptSubmissionRef.current === scope) promptSubmissionRef.current = null;
        setPromptPipeline(current => current?.scope === scope ? null : current);
        if (conversationExecutionRef.current?.scope === scope) conversationExecutionRef.current = null;
        if (didNotStart) return;
        if (promptScopeRef.current === scope && isCurrent()
          && ["running", "waiting"].includes(agentRunRef.current.state)) {
          setAgentRun(run => error ? failedRun(run, error instanceof Error ? error.message : "原任务恢复未完成")
            : returned?.interlude || returned?.status === "cancelled" || returned?.status === "paused" && eventRecord(returned.pauseAfterStep)?.remainingAttempts === 0 ? stoppedRun(run)
              : returned?.status === "completed" ? { ...run, state: "completed", completedAt: Date.now() }
                : { ...run, state: "error", error: "原任务尚未完成，请查看任务状态继续处理。", completedAt: Date.now() });
        }
      };
  },
 };
 function read() { return executionSession(controller); }
 return controller;
}
export type ConversationExecution = ReturnType<typeof createController>;
