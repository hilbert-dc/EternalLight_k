import { useEffect, useRef, type RefObject } from "react";
import { createAgentRun, reduceAgentEvent, summarizeAgentRun, type AgentRun } from "../agent-events";
import { applyChapterStructureEvent } from "../chapter-structure-events";
import { applyBackgroundExecutionEvent, applyConversationExecutionEvent, createConversationExecutionOwner } from "../conversation-execution-events";
import { boundGenerationHandoff } from "../generation-handoff";
import { confirmsNativeStop, failedRun, ownsStop, stoppedRun } from "../execution-stop";
import { truncatedExecutionNotice } from "../execution-snapshot";
import type { NovelCodexApi, WorkTaskSummary } from "../types";
import type { ExecutionHealth } from "../AgentStatus";
import type { useSelectedConversation } from "../useSelectedConversation";
import type { useComposerDrafts } from "../useComposerDrafts";
import type { useConversationDirectory } from "../useConversationDirectory";
import type { ConversationExecution } from "./useConversationExecution";
import { executionSession } from "./execution-internals";
import { eventRecord, eventThreadId, isResolvedUserInputEvent } from "./protocol-records";
const MAX_PENDING_INTERACTIONS = 4;
export type ProtocolPorts = {
 api?: NovelCodexApi; projectId: string; currentProjectRef: RefObject<string>;
 conversationProjectReady: boolean; selectedConversation: ReturnType<typeof useSelectedConversation>;
 conversationSelectionRef: RefObject<ReturnType<typeof useSelectedConversation>>;
 mediaContextRef: RefObject<unknown>; liveWorkListRef: RefObject<WorkTaskSummary[]>;
 composerDraftsRef: RefObject<ReturnType<typeof useComposerDrafts>>;
 directory: Pick<ReturnType<typeof useConversationDirectory>, "applyEvent" | "report">;
 onRefresh: () => void; onHealth: (health: ExecutionHealth[]) => void;
 onStructureEvent: (method: unknown, params: Record<string, unknown>) => void;
};
export function useConversationProtocol(controller: ConversationExecution, input: ProtocolPorts) {
 const session = executionSession(controller);
 const ports = useRef(input); ports.current = input;
 const { api, projectId, currentProjectRef, conversationProjectReady, selectedConversation,
 conversationSelectionRef, mediaContextRef, liveWorkListRef, composerDraftsRef, directory } = input;
 const setThreadAction = directory.report;
 const { userInputQueue, setUserInputQueue, chapterWriting, setChapterWriting, chapterWritingRef, chapterWritingPendingRef, chapterWritingPending, setChapterWritingPending, selectChapterWriting, activity, setActivity, agentRun, commitAgentRun, executionConversationRef, conversationRuns, completedExecutionRequests, executionSnapshotBuffer, agentRunRef, promptScopeRef, stopTicketRef, executionRequestRef, executionRetryRef, conversationExecutionRef, setAgentRun, sentRevisionRepair, setSentRevisionRepair, sentRepairHistoryRef, promptSubmissionRef, submittedAuthorDraftRef, submittedRequestScopes, promptPipeline, setPromptPipeline, steerSubmissionRef, generationHandoff, setGenerationHandoff, generationHandoffRef, advisorySteeredRef, successfulSendRef } = session;

  useEffect(() => {
    if (!api || !conversationProjectReady || !selectedConversation.id) return;
    let cancelled = false;
    const threadId = selectedConversation.id;
    void api.agent.listExecutions(projectId).then(snapshots => {
      if (cancelled || currentProjectRef.current !== projectId || promptSubmissionRef.current) return;
      const snapshot = snapshots.find(item => item.projectId === projectId && item.threadId === threadId);
      if (!snapshot || completedExecutionRequests.current.has(snapshot.requestId) || conversationRuns.current.get(threadId)?.submitting) return;
      const scope = {}, owner = createConversationExecutionOwner(projectId);
      let run: AgentRun = {...createAgentRun(snapshot.prompt), threadId: snapshot.executionThreadId ?? threadId};
      if (snapshot.workflow) run = applyConversationExecutionEvent(run, {method: "agent/workflow", params: {workflow: snapshot.workflow}}, owner);
      if (snapshot.turnId) {
        run = reduceAgentEvent(run, {method: "turn/started", params: {threadId: run.threadId, turn: {id: snapshot.turnId, status: "inProgress"}}});
      }
      const restored=executionSnapshotBuffer.current.merge(snapshot);
      for(const event of restored.events) if(event.method!=="agent/workflow") run=reduceAgentEvent(run,event);
      if(snapshot.truncated) setThreadAction(truncatedExecutionNotice);
      if (restored.questions.length) run = {...run, state: "waiting"};
      promptScopeRef.current = scope; promptSubmissionRef.current = scope;
      executionConversationRef.current = threadId;
      executionRequestRef.current = {requestId: snapshot.requestId, projectId, scope};
      conversationExecutionRef.current = {api, scope, owner};
      executionRetryRef.current = {projectId, scope, history: run.history, execution: {kind: "taskRecovery", operation: "原任务", view: "chapters"}};
      setUserInputQueue(restored.questions);
      setAgentRun(run); setPromptPipeline({projectId, scope});
    }).catch(error => { if (!cancelled) setThreadAction(`运行状态读取失败：${error instanceof Error ? error.message : "请重试"}`); });
    return () => { cancelled = true; };
  }, [api, projectId, conversationProjectReady, selectedConversation.id]);

  useEffect(() => {
    if (!api) return;
    return api.agent.onEvent(event => {
      if(!executionSnapshotBuffer.current.capture(event,currentProjectRef.current))return;
      const healthEvent = eventRecord(event);
      if (healthEvent?.method === "agent/message-updated") {
        const params = eventRecord(healthEvent.params);
        const request = typeof params?.messageId === "string" ? submittedRequestScopes.current.get(params.messageId) : null;
        if (request && params?.projectId === request.projectId && params.status === "queued") {
          submittedAuthorDraftRef.current.get(request.scope)?.(); submittedAuthorDraftRef.current.delete(request.scope);
        }
        return;
      }
      const domain = eventRecord(healthEvent?.hostExecution);
      if (healthEvent?.method === "agent/execution-ended") {
        const params = eventRecord(healthEvent.params), failure = eventRecord(params?.failure);
        const requestId = params?.requestId;
        if (typeof requestId !== "string") return;
        completedExecutionRequests.current.add(requestId);
        const saved = [...conversationRuns.current.values()].find(item => item.request?.requestId === requestId && item.request?.projectId === params?.projectId);
        if (saved) {
          saved.submitting = false; saved.pipeline = null;
          if (failure) saved.run = failure.code === "TASK_CANCELLED" ? stoppedRun(saved.run) : failedRun(saved.run, String(failure.message));
        }
        if (executionRequestRef.current?.requestId === requestId && executionRequestRef.current?.projectId === params?.projectId) {
          promptSubmissionRef.current = null; conversationExecutionRef.current = null;
          setPromptPipeline(null);
          if (saved) { setAgentRun(saved.run); setUserInputQueue([]); }
        }
        return;
      }
      if (domain && typeof domain.threadId === "string"
        && (domain.projectId !== currentProjectRef.current || domain.threadId !== executionConversationRef.current)) {
        const saved = conversationRuns.current.get(domain.threadId);
        if (saved && saved.request?.projectId === domain.projectId && saved.request?.requestId === domain.requestId) {
          const bound = saved.pipeline ? applyConversationExecutionEvent(saved.run, event, saved.pipeline.owner) : saved.run;
          saved.run = bound !== saved.run ? bound : reduceAgentEvent(saved.run, event);
          if (isResolvedUserInputEvent(event)) saved.questions = saved.questions?.filter(request =>
            request.executionId !== domain.id || String(request.requestId) !== String(event.params.requestId));
          if (healthEvent?.method === "agent/error") saved.questions = [];
        }
        if (domain.projectId === currentProjectRef.current) directory.applyEvent(event);
        return;
      }
      if (domain && domain.projectId !== currentProjectRef.current) return;
      if (healthEvent?.method === "agent/execution-request") {
        const params = eventRecord(healthEvent.params);
        if (params?.projectId === currentProjectRef.current && typeof params.requestId === "string") {
          executionRequestRef.current = { requestId: params.requestId, projectId: params.projectId as string, scope: promptScopeRef.current };
          submittedRequestScopes.current.set(params.requestId, {projectId: params.projectId as string, scope: promptScopeRef.current});
          const saved = executionConversationRef.current ? conversationRuns.current.get(executionConversationRef.current) : null;
          if (saved?.scope === promptScopeRef.current) saved.request = executionRequestRef.current;
        }
        return;
      }
      if (healthEvent?.method === "agent/message-started") {
        const message = eventRecord(eventRecord(healthEvent.params)?.message);
        const input = eventRecord(message?.input);
        if (message?.projectId === currentProjectRef.current && typeof message.threadId === "string"
          && message.threadId !== conversationSelectionRef.current.current.current.id && typeof message.id === "string" && typeof input?.text === "string") {
          const scope = {}, prior = conversationRuns.current.get(message.threadId), summary = prior && summarizeAgentRun(prior.run);
          conversationRuns.current.set(message.threadId, {scope, retry:null, questions:[], submitting:true,
            request:{requestId:message.id, projectId:message.projectId as string, scope},
            pipeline:{api,scope,owner:createConversationExecutionOwner(message.projectId as string)},
            run:{...createAgentRun(input.text, summary ? [summary,...prior!.run.history] : prior?.run.history ?? []),threadId:message.threadId}});
        }
        if (message?.projectId === currentProjectRef.current && message.threadId === conversationSelectionRef.current.current.current.id
          && typeof message.id === "string" && typeof input?.text === "string") {
          const scope = {};
          promptScopeRef.current = scope; promptSubmissionRef.current = scope;
          executionConversationRef.current = message.threadId as string;
          executionRequestRef.current = { requestId: message.id, projectId: message.projectId as string, scope };
          conversationExecutionRef.current = { api, scope, owner: createConversationExecutionOwner(message.projectId as string) };
          setGenerationHandoff(null);
          advisorySteeredRef.current = false;
          const previous = summarizeAgentRun(agentRunRef.current);
          setAgentRun({ ...createAgentRun(input.text, previous ? [previous, ...agentRunRef.current.history] : agentRunRef.current.history), threadId: message.threadId as string });
          setPromptPipeline({ projectId: message.projectId as string, scope });
        }
        return;
      }
      if (healthEvent?.method === "agent/message-ended") {
        const params = eventRecord(healthEvent.params), request = executionRequestRef.current;
        if (request && request.requestId === params?.messageId && request.projectId === params.projectId && request.scope === promptScopeRef.current) {
          if (typeof params.error === "string") setAgentRun(run => failedRun(run, params.error as string));
          const run = agentRunRef.current;
          const proposal = !params.error && !advisorySteeredRef.current ? boundGenerationHandoff(params.generationHandoff, {
            projectId: request.projectId, threadId: run.threadId, taskId: run.taskId, authorIntent: run.prompt,
          }) : null;
          setGenerationHandoff(proposal ? { proposal, scope: request.scope } : null);
          promptSubmissionRef.current = null; conversationExecutionRef.current = null;
          setPromptPipeline(current => current?.scope === request.scope ? null : current);
          ports.current.onRefresh();
        }
        return;
      }
      if (healthEvent?.method === "agent/execution-health" && Array.isArray(eventRecord(healthEvent.params)?.health)) {
        ports.current.onHealth(eventRecord(healthEvent.params)!.health as ExecutionHealth[]);
        return;
      }
      if (mediaContextRef.current) return;
      const previous = agentRunRef.current;
      const stop = stopTicketRef.current;
      const stopped = stop && ownsStop(stop, { scope: promptScopeRef.current, projectId: currentProjectRef.current, threadId: previous.threadId, turnId: previous.turnId }) && confirmsNativeStop(stop, event);
      if (stopped) stop.settle(true);
      const automatic = applyChapterStructureEvent(previous, event, currentProjectRef.current);
      const pipeline = conversationExecutionRef.current;
      const pipelineOwnsEvent = pipeline?.api === api && pipeline.scope === promptScopeRef.current
        && promptSubmissionRef.current === pipeline.scope && pipeline.owner.projectId === currentProjectRef.current;
      const pendingStart = pipelineOwnsEvent ? pipeline.pendingStart : null;
      const pendingSummary = pendingStart ? summarizeAgentRun(pendingStart.originalRun) : null;
      const bindingRun = pendingStart && eventRecord(event)?.method === "agent/workflow"
        ? createAgentRun(pendingStart.goal, pendingSummary ? [pendingSummary, ...pendingStart.originalRun.history] : pendingStart.originalRun.history) : previous;
      const bound = pipelineOwnsEvent ? applyConversationExecutionEvent(bindingRun, event, pipeline.owner) : bindingRun;
      const background = !pipelineOwnsEvent ? applyBackgroundExecutionEvent(previous, event, liveWorkListRef.current,
        currentProjectRef.current, conversationSelectionRef.current.current.current.id) : null;
      if (background) executionConversationRef.current = background.rootThreadId;
      const coordinated = background?.run ?? (bound === bindingRun ? previous : bound);
      if (pipelineOwnsEvent && coordinated !== previous) delete pipeline.pendingStart;
      const workflowTask = eventRecord(eventRecord(eventRecord(event)?.params)?.workflow)?.task;
      const workflowTaskId = eventRecord(workflowTask)?.id;
      const rejectedWorkflow = pipelineOwnsEvent && eventRecord(event)?.method === "agent/workflow"
        && (typeof workflowTaskId !== "string" || (!pipeline.owner.parents.has(workflowTaskId) && !pipeline.owner.workers.has(workflowTaskId)));
      const next = stopped ? stoppedRun(previous) : automatic !== previous ? automatic : coordinated !== previous ? coordinated
        : rejectedWorkflow || (pipelineOwnsEvent && pendingStart) ? previous : reduceAgentEvent(previous, event);
      const accepted = next !== previous;
      if (accepted) {
        const retry = executionRetryRef.current;
        if (coordinated !== previous && retry && retry.scope === pipeline?.scope) {
          retry.history = next.history;
        }
        if (automatic !== previous && automatic.origin === "chapterStructure" && automatic.taskId !== previous.taskId) {
          const scope = {};
          promptScopeRef.current = scope; stopTicketRef.current = null;
          executionRetryRef.current = { projectId: currentProjectRef.current, scope, history: automatic.history,
            execution: { kind: "taskRecovery", operation: "章节衔接检查", view: "chapters" } };
              }
        setAgentRun(next);
        if (pipelineOwnsEvent && next.threadId && eventRecord(event)?.method === "agent/workflow"
          && executionRetryRef.current?.execution.kind === "discussion" && !conversationSelectionRef.current.current.current.id) {
          executionConversationRef.current = next.threadId;
          composerDraftsRef.current.bindNewConversation(next.threadId);
          conversationSelectionRef.current.select(next.threadId);
          const retry = executionRetryRef.current;
          if (retry?.execution.kind === "discussion" && !retry.execution.threadId) executionRetryRef.current = { ...retry, execution: { ...retry.execution, threadId: next.threadId } };
        }
        directory.applyEvent(event);
        if (isResolvedUserInputEvent(event)) setUserInputQueue(current => current.filter(request => String(request.requestId) !== String(event.params.requestId)));
        if (eventRecord(event)?.method === "agent/error") setUserInputQueue([]);
      }
      const message = eventRecord(event);
      const params = eventRecord(message?.params);
      if (message?.method === "agent/planning-review-started") {
        const retry = executionRetryRef.current;
        if (retry?.execution.kind === "discussion" && retry.scope === promptScopeRef.current
          && promptSubmissionRef.current === retry.scope && retry.projectId === currentProjectRef.current
          && params?.projectId === retry.projectId && params.sourceTaskId === previous.taskId
          && typeof params.runId === "string" && (params.artifactKind === "outline" || params.artifactKind === "plan")) {
          executionRetryRef.current = { ...retry, execution: { kind: "taskRecovery", operation: "规划深审",
            view: params.artifactKind === "outline" ? "outline" : "plans", intent: retry.execution.prompt } };
        }
      }
      if (message?.method === "agent/candidate-started") {
        const retry = executionRetryRef.current;
        const sourceTaskId = params?.sourceTaskId;
        if (retry?.execution.kind === "discussion" && retry.scope === promptScopeRef.current
          && promptSubmissionRef.current === retry.scope && retry.projectId === currentProjectRef.current
          && params?.projectId === retry.projectId && typeof sourceTaskId === "string" && sourceTaskId === previous.taskId
          && typeof params?.actionId === "string" && typeof params?.operation === "string"
          && ["generateBrief", "generateOutline", "generatePlan", "generatePropertyRules", "generateChapter", "modelKnowledge", "extractCommitments", "reviseCommitment"].includes(params.operation)) {
          const operation = params.operation;
          const view = operation === "generateBrief" ? "brief" : operation === "generateOutline" ? "outline"
            : operation === "generatePlan" ? "plans" : operation === "generatePropertyRules" ? "world" : "chapters";
          const target = eventRecord(params.target);
          executionRetryRef.current = { ...retry, execution: { kind: "taskRecovery", operation, view, intent: retry.execution.prompt,
            ...(typeof target?.chapterId === "string" ? { chapterId: target.chapterId } : {}) } };
        }
      }
      // Name/archive notifications update known project threads, not a turn.
      if (!accepted && ["thread/name/updated", "thread/archived", "thread/unarchived"].includes(String(message?.method))) {
        const threadId = eventThreadId(event);
        directory.applyEvent(event);
      }
      if (params?.projectId === currentProjectRef.current) {
        if (message?.method === "agent/work-updated"
          && typeof params.workTaskId === "string" && params.workTaskId.length <= 160 && /^work-[a-zA-Z0-9-]+$/.test(params.workTaskId)
          && typeof params.threadId === "string" && params.threadId.length > 0
          && params.threadId.length <= 160 && params.threadId.trim() === params.threadId && !/[\p{Cc}]/u.test(params.threadId)) {
          ports.current.onRefresh();
        }
        ports.current.onStructureEvent(message?.method, params);
      }
    });
  }, [api]);

  useEffect(() => {
    if (!api) return;
    return api.agent.onUserInputRequest(request => {
      const background = [...conversationRuns.current.values()].find(saved => saved.request?.projectId === request.projectId && saved.run.threadId === request.threadId
        && saved.run.turnId === request.turnId && saved.scope !== promptScopeRef.current);
      if (background && ["running", "waiting"].includes(background.run.state)) {
        if (!background.questions?.some(item => item.executionId === request.executionId && item.requestId === request.requestId)) {
          background.questions = [...(background.questions ?? []), request];
          background.run = {...background.run, state: "waiting"};
        }
        return;
      }
      if (request.projectId !== currentProjectRef.current) return;
      const run = agentRunRef.current;
      if (!["running", "waiting"].includes(run.state) || !request.threadId || !request.turnId || request.threadId !== run.threadId || request.turnId !== run.turnId) return;
      setUserInputQueue(current => current.some(item => item.executionId === request.executionId && String(item.requestId) === String(request.requestId)) || current.length >= MAX_PENDING_INTERACTIONS ? current : [...current, request]);
      setActivity(items => ["创作引擎正在等待你的回答。", ...items].slice(0, 4));
    });
  }, [api]);

  useEffect(() => {
    const saved = executionConversationRef.current ? conversationRuns.current.get(executionConversationRef.current) : null;
    if (saved?.scope === promptScopeRef.current) saved.questions = userInputQueue;
    if (userInputQueue.length) setAgentRun(run => run.state === "running" ? {...run,state:"waiting"} : run);
  }, [userInputQueue]);
}
