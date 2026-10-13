import { useEffect, useRef, useState } from "react";
import { createAgentRun, type AgentRun } from "../agent-events";
import type { UserInputRequest } from "../app-workspace-types";
import type { BoundExecutionRetry } from "../execution-retry";
import { ExecutionSnapshotBuffer } from "../execution-snapshot";
import type { StopTicket } from "../execution-stop";
import type { ConversationExecutionOwner } from "../conversation-execution-events";
import type { ChapterWritingRequest } from "../chapter-writing";
import type { GenerationHandoffProposal, NovelCodexApi, RevisionRepairPreparation } from "../types";

/** Internal execution state shared by protocol, submission and recovery commands. */
export function useExecutionSession(api: NovelCodexApi | undefined) {

  const [userInputQueue, setUserInputQueue] = useState<UserInputRequest[]>([]);

  const [chapterWriting, setChapterWriting] = useState<ChapterWritingRequest | null>(null);

  const chapterWritingRef = useRef<ChapterWritingRequest | null>(null);

  const chapterWritingPendingRef = useRef<object | null>(null);

  const [chapterWritingPending, setChapterWritingPending] = useState(false);

  function selectChapterWriting(request: ChapterWritingRequest | null) {
    chapterWritingRef.current = request;
    setChapterWriting(request);
  }

  const [activity, setActivity] = useState<string[]>([]);

  const [agentRun, commitAgentRun] = useState<AgentRun>(createAgentRun());

  const executionConversationRef = useRef<string | null>(null);

  const conversationRuns = useRef(new Map<string, { run: AgentRun; retry: BoundExecutionRetry | null; scope: object; pipeline: typeof conversationExecutionRef.current; request?: typeof executionRequestRef.current; submitting?: boolean; questions?: UserInputRequest[] }>());

  const completedExecutionRequests = useRef(new Set<string>());

  const executionSnapshotBuffer = useRef(new ExecutionSnapshotBuffer());

  const agentRunRef = useRef(agentRun);

  const promptScopeRef = useRef({});

  const stopTicketRef = useRef<StopTicket | null>(null);

  const executionRequestRef = useRef<{ requestId: string; projectId: string; scope: object } | null>(null);

  const executionRetryRef = useRef<BoundExecutionRetry | null>(null);

  const conversationExecutionRef = useRef<{ api: typeof api; scope: object; owner: ConversationExecutionOwner; pendingStart?: { goal: string; originalRun: AgentRun } } | null>(null);

  function setAgentRun(update: AgentRun | ((run: AgentRun) => AgentRun)) {
    const next = typeof update === "function" ? update(agentRunRef.current) : update;
    const retry = executionRetryRef.current;
    if (retry && retry.scope === promptScopeRef.current && retry.history === null) retry.history = next.history;
    agentRunRef.current = next;
    const conversationId = executionConversationRef.current ?? next.threadId;
    if (conversationId) conversationRuns.current.set(conversationId, { run: next, retry: executionRetryRef.current, scope: promptScopeRef.current, pipeline: conversationExecutionRef.current, request: executionRequestRef.current, submitting: promptSubmissionRef.current === promptScopeRef.current, questions: conversationRuns.current.get(conversationId)?.questions ?? [] });
    commitAgentRun(next);
  }

  const [sentRevisionRepair, setSentRevisionRepair] = useState<RevisionRepairPreparation | null>(null);

  // Reducers preserve this array during one run; a new root gets a fresh history.
  const sentRepairHistoryRef = useRef<AgentRun["history"] | null>(null);

  const promptSubmissionRef = useRef<object | null>(null);

  const submittedAuthorDraftRef = useRef(new Map<object, () => void>());

  const submittedRequestScopes = useRef(new Map<string, {projectId: string; scope: object}>());

  const [promptPipeline, setPromptPipeline] = useState<{ projectId: string; scope: object } | null>(null);

  const steerSubmissionRef = useRef<object | null>(null);

  useEffect(() => () => { promptScopeRef.current = {}; }, []);

  const [generationHandoff, setGenerationHandoff] = useState<{ proposal: GenerationHandoffProposal; scope: object } | null>(null);

  const generationHandoffRef = useRef(generationHandoff);

  generationHandoffRef.current = generationHandoff;

  const advisorySteeredRef = useRef(false);

  const successfulSendRef = useRef<object | null>(null);
  function preserveCurrentConversation() {
    const id = executionConversationRef.current;
    if (id) conversationRuns.current.set(id, { run: agentRunRef.current, retry: executionRetryRef.current,
      scope: promptScopeRef.current, pipeline: conversationExecutionRef.current, request: executionRequestRef.current,
      submitting: promptSubmissionRef.current === promptScopeRef.current, questions: userInputQueue });
  }
  function activateConversation(projectId: string, threadId: string | null, fresh = false) {
    preserveCurrentConversation();
    const saved = !fresh && threadId ? conversationRuns.current.get(threadId) : null;
    promptScopeRef.current = saved?.scope ?? {};
    executionRetryRef.current = saved?.retry ?? null;
    conversationExecutionRef.current = saved?.pipeline ?? null;
    executionConversationRef.current = threadId;
    executionRequestRef.current = saved?.request ?? null;
    promptSubmissionRef.current = saved?.submitting ? saved.scope : null;
    setPromptPipeline(saved?.submitting ? { projectId, scope: saved.scope } : null);
    setAgentRun(saved?.run ?? { ...createAgentRun(), threadId });
    setUserInputQueue(saved?.questions ?? []); setGenerationHandoff(null);
  }
  function leaveProject() {
    preserveCurrentConversation();
    executionConversationRef.current = null; executionRequestRef.current = null;
    promptSubmissionRef.current = null; conversationExecutionRef.current = null; setPromptPipeline(null);
    selectChapterWriting(null); setSentRevisionRepair(null); sentRepairHistoryRef.current = null;
    promptScopeRef.current = {}; executionRetryRef.current = null;
    setAgentRun(createAgentRun()); setUserInputQueue([]);
  }
return { activateConversation, leaveProject, userInputQueue, setUserInputQueue, chapterWriting, setChapterWriting, chapterWritingRef, chapterWritingPendingRef, chapterWritingPending, setChapterWritingPending, selectChapterWriting, activity, setActivity, agentRun, commitAgentRun, executionConversationRef, conversationRuns, completedExecutionRequests, executionSnapshotBuffer, agentRunRef, promptScopeRef, stopTicketRef, executionRequestRef, executionRetryRef, conversationExecutionRef, setAgentRun, sentRevisionRepair, setSentRevisionRepair, sentRepairHistoryRef, promptSubmissionRef, submittedAuthorDraftRef, submittedRequestScopes, promptPipeline, setPromptPipeline, steerSubmissionRef, generationHandoff, setGenerationHandoff, generationHandoffRef, advisorySteeredRef, successfulSendRef };
}
export type ExecutionSession = ReturnType<typeof useExecutionSession>;
