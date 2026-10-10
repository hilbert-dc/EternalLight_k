import { isComposerSubmit } from "./composer-keyboard";
import { ExecutionSnapshotBuffer, truncatedExecutionNotice } from "./execution-snapshot";
import {WorkInterludePanel} from "./WorkInterludePanel";
import { ConversationMessageQueue } from "./ConversationMessageQueue";
import { ReadFailureNotice } from "./ReadFailureNotice";
import { useReasoningSelection } from "./useReasoningSelection";
import { ConversationExecutionStatus } from "./ConversationExecutionStatus";
import { workStopDescription } from "../shared/work-recovery-presentation.mjs";
import { EditionSwitcher } from "./multimedia/EditionSwitcher";
import { CreateMediaEditionDialog, type MediaEditionSource } from "./multimedia/CreateMediaEditionDialog";
import { readSelectedEdition, rememberSelectedEdition } from "./multimedia/edition-selection";
import { clearAuthorDrafts } from "./author-draft-storage";
import { threadHistory } from "./thread-history";
import { conversationRetryCandidates, conversationRetryTask, isConversationRetryCandidate } from "./conversation-retry";
import { ComposerRetryControl } from "./ComposerRetryControl";
import type { WorkTaskSummary } from "./types";
import { RuntimeConnectionStatus } from "./RuntimeConnectionStatus";
import { ConversationComparison } from "./ConversationComparison";
import { taskResultDiscussionExcerpt, type TaskResultDiscussion } from "./task-result-excerpt";
import { ComposerModelPicker } from "./ComposerModelPicker";
import { AuthorQuestionCard, type UserInputQuestion, type UserInputResponse, type ResolveUserInput } from "./AuthorQuestionCard";
import { ConversationReplyActions } from "./ConversationReplyActions";
import { ExecutionDiagnostics } from "./ExecutionDiagnostics";
import { ConversationPrompt } from "./ConversationPrompt";
import { compactTaskName, taskOperationName } from "./task-operation-presentation";
import type { ChapterTitlePreview } from "./chapter-title";
import { useChapterDeletion } from "./useChapterDeletion";
import { ChapterDeleteDialog } from "./ChapterDeleteDialog";
import { useAgentAvailability } from "./useAgentAvailability";
import { isModelConnectionUnavailable } from "./agent-error-description";
import { DiscussionSourceChip } from "./DiscussionSourceChip";
import { SelectionDiscussion, DiscussionExcerptCards } from "./SelectionDiscussion";
import { ConversationNavigator } from "./ConversationNavigator";
import { ConversationReader } from "./ConversationReader";
import "./conversation-workspace.css";
import { useSelectedConversation } from "./useSelectedConversation";
import { resolveCommitmentNavigation } from "./commitment-navigation";
import { resolveTaskResult, type ResolvedTaskResult } from "./task-result-navigation";
import type { CommitmentDecisionResult, TaskResultDestination } from "./task-results";
import { SavedAnalysisDialog } from "./SavedAnalysisDialog";
import type { NovelCodexApi, ConversationFocus, DiscussionSource, DiscussionExcerpt, CommitmentDetail, CommitmentNavigationRequest, CommitmentNavigationSelection, SavedAnalysis } from "./types";
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode, type CSSProperties, type PointerEvent as ReactPointerEvent, type RefObject } from "react";
import { createAgentRun, reduceAgentEvent, summarizeAgentRun, type AgentRun } from "./agent-events";
import { createStopTicket, ownsStop, isConfirmedCancellation, stoppedRun, failedRun, confirmsNativeStop, StoppedExecutionError, type StopTicket } from "./execution-stop";
import { ExportDialog } from "./ExportDialog";
import { ExecutionScopeDialog } from "./ExecutionScopeDialog";
import { DesiredOutcomes } from "./DesiredOutcomes";
import { ModuleRecordsPopover, type ModuleRecordSource } from "./ModuleRecordsPopover";
import { StoryReferenceView } from "./StoryReferenceView";
import "./page-layout.css";
import { ProjectCreateDialog } from "./ProjectCreateDialog";
import { MediaApplication } from "./multimedia/MediaApplication";
import type { CreativeEdition, CreativeMedium, MediaProject } from "../shared/multimedia-types";
import { ProjectInformation } from "./ProjectInformation";
import { lastChapter, rememberChapter } from "./chapter-location";
import { SearchResults, searchNavigation } from "./SearchResults";
import { useWorkspaceSearch } from "./useWorkspaceSearch";
import { usePlanningSubmission } from "./usePlanningSubmission";
import { ConversationTools } from "./ConversationTools";
import { useConversationOutbox } from "./useConversationOutbox";
import { useComposerDrafts } from "./useComposerDrafts";
import { useModelConnection } from "./useModelConnection";
import { ModelConnectionNotice } from "./ModelConnectionNotice";
import { CreativeWorkspaceView } from "./CreativeWorkspaceView";
import { useWorkCatalog } from "./useWorkCatalog";
import { OutlineVolumeNavigation } from "./OutlineVolumeNavigation";
import { SidebarWorkDirectory, type SidebarChapterCreationRequest } from "./SidebarWorkDirectory";
import { ConversationRecovery } from "./ConversationRecovery";
import { WorkTaskPanel } from "./WorkTaskPanel";
import { ChapterStructureNotice, type StructureCheckFailure } from "./ChapterStructureNotice";
import { applyChapterStructureEvent } from "./chapter-structure-events";
import { applyBackgroundExecutionEvent, applyConversationExecutionEvent, createConversationExecutionOwner, createRecoveredExecutionOwner, type ConversationExecutionOwner } from "./conversation-execution-events";
import { ConversationTimeline } from "./ConversationTimeline";
import { ConversationOutput, isUnfinishedConversationTurn, type FinalReplyDisplayState } from "./ConversationOutput";
import { AgentStatusBar } from "./AgentStatus";
import { describeAgentError } from "./agent-error-description";
import { useWorkTaskControls } from "./useWorkTaskControls";
import { useWorkTaskList } from "./useWorkTaskList";
import { attemptContainsRun, type WorkConversationContent } from "./work-conversation";
import { DiscussionSyncNotice } from "./DiscussionSyncNotice";
import { chapterWritingMatches, type ChapterWritingRequest } from "./chapter-writing";
import { ComposerDraftNotice } from "./ComposerDraftNotice";
import { type BoundExecutionRetry, type ExecutionRetry } from "./execution-retry";
import "./execution-recovery.css";
import { FailedRevisionRepair, PreparedRevisionRepair } from "./PreparedRevisionRepair";
import { useRevisionRepair } from "./useRevisionRepair";
import { CharacterCatalog } from "./CharacterCatalog";
import { IdentityDecisionPanel } from "./IdentityDecisionPanel";
import { PropertyRulesPanel } from "./PropertyRulesPanel";
import { LoginScreen } from "./LoginScreen";
import { UserSettings } from "./UserSettings";
import { WorkspaceDiagnostics } from "./WorkspaceDiagnostics";
import { authorInitial, hashPassword, type AuthorProfile } from "./author-session";
import { useAuthorSession } from "./use-author-session";
import { MarkdownContent } from "./MarkdownContent";
import { MetricsPanel } from "./MetricsPanel";
import type { BackgroundTerminal, Chapter, ExecutionScopeConfirmation, PlanningReviewStartRequest, ProjectSummary, ProposedAction, RecoveryRun, RuntimeState, RuntimeStatus, ThreadDetail, ThreadSummary, ViewId } from "./types";

import { authorIntentError } from "../shared/author-input-budget.mjs";
import type { GenerationHandoffProposal, RevisionContinuationPreparation, RevisionPredecessor, RevisionRepairPreparation } from "./types";
import { ConversationGenerationHandoff } from "./ConversationGenerationHandoff";

const DEFAULT_PROJECT_ID = "project-last-lamp";
const MAX_PENDING_INTERACTIONS = 4;
type AgentStatus = { executionHealth?: import("./AgentStatus").ExecutionHealth[]; configured: boolean; state: string; mode: "novel-agent-engine"; error: string | null; provider?: string; model?: string; reasoningEffort?: string | null; commandSource?: string | null; cliPath?: string | null; isolatedHome?: string | null };
type AgentConnection = Awaited<ReturnType<NovelCodexApi["agent"]["listConnections"]>>["connections"][number];
type UserInputRequest = { projectId: string; executionId: string; requestId: string | number; method: "item/tool/requestUserInput"; params: Record<string, unknown>; threadId?: string | null; turnId?: string | null; itemId?: string | null };
type PendingNavigation = { kind: "edition"; edition: CreativeEdition } | { kind: "new-edition" } | { kind: "delete-chapter"; projectId: string; chapterId: string } | { kind: "volume"; projectId: string; volumeId: string; expectedNodeId?: string } | { kind: "new-chapter"; request: SidebarChapterCreationRequest } | { kind: "commitment"; request: CommitmentNavigationRequest } | { kind: "commitment-edit"; result: CommitmentDecisionResult; detail: CommitmentDetail } | { kind: "result"; result: TaskResultDestination } | { kind: "view"; view: ViewId } | { kind: "chapter"; chapterId: string } | { kind: "project"; projectId: string } | { kind: "new-project" } | { kind: "close" } | { kind: "account" };
const emptyState: RuntimeState = {
  project: { id: "", title: "正在加载作品…", logline: "", updated_at: "" },
  canon: { characters: [], relationships: [], events: [], locations: [], plotThreads: [], worldRules: [], notes: [] },
  narrativeIr: { version: "1.0", scenes: [], anchors: [] },
  novelEdition: { chapters: [] },
};

const PANE_KEY = "novel-codex:layout-panes";
type LayoutPanes = { sidebar: number; rail: number; inspector: number };
function clampPane(value: number, min: number, max: number) { return Math.min(max, Math.max(min, Math.round(value))); }
function loadPanes(): LayoutPanes {
  const fallback = { sidebar: 248, rail: 420, inspector: 245 };
  try {
    const parsed = JSON.parse(localStorage.getItem(PANE_KEY) ?? "") as Partial<LayoutPanes>;
    return {
      sidebar: clampPane(Number(parsed.sidebar) || fallback.sidebar, 72, 480),
      rail: clampPane(Number(parsed.rail) || fallback.rail, 300, 840),
      inspector: clampPane(Number(parsed.inspector) || fallback.inspector, 88, 720),
    };
  } catch {
    return fallback;
  }
}
function PaneSash({ orientation, label, value, min, max, nextValue, onResize }: { orientation: "vertical" | "horizontal"; label: string; value: number; min: number; max: number; nextValue: (start: number, dx: number, dy: number) => number; onResize: (value: number) => void }) {
  const valueRef = useRef(value);
  const nextRef = useRef(nextValue);
  const resizeRef = useRef(onResize);
  valueRef.current = value;
  nextRef.current = nextValue;
  resizeRef.current = onResize;
  const apply = (start: number, dx: number, dy: number) => resizeRef.current(clampPane(nextRef.current(start, dx, dy), min, max));
  const onPointerDown = (event: ReactPointerEvent<HTMLDivElement>) => {
    if (event.button !== 0) return;
    event.preventDefault();
    const sash = event.currentTarget;
    const start = valueRef.current;
    const originX = event.clientX;
    const originY = event.clientY;
    sash.setPointerCapture(event.pointerId);
    sash.classList.add("dragging");
    const previousCursor = document.body.style.cursor;
    const previousSelect = document.body.style.userSelect;
    document.body.style.cursor = orientation === "vertical" ? "col-resize" : "row-resize";
    document.body.style.userSelect = "none";
    const move = (next: PointerEvent) => apply(start, next.clientX - originX, next.clientY - originY);
    const end = () => {
      sash.classList.remove("dragging");
      document.body.style.cursor = previousCursor;
      document.body.style.userSelect = previousSelect;
      sash.removeEventListener("pointermove", move);
      sash.removeEventListener("pointerup", end);
      sash.removeEventListener("pointercancel", end);
    };
    sash.addEventListener("pointermove", move);
    sash.addEventListener("pointerup", end);
    sash.addEventListener("pointercancel", end);
  };
  return <div className="pane-sash" role="separator" aria-orientation={orientation} aria-label={label} aria-valuemin={min} aria-valuemax={max} aria-valuenow={value} tabIndex={0} onPointerDown={onPointerDown} onKeyDown={(event) => { const step = event.shiftKey ? 48 : 16; const dx = event.key === "ArrowRight" ? step : event.key === "ArrowLeft" ? -step : 0; const dy = event.key === "ArrowDown" ? step : event.key === "ArrowUp" ? -step : 0; if (!dx && !dy) return; event.preventDefault(); apply(value, dx, dy); }} />;
}

const navGroups: { label: string; items: { id: ViewId; label: string; icon: string }[] }[] = [
  { label: "故事", items: [{ id: "brief", label: "作品概况", icon: "◫" }, { id: "chapters", label: "作品目录", icon: "▤" }, { id: "characters", label: "角色", icon: "♙" }] },
  { label: "规划", items: [{ id: "outline", label: "全书大纲", icon: "≡" }, { id: "plans", label: "近章规划", icon: "⋮" }] },
  { label: "参考", items: [{ id: "plot", label: "伏笔与悬念", icon: "⌁" }, { id: "timeline", label: "时间线", icon: "◷" }, { id: "world", label: "世界与规则", icon: "◇" }] },
];

function isResolvedUserInputEvent(event: unknown): event is { method: "serverRequest/resolved"; params: { requestId: string | number } } {
  if (event === null || typeof event !== "object") return false;
  const message = event as { method?: unknown; params?: { requestId?: unknown } };
  return message.method === "serverRequest/resolved" && (typeof message.params?.requestId === "string" || typeof message.params?.requestId === "number");
}

function userInputQuestions(params: Record<string, unknown>): UserInputQuestion[] { const questions = Array.isArray(params.questions) ? params.questions : []; return questions.slice(0, 3).flatMap((entry) => { const question = eventRecord(entry); if (!question || typeof question.id !== "string" || !question.id.trim() || typeof question.question !== "string") return []; const options = Array.isArray(question.options) ? question.options.slice(0, 6).flatMap((entry) => { const option = eventRecord(entry); return option && typeof option.label === "string" && option.label.trim() ? [{ label: option.label.slice(0, 240), description: typeof option.description === "string" && option.description.trim() ? option.description.slice(0, 240) : null }] : []; }) : []; return [{ id: question.id.slice(0, 240), header: typeof question.header === "string" && question.header.trim() ? question.header.slice(0, 120) : null, question: question.question.slice(0, 240), options, isOther: question.isOther === true }]; }); }

function eventRecord(value: unknown): Record<string, unknown> | null { return value !== null && typeof value === "object" && !Array.isArray(value) ? value as Record<string, unknown> : null; }
function eventText(value: unknown): string | null { return typeof value === "string" && value.trim() ? value.slice(0, 240) : null; }
function eventThreadId(event: unknown): string | null { const message = eventRecord(event); const params = eventRecord(message?.params); const thread = eventRecord(params?.thread); const id = typeof params?.threadId === "string" ? params.threadId : typeof thread?.id === "string" ? thread.id : null; return id && id.trim() && id.length <= 240 && !/[\r\n]/.test(id) ? id : null; }
function eventThreadStatus(event: unknown): string | null { const message = eventRecord(event); const params = eventRecord(message?.params); const status = eventRecord(params?.status); const type = eventText(status?.type); const flags = Array.isArray(status?.activeFlags) ? status.activeFlags : []; if (flags.some((flag) => flag === "waitingOnApproval" || flag === "waitingOnUserInput")) return "waiting"; if (type) return type; if (message?.method === "turn/started") return "active"; if (message?.method === "turn/completed") { const turn = eventRecord(params?.turn); return turn?.status === "failed" ? "systemError" : turn?.status === "interrupted" ? "interrupted" : "idle"; } return null; }
function applyThreadEvent(threads: ThreadSummary[], event: unknown, _archived: boolean): ThreadSummary[] { const threadId = eventThreadId(event); const method = eventRecord(event)?.method; const status = eventThreadStatus(event); return threadId && status && ["thread/status/changed", "turn/started", "turn/completed"].includes(String(method)) ? threads.map(thread => thread.id === threadId ? { ...thread, status } : thread) : threads; }

function App() {
  const api = window.novelCodex;
  const [theme, setTheme] = useState<"light" | "dark">(() => {
    try { return localStorage.getItem("novel-codex-theme") === "dark" ? "dark" : "light"; } catch { return "light"; }
  });
  const authorSession = useAuthorSession();
  const { author, signedIn } = authorSession;
  const [accountOpen, setAccountOpen] = useState(false);
  const [projectInformationOpen, setProjectInformationOpen] = useState(false);
  const [activeView, setActiveView] = useState<ViewId>("chapters");
  const [state, setState] = useState<RuntimeState>(emptyState);
  const [mediaContext, setMediaContext] = useState<{ project: MediaProject; edition: CreativeEdition } | null>(null);
  const mediaContextRef = useRef(mediaContext); mediaContextRef.current = mediaContext;
  const [mediaBusy, setMediaBusy] = useState(false);
  const [editionCatalog, setEditionCatalog] = useState<{ projectId: string; editions: CreativeEdition[]; selectedId: string | null }>({ projectId: "", editions: [], selectedId: null });
  const [editionCreateOpen, setEditionCreateOpen] = useState(false);
  const [editionSource, setEditionSource] = useState<MediaEditionSource | null>(null);
  const [editionError, setEditionError] = useState<string | null>(null);
  const editionNavigationEpoch = useRef(0), editionOpening = useRef(false);
  const [activeProjectId, setActiveProjectId] = useState(() => {
    try { return localStorage.getItem("novel-codex-active-project") ?? DEFAULT_PROJECT_ID; } catch { return DEFAULT_PROJECT_ID; }
  });
  const activeProjectSelectionRef = useRef(activeProjectId); activeProjectSelectionRef.current = activeProjectId;
  const [projects, setProjects] = useState<ProjectSummary[]>([]);
  const [projectLoading, setProjectLoading] = useState(true);
  const [exportOpen, setExportOpen] = useState(false);
  const [projectCreateOpen, setProjectCreateOpen] = useState(false);
  const [runtimeStatus, setRuntimeStatus] = useState<RuntimeStatus>({ state: "offline", mode: "local", error: "正在连接 Creative Runtime…" });
  const [agentStatus, setAgentStatus] = useState<AgentStatus>({ configured: false, state: "idle", mode: "novel-agent-engine", error: null });
  const [agentConnections, setAgentConnections] = useState<AgentConnection[]>([]);
  const [userInputQueue, setUserInputQueue] = useState<UserInputRequest[]>([]);
  const [selectedChapterId, setSelectedChapterId] = useState("");
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
  const [proposedActions, setProposedActions] = useState<{ projectId: string; actions: ProposedAction[] }>({ projectId: "", actions: [] });
  const [approvingActionId, setApprovingActionId] = useState<string | null>(null);
  const [scopeAction, setScopeAction] = useState<{ projectId: string; action: ProposedAction } | null>(null);
  const currentProjectRef = useRef(activeProjectId);
  currentProjectRef.current = activeProjectId;
  const [threads, setThreads] = useState<ThreadSummary[]>([]);
  const [recoveryRuns, setRecoveryRuns] = useState<RecoveryRun[]>([]);
  const [threadLoading, setThreadLoading] = useState(false);
  const [threadAction, setThreadAction] = useState<string | null>(null);
  const [threadArchived, setThreadArchived] = useState(false);
  const threadListRef = useRef<object | null>(null);
  const scopeExecutionRef = useRef<{ projectId: string; scope: object } | null>(null);
  const [inspectorOpen, setInspectorOpen] = useState(false);
  const [contextTab, setContextTab] = useState<"story" | "task" | "changes" | "usage" | "diagnostics" | "settings">("story");
  const [panes, setPanes] = useState<LayoutPanes>(loadPanes);
  useEffect(() => { try { localStorage.setItem(PANE_KEY, JSON.stringify(panes)); } catch { /* The layout still follows the current drag if storage is unavailable. */ } }, [panes]);
  const [viewport, setViewport] = useState(() => ({ width: window.innerWidth, height: window.innerHeight }));
  useEffect(() => {
    const resize = () => setViewport({ width: window.innerWidth, height: window.innerHeight });
    window.addEventListener("resize", resize);
    return () => window.removeEventListener("resize", resize);
  }, []);
  const sidebarWidth = Math.min(panes.sidebar, Math.max(72, viewport.width - 670));
  const railWidth = Math.min(panes.rail, Math.max(300, viewport.width - sidebarWidth - 370));
  const inspectorMax = Math.max(160, viewport.height - 360);
  const layoutStyle = { "--nc-sidebar": `${sidebarWidth}px`, "--nc-rail": `${railWidth}px`, "--nc-inspector": `${Math.min(panes.inspector, inspectorMax)}px` } as CSSProperties;
  const sidebarMax = Math.min(480, Math.max(72, viewport.width - railWidth - 370));
  const railMax = Math.min(840, Math.max(300, viewport.width - sidebarWidth - 370));
  const [backgroundTerminals, setBackgroundTerminals] = useState<BackgroundTerminal[]>([]);
  const [backgroundTerminalLoading, setBackgroundTerminalLoading] = useState(false);
  const [creativeRefreshToken, bumpCreativeRefreshToken] = useState(0);
  const [formalStateRefreshToken, bumpFormalStateRefreshToken] = useState(0);
  const setCreativeRefreshToken = (update: number | ((value: number) => number)) => {
    bumpCreativeRefreshToken(update);
    bumpFormalStateRefreshToken(value => value + 1);
  };
  const [formalReadError, setFormalReadError] = useState<{ projectId: string; message: string } | null>(null);
  const [creativeDirty, updateCreativeDirty] = useState(false);
  const mainPanelRef = useRef<HTMLElement>(null);
  const [moduleActionsContainer, setModuleActionsContainer] = useState<HTMLDivElement | null>(null);
  const [taskConversationContainer, setTaskConversationContainer] = useState<HTMLDivElement | null>(null);
  const creativeDirtyRef = useRef(creativeDirty);
  const setCreativeDirty = useCallback((value: boolean) => { creativeDirtyRef.current = value; updateCreativeDirty(value); }, []);
  const [discussionSyncNotice, setDiscussionSyncNotice] = useState<string | null>(null);
  const [structureFailures, setStructureFailures] = useState<StructureCheckFailure[]>([]);
  const [discussionSyncBusy, setDiscussionSyncBusy] = useState(false);
  const discussionSyncRef = useRef<object | null>(null);
  const deferredDiscussionSync = useRef<{ api: typeof api; projectId: string; scope: object } | null>(null);
  const resultNavigation = useRef(0);
  const resultDiscussionRequest = useRef(0);
  const [commitmentSelection, setCommitmentSelection] = useState<CommitmentNavigationSelection | null>(null);
  const [requestedCommitmentEdit, setRequestedCommitmentEdit] = useState<{ requestId: string; result: CommitmentDecisionResult; detail: CommitmentDetail } | null>(null);
  const [pendingDiscussionSource, setPendingDiscussionSource] = useState<DiscussionSource | null>(null);
  const pendingDiscussionSourceRef = useRef(pendingDiscussionSource);
  pendingDiscussionSourceRef.current = pendingDiscussionSource;
  const [savedAnalysis, setSavedAnalysis] = useState<SavedAnalysis | null>(null);
  const [taskResultSelection, setTaskResultSelection] = useState<(ResolvedTaskResult & { requestId: string }) | null>(null);
  const [taskResultError, setTaskResultError] = useState<string | null>(null);
  const [pendingNavigation, setPendingNavigation] = useState<PendingNavigation | null>(null);
  const [moduleRecords, setModuleRecords] = useState<(ModuleRecordSource & {
    api: typeof api; opener: HTMLElement | null; requestId: number;
  }) | null>(null);
  const moduleRecordsRequest = useRef(0);
  const [selectedOutlineVolume, setSelectedOutlineVolume] = useState<{ projectId: string; volumeId: string } | null>(null);
  const [requestedChapterCreation, setRequestedChapterCreation] = useState<SidebarChapterCreationRequest | null>(null);
  const [savingBeforeLeave, setSavingBeforeLeave] = useState(false);
  const [revisionContinuation, setRevisionContinuation] = useState<RevisionContinuationPreparation | null>(null);
  const [sentRevisionRepair, setSentRevisionRepair] = useState<RevisionRepairPreparation | null>(null);
  // Reducers preserve this array during one run; a new root gets a fresh history.
  const sentRepairHistoryRef = useRef<AgentRun["history"] | null>(null);
  const currentSentRepair = sentRepairHistoryRef.current === agentRun.history ? sentRevisionRepair : null;
  const reconnectRef = useRef<object | null>(null);
  const [reconnectNotice, setReconnectNotice] = useState<{ scope: object; message: string } | null>(null);
  const promptSubmissionRef = useRef<object | null>(null);
  const submittedAuthorDraftRef = useRef(new Map<object, () => void>());
  const submittedRequestScopes = useRef(new Map<string, {projectId: string; scope: object}>());
  const [promptPipeline, setPromptPipeline] = useState<{ projectId: string; scope: object } | null>(null);
  const steerSubmissionRef = useRef<object | null>(null);
  useEffect(() => () => { promptScopeRef.current = {}; }, []);
  const [generationHandoff, setGenerationHandoff] = useState<{ proposal: GenerationHandoffProposal; scope: object } | null>(null);
  const generationHandoffRef = useRef(generationHandoff);
  generationHandoffRef.current = generationHandoff;
  const [commandPaletteOpen, setCommandPaletteOpen] = useState(false);
  const composerRef = useRef<HTMLTextAreaElement | null>(null);
  const advisorySteeredRef = useRef(false);
  const planningSaveHandlerRef = useRef<(() => Promise<boolean>) | null>(null);
  const registerPlanningSaveHandler = useCallback((handler: (() => Promise<boolean>) | null) => { planningSaveHandlerRef.current = handler; }, []);
  const creativeSaveHandlerRef = useRef<(() => Promise<boolean>) | null>(null);
  const creativeDiscardHandlerRef = useRef<(() => boolean) | null>(null);
  const recoveryNoticeShownRef = useRef(false);
  const activeLabel = useMemo(() => navGroups.flatMap((group) => group.items).find((item) => item.id === activeView)?.label ?? "作品", [activeView]);
  const projectId = mediaContext?.project.id ?? state.project.id;
  const selectedConversation = useSelectedConversation(author?.email ?? "", projectId);
  const conversationSelectionRef = useRef(selectedConversation); conversationSelectionRef.current = selectedConversation;
  const [conversationChanging, setConversationChanging] = useState(false);
  const conversationChangeRef = useRef<object | null>(null);
  const [conversationReading, setConversationReading] = useState<{ projectId: string; threadId: string; detail: ThreadDetail | null; loading: boolean; error: string | null } | null>(null);
  const conversationReadRef = useRef<object | null>(null);
  const selectedThread = threads.find(thread => thread.id === selectedConversation.id) ?? (conversationReading?.projectId === projectId && conversationReading.threadId === selectedConversation.id ? conversationReading.detail?.thread : null);
  const executionConversationId = executionConversationRef.current ?? agentRun.threadId;
  const viewingExecution = selectedConversation.id === executionConversationId || !selectedConversation.id && (!executionConversationId
    || Boolean(promptSubmissionRef.current && conversationExecutionRef.current?.scope === promptScopeRef.current)
    || Boolean(!executionConversationRef.current && agentRun.prompt && executionRetryRef.current?.projectId === projectId && executionRetryRef.current.scope === promptScopeRef.current));
  const activeConversationDetail = conversationReading?.projectId === projectId && conversationReading.threadId === selectedConversation.id ? conversationReading : null;
  const sharedStateOwner = useMemo(() => ({}), [api, projectId, activeProjectId, signedIn]);
  const sharedStateOwnerRef = useRef(sharedStateOwner); sharedStateOwnerRef.current = sharedStateOwner;
  const promptPipelineBusy = promptPipeline?.projectId === projectId && promptPipeline.scope === promptScopeRef.current
    && promptSubmissionRef.current === promptPipeline.scope;
  const baseExecutionBusy = Boolean(promptPipelineBusy) || agentRun.state === "running" || agentRun.state === "waiting";
  const conversationProjectReady = signedIn && !mediaContext && Boolean(projectId) && projectId === activeProjectId;
  const workDirectory = useWorkCatalog(api, projectId, creativeRefreshToken, conversationProjectReady);
  const outlineVolume = conversationProjectReady && selectedOutlineVolume?.projectId === projectId
    ? workDirectory.catalog?.volumes.find(volume => volume.id === selectedOutlineVolume.volumeId) ?? null : null;
  useEffect(() => {
    if (!conversationProjectReady || selectedOutlineVolume && (selectedOutlineVolume.projectId !== projectId
      || workDirectory.catalog && !workDirectory.catalog.volumes.some(volume => volume.id === selectedOutlineVolume.volumeId))) setSelectedOutlineVolume(null);
  }, [conversationProjectReady, projectId, workDirectory.catalog, selectedOutlineVolume]);
  const writingRoute = { projectId: conversationProjectReady ? projectId : "", view: accountOpen ? "account" : activeView, chapterId: selectedChapterId };
  const writingRouteRef = useRef(writingRoute);
  writingRouteRef.current = writingRoute;
  const currentChapterWriting = chapterWritingMatches(chapterWriting, writingRoute) ? chapterWriting : null;
  if (!chapterWritingMatches(chapterWritingRef.current, writingRoute)) chapterWritingRef.current = null;
  useEffect(() => {
    if (!chapterWritingMatches(chapterWriting, writingRoute)) selectChapterWriting(null);
  }, [projectId, activeProjectId, activeView, accountOpen, selectedChapterId, conversationProjectReady]);
  function prepareChapterWriting(request: ChapterWritingRequest) {
    if (!chapterWritingMatches(request, writingRouteRef.current) || currentProjectRef.current !== request.projectId || agentRunRef.current.state === "running"
      || agentRunRef.current.state === "waiting" || chapterWritingPendingRef.current || promptSubmissionRef.current) return;
    revisionRepair.cancel();
    setSentRevisionRepair(null);
    composerDrafts.setSource(null); setPendingDiscussionSource(null);
    selectChapterWriting(request);
    setComposer(value => value.trim() ? value : request.defaultIntent);
    composerRef.current?.focus();
  }
  const taskList = useWorkTaskList(api, projectId, creativeRefreshToken, baseExecutionBusy, conversationProjectReady);
  const liveWorkListRef = useRef(taskList.tasks); liveWorkListRef.current = taskList.tasks;
  const taskControlScopeRef = useRef(taskList.scope);
  taskControlScopeRef.current = taskList.scope;
  const [taskControlMessage, setTaskControlMessage] = useState<{ scope: object; error: { targetId: string; message: string } | null; notice: { targetId: string; message: string } | null } | null>(null);
  const retryContextRef = useRef<{ api: NovelCodexApi | undefined; projectId: string; scope: object;
    owner: { scope: string; id: string | null }; choices: WorkTaskSummary[]; blocked: boolean; agentAvailable: boolean } | null>(null);
  const retryRequestRef = useRef<{ scope: object; ownerScope: string; threadId: string | null } | null>(null);
  const [retryPending, setRetryPending] = useState<{ scope: object; ownerScope: string; threadId: string | null } | null>(null);
  const pauseContextRef = useRef<{ api: NovelCodexApi | undefined; projectId: string; scope: object;
    owner: { scope: string; id: string | null }; choices: WorkTaskSummary[]; blocked: boolean } | null>(null);
  const pauseRequestRef = useRef<{ scope: object; ownerScope: string; threadId: string | null } | null>(null);
  const [pausePending, setPausePending] = useState<{ scope: object; ownerScope: string; threadId: string | null } | null>(null);
  const taskControls = useWorkTaskControls({ scope: taskList.scope,
    isBlocked: () => !conversationProjectReady || !api || creativeDirtyRef.current || !!taskList.error || !taskList.loaded,
    isControlBlocked: () => !conversationProjectReady || !api || !!taskList.error || !taskList.loaded,
    onBeforeResume: async workTaskId => {
      const controlOwner = taskList.scope;
      const selectedOwner = selectedConversation.current.current;
      const work = await api!.runtime.getWorkTask(projectId, workTaskId);
      if (taskControlScopeRef.current !== controlOwner || currentProjectRef.current !== projectId
        || selectedConversation.current.current.scope !== selectedOwner.scope || selectedConversation.current.current.id !== selectedOwner.id
        || work.threadId !== (selectedOwner.id ?? "")
        || work.projectId !== projectId || work.id !== workTaskId || !(["paused", "blocked"].includes(work.status)
          || work.status === "completed" && work.pendingContinuation)) throw new Error("任务来源已变化，尚未继续执行。请重新读取原任务。");
      const owner = createRecoveredExecutionOwner(work);
      if (!owner) throw new Error("原任务的对话来源未能核对，尚未继续执行。");
      const scope = beginExecutionScope({ kind: "taskRecovery", operation: "继续原任务", view: activeView, intent: work.goal });
      promptSubmissionRef.current = scope;
      setPromptPipeline({ projectId, scope });
      const originalRun = agentRunRef.current;
      conversationExecutionRef.current = { api, scope, owner, pendingStart: { goal: work.goal, originalRun } };
      return (result, error) => {
        const returned = eventRecord(result), notStarted = eventRecord(returned?.resumeFailure);
        const didNotStart = promptScopeRef.current === scope && currentProjectRef.current === projectId && taskControlScopeRef.current === controlOwner
          && returned?.id === workTaskId && returned.projectId === projectId && returned.status === "paused"
          && notStarted?.code === "WORK_RESUME_NOT_STARTED" && notStarted.workTaskId === workTaskId
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
        if (promptScopeRef.current === scope && currentProjectRef.current === projectId
          && ["running", "waiting"].includes(agentRunRef.current.state)) {
          setAgentRun(run => error ? failedRun(run, error instanceof Error ? error.message : "原任务恢复未完成")
            : returned?.interlude || returned?.status === "cancelled" || returned?.status === "paused" && eventRecord(returned.pauseAfterStep)?.remainingAttempts === 0 ? stoppedRun(run)
              : returned?.status === "completed" ? { ...run, state: "completed", completedAt: Date.now() }
                : { ...run, state: "error", error: "原任务尚未完成，请查看任务状态继续处理。", completedAt: Date.now() });
        }
      };
    },
    onControlApplied: (workTaskId) => {
      const pipeline = conversationExecutionRef.current;
      if (pipeline?.pendingStart && pipeline.api === api && pipeline.scope === promptScopeRef.current
        && pipeline.owner.workTaskId === workTaskId && pipeline.owner.projectId === currentProjectRef.current) {
        if (promptSubmissionRef.current === pipeline.scope) promptSubmissionRef.current = null;
        setPromptPipeline(current => current?.scope === pipeline.scope ? null : current);
        conversationExecutionRef.current = null;
      }
    },
    refresh: taskList.refresh, onFormalChange: () => refreshFormalState(() => taskControlScopeRef.current === taskList.scope),
    onError: (error, targetId) => setTaskControlMessage(previous => ({ scope: taskList.scope, error: error === null ? null : { targetId, message: error }, notice: previous?.scope === taskList.scope ? previous.notice : null })),
    onNotice: (notice, targetId) => setTaskControlMessage(previous => ({ scope: taskList.scope, notice: notice === null ? null : { targetId, message: notice }, error: previous?.scope === taskList.scope ? previous.error : null })) });
  const chapterDeletion = useChapterDeletion({ api, projectId, enabled: conversationProjectReady,
    isBlocked: () => baseExecutionBusy || taskControls.pending || !taskList.loaded || Boolean(taskList.error) || taskList.activeCount > 0
      || creativeDirtyRef.current || workDirectory.busy || !workDirectory.catalog || Boolean(workDirectory.error),
    onApplied: (next, deletedId) => {
      const deleted = state.novelEdition.chapters.find(chapter => chapter.id === deletedId);
      const remaining = [...next.novelEdition.chapters].sort((left, right) => left.number - right.number);
      const current = remaining.find(chapter => chapter.id === selectedChapterId);
      const nearest = remaining.find(chapter => chapter.number >= (deleted?.number ?? 0)) ?? remaining.at(-1);
      setState(next);
      if (!current) { setSelectedChapterId(nearest?.id ?? ""); if (nearest) rememberChapter(projectId, nearest.id); }
      setModuleRecords(null); setTaskResultSelection(null); setCommitmentSelection(null); setRequestedCommitmentEdit(null);
      setCreativeRefreshToken(value => value + 1);
      setActivity(items => ["章节已从作品目录和正文中删除。", ...items].slice(0, 4));
    } });
  const executionBusy = mediaBusy || baseExecutionBusy || taskControls.pending || chapterDeletion.active;
  useEffect(() => {
    if (!taskControls.resuming) return;
    taskList.poll();
    const timer = window.setInterval(taskList.poll, 1500);
    return () => window.clearInterval(timer);
  }, [taskControls.resuming, taskList.poll]);
  const modelConnection = useModelConnection({ api, scope: `${signedIn ? author?.email ?? "" : "signed-out"}:${activeProjectId}`,
    blocked: !signedIn,
    onConnections: setAgentConnections,
    onStatus: next => {
      setAgentStatus(next);
      setAgentRun(run => ["running", "waiting"].includes(run.state) ? run : ({ ...run, provider: next.provider ?? null, model: next.model ?? null, connectionState: next.configured ? "ready" : "offline", error: null }));
      setActivity(items => [`已应用模型：${next.model ?? "当前模型"}。`, ...items].slice(0, 4));
    } });
  const reasoningSelection = useReasoningSelection({ api, provider: agentStatus.provider, model: agentStatus.model,
    onChange: reasoningEffort => setAgentStatus(current => ({ ...current, reasoningEffort })) });
  const agentConfigSaving = modelConnection.busy;
  const search = useWorkspaceSearch(api, projectId);
  const composerDrafts = useComposerDrafts(projectId, selectedConversation.id, author?.email ?? "");
  const conversationScope = JSON.stringify([author?.email ?? "", projectId, selectedConversation.id]);
  const outbox = useConversationOutbox(api, projectId, selectedConversation.id);
  const [comparison, setComparison] = useState<{ projectId: string; threadId: string; sourceId: string } | null>(null);
  const conversationReadingAreaRef = useRef<HTMLDivElement>(null);
  const [conversationSearchContainer, setConversationSearchContainer] = useState<HTMLDivElement | null>(null);
  const [recoveryProject, setRecoveryProject] = useState<string | null>(null);
  const successfulSendRef = useRef<object | null>(null);

  const composerDraftsRef = useRef(composerDrafts); composerDraftsRef.current = composerDrafts;
  const resultDiscussionControls = useRef({ api, executionBusy, conversationChanging, writing: currentChapterWriting, repair: false, archived: Boolean(selectedThread?.isArchived) });
  const [newConversationExcerpt, setNewConversationExcerpt] = useState<{ projectId: string; threadId: string; excerpt: DiscussionExcerpt } | null>(null);
  const composer = composerDrafts.value;
  function setComposer(value: string | ((previous: string) => string)) {
    if (typeof value === "string" && value !== composer) planningSubmission.invalidate();
    composerDrafts.setValue(value);
  }
  useEffect(() => {
    if (!newConversationExcerpt || newConversationExcerpt.projectId !== projectId || newConversationExcerpt.threadId !== selectedConversation.id) return;
    if (composerDrafts.addExcerpt(newConversationExcerpt.excerpt)) {
      requestAnimationFrame(() => composerRef.current?.focus({ preventScroll: true }));
    }
    setNewConversationExcerpt(null);
  }, [newConversationExcerpt, selectedConversation.id, projectId]);
  async function readSelectedConversation(threadId: string) {
    if (!api || !projectId) return;
    const ticket = {}; conversationReadRef.current = ticket;
    const current = () => conversationReadRef.current === ticket && currentProjectRef.current === projectId
      && conversationSelectionRef.current.current.current.id === threadId;
    setConversationReading({ projectId, threadId, detail: null, loading: true, error: null });
    try {
      const detail = await api.agent.readConversation(projectId, threadId);
      if (!current()) return;
      if (detail.thread.id !== threadId || detail.thread.projectId !== projectId) throw new Error("对话不属于当前作品");
      setConversationReading({ projectId, threadId, detail, loading: false, error: null });
    } catch (error) { if (current()) setConversationReading({ projectId, threadId, detail: null, loading: false, error: error instanceof Error ? error.message : "读取对话失败" }); }
  }
  useEffect(() => {
    conversationReadRef.current = null;
    if (conversationProjectReady && selectedConversation.id) void readSelectedConversation(selectedConversation.id);
    else setConversationReading(null);
    return () => { conversationReadRef.current = null; };
  }, [api, projectId, conversationProjectReady, selectedConversation.id]);
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
  function selectConversation(thread: ThreadSummary | null) {
    if (!conversationProjectReady || conversationChanging || !composerDrafts.preserve()) return;
    if (thread && thread.projectId !== projectId) { setThreadAction("对话不属于当前作品"); return; }
    if ((thread?.id ?? null) !== selectedConversation.id) { resultNavigation.current += 1; setRequestedCommitmentEdit(null); }
    const originalId = executionConversationRef.current;
    if (originalId) conversationRuns.current.set(originalId, { run: agentRunRef.current, retry: executionRetryRef.current,
      scope: promptScopeRef.current, pipeline: conversationExecutionRef.current, request: executionRequestRef.current,
      submitting: promptSubmissionRef.current === promptScopeRef.current, questions: userInputQueue });
    selectedConversation.select(thread?.id ?? null);
    setPendingDiscussionSource(null); selectChapterWriting(null); revisionRepair.cancel();
    {
      const saved = thread ? conversationRuns.current.get(thread.id) : null;
      promptScopeRef.current = saved?.scope ?? {};
      executionRetryRef.current = saved?.retry ?? null;
      conversationExecutionRef.current = saved?.pipeline ?? null;
      executionConversationRef.current = thread?.id ?? null;
      executionRequestRef.current = saved?.request ?? null;
      promptSubmissionRef.current = saved?.submitting ? saved.scope : null;
      setPromptPipeline(saved?.submitting ? {projectId, scope: saved.scope} : null);
      setAgentRun(saved?.run ?? { ...createAgentRun(), threadId: thread?.id ?? null });
      setUserInputQueue(saved?.questions ?? []); setGenerationHandoff(null);
    }
    requestAnimationFrame(() => composerRef.current?.focus());
  }
  async function createConversation(excerpt?: DiscussionExcerpt, lastTurnId?: string) {
    if (!api || !conversationProjectReady || conversationChangeRef.current || !composerDrafts.preserve()) return false;
    const ticket = {}; conversationChangeRef.current = ticket; setConversationChanging(true); setThreadAction(null);
    try {
      const result = await api.agent.createThread(projectId, lastTurnId && selectedConversation.id ? { sourceThreadId: selectedConversation.id, lastTurnId } : {});
      if (conversationChangeRef.current !== ticket || currentProjectRef.current !== projectId) return false;
      if (result.thread.projectId !== projectId || result.thread.isArchived) throw new Error("新对话回执不属于当前作品");
      setThreadArchived(false); setThreads(current => [result.thread, ...current.filter(thread => thread.id !== result.thread.id)]);
      // Selection is published only after the persistent registration receipt.
      resultNavigation.current += 1; setRequestedCommitmentEdit(null);
      const originalId = executionConversationRef.current;
      if (originalId) conversationRuns.current.set(originalId, {run: agentRunRef.current, retry: executionRetryRef.current,
        scope: promptScopeRef.current, pipeline: conversationExecutionRef.current, request: executionRequestRef.current,
        submitting: promptSubmissionRef.current === promptScopeRef.current, questions: userInputQueue});
      promptSubmissionRef.current = null; executionRequestRef.current = null; setPromptPipeline(null);
      selectedConversation.select(result.thread.id);
      executionConversationRef.current = result.thread.id; promptScopeRef.current = {}; executionRetryRef.current = null;
      conversationExecutionRef.current = null; setAgentRun({ ...createAgentRun(), threadId: result.thread.id });
      setPendingDiscussionSource(null); selectChapterWriting(null); revisionRepair.cancel();
      setGenerationHandoff(null); setUserInputQueue([]);
      if (excerpt) setNewConversationExcerpt({ projectId, threadId: result.thread.id, excerpt });
      requestAnimationFrame(() => composerRef.current?.focus());
      return true;
    } catch (error) {
      if (currentProjectRef.current === projectId) setThreadAction(`创建对话失败：${error instanceof Error ? error.message : "请重试"}`);
      return false;
    } finally { if (conversationChangeRef.current === ticket) { conversationChangeRef.current = null; setConversationChanging(false); } }
  }
  async function renameConversation(thread: ThreadSummary, title: string) {
    if (!api || conversationChanging) throw new Error("正在切换对话，请稍后重命名");
    const result = await api.agent.renameThread(projectId, thread.id, title);
    if (currentProjectRef.current !== projectId) return;
    setThreads(current => current.map(item => item.id === thread.id ? result.thread : item));
  }
  useEffect(() => {
    if (!executionBusy && !viewingExecution && selectedThread && !activeConversationDetail?.loading) selectConversation(selectedThread);
  }, [executionBusy]);
  useEffect(() => { setRevisionContinuation(null); }, [projectId, activeView]);
  const selectedChapter = state.novelEdition.chapters.find((chapter) => chapter.id === selectedChapterId) ?? state.novelEdition.chapters[0];
  const [chapterTitlePreview, setChapterTitlePreview] = useState<{ api: typeof api; preview: ChapterTitlePreview; token: object } | null>(null);
  const titlePreviewRoute = useRef({ api, projectId, chapterId: selectedChapter?.id, active: conversationProjectReady && activeView === "chapters" && !accountOpen });
  titlePreviewRoute.current = { api, projectId, chapterId: selectedChapter?.id, active: conversationProjectReady && activeView === "chapters" && !accountOpen };
  const syncChapterTitlePreview = useCallback((preview: ChapterTitlePreview) => {
    const route = titlePreviewRoute.current;
    if (!route.active || route.api !== api || route.projectId !== preview.projectId || route.chapterId !== preview.chapterId) return () => {};
    const token = {};
    setChapterTitlePreview({ api, preview, token });
    return () => setChapterTitlePreview(current => current?.token === token ? null : current);
  }, [api]);
  const currentChapterTitlePreview = chapterTitlePreview && chapterTitlePreview.api === api && titlePreviewRoute.current.active
    && chapterTitlePreview.preview.projectId === projectId && chapterTitlePreview.preview.chapterId === selectedChapter?.id
    ? chapterTitlePreview.preview : null;

  const recordsCurrent = Boolean(moduleRecords && conversationProjectReady && !accountOpen
    && moduleRecords.api === api && moduleRecords.projectId === projectId && moduleRecords.view === activeView
    && (moduleRecords.view !== "chapters" || moduleRecords.chapterId === selectedChapter?.id)
    && (moduleRecords.view !== "outline" || moduleRecords.volumeId === (outlineVolume?.id ?? null)
      && (!moduleRecords.volumeId || moduleRecords.outlineNodeId === (outlineVolume?.source?.nodeId ?? null))));
  useEffect(() => {
    if (moduleRecords && !recordsCurrent) setModuleRecords(null);
  }, [moduleRecords, recordsCurrent]);
  const syncOutlineRecordScope = useCallback((outlineNodeId: string | null) => {
    setModuleRecords(current => current && current.api === api && current.projectId === projectId && current.view === "outline"
      && current.outlineNodeId !== outlineNodeId ? null : current);
  }, [api, projectId]);
  function openModuleRecords(outlineNodeId: string | null = null) {
    if (!conversationProjectReady || accountOpen || !["brief", "outline", "plans", "chapters"].includes(activeView)
      || activeView === "chapters" && !selectedChapter) return;
    const view = activeView as ModuleRecordSource["view"];
    const next = { api, projectId, view, chapterId: view === "chapters" ? selectedChapter!.id : null,
      volumeId: view === "outline" ? outlineVolume?.id ?? null : null,
      outlineNodeId: view === "outline" ? outlineNodeId : null,
      label: view === "chapters" ? `第 ${selectedChapter!.number} 章 · ${selectedChapter!.title}`
        : view === "brief" ? "创作方向" : view === "plans" ? "近章规划"
        : outlineVolume?.title ?? (outlineNodeId ? "本卷卷纲" : "全书大纲"),
      opener: document.activeElement instanceof HTMLElement ? document.activeElement : null,
      requestId: ++moduleRecordsRequest.current };
    setModuleRecords(current => current && current.api === api && current.projectId === projectId && current.view === view
      && current.chapterId === next.chapterId && current.volumeId === next.volumeId && current.outlineNodeId === next.outlineNodeId ? null : next);
  }
  // A failed run is retryable; only missing configuration disables submission.
  // The host still checks the current plan, credentials, scope and budget.
  const agentAvailable = Boolean(api && runtimeStatus.state === "online" && agentStatus.configured && !agentConfigSaving);
  useAgentAvailability({ api, blocked: executionBusy || agentConfigSaving, provider: agentStatus.provider, model: agentStatus.model,
    canApply: () => !promptSubmissionRef.current && !["running", "waiting"].includes(agentRunRef.current.state), onStatus: setAgentStatus });
  const revisionRepair = useRevisionRepair({ api, projectId, activeProjectId, composer,
    blocked: creativeDirty || !agentAvailable || executionBusy });
  resultDiscussionControls.current = { api, executionBusy, conversationChanging, writing: currentChapterWriting, repair: Boolean(revisionRepair.prepared), archived: Boolean(selectedThread?.isArchived) };
  const planningSubmissionEligible = conversationProjectReady && !accountOpen && ["brief", "outline", "plans"].includes(activeView)
    && agentAvailable && !executionBusy && !currentChapterWriting && !composerDrafts.source && !composerDrafts.excerpts.length && !pendingDiscussionSource
    && !revisionRepair.prepared && !revisionRepair.checking && !currentSentRepair;
  const planningSubmission = usePlanningSubmission({ api, projectId, view: accountOpen ? "account" : activeView,
    eligible: planningSubmissionEligible, composer, sourceIdentity: JSON.stringify(composerDrafts.source ?? null),
    canSendAfterSave: () => !creativeDirtyRef.current && !taskControls.hasPendingRequests() && !promptSubmissionRef.current });
  useEffect(() => {
    const pending = deferredDiscussionSync.current;
    if (!pending) return;
    if (pending.api !== api || pending.projectId !== projectId || pending.scope !== promptScopeRef.current) { deferredDiscussionSync.current = null; return; }
    if (creativeDirty || executionBusy || promptSubmissionRef.current || discussionSyncRef.current || planningSubmission.isPending()) return;
    deferredDiscussionSync.current = null;
    void refreshDiscussionWorkspace();
  }, [api, projectId, creativeDirty, executionBusy, promptPipelineBusy, discussionSyncNotice, planningSubmission.pending]);



  useEffect(() => {
    if (!api || !conversationProjectReady) return;
    let current = true;
    void api.agent.listProposedActions(projectId).then(({ actions }) => {
      if (current) setProposedActions({ projectId, actions });
    }).catch(() => { if (current) setProposedActions({ projectId, actions: [] }); });
    return () => { current = false; };
  }, [api, projectId, conversationProjectReady]);

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    try { localStorage.setItem("novel-codex-theme", theme); } catch { /* Storage can be unavailable in hardened sessions. */ }
  }, [theme]);

  useEffect(() => {
    try { localStorage.setItem("novel-codex-active-project", activeProjectId); } catch { /* Storage can be unavailable in hardened sessions. */ }
  }, [activeProjectId]);

  useEffect(() => {
    if (!api) return;
    return api.app.onCloseRequested(() => {
      if (creativeDirty) setPendingNavigation({ kind: "close" });
      else void api.app.confirmClose();
    });
  }, [api, creativeDirty]);

  useEffect(() => {
    if (!signedIn || !author) { setProjectLoading(false); return; }
    let cancelled = false;
    setProjectLoading(true);
    async function load() {
      if (editionOpening.current) return;
      const editionTicket = editionNavigationEpoch.current;
      if (!api) { setProjectLoading(false); return; }
      try {
        const status = await api.runtime.status();
        if (!cancelled) setRuntimeStatus(status);
        const agent = await api.agent.status();
        const availableConnections = await api.agent.listConnections();
        const recoveryRuns = (await api.agent.recoveryReport()).filter(authorFacingRecoveryRun);
        if (!cancelled) {
          setAgentStatus(agent);
          setAgentConnections(availableConnections.connections);
          setRecoveryRuns(recoveryRuns);
          setAgentRun((run) => ({ ...run, model: agent.model ?? run.model, provider: agent.provider ?? run.provider, connectionState: agent.configured ? run.connectionState === "error" ? "error" : "ready" : "offline" }));
          if (!recoveryNoticeShownRef.current && recoveryRuns.length) {
            recoveryNoticeShownRef.current = true;
            setActivity((items) => ["部分对话的保存结果待核对，可在对应作品的对话栏查看。", ...items].slice(0, 4));
          }
        }
        try {
          const available = await api.runtime.listProjects();
          if (!cancelled) setProjects(available.projects);
          if (cancelled) return;
          if (available.projects.length === 0) {
            setMediaContext(null);
            setState({ ...emptyState, project: { id: "", title: "创建你的第一部作品", logline: "", updated_at: "" } });
            setSelectedChapterId("");
            return;
          }
          if (!available.projects.some(project => project.id === activeProjectId)) {
            setActiveProjectId(available.projects[0].id);
            return;
          }
        } catch { /* The active project can still load when list metadata is unavailable. */ }
        const { editions } = await api.multimedia.listEditions(activeProjectId);
        if (cancelled || editionTicket !== editionNavigationEpoch.current) return;
        const savedEdition = readSelectedEdition(author?.email ?? "", activeProjectId);
        const edition = editions.find(item => item.id === savedEdition) ?? (editions.length === 1 ? editions[0] : null);
        setEditionCatalog({ projectId: activeProjectId, editions, selectedId: edition?.id ?? null });
        setEditionError(null);
        if (!editions.length) throw new Error("作品尚无可编辑版本");
        if (!edition) return;
        if (edition.medium !== "novel") {
          const workspace = await api.multimedia.readWorkspace(activeProjectId, edition.id, edition.branchId);
          if (!cancelled && editionTicket === editionNavigationEpoch.current) { setMediaContext({ project: workspace.project, edition: workspace.edition }); rememberSelectedEdition(author?.email ?? "", activeProjectId, edition.id); }
          return;
        }
        const next = await api.runtime.getState(activeProjectId);
        if (!cancelled && editionTicket === editionNavigationEpoch.current) setMediaContext(null);
        if (!cancelled && editionTicket === editionNavigationEpoch.current) { rememberSelectedEdition(author?.email ?? "", activeProjectId, edition.id); setState(next); setSelectedChapterId(current => next.novelEdition.chapters.some(chapter => chapter.id === current) ? current : lastChapter(activeProjectId, next.novelEdition.chapters)); }
      } catch (error) {
        if (!cancelled) setRuntimeStatus({ state: "offline", mode: "local", error: error instanceof Error ? error.message : "Creative Runtime 不可用" });
      } finally { if (!cancelled) setProjectLoading(false); }
    }
    void load();
    const timer = window.setTimeout(() => void load(), 900);
    return () => { cancelled = true; window.clearTimeout(timer); };
  }, [api, activeProjectId, author?.email, signedIn]);

  useEffect(() => {
    if (!api) return;
    let cancelled = false;
    const refreshStatus = async () => {
      try {
        const status = await api.runtime.status();
        if (!cancelled) setRuntimeStatus(status);
      } catch (error) {
        if (!cancelled) setRuntimeStatus({ state: "offline", mode: "local", error: error instanceof Error ? error.message : "Creative Runtime 不可用" });
      } finally { if (!cancelled) setProjectLoading(false); }
    };
    const timer = window.setInterval(() => void refreshStatus(), 2_000);
    void refreshStatus();
    return () => { cancelled = true; window.clearInterval(timer); };
  }, [api]);

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
        if (domain.projectId === currentProjectRef.current) setThreads(current => applyThreadEvent(current, event, threadArchived));
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
          promptSubmissionRef.current = null; conversationExecutionRef.current = null;
          setPromptPipeline(current => current?.scope === request.scope ? null : current);
          setCreativeRefreshToken(value => value + 1);
        }
        return;
      }
      if (healthEvent?.method === "agent/execution-health" && Array.isArray(eventRecord(healthEvent.params)?.health)) {
        setAgentStatus(current => ({ ...current, executionHealth: eventRecord(healthEvent.params)!.health as import("./AgentStatus").ExecutionHealth[] }));
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
        setThreads(current => applyThreadEvent(current, event, threadArchived));
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
        setThreads(current => current.some(thread => thread.id === threadId) ? applyThreadEvent(current, event, threadArchived) : current);
      }
      if (params?.projectId === currentProjectRef.current) {
        if (message?.method === "agent/work-updated"
          && typeof params.workTaskId === "string" && params.workTaskId.length <= 160 && /^work-[a-zA-Z0-9-]+$/.test(params.workTaskId)
          && typeof params.threadId === "string" && params.threadId.length > 0
          && params.threadId.length <= 160 && params.threadId.trim() === params.threadId && !/[\p{Cc}]/u.test(params.threadId)) {
          setCreativeRefreshToken(value => value + 1);
        }
        if (message?.method === "structure/error" && typeof params.message === "string") {
          const failure = { projectId: currentProjectRef.current, changeId: typeof params.changeId === "string" ? params.changeId : null, message: params.message.slice(0, 240) };
          setStructureFailures(current => [...current.filter(item => item.projectId !== failure.projectId || item.changeId !== failure.changeId), failure]);
          setCreativeRefreshToken(value => value + 1);
        }
        if (message?.method === "agent/structure-updated") {
          if (params.completed === true) setStructureFailures(current => current.filter(item => item.projectId !== params.projectId || item.changeId !== params.changeId));
          setCreativeRefreshToken(value => value + 1);
        }
        if (message?.method === "agent/structure-started") setCreativeRefreshToken(value => value + 1);
      }
    });
  }, [api, threadArchived]);
  useEffect(() => {
    if (!api) return;
    const selectedProjectId = signedIn && author && runtimeStatus.state === "online" && projectId === activeProjectId ? projectId : null;
    const edition = mediaContext?.project.id === selectedProjectId && !chapterDeletion.active && !workDirectory.busy ? mediaContext.edition : null;
    void api.agent.setWorkspaceContext({ projectId: selectedProjectId, dirty: creativeDirty || chapterDeletion.active || workDirectory.busy,
      mediaScope: edition ? {projectId: edition.projectId, editionId: edition.editionId, scopeId: edition.scopeId, branchId: edition.branchId} : null }).catch(() => {});
    return () => { void api.agent.setWorkspaceContext({ projectId: null, dirty: false, mediaScope: null }).catch(() => {}); };
  }, [api, projectId, activeProjectId, mediaContext, creativeDirty, chapterDeletion.active, workDirectory.busy, signedIn, author, runtimeStatus.state]);
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
  useEffect(() => {
    if (!api || !conversationProjectReady) return;
    let cancelled = false;
    setThreadLoading(true);
    void api.agent.listThreads(projectId, threadArchived).then((result) => { if (!cancelled) { setThreads(result.threads); } }).catch(() => { if (!cancelled) setThreadAction("任务历史暂不可用"); }).finally(() => { if (!cancelled) setThreadLoading(false); });
    return () => { cancelled = true; };
  }, [api, conversationProjectReady, projectId, activeProjectId, threadArchived]);

  const registerCreativeSaveHandler = useCallback((handler: (() => Promise<boolean>) | null) => { creativeSaveHandlerRef.current = handler; }, []);
  const registerCreativeDiscardHandler = useCallback((handler: (() => boolean) | null) => { creativeDiscardHandlerRef.current = handler; }, []);
  function applyNavigation(action: PendingNavigation) {
    if (action.kind === "edition") { void openEdition(action.edition).catch(reason => setEditionError(reason instanceof Error ? reason.message : "作品版本未打开，请重试。")); return; }
    if (action.kind === "new-edition") { setEditionCreateOpen(true); return; }
    if (action.kind === "delete-chapter") {
      if (action.projectId === currentProjectRef.current && conversationProjectReady && !executionBusy && !creativeDirtyRef.current) chapterDeletion.open(action.chapterId);
      return;
    }
    if (chapterDeletion.active) return;
    if (action.kind === "volume" && (!conversationProjectReady || action.projectId !== currentProjectRef.current
      || executionBusy || workDirectory.busy || promptSubmissionRef.current || ["running", "waiting"].includes(agentRunRef.current.state)
      || workDirectory.error || !workDirectory.catalog?.volumes.some(volume => volume.id === action.volumeId
        && (!action.expectedNodeId || volume.source?.nodeId === action.expectedNodeId)))) return;
    setModuleRecords(null);
    if (action.kind !== "volume") setSelectedOutlineVolume(null);
    if (action.kind !== "new-chapter") setRequestedChapterCreation(null);
    const ticket = ++resultNavigation.current;
    const readingAnalysis = action.kind === "result" && (action.result.kind === "planningReview" || action.result.kind === "hypothesis");
    if (!readingAnalysis) setTaskResultSelection(null);
    setCommitmentSelection(null);
    setRequestedCommitmentEdit(null);
    setSavedAnalysis(null); setTaskResultError(null);
    if (action.kind === "commitment-edit") {
      const { result, detail } = action;
      if (!api || !conversationProjectReady || result.projectId !== currentProjectRef.current || executionBusy
        || result.deliveryState !== "recorded" || detail.projectId !== result.projectId || !detail.sourceCurrent || !detail.authorControls
        || detail.objectId !== `commitment-${detail.payload.id}` || detail.extractionOrigin?.chapterId !== result.chapterId
        || detail.extractionOrigin.reportRevisionId !== result.referenceId) return;
      const { deliveryState: _deliveryState, ...destination } = result;
      void resolveTaskResult(api, destination).then(resolved => {
        if (resultNavigation.current !== ticket || currentProjectRef.current !== result.projectId) return;
        if (creativeDirtyRef.current || promptSubmissionRef.current || ["running", "waiting"].includes(agentRunRef.current.state)
          || resolved.chapterId !== result.chapterId || resolved.view !== "chapters") {
          setTaskResultError("当前编辑、任务或伏笔来源已变化，请核对后重新打开调整。"); return;
        }
        setAccountOpen(false); setActiveView("chapters");
        rememberChapter(result.projectId, result.chapterId); setSelectedChapterId(result.chapterId);
        setRequestedCommitmentEdit({ requestId: crypto.randomUUID(), result, detail });
      }).catch(reason => {
        if (resultNavigation.current === ticket && currentProjectRef.current === result.projectId) {
          setTaskResultError(reason instanceof Error ? reason.message : "读取伏笔调整来源失败，请重试。");
        }
      });
      return;
    }
    if (action.kind === "commitment") {
      const request = action.request;
      if (!api || request.projectId !== currentProjectRef.current || executionBusy) return;
      void resolveCommitmentNavigation(api, request).then(resolved => {
        if (resultNavigation.current !== ticket || currentProjectRef.current !== request.projectId) return;
        if (creativeDirtyRef.current || promptSubmissionRef.current || ["running", "waiting"].includes(agentRunRef.current.state)) {
          setTaskResultError("当前编辑或任务尚未结束，请处理后重新打开伏笔。"); return;
        }
        setAccountOpen(false); setActiveView("chapters");
        rememberChapter(request.projectId, resolved.chapterId); setSelectedChapterId(resolved.chapterId);
        setCommitmentSelection({ requestId: crypto.randomUUID(), request, chapterId: resolved.chapterId });
      }).catch(reason => {
        if (resultNavigation.current === ticket && currentProjectRef.current === request.projectId) setTaskResultError(reason instanceof Error ? reason.message : "读取伏笔失败，请重试。");
      });
      return;
    }
    if (action.kind === "result") {
      if (!api || action.result.projectId !== currentProjectRef.current) return;
      void resolveTaskResult(api, action.result).then(resolved => {
        if (resultNavigation.current !== ticket || currentProjectRef.current !== action.result.projectId) return;
        if (resolved.analysis) { setSavedAnalysis(resolved.analysis); return; }
        if (creativeDirtyRef.current) {
          setTaskResultError("当前编辑尚未保存，请处理后重新打开结果。"); return;
        }
        setAccountOpen(false); setActiveView(resolved.view);
        if (resolved.chapterId) { rememberChapter(projectId, resolved.chapterId); setSelectedChapterId(resolved.chapterId); }
        setTaskResultSelection({ ...resolved, requestId: crypto.randomUUID() });
      }).catch(reason => {
        if (resultNavigation.current === ticket && currentProjectRef.current === action.result.projectId) setTaskResultError(reason instanceof Error ? reason.message : "读取任务结果失败，请重试。");
      });
      return;
    }
    if (action.kind === "new-chapter") {
      if (!conversationProjectReady || action.request.projectId !== currentProjectRef.current || executionBusy
        || promptSubmissionRef.current || ["running", "waiting"].includes(agentRunRef.current.state)) return;
      setAccountOpen(false); setActiveView("chapters");
      setRequestedChapterCreation(action.request);
      return;
    }
    if (action.kind === "volume") {
      setSelectedOutlineVolume({ projectId: action.projectId, volumeId: action.volumeId });
      setAccountOpen(false); setActiveView("outline");
      return;
    }
    if (action.kind === "close") { void api?.app.confirmClose(); return; }
    if (action.kind === "account") { setAccountOpen(true); return; }
    if (action.kind === "view") { setAccountOpen(false); setActiveView(action.view); return; }
    if (action.kind === "chapter") { rememberChapter(projectId, action.chapterId); setAccountOpen(false); setSelectedChapterId(action.chapterId); setActiveView("chapters"); return; }
    if (action.kind === "project") {
      resetProjectSession();
      setActiveProjectId(action.projectId);
      setActivity((items) => ["正在打开作品…", ...items].slice(0, 4));
      return;
    }
    if (action.kind === "new-project") { setProjectCreateOpen(true); return; }
  }
  function requestNavigation(action: PendingNavigation) {
    if (planningSubmission.isPending()) planningSubmission.invalidate();
    if ((action.kind === "project" || action.kind === "new-project" || action.kind === "edition" || action.kind === "new-edition") && !composerDrafts.preserve()) return;
    if (creativeDirty && !(action.kind === "result" && ["planningReview", "hypothesis"].includes(action.result.kind))) { setPendingNavigation(action); return; }
    applyNavigation(action);
  }
  function openOutlineNode(nodeId: string): "volume" | "node" | "blocked" {
    if (!conversationProjectReady || activeView !== "outline" || executionBusy || workDirectory.busy || workDirectory.error || !workDirectory.catalog) return "blocked";
    const matches = workDirectory.catalog.volumes.filter(volume => volume.source?.nodeId === nodeId);
    if (matches.length > 1) return "blocked";
    if (matches.length === 0) return "node";
    requestNavigation({ kind: "volume", projectId, volumeId: matches[0].id, expectedNodeId: nodeId });
    return "volume";
  }
  function navigateView(nextView: ViewId) {
    if (nextView === activeView && !accountOpen && !selectedOutlineVolume) return;
    requestNavigation({ kind: "view", view: nextView });
  }
  function selectChapter(chapter: Chapter) {
    if (chapter.id === selectedChapterId && activeView === "chapters" && !accountOpen) return;
    requestNavigation({ kind: "chapter", chapterId: chapter.id });
  }
  function resetProjectSession() {
    const originalId = executionConversationRef.current;
    if (originalId) conversationRuns.current.set(originalId, {run:agentRunRef.current,retry:executionRetryRef.current,
      scope:promptScopeRef.current,pipeline:conversationExecutionRef.current,request:executionRequestRef.current,
      submitting:promptSubmissionRef.current === promptScopeRef.current,questions:userInputQueue});
    executionConversationRef.current = null; executionRequestRef.current = null;
    promptSubmissionRef.current = null; conversationExecutionRef.current = null; setPromptPipeline(null);
    setEditionSource(null); setEditionError(null);
    setRecoveryProject(null);
    setModuleRecords(null);
    setSelectedOutlineVolume(null);
    setRequestedChapterCreation(null);
    resultNavigation.current += 1;
    setCommitmentSelection(null); setRequestedCommitmentEdit(null); setPendingDiscussionSource(null);
    setSavedAnalysis(null);
    setTaskResultSelection(null);
    setTaskResultError(null);
    selectChapterWriting(null);
    revisionRepair.cancel();
    setSentRevisionRepair(null);
    sentRepairHistoryRef.current = null;
    promptScopeRef.current = {};
    executionRetryRef.current = null;
    threadListRef.current = null;
    setThreadLoading(false);
    deferredDiscussionSync.current = null;
    setDiscussionSyncNotice(null);
    setAgentRun(createAgentRun());
    setUserInputQueue([]);
    setThreads([]);
    setThreadAction(null);
    search.reset();
    setAccountOpen(false);
    setProjectInformationOpen(false);
    setSelectedChapterId("");
    setActiveView("chapters");
  }
  function selectProject(nextProjectId: string) {
    if (nextProjectId === activeProjectId || editionOpening.current) return;
    requestNavigation({ kind: "project", projectId: nextProjectId });
  }
  async function openEdition(requested: CreativeEdition) {
    if (!api || editionOpening.current || requested.projectId !== activeProjectId) throw new Error("当前无法切换作品版本。");
    editionOpening.current = true; const ticket = ++editionNavigationEpoch.current;
    setProjectLoading(true); setEditionError(null);
    try {
      const { editions } = await api.multimedia.listEditions(activeProjectId);
      const edition = editions.find(item => item.id === requested.id && item.projectId === requested.projectId && item.scopeId === requested.scopeId && item.branchId === requested.branchId && item.medium === requested.medium);
      if (!edition) throw new Error("作品版本归属已变化，请重新读取。");
      const media = edition.medium !== "novel" ? await api.multimedia.readWorkspace(activeProjectId, edition.id, edition.branchId) : null;
      const novel = edition.medium === "novel" ? await api.runtime.getState(activeProjectId) : null;
      if (ticket !== editionNavigationEpoch.current || requested.projectId !== activeProjectSelectionRef.current) return;
      resetProjectSession();
      if (media) { if (media.edition.id !== edition.id || media.project.id !== activeProjectId) throw new Error("作品版本回执归属不匹配。"); setMediaContext({ project: media.project, edition: media.edition }); }
      else if (novel) { setMediaContext(null); setState(novel); setSelectedChapterId(lastChapter(activeProjectId, novel.novelEdition.chapters)); }
      setEditionCatalog({ projectId: activeProjectId, editions, selectedId: edition.id });
      rememberSelectedEdition(author?.email ?? "", activeProjectId, edition.id);
    } finally { editionOpening.current = false; if (ticket === editionNavigationEpoch.current) setProjectLoading(false); }
  }
  function selectEdition(edition: CreativeEdition) { if (!executionBusy && !projectLoading && edition.id !== editionCatalog.selectedId) requestNavigation({ kind: "edition", edition }); }
  async function createProject(title: string, logline: string, medium: CreativeMedium, idempotencyKey: string) {
    if (!api) throw new Error("桌面连接未加载，请重新打开应用。");
    const next = await api.runtime.createProject(title, logline, medium, idempotencyKey);
    setProjects((items) => [next.project, ...items.filter((project) => project.id !== next.project.id)]);
    const edition = next.editions.find(item => item.id === next.activeEditionId);
    if (!edition || edition.medium !== medium) throw new Error("作品创建回执未包含所选作品形式");
    setEditionCatalog({ projectId: next.project.id, editions: next.editions, selectedId: edition.id });
    rememberSelectedEdition(author?.email ?? "", next.project.id, edition.id);
    if (medium === "novel") {
      const projection = await api.runtime.getState(next.project.id);
      setState(projection); setMediaContext(null);
      setSelectedChapterId(projection.novelEdition.chapters[0]?.id ?? "");
    } else setMediaContext({ project: next.project, edition });
    setActiveProjectId(next.project.id);
    resetProjectSession();
    setActivity((items) => [`作品“${next.project.title}”已创建。`, ...items].slice(0, 4));
  }
  useEffect(() => {
    if (!api || !conversationProjectReady) return;
    let cancelled = false;
    void api.runtime.getState(projectId).then(next => {
      if (!cancelled && currentProjectRef.current === projectId && next.project.id === projectId) { setState(next); setFormalReadError(null); }
    }).catch(() => { if (!cancelled && currentProjectRef.current === projectId) setFormalReadError({ projectId, message: "章节目录尚未刷新，已保存内容仍保留。" }); });
    return () => { cancelled = true; };
  }, [api, projectId, conversationProjectReady, formalStateRefreshToken]);
  async function refreshFormalState(canApply?: () => boolean) {
    if (!api) return;
    const next = await api.runtime.getState(projectId);
    if (sharedStateOwnerRef.current !== sharedStateOwner || currentProjectRef.current !== projectId || (canApply && !canApply())) return;
    setState(next);
    const chapter = next.novelEdition.chapters.find((item) => item.id === selectedChapterId) ?? next.novelEdition.chapters[0];
    setSelectedChapterId(chapter?.id ?? "");
    bumpCreativeRefreshToken((value) => value + 1);
  }
  async function refreshDiscussionWorkspace() {
    if (!api || discussionSyncRef.current || creativeDirtyRef.current || promptSubmissionRef.current) return;
    const operation = {};
    const scope = promptScopeRef.current;
    const isCurrent = () => promptScopeRef.current === scope && currentProjectRef.current === projectId;
    discussionSyncRef.current = operation;
    deferredDiscussionSync.current = null;
    setDiscussionSyncBusy(true);
    try {
      await refreshFormalState(() => isCurrent() && !creativeDirtyRef.current);
      if (isCurrent()) {
        deferredDiscussionSync.current = creativeDirtyRef.current ? { api, projectId, scope } : null;
        setDiscussionSyncNotice(creativeDirtyRef.current ? "中央文档有未保存修改，保存或放弃修改后会自动同步工作区。" : null);
      }
    } catch (error) {
      if (isCurrent()) setDiscussionSyncNotice(`回复已完成；工作区读取失败：${error instanceof Error ? error.message : "未知错误"}。重新读取不会再次调用模型。`);
    } finally {
      if (discussionSyncRef.current === operation) {
        discussionSyncRef.current = null;
        setDiscussionSyncBusy(false);
      }
    }
  }
  async function syncExecutionWorkspace(label: string, scope = promptScopeRef.current) {
    const isCurrent = () => promptScopeRef.current === scope && currentProjectRef.current === projectId;
    if (!isCurrent()) return;
    try {
      if (!creativeDirtyRef.current) await refreshFormalState(() => isCurrent() && !creativeDirtyRef.current);
      if (isCurrent()) {
        deferredDiscussionSync.current = creativeDirtyRef.current ? { api, projectId, scope } : null;
        setDiscussionSyncNotice(creativeDirtyRef.current ? `${label}；中央文档有未保存修改，保存或放弃修改后会自动同步工作区。` : null);
      }
    } catch (error) {
      if (isCurrent()) setDiscussionSyncNotice(`${label}；工作区读取失败：${error instanceof Error ? error.message : "未知错误"}。重新读取不会再次调用模型。`);
    }
    if (isCurrent()) void refreshThreads(false, false);
  }
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
      || creativeDirtyRef.current && !(expected?.execution.kind === "discussion" && expected.execution.conversationFocus?.readOnly)) return;
    if (expected && (executionRetryRef.current !== expected || expected.scope !== promptScopeRef.current
      || expected.projectId !== projectId || expected.history !== history)) return;
    if (expected?.execution.kind === "discussion") {
      void submitPrompt(expected.execution.prompt, expected.execution.conversationFocus ?? null, expected.execution.threadId ?? null);
      return;
    }

  }

  async function saveAndContinueNavigation() {
    const handler = creativeSaveHandlerRef.current;
    if (!pendingNavigation || !handler || savingBeforeLeave) return;
    setSavingBeforeLeave(true);
    try {
      if (!(await handler())) return;
      const action = pendingNavigation;
      setCreativeDirty(false);
      setPendingNavigation(null);
      applyNavigation(action);
    } finally { setSavingBeforeLeave(false); }
  }

  function discardAndContinueNavigation() {
    if (!pendingNavigation) return;
    const action = pendingNavigation;
    const discard = creativeDiscardHandlerRef.current;
    if (!discard || !discard()) return;
    setCreativeDirty(false);
    setPendingNavigation(null);
    applyNavigation(action);
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
  async function discussSavedResult(request: TaskResultDiscussion) {
    const { result } = request;
    if (!api || !conversationProjectReady || executionBusy || conversationChanging || selectedThread?.isArchived || result.projectId !== projectId) return;
    if (currentChapterWriting || revisionRepair.prepared) {
      setTaskResultError("输入框已有待发送的创作来源，请先发送或取消该指令，再引用成果。");
      return;
    }
    const ticket = ++resultDiscussionRequest.current, navigation = resultNavigation.current;
    const threadId = selectedConversation.id, version = composerDrafts.version(), scope = promptScopeRef.current;
    const current = () => ticket === resultDiscussionRequest.current && navigation === resultNavigation.current
      && currentProjectRef.current === projectId && conversationSelectionRef.current.id === threadId
      && promptScopeRef.current === scope && composerDraftsRef.current.version() === version
      && resultDiscussionControls.current.api === api && !resultDiscussionControls.current.executionBusy
      && !resultDiscussionControls.current.conversationChanging && !resultDiscussionControls.current.archived
      && !resultDiscussionControls.current.writing && !resultDiscussionControls.current.repair && !promptSubmissionRef.current;
    try {
      const excerpt = await taskResultDiscussionExcerpt(api, request);
      if (!current()) return;
      if (composerDrafts.addExcerpt(excerpt)) {
        setTaskResultError(null);
        composerRef.current?.focus({ preventScroll: true });
      }
    } catch (error) { if (current()) setTaskResultError(error instanceof Error ? error.message : "成果引用失败，请重试。"); }
  }
  async function openBranchSource(id: string) {
    const originalScope = conversationSelectionRef.current.id;
    try {
      if (!api) return;
      const detail = await api.agent.readConversation(projectId, id);
      if (currentProjectRef.current !== projectId || conversationSelectionRef.current.id !== originalScope) return;
      if (detail.thread.id !== id || detail.thread.projectId !== projectId) throw Error("原对话来源不匹配");
      await selectConversation(detail.thread);
    } catch { setActivity(items => ["原对话暂时无法读取，请稍后重试。", ...items].slice(0, 4)); }
  }
  async function queuePrompt(kind: "afterWork" | "afterTurn" = "afterWork") {
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
    const trigger: import("./types").MessageTrigger = kind === "afterTurn"
      ? {kind, taskId: agentRun.taskId!, turnId: agentRun.turnId!, controlRevision: work.controlRevision}
      : {kind, workTaskId: work.id, controlRevision: work.controlRevision};
    if (await outbox.add(composer, focus, trigger)) acceptedDraft();
  }
  async function submitPrompt(requestedPrompt?: string, requestedFocus?: ConversationFocus | null, requestedThreadId?: string | null) {
    if (conversationChanging || activeConversationDetail?.loading || selectedThread?.isArchived || executionBusy && !viewingExecution) return;
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
    if (creativeDirty && !excerptDiscussion && (!writing || agentRun.state === "running")) {
      if (requestedPrompt !== undefined || requestedFocus !== undefined || !planningSubmissionEligible) {
        setActivity((items) => ["请先保存当前正文、事实表单或审阅决定，再启动或追加 Agent 任务。", ...items].slice(0, 4));
        return;
      }
      const save = planningSaveHandlerRef.current;
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
    setProposedActions((current) => current.projectId === projectId ? current : { projectId, actions: [] });
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
      setProposedActions({ projectId, actions: latest.actions });
      const proposal = result.generationHandoff;
      const sourceTask = eventRecord(result.workflow?.task);
      setGenerationHandoff(!result.executionFailure && !repairSource && !advisorySteeredRef.current && proposal
        && !proposal.resumeRun && proposal.projectId === projectId && proposal.state === "proposed" && proposal.authorIntent === prompt
        && proposal.sourceTaskId === sourceTask?.id && proposal.threadId === sourceTask.threadId
        ? { proposal, scope: promptScope } : null);
      setActivity((items) => [`${result.executionFailure ? "执行状态" : "创作引擎"}：${result.message}`, ...items].slice(0, 4));
      // A read failure after a successful turn must not offer a paid model retry.
      try {
        if (!creativeDirtyRef.current) await refreshFormalState(() => promptIsCurrent() && !creativeDirtyRef.current);
        if (!promptIsCurrent()) return;
        deferredDiscussionSync.current = creativeDirtyRef.current ? { api, projectId, scope: promptScope } : null;
        setDiscussionSyncNotice(creativeDirtyRef.current ? "回复已完成；中央文档有未保存修改，保存或放弃修改后会自动同步工作区。" : null);
      } catch (error) {
        if (!promptIsCurrent()) return;
        setDiscussionSyncNotice(`回复已完成；工作区读取失败：${error instanceof Error ? error.message : "未知错误"}。重新读取不会再次调用模型。`);
      }
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
        if (saved?.scope === promptScope) saved.run = failedRun(saved.run, error instanceof Error ? error.message : "执行未完成");
        return;
      }
      const message = error instanceof Error ? error.message : "未知错误";
      if (isModelConnectionUnavailable(message)) {
        setAgentStatus(status => ({ ...status, configured: false, state: "error", error: message }));
        if (requestedPrompt === undefined && !agentRunRef.current.taskId && !agentRunRef.current.turnId
          && composerDrafts.version() === submittedDraftVersion) {
          composerDrafts.setValue(composer);
          composerDrafts.setSource(composerDrafts.source);
          selectedExcerpts.forEach(item => composerDrafts.addExcerpt(item));
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
      if (!replyReceived && promptIsCurrent() && !creativeDirtyRef.current) setCreativeRefreshToken(value => value + 1);
    }
  }
  async function approveProposedAction(action: ProposedAction) {
    if (!api || approvingActionId || executionBusy || creativeDirty) return;
    setApprovingActionId(action.id);
    const executionScope = beginExecutionScope({ kind: "taskRecovery", operation: action.operation, view: action.operation === "generateBrief" ? "brief" : action.operation === "generateOutline" ? "outline" : action.operation === "generatePlan" ? "plans" : action.operation === "generatePropertyRules" ? "world" : "chapters", intent: action.instruction });
    const executionIsCurrent = () => promptScopeRef.current === executionScope && currentProjectRef.current === projectId;
    const previous = summarizeAgentRun(agentRun);
    const history = previous ? [previous, ...agentRun.history] : agentRun.history;
    setAgentRun(createAgentRun(`批准候选修改：${action.instruction}`, history));
    try {
      if (!executionIsCurrent()) return;
      const result = await api.agent.approveAction(projectId, action.sourceTaskId, action.id);
      if (!executionIsCurrent()) return;
      const hasSavedResults = Boolean(result.candidateResults?.length);
      const outcome = result.executionFailure
        ? hasSavedResults ? "已有候选结果保留，后续处理尚未完成。" : "本轮处理尚未完成，请查看对话说明。"
        : hasSavedResults ? "本轮候选处理已完成，尚未正式采用。" : "本轮处理已结束，请查看对话说明。";
      setActivity(items => [outcome, ...items].slice(0, 4));
      await syncExecutionWorkspace(outcome, executionScope);
      if (!executionIsCurrent()) return;
      if (result.executionFailure) {
        await reportExecutionFailure(result.executionFailure.message, executionScope, "候选后续处理未完成");
      } else {
        setAgentRun(run => run.state === "running" || run.state === "waiting"
          ? { ...run, state: "completed", completedAt: Date.now(), summary: outcome, error: null } : run);
      }
    } catch (error) {
      if (!executionIsCurrent()) return;
      const message = error instanceof Error ? error.message : "未知错误";
      await reportExecutionFailure(message, executionScope, "候选执行失败");
    } finally {
      const refreshed = await api.agent.listProposedActions(projectId).catch(() => null);
      if (executionIsCurrent()) {
        if (refreshed) setProposedActions({ projectId, actions: refreshed.actions });
        setApprovingActionId(null);
      }
    }
  }
  async function scopeExecutionFinished(result: ExecutionScopeConfirmation) {
    const execution = scopeExecutionRef.current;
    const isCurrent = () => execution?.projectId === projectId && execution.scope === promptScopeRef.current && currentProjectRef.current === projectId;
    if (!isCurrent()) return;
    const outcome = !result.work ? "范围已保存；执行结果待核对，请查看创作任务。"
      : result.work.status === "completed" ? "本轮工作步骤已结束，候选仍待审阅，未正式采用。"
      : `范围已保存；创作任务当前为 ${result.work.status}，尚未完成。`;
    setActivity(items => [outcome, ...items].slice(0, 4));
    setAgentRun(run => run.state !== "running" && run.state !== "waiting" ? run : {
      ...run, state: result.work?.status === "completed" ? "completed" : ["paused", "cancelled"].includes(result.work?.status ?? "") ? "cancelled" : "error",
      summary: outcome, completedAt: Date.now(), error: result.work?.status === "completed" || result.work?.status === "paused" || result.work?.status === "cancelled" ? null : outcome,
    });
    const latest = await api?.agent.listProposedActions(projectId).catch(() => null);
    if (!isCurrent()) return;
    if (latest) setProposedActions({ projectId, actions: latest.actions });
    await syncExecutionWorkspace(outcome, execution!.scope);
  }
  async function declineProposedAction(action: ProposedAction) {
    if (!api || approvingActionId || executionBusy) return;
    setApprovingActionId(action.id);
    try {
      await api.agent.declineAction(projectId, action.sourceTaskId, action.id);
      setActivity((items) => [`已拒绝候选修改：${action.instruction}。不会创建创作任务。`, ...items].slice(0, 4));
    } catch (error) {
      const message = error instanceof Error ? error.message : "未知错误";
      setActivity((items) => [`拒绝候选失败：${message}`, ...items].slice(0, 4));
    } finally {
      const refreshed = await api.agent.listProposedActions(projectId).catch(() => null);
      if (refreshed) setProposedActions({ projectId, actions: refreshed.actions });
      setApprovingActionId(null);
    }
  }
  async function generateChapter(chapter: Chapter, intent: string, options: { povCharacterId: string | null; storyTime: number | null }) {
    if (agentRunRef.current.state === "running" || agentRunRef.current.state === "waiting") throw new Error("当前已有 Agent 任务正在执行");
    const prompt = `生成第 ${chapter.number} 章：${intent}`;
    const executionScope = beginExecutionScope({ kind: "taskRecovery", operation: "章节生成", view: "chapters", chapterId: chapter.id, intent });
    const executionIsCurrent = () => promptScopeRef.current === executionScope && currentProjectRef.current === projectId;
    const previous = summarizeAgentRun(agentRun);
    const history = previous ? [previous, ...agentRun.history] : agentRun.history;
    setUserInputQueue([]);
    setAgentRun(createAgentRun(prompt, history));
    setActivity((items) => [`开始生成第 ${chapter.number} 章：${chapter.title}`, ...items].slice(0, 4));
    try {
      if (!executionIsCurrent()) return;
      if (!api) throw new Error("桌面 Agent 未连接，当前只能手动编辑");
      const result = await api.agent.generateChapter(projectId, {
        chapterId: chapter.id,
        chapterNumber: chapter.number,
        povCharacterId: options.povCharacterId,
        storyTime: options.storyTime,
        intent,
      });
      if (!executionIsCurrent()) return;
      setActivity((items) => [`第 ${chapter.number} 章候选已保存，等待审阅。`, `创作引擎：${result.message}`, ...items].slice(0, 4));
      await syncExecutionWorkspace("章节候选已保存", executionScope);
    } catch (error) {
      if (!executionIsCurrent()) return;
      const message = error instanceof Error ? error.message : "未知错误";
      const failed = await reportExecutionFailure(message, executionScope, "章节生成未完成");
      await syncExecutionWorkspace("章节执行已返回", executionScope);
      if (!failed && executionIsCurrent() && agentRunRef.current.state === "cancelled") throw new StoppedExecutionError();
      if (failed) throw error;
    }
  }
  async function generateWork(intent: string, targetChapters: number, expectedProjectRevision: number,
    expectedBaseSnapshotId: string, expectedSourceDigest: string, revisionPredecessor: RevisionPredecessor | null, generationProposalId: string | null = null) {
    if (creativeDirtyRef.current || promptSubmissionRef.current || agentRunRef.current.state === "running" || agentRunRef.current.state === "waiting") throw new Error("请先保存当前修改并等待正在执行的任务结束");
    const handoff = generationHandoffRef.current;
    if (generationProposalId && (!handoff || handoff.scope !== promptScopeRef.current
      || handoff.proposal.proposalId !== generationProposalId || handoff.proposal.projectId !== projectId
      || handoff.proposal.authorIntent !== intent || handoff.proposal.targetChapters !== targetChapters
      || revisionPredecessor)) throw new Error("创作安排已变化，请从当前对话重新核对。");
    const executionScope = beginExecutionScope({ kind: "taskRecovery", operation: "持续创作", view: "chapters", intent });
    const executionIsCurrent = () => promptScopeRef.current === executionScope && currentProjectRef.current === projectId;
    const originalRun = agentRunRef.current;
    let hostNotStarted = false;
    const startingRun = createAgentRun(intent, summarizeAgentRun(originalRun) ? [summarizeAgentRun(originalRun)!, ...originalRun.history] : originalRun.history);
    promptSubmissionRef.current = executionScope;
    setPromptPipeline({ projectId, scope: executionScope });
    conversationExecutionRef.current = handoff && generationProposalId ? { api, scope: executionScope,
      owner: createConversationExecutionOwner(projectId, { taskId: handoff.proposal.sourceTaskId, threadId: handoff.proposal.threadId }),
      pendingStart: { goal: intent, originalRun } } : null;
    if (generationProposalId) setGenerationHandoff(null);
    setUserInputQueue([]);
    if (!generationProposalId) setAgentRun(startingRun);
    setActivity((items) => [`开始创作 ${targetChapters} 章工作稿`, ...items].slice(0, 4));
    try {
      if (!executionIsCurrent()) return;
      if (!api) throw new Error("桌面 Agent 未连接，无法自动创作");
      const run = await api.agent.generateWork(projectId, intent, targetChapters,
        expectedProjectRevision, expectedBaseSnapshotId, expectedSourceDigest, revisionPredecessor, generationProposalId);
      if (!executionIsCurrent()) return;
      if (run.resumeFailure) {
        const proof = run.resumeFailure, paused = run.pausedWork;
        if (!run.id || run.status !== "paused" || proof.code !== "WORK_RESUME_NOT_STARTED"
          || proof.workTaskId !== `work-${run.id}` || typeof proof.message !== "string" || !proof.message.trim()
          || !paused || paused.id !== proof.workTaskId || paused.projectId !== projectId || paused.status !== "paused"
          || !Number.isInteger(paused.version) || paused.version < 0
          || (generationProposalId && paused.threadId !== handoff?.proposal.threadId)
          || (generationProposalId ? !conversationExecutionRef.current?.pendingStart : agentRunRef.current !== startingRun)) {
          throw new Error("新创作安排的启动结果未能核对，请重新读取任务。");
        }
        hostNotStarted = true;
        setAgentRun(originalRun);
        setTaskControlMessage({ scope: taskList.scope, error: { targetId: paused.id, message: `原任务尚未启动：${proof.message}` }, notice: null });
        throw new Error(`原任务尚未启动：${proof.message}`);
      }
      setGenerationHandoff((current) => current?.proposal.projectId === projectId ? null : current);
      setActivity((items) => [run.status === "completed" ? `${targetChapters} 章工作稿已生成` : run.status === "paused" ? "创作已暂停" : workStopDescription(run.blockedReason), ...items].slice(0, 4));
      await syncExecutionWorkspace("创作执行已返回，结果已保留", executionScope);
    } catch (error) {
      if (!executionIsCurrent()) return;
      const message = error instanceof Error ? error.message : "未知错误";
      if (hostNotStarted) {
        setActivity(items => [message, ...items].slice(0, 4));
        await syncExecutionWorkspace("创作任务未启动，原对话和已保存内容保留", executionScope);
        return;
      }
      const failed = await reportExecutionFailure(message, executionScope, "自动创作未完成");
      await syncExecutionWorkspace("创作执行已返回，可能有部分候选", executionScope);
      if (!failed && executionIsCurrent() && agentRunRef.current.state === "cancelled") throw new StoppedExecutionError();
      if (failed) throw error;
    } finally {
      if (promptSubmissionRef.current === executionScope) promptSubmissionRef.current = null;
      setPromptPipeline(current => current?.scope === executionScope ? null : current);
      if (conversationExecutionRef.current?.scope === executionScope) conversationExecutionRef.current = null;
    }
  }
  async function continueWork(runId: string, action: "continue" | "retry" | "feedbackContinue" | "feedbackRepair" = "continue", assessmentRevisionId = "", instruction = "") {
    if (creativeDirtyRef.current || promptSubmissionRef.current || agentRunRef.current.state === "running" || agentRunRef.current.state === "waiting") throw new Error("请先保存当前修改并等待正在执行的任务结束");
    const executionScope = beginExecutionScope({ kind: "taskRecovery", operation: "继续创作", view: "chapters", intent: instruction });
    const executionIsCurrent = () => promptScopeRef.current === executionScope && currentProjectRef.current === projectId;
    const originalRun = agentRunRef.current;
    let hostNotStarted = false;
    promptSubmissionRef.current = executionScope;
    setPromptPipeline({ projectId, scope: executionScope });
    setUserInputQueue([]);
    setActivity((items) => [action === "retry" ? "重试失败的创作步骤" : "继续已有创作运行", ...items].slice(0, 4));
    try {
      if (!executionIsCurrent()) return;
      if (!api) throw new Error("桌面 Agent 未连接，无法继续创作");
      const work = await api.runtime.getWorkTask(projectId, `work-${runId}`);
      if (!executionIsCurrent()) return;
      const owner = work.projectId === projectId && work.id === `work-${runId}` && work.kind === "generation"
        ? createRecoveredExecutionOwner(work) : null;
      if (!owner) throw new Error("原创作任务的来源未能核对，尚未继续执行。");
      conversationExecutionRef.current = { api, scope: executionScope, owner, pendingStart: { goal: work.goal, originalRun } };
      const run = action === "retry" ? await api.agent.retryWork(projectId, runId)
        : action === "feedbackContinue" || action === "feedbackRepair"
          ? await api.agent.resolveGenerationFeedback(projectId, runId, assessmentRevisionId, action === "feedbackRepair" ? "repairDraft" : "continue", instruction)
          : await api.agent.continueWork(projectId, runId);
      if (!executionIsCurrent()) return;
      if (run.resumeFailure) {
        const proof = run.resumeFailure, paused = run.pausedWork;
        if (run.id !== runId || run.status !== "paused" || proof.code !== "WORK_RESUME_NOT_STARTED"
          || proof.workTaskId !== work.id || typeof proof.message !== "string" || !proof.message.trim() || !paused || paused.id !== work.id
          || paused.projectId !== projectId || paused.threadId !== work.threadId || paused.status !== "paused"
          || !Number.isInteger(paused.version) || paused.version < work.version || !conversationExecutionRef.current?.pendingStart) {
          throw new Error("原创作任务恢复结果的来源无法核对，请重新读取任务。");
        }
        hostNotStarted = true;
        throw new Error(`原任务尚未启动：${proof.message}`);
      }
      setActivity((items) => [run.status === "completed" ? "自动创作已完成" : run.status === "running" ? "创作进行中" : run.status === "paused" ? "创作已暂停" : workStopDescription(run.blockedReason), ...items].slice(0, 4));
      await syncExecutionWorkspace("继续创作已返回，结果已保留", executionScope);
    } catch (error) {
      if (!executionIsCurrent()) return;
      const message = error instanceof Error ? error.message : "未知错误";
      if (hostNotStarted) {
        setTaskControlMessage({ scope: taskList.scope, error: { targetId: `work-${runId}`, message }, notice: null });
        setActivity(items => [message, ...items].slice(0, 4));
        await syncExecutionWorkspace("原任务未启动，已保存内容保留", executionScope);
        return;
      }
      const failed = await reportExecutionFailure(message, executionScope, "继续创作失败");
      await syncExecutionWorkspace("继续创作已返回，可能有部分候选", executionScope);
      if (!failed && executionIsCurrent() && agentRunRef.current.state === "cancelled") throw new StoppedExecutionError();
      if (failed) throw error;
    } finally {
      if (promptSubmissionRef.current === executionScope) promptSubmissionRef.current = null;
      setPromptPipeline(current => current?.scope === executionScope ? null : current);
      if (conversationExecutionRef.current?.scope === executionScope) conversationExecutionRef.current = null;
    }
  }
  async function reviewPlanning(request: PlanningReviewStartRequest) {
    if (creativeDirtyRef.current || promptSubmissionRef.current || agentRunRef.current.state === "running" || agentRunRef.current.state === "waiting") throw new Error("请先保存当前修改并等待正在执行的任务结束");
    const prompt = `规划深审：${request.authorIntent}`;
    const executionScope = beginExecutionScope({ kind: "taskRecovery", operation: "规划深审", view: request.artifactKind === "plan" ? "plans" : "outline", intent: request.authorIntent });
    const executionIsCurrent = () => promptScopeRef.current === executionScope && currentProjectRef.current === projectId;
    const previous = summarizeAgentRun(agentRun);
    const history = previous ? [previous, ...agentRun.history] : agentRun.history;
    setUserInputQueue([]);
    setAgentRun(createAgentRun(prompt, history));
    setActivity((items) => [`开始规划深审 · ${request.roles.length} 席`, ...items].slice(0, 4));
    try {
      if (!executionIsCurrent()) return;
      if (!api) throw new Error("桌面 Agent 未连接，无法启动规划深审");
      const result = await api.agent.reviewPlanning(projectId, request);
      if (!executionIsCurrent()) return;
      const resultLabel = result.run.status === "completed" ? "全部完成" : result.run.status === "partial" ? "部分完成" : "已停止";
      setActivity((items) => [`规划深审${resultLabel}，已保存 ${result.run.reports.length}/${result.run.roles.length} 席报告。`, ...items].slice(0, 4));
      await syncExecutionWorkspace(`规划深审${resultLabel}`, executionScope);
    } catch (error) {
      if (!executionIsCurrent()) return;
      const message = error instanceof Error ? error.message : "未知错误";
      const failed = await reportExecutionFailure(message, executionScope, "规划深审未完整结束");
      await syncExecutionWorkspace("规划深审已返回，已保存报告可重新读取", executionScope);
      if (!failed && executionIsCurrent() && agentRunRef.current.state === "cancelled") throw new StoppedExecutionError();
      if (failed) throw error;
    }
  }
  async function cancelPlanningReview(runId: string) {
    const scope = promptScopeRef.current;
    if (!api) throw new Error("Runtime 尚未连接，无法停止规划深审");
    const run = await api.runtime.cancelPlanningReview(projectId, runId);
    if (promptScopeRef.current !== scope || currentProjectRef.current !== projectId) return;
    setAgentRun(stoppedRun);
    setActivity((items) => [`规划深审已停止，已保存 ${run.reports.length}/${run.roles.length} 席报告。`, ...items].slice(0, 4));
    await syncExecutionWorkspace("规划深审已停止", scope);
  }
  async function refreshThreads(archived = threadArchived, clearAction = true) {
    if (!api || !conversationProjectReady) return;
    const ticket = {};
    threadListRef.current = ticket;
    const isCurrent = () => threadListRef.current === ticket && currentProjectRef.current === projectId;
    setThreadLoading(true);
    if (clearAction) setThreadAction(null);
    try { const result = await api.agent.listThreads(projectId, archived); if (isCurrent()) { setThreads(result.threads); } } catch { if (isCurrent()) setThreadAction("任务历史暂不可用"); } finally { if (isCurrent()) setThreadLoading(false); }
  }
  async function inspectRecovery(run: RecoveryRun) {
    if (!api || run.projectId !== projectId || executionBusy) throw new Error("当前无法读取此任务，请稍后重试");
    return api.agent.readThread(run.projectId, run.threadId);
  }
  async function reconcileRecovery(run: RecoveryRun) {
    if (!api || run.projectId !== projectId || executionBusy) throw new Error("当前无法核对此任务，请稍后重试");
    const reports = (await api.agent.recoveryReport()).filter(authorFacingRecoveryRun);
    if (currentProjectRef.current !== run.projectId) return false;
    setRecoveryRuns(reports);
    await taskList.refresh();
    return !reports.some(entry => entry.runId === run.runId && entry.projectId === run.projectId);
  }
  async function toggleThreadPin(thread: ThreadSummary) {
    if (!api || !agentStatus.configured) return;
    const nextPinned = !thread.isPinned;
    setThreadAction(null);
    try {
      await api.agent.setThreadPinned(projectId, thread.id, nextPinned);
      await refreshThreads(threadArchived, false);
      setThreadAction(null);
    } catch (error) {
      setThreadAction(`更新置顶状态失败：${error instanceof Error ? error.message : "未知错误"}`);
    }
  }
  async function toggleThreadArchive(thread: ThreadSummary) {
    if (!api || !agentStatus.configured) return;
    if (conversationRuns.current.get(thread.id)?.submitting || ["running", "waiting"].includes(conversationRuns.current.get(thread.id)?.run.state ?? "")) {
      setThreadAction("此对话正在执行，完成或停止后才能归档。");
      return;
    }
    const restoring = threadArchived;
    setThreadAction(null);
    try {
      if (restoring) {
        await api.agent.unarchiveThread(projectId, thread.id);
      } else {
        await api.agent.archiveThread(projectId, thread.id);
        if (thread.id === selectedConversation.id) selectConversation(null);
      }
      await refreshThreads(threadArchived, false);
      setThreadAction(null);
    } catch (error) {
      setThreadAction(`${restoring ? "恢复" : "归档"}失败：${error instanceof Error ? error.message : "未知错误"}`);
    }
  }
  function toggleThreadArchiveView() { setThreadAction(null); setThreadArchived((archived) => !archived); }
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
        return { ...run, state: effectiveState, connectionState: "ready", error: result.recovery?.state === "error" ? run.error : null,
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
  function reviewAgent() {
    setAccountOpen(false);
    navigateView("chapters");
  }
  function configureAgent(provider: string, model: string) { return modelConnection.run(provider, model, "apply", provider === agentStatus.provider && model === agentStatus.model ? agentStatus.reasoningEffort ?? null : null); }
  function verifyAgent(provider: string, model: string) { return modelConnection.run(provider, model, "verify", provider === agentStatus.provider && model === agentStatus.model ? agentStatus.reasoningEffort ?? null : null); }
  function selectAgentConnection(connection: AgentConnection) {
    return modelConnection.run(connection.provider, connection.model, connection.status === "verified" ? "apply" : "verifyAndApply", connection.reasoning?.effort ?? null);
  }
  async function loadBackgroundTerminals() {
    if (!api || !agentRun.threadId) return;
    setBackgroundTerminalLoading(true);
    try { setBackgroundTerminals((await api.agent.listBackgroundTerminals(projectId, agentRun.threadId)).terminals); } catch (error) { setActivity((items) => [`后台终端读取失败：${error instanceof Error ? error.message : "未知错误"}`, ...items].slice(0, 4)); } finally { setBackgroundTerminalLoading(false); }
  }
  async function terminateBackgroundTerminal(processId: string) {
    if (!api || !agentRun.threadId) return;
    try { await api.agent.terminateBackgroundTerminal(projectId, agentRun.threadId, processId); setBackgroundTerminals((terminals) => terminals.filter((terminal) => terminal.processId !== processId)); setActivity((items) => [`后台终端 ${processId} 已终止。`, ...items].slice(0, 4)); } catch (error) { setActivity((items) => [`终止后台终端失败：${error instanceof Error ? error.message : "未知错误"}`, ...items].slice(0, 4)); }
  }
  async function resolveUserInput(response: UserInputResponse): Promise<boolean> { const current = userInputQueue[0]; if (!current || !api) return false; try { await api.agent.resolveUserInput({ projectId: current.projectId, executionId: current.executionId, requestId: current.requestId }, response); setUserInputQueue((queue) => queue.filter((request) => request.executionId !== current.executionId || String(request.requestId) !== String(current.requestId))); setActivity((items) => ["已提交作者回答。", ...items].slice(0, 4)); return true; } catch (error) { setActivity((items) => [`作者回答提交失败：${error instanceof Error ? error.message : "未知错误"}`, ...items].slice(0, 4)); return false; } }
  function prepareConversation(prompt: string, source?: DiscussionSource) {
    if (!conversationProjectReady || executionBusy || source && source.focus.reference.projectId !== projectId) return;
    if (source && (currentChapterWriting || revisionRepair.prepared)) {
      setActivity(items => ["输入框已有待发送的创作来源，请先发送或取消该指令，再引用内容。", ...items].slice(0, 4));
      return;
    }
    if (source && composerDrafts.source && JSON.stringify(source.focus) !== JSON.stringify(composerDrafts.source.focus)) {
      setPendingDiscussionSource(source); composerRef.current?.focus({ preventScroll: true }); return;
    }
    if (!source) { selectChapterWriting(null); revisionRepair.cancel(); setSentRevisionRepair(null); }
    const removedSource = !source && Boolean(composerDrafts.source);
    composerDrafts.setSource(source ?? null); setPendingDiscussionSource(null);
    if (composer.trim()) setActivity(items => [source ? "已附加讨论来源，保留你正在输入的文字。" : removedSource ? "已切换为当前页面讨论，移除原精确来源；输入文字已保留。" : "已保留你正在输入的内容；可在当前对话中补充要处理的事项。", ...items].slice(0, 4));
    else if (!source) { setComposer(prompt); if (removedSource) setActivity(items => ["已切换为当前页面讨论，移除原精确来源。", ...items].slice(0, 4)); }
    composerRef.current?.focus({ preventScroll: true });
  }
  function addDiscussionExcerpt(excerpt: DiscussionExcerpt) {
    if (!conversationProjectReady || executionBusy || excerpt.projectId !== projectId) return false;
    if (currentChapterWriting || revisionRepair.prepared) {
      setActivity(items => ["输入框已有待发送的创作来源，请先发送或取消该指令，再引用内容。", ...items].slice(0, 4));
      return false;
    }
    if (!composerDrafts.addExcerpt(excerpt)) return false;
    requestAnimationFrame(() => composerRef.current?.focus({ preventScroll: true }));
    return true;
  }
  function openFactReview() { prepareConversation("请核对当前作品的故事设定，说明有来源的事实、候选解释和需要我决定的冲突，先不要改写或发布。"); }

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.shiftKey && event.key.toLowerCase() === "p") { event.preventDefault(); setCommandPaletteOpen((open) => !open); return; }
      if (event.key === "Escape") { setCommandPaletteOpen(false); search.close(); }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [search.close]);

  const connectionState = agentRun.connectionState;
  const renderedRetry = executionRetryRef.current;
  const visibleAgentStatus = { ...agentStatus, state: agentRun.state === "waiting" ? "waiting" : executionBusy ? "running" : connectionState === "reconnecting" ? "reconnecting" : agentRun.state === "error" || connectionState === "error" ? "error" : agentStatus.state };
  async function signOut() {
    await authorSession.signOut();
    composerDrafts.clear(); setPendingDiscussionSource(null);
    setAccountOpen(false);
  }
  async function deleteAccount() {
    const deletedAuthorKey = author?.email;
    await authorSession.deleteAccount();
    if (deletedAuthorKey) clearAuthorDrafts(deletedAuthorKey);
    composerDrafts.clear(); setPendingDiscussionSource(null);
    setAccountOpen(false);
  }
  async function saveProfile(next: Pick<AuthorProfile, "name" | "email">) {
    if (!author) return "作者资料不存在。";
    if (!next.name) return "请填写笔名。";
    if (!/^\S+@\S+\.\S+$/.test(next.email)) return "请填写有效邮箱。";
    const profile = { ...author, name: next.name, email: next.email };
    await authorSession.signIn(profile);
    return null;
  }
  async function changePassword(current: string, next: string) {
    if (!author) return "作者资料不存在。";
    if (await hashPassword(current, author.passwordSalt) !== author.passwordHash) return "当前密码不正确。";
    const passwordSalt = crypto.randomUUID();
    const profile = { ...author, passwordSalt, passwordHash: await hashPassword(next, passwordSalt) };
    await authorSession.signIn(profile);
    return null;
  }
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
    && renderedRetry?.execution.kind === "discussion" && renderedRetry.execution.conversationFocus?.readOnly;
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

  if (authorSession.loading || authorSession.error) {
    return <div className="login-screen"><div className="login-card" role="status">
      <h1>小说工坊</h1>
      <p>{authorSession.loading ? "正在恢复工作区…" : authorSession.error}</p>
      {!authorSession.loading && <button type="button" className="primary-button" onClick={() => void authorSession.restore()}>重试</button>}
    </div></div>;
  }
  if (!signedIn || !author) {
    return <LoginScreen existing={author} theme={theme} onToggleTheme={() => setTheme(theme === "light" ? "dark" : "light")} onSignedIn={authorSession.signIn} />;
  }

  const editionControls = editionCatalog.projectId === activeProjectId && editionCatalog.editions.length > 0 ? <EditionSwitcher editions={editionCatalog.editions} currentEditionId={editionCatalog.selectedId} busy={executionBusy || projectLoading} onSelect={selectEdition} onCreate={() => requestNavigation({ kind: "new-edition" })} /> : null;
  const editionDialog = editionCreateOpen && api ? <CreateMediaEditionDialog key={activeProjectId} api={api.multimedia} projectId={activeProjectId} source={editionSource} onClose={() => setEditionCreateOpen(false)} onCreated={openEdition} /> : null;
  if (editionCatalog.projectId === activeProjectId && !editionCatalog.selectedId && editionCatalog.editions.length > 1) return <div className="app-shell"><header className="topbar"><ProjectSwitcher projects={projects} activeProjectId={activeProjectId} busy={projectLoading} onSelect={selectProject} onCreate={() => requestNavigation({ kind: "new-project" })} /></header><main className="edition-selection-required"><h2>选择要继续的作品版本</h2><p>这部作品包含多个版本，分别保存创作内容与对话。</p>{editionCatalog.editions.map(edition => <button type="button" key={edition.id} className="secondary-button" disabled={projectLoading} onClick={() => selectEdition(edition)}>{edition.title} · {edition.medium === "comic" ? "漫画" : edition.medium === "animation" ? "动画" : "小说"}</button>)}{editionError && <p role="alert">{editionError}</p>}</main>{projectCreateOpen && <ProjectCreateDialog onClose={() => setProjectCreateOpen(false)} onSubmit={createProject} />}</div>;
  if (mediaContext && api && mediaContext.project.id === activeProjectId) return <>
    <MediaApplication key={`${author.email}:${projectId}:${mediaContext.edition.id}`} api={api} edition={mediaContext.edition} authorKey={author.email}
      onEditionSourceChange={setEditionSource} onBusyChange={setMediaBusy} onDirtyChange={setCreativeDirty} onSaveHandlerChange={registerCreativeSaveHandler} onDiscardHandlerChange={registerCreativeDiscardHandler}
      topbar={<header className="topbar"><div className="brand-lockup"><div className="brand-mark">✦</div><span>创作工坊</span></div><ProjectSwitcher projects={projects} activeProjectId={activeProjectId} busy={projectLoading} onSelect={selectProject} onCreate={() => requestNavigation({ kind: "new-project" })} />{editionControls}<button className="icon-button" type="button" aria-label="切换主题" onClick={() => setTheme(value => value === "light" ? "dark" : "light")}>◐</button></header>} />
    {editionDialog}{editionError && <p className="dialog-error" role="alert">{editionError}</p>}
    {projectCreateOpen && <ProjectCreateDialog onClose={() => setProjectCreateOpen(false)} onSubmit={createProject} />}
    {pendingNavigation && <UnsavedChangesDialog saving={savingBeforeLeave} onSave={() => void saveAndContinueNavigation()} onDiscard={discardAndContinueNavigation} onStay={() => { if (pendingNavigation.kind === "close") void api.app.cancelClose(); setPendingNavigation(null); }} />}
  </>;

  const creativeConversationOpen = conversationProjectReady && !accountOpen && (["brief", "outline", "plans", "chapters"] as ViewId[]).includes(activeView);
  const renderCurrentConversation = (run: AgentRun, taskBound: boolean, finalReplyState?: FinalReplyDisplayState) => <EnhancedAgentTranscript projectId={projectId} run={run} taskBound={taskBound} finalReplyState={finalReplyState} userInputRequest={userInputQueue[0] ?? null} queuedUserInputCount={Math.max(0, userInputQueue.length - 1)} onResolveUserInput={resolveUserInput} onReview={() => void reviewAgent()} />;
  const hasSelectedWorkTasks = taskList.tasks.some(task => task.projectId === projectId && task.threadId === (selectedConversation.id ?? ""));
  const showConversationReader = !hasSelectedWorkTasks && (!viewingExecution || !agentRun.prompt && !agentRun.history.length);
  const currentExecutionOwner = conversationExecutionRef.current;
  const sameAuthorSource = currentExecutionOwner && currentExecutionOwner.api === api && currentExecutionOwner.scope === promptScopeRef.current
    && currentExecutionOwner.owner.projectId === projectId && currentExecutionOwner.owner.originTaskId
    && taskList.tasks.filter(task => task.projectId === projectId && task.threadId === currentExecutionOwner.owner.originalThreadId
      && task.steps.some(step => step.attempts.some(attempt => attempt.id === currentExecutionOwner.owner.originTaskId
        && attempt.threadId === currentExecutionOwner.owner.originalThreadId))).length === 1;
  const localTurnIds = new Set([agentRun.turnId, ...agentRun.history.map(turn => turn.turnId)]);
  const conversationTaskIds = new Set([...(viewingExecution ? [agentRun.taskId, ...agentRun.history.map(turn => turn.taskId)] : []),
    ...taskList.tasks.filter(task => task.threadId === selectedConversation.id
      || viewingExecution && task.steps.some(step => step.attempts.some(attempt => attemptContainsRun(attempt, agentRun))))
      .flatMap(task => task.steps.flatMap(step => step.attempts.map(attempt => attempt.id)))]);
  const selectedConversationActions = proposedActions.projectId === projectId
    ? proposedActions.actions.filter(action => conversationTaskIds.has(action.sourceTaskId)) : [];
  const earlierConversationDetail = activeConversationDetail?.detail ? { ...activeConversationDetail.detail,
    turns: activeConversationDetail.detail.turns.filter(turn => (!viewingExecution || !localTurnIds.has(turn.id))
      && !taskList.tasks.some(task => task.threadId === selectedConversation.id
        && task.steps.some(step => step.attempts.some(attempt => attempt.turnId === turn.id)))) } : null;
  const restoredTaskHistory = activeConversationDetail?.detail?.thread.projectId === projectId
    && activeConversationDetail.detail.thread.id === selectedConversation.id
    ? threadHistory(activeConversationDetail.detail).filter(entry => (!viewingExecution || !localTurnIds.has(entry.turnId))
      && taskList.tasks.some(task => task.projectId === projectId && task.threadId === selectedConversation.id
        && task.steps.some(step => step.attempts.some(attempt => attemptContainsRun(attempt, entry))))) : [];
  const conversationRun = viewingExecution ? { ...agentRun, history: [...agentRun.history, ...restoredTaskHistory] }
    : { ...createAgentRun(), history: restoredTaskHistory };
  const statusWorks = taskList.tasks.filter(task => task.projectId === projectId && Boolean(selectedConversation.id) && task.threadId === selectedConversation.id);
  const statusOwnsExecution = viewingExecution || statusWorks.some(work => work.steps.some(step => step.attempts.some(attempt => attemptContainsRun(attempt, agentRun))));
  const conversationContent: WorkConversationContent = { threadId: selectedConversation.id, run: conversationRun,
    renderCurrent: ({ taskBound, finalReplyState }) => <>{renderCurrentConversation({ ...agentRun, history: [] }, taskBound || Boolean(sameAuthorSource), finalReplyState)}
      {agentRun.state === "completed" && agentRun.turnId && agentRun.threadId === selectedConversation.id && <button type="button" className="text-button" disabled={executionBusy || conversationChanging || Boolean(selectedThread?.isArchived)} onClick={() => void createConversation(undefined, agentRun.turnId!)}>从此回合新建分支对话</button>}
      {api && generationHandoff?.scope === promptScopeRef.current && generationHandoff.proposal.projectId === projectId
        && agentRun.state === "completed" && !agentRun.error && <ConversationGenerationHandoff key={generationHandoff.proposal.proposalId}
          api={api} proposal={generationHandoff.proposal} scope={generationHandoff.scope}
          blocked={executionBusy || creativeDirty || !agentAvailable} onCancel={() => {
            if (generationHandoffRef.current !== generationHandoff) return;
            setGenerationHandoff(null); composerRef.current?.focus({ preventScroll: true });
          }} onConfirm={async () => {
            if (generationHandoffRef.current !== generationHandoff || generationHandoff.scope !== promptScopeRef.current
              || currentProjectRef.current !== projectId) return;
            const proposal = generationHandoff.proposal;
            await generateWork(proposal.authorIntent, proposal.targetChapters, proposal.expectedRevision,
              proposal.baseSnapshotId, proposal.sourceDigest, null, proposal.proposalId);
          }} />}</>,
    renderHistory: (entry, { taskBound, finalReplyState }) => <><EarlierTurn projectId={projectId} entry={entry} taskBound={taskBound} finalReplyState={finalReplyState} />{entry.state === "completed" && entry.turnId && entry.threadId === selectedConversation.id && <button type="button" className="text-button" disabled={executionBusy || conversationChanging || Boolean(selectedThread?.isArchived)} onClick={() => void createConversation(undefined, entry.turnId!)}>从此回合新建分支对话</button>}</> };

  return <div className="app-shell"><header className="topbar"><div className="brand-lockup"><div className="brand-mark">✦</div><span>小说工坊</span><span className="beta-pill">本地</span></div><div className="topbar-center"><ProjectSwitcher projects={projects} activeProjectId={activeProjectId} busy={projectLoading} onInformation={conversationProjectReady ? () => setProjectInformationOpen(true) : undefined} onSelect={selectProject} onCreate={() => requestNavigation({ kind: "new-project" })} />{editionControls}<span className="save-state"><span className={`save-dot ${creativeDirty || discussionSyncNotice ? "pending" : ""}`} />{creativeDirty ? "创作修改未保存" : discussionSyncNotice ? "工作区待同步" : "工作区已同步"}</span></div><div className="topbar-actions">{search.open && <input autoFocus className="top-search" aria-label="搜索故事内容" value={search.query} onChange={(event) => search.setQuery(event.target.value)} onKeyDown={(event) => { if (event.key === "Enter" && !event.nativeEvent.isComposing) { event.preventDefault(); void search.submit(); } }} placeholder="搜索角色、场景、章节" />}<button className="icon-button" title="搜索" onClick={search.toggle}>⌕</button><button className="icon-button" title="命令面板 ⌘⇧P" onClick={() => setCommandPaletteOpen(true)}>⌘</button><button className="icon-button theme-toggle" title={theme === "light" ? "切换深色主题" : "切换浅色主题"} aria-label={theme === "light" ? "切换深色主题" : "切换浅色主题"} onClick={() => setTheme(theme === "light" ? "dark" : "light")}>{theme === "light" ? "☾" : "☼"}</button><button type="button" className="avatar" title="用户设置" aria-label="打开用户设置" onClick={() => requestNavigation({ kind: "account" })}>{authorInitial(author.name)}</button></div></header><div className="workspace" style={layoutStyle}><Sidebar outlineDirectory={conversationProjectReady ? <OutlineVolumeNavigation key={projectId} catalog={workDirectory.catalog} error={workDirectory.error} busy={executionBusy || workDirectory.busy} active={activeView === "outline" && !accountOpen} selectedVolumeId={activeView === "outline" && !accountOpen ? outlineVolume?.id ?? null : null} onNavigate={() => navigateView("outline")} onSelect={volumeId => requestNavigation({ kind: "volume", projectId, volumeId })} onRetry={() => void workDirectory.refresh()} /> : null} directory={conversationProjectReady ? <SidebarWorkDirectory directory={workDirectory} titlePreview={currentChapterTitlePreview} key={projectId} api={api} projectId={projectId} chapters={state.novelEdition.chapters} selectedChapterId={selectedChapterId} refreshToken={creativeRefreshToken} enabled={conversationProjectReady} busy={executionBusy} dirty={creativeDirty} chapterDeletionDisabled={!taskList.loaded || Boolean(taskList.error) || taskList.activeCount > 0} onSelectChapter={selectChapter} onDeleteChapter={chapterId => requestNavigation({ kind: "delete-chapter", projectId, chapterId })} onCreateChapter={request => requestNavigation({ kind: "new-chapter", request })} /> : null} activeView={activeView} state={state} onNavigate={navigateView} onCreateProject={() => requestNavigation({ kind: "new-project" })} runtimeConnection={<RuntimeConnectionStatus status={runtimeStatus} onStatus={setRuntimeStatus} retryStartup={api?.runtime.retryStartup} />} conversationNavigation={conversationProjectReady ? <ConversationNavigator key={projectId} threads={threads} selectedId={selectedConversation.id} runningIds={[...new Set([executionBusy ? executionConversationId : null,...Array.from(conversationRuns.current.entries()).filter(([,value])=>value.submitting).map(([id])=>id)].filter((id):id is string=>Boolean(id))) ]} busy={conversationChanging} loading={threadLoading} error={threadAction} archived={threadArchived} onSelect={selectConversation} onCreate={() => void createConversation()} onRename={renameConversation} onPin={thread => void toggleThreadPin(thread)} onArchive={thread => void toggleThreadArchive(thread)} onToggleArchived={toggleThreadArchiveView} onRetry={() => void refreshThreads()} /> : null} busy={executionBusy} /><PaneSash orientation="vertical" label="调整导航宽度" value={sidebarWidth} min={72} max={sidebarMax} nextValue={(start, dx) => start + dx} onResize={(sidebar) => setPanes((current) => ({ ...current, sidebar }))} /><main ref={mainPanelRef} data-discussion-document={conversationProjectReady && !accountOpen ? "" : undefined} data-discussion-project={projectId} data-discussion-view={activeView} data-discussion-label={activeLabel} data-discussion-dirty={creativeDirty} data-view={accountOpen ? "settings" : activeView} className={`main-panel${accountOpen ? " account-open" : activeView === "chapters" ? " chapter-main-panel" : ""}`}>{(accountOpen || activeView !== "chapters") && <div className="content-header"><div className="breadcrumbs"><h1>{accountOpen ? "用户设置" : activeLabel}</h1></div><div className="header-actions"><div className="module-records-toolbar-slot" ref={setModuleActionsContainer} />{accountOpen ? <button type="button" className="secondary-button" onClick={() => setAccountOpen(false)}>返回作品</button> : <button type="button" className="secondary-button" onClick={() => setExportOpen(true)}>导出</button>}</div></div>}{accountOpen ? <UserSettings profile={author} theme={theme} diagnostics={api && projectId ? <WorkspaceDiagnostics key={projectId} api={api} projectId={projectId} projectTitle={state.project.title} refreshToken={creativeRefreshToken} /> : undefined} onTheme={setTheme} onSaveProfile={saveProfile} onChangePassword={changePassword} onSignOut={signOut} onDeleteAccount={deleteAccount} /> : null}{(agentStatus.executionHealth?.some(item => !item.projectId || item.projectId === projectId) || connectionState === "reconnecting" || connectionState === "error" || agentRun.state === "error" || agentRun.state === "waiting" || !agentStatus.configured) && <AgentStatusBar projectId={projectId} status={visibleAgentStatus} connectionState={connectionState} operationError={agentRun.error} threadId={agentRun.threadId} onReconnect={() => void reconnectAgent()} />}<div className="sr-only" role="status" aria-live="polite">{connectionState === "reconnecting" ? "正在恢复任务连接" : visibleAgentStatus.state === "running" ? "Agent 正在执行" : visibleAgentStatus.state === "waiting" ? "等待你的输入或授权" : ""}</div>{search.open && <SearchResults state={search.state} onRetry={() => void search.submit()} onSelect={(result) => {
  const target = searchNavigation(result, state.novelEdition.chapters);
  if (!target) { search.reportError("对应章节已不可用，请重新搜索。"); return; }
  requestNavigation(target);
  search.close();
  setAccountOpen(false);
}} />}
{discussionSyncNotice && <DiscussionSyncNotice message={discussionSyncNotice} busy={discussionSyncBusy} blocked={creativeDirty || executionBusy} onRefresh={() => void refreshDiscussionWorkspace()} />}
{!accountOpen && activeView === "chapters" && !projectId && !projectLoading && <div className="creative-empty project-empty-state"><strong>开始你的第一部作品</strong><p>创建作品后，可以在对话中讨论构想，也可以直接写第一章。</p><button className="primary-button" onClick={() => requestNavigation({ kind: "new-project" })}>新建作品</button></div>}
{!accountOpen && activeView === "chapters" && (projectLoading && !projectId || projectId && !conversationProjectReady) && <p role="status">正在打开作品…</p>}
{!accountOpen && activeView === "chapters" && conversationProjectReady && state.novelEdition.chapters.length === 0 && <div className="chapter-start"><p>还没有章节，可以先聊聊构想，也可以直接开始写作。</p><button type="button" className="secondary-button" onClick={() => composerRef.current?.focus()}>讨论小说构想</button></div>}
{conversationProjectReady && !accountOpen && (["brief", "outline", "plans", "chapters"] as ViewId[]).includes(activeView) && <CreativeWorkspaceView moduleActionsContainer={moduleActionsContainer} recordsOpen={recordsCurrent} onOpenVersions={openModuleRecords} onOutlineRecordScopeChange={syncOutlineRecordScope} outlineCatalogError={workDirectory.error} outlineVolume={activeView === "outline" ? outlineVolume : null} onCloseOutlineVolume={() => navigateView("outline")} onOpenOutlineNode={openOutlineNode} onLinkOutline={(volumeId, source) => workDirectory.update({ type: "linkOutline", volumeId, source })} onChapterCreationPrepared={requestId => setRequestedChapterCreation(current => current?.requestId === requestId ? null : current)} requestedChapterCreation={requestedChapterCreation?.projectId === projectId ? requestedChapterCreation : null} requestedCommitment={commitmentSelection?.request.projectId === projectId ? commitmentSelection : null} requestedCommitmentEdit={requestedCommitmentEdit?.result.projectId === projectId ? requestedCommitmentEdit : null} onCommitmentEditPrepared={requestId => setRequestedCommitmentEdit(current => current?.requestId === requestId ? null : current)} taskControls={taskControls} taskResultSelection={taskResultSelection?.result.projectId === projectId ? taskResultSelection : null} onDiscussTaskResult={result => void discussSavedResult(result)} onOpenTaskResult={result => requestNavigation({ kind: "result", result })} onDiscuss={prepareConversation} onExport={() => setExportOpen(true)} onPrepareChapterWriting={prepareChapterWriting} taskList={taskList} conversation={conversationContent} taskConversationContainer={showConversationReader ? null : taskConversationContainer} api={api} projectId={projectId} view={activeView as "brief" | "outline" | "plans" | "chapters"} state={state} selectedChapterId={selectedChapterId} onSelectChapter={selectChapter} onGenerateWork={generateWork} onContinueWork={(runId) => continueWork(runId)} onRetryWork={(runId) => continueWork(runId, "retry")} onResolveFeedback={(runId, revisionId, choice, instruction) => continueWork(runId, choice === "repairDraft" ? "feedbackRepair" : "feedbackContinue", revisionId, instruction)} onReviewPlanning={reviewPlanning} onCancelPlanningReview={cancelPlanningReview} onGenerateChapter={generateChapter} onOpenPlanningTarget={(kind) => navigateView(kind === "plan" ? "plans" : "outline")} onReturnToFeedback={() => navigateView("chapters")} onFormalChange={refreshFormalState} onDirtyChange={setCreativeDirty} onChapterTitlePreview={syncChapterTitlePreview} onSaveHandlerChange={registerCreativeSaveHandler} onPlanningSaveHandlerChange={registerPlanningSaveHandler} onDiscardHandlerChange={registerCreativeDiscardHandler} agentAvailable={agentAvailable} agentRunning={baseExecutionBusy || taskControls.pending} refreshToken={creativeRefreshToken}
  revisionContinuation={revisionContinuation?.projectId === projectId ? revisionContinuation : null}
  onPrepareRevisionContinuation={setRevisionContinuation} onDismissRevisionContinuation={() => setRevisionContinuation(null)}
  onPrepareRevisionRepair={preparation => {
    if (!revisionRepair.prepare(preparation)) return;
    composerDrafts.setSource(null); setPendingDiscussionSource(null);
    selectChapterWriting(null);
    setSentRevisionRepair(null);
    setComposer(current => current.trim() ? current : "我想讨论下一次改稿的方向，请先和我核对需要调整的内容。");
    composerRef.current?.focus();
  }} />}
{!accountOpen && activeView !== "brief" && activeView !== "outline" && activeView !== "plans" && activeView !== "chapters" && <>{activeView === "characters" ? <CharacterCatalog onDiscuss={prepareConversation} api={api} projectId={projectId} refreshToken={creativeRefreshToken} chapters={state.novelEdition.chapters} onOpenChapter={selectChapter} /> : <StoryReferenceView api={api} projectId={projectId} view={activeView} refreshToken={creativeRefreshToken} onDiscuss={prepareConversation} onOpenCommitment={request => requestNavigation({ kind: "commitment", request })} onOpenChapter={chapterId => { const chapter = state.novelEdition.chapters.find(item => item.id === chapterId); if (chapter) selectChapter(chapter); }} />}{activeView === "characters" && <details className="character-identity-tools"><summary>同名与身份管理</summary><IdentityDecisionPanel agentRunning={executionBusy} key={projectId} api={api} projectId={projectId} refreshToken={creativeRefreshToken} onFormalChange={refreshFormalState} /></details>}{activeView === "world" && <PropertyRulesPanel agentRunning={executionBusy} requestedResult={taskResultSelection?.result.kind === "propertyRules" ? { requestId: taskResultSelection.requestId, result: taskResultSelection.result } : null} api={api} projectId={projectId} refreshToken={creativeRefreshToken} onFormalChange={refreshFormalState} onDirtyChange={setCreativeDirty} onSaveHandlerChange={registerCreativeSaveHandler} onDiscardHandlerChange={registerCreativeDiscardHandler} />}</>}
</main><PaneSash orientation="vertical" label="调整对话栏宽度" value={railWidth} min={300} max={railMax} nextValue={(start, dx) => start - dx} onResize={(rail) => setPanes((current) => ({ ...current, rail }))} /><aside className="conversation-rail" data-conversation-id={selectedConversation.id ?? ""} aria-label="创作引擎协作栏"><header className="conversation-shell-header"><strong title={selectedThread?.name ?? "新对话"}>{selectedThread?.name ?? "新对话"}</strong><ConversationTools key={conversationScope} scope={conversationScope} container={conversationReadingAreaRef} searchContainer={conversationSearchContainer} inspectorOpen={inspectorOpen} onInspector={() => setInspectorOpen(open => !open)} savingCount={recoveryRuns.filter(run => run.projectId === projectId).length} currentSavingCount={recoveryRuns.filter(run => run.projectId === projectId && Boolean(selectedConversation.id) && run.threadId === selectedConversation.id).length} onRecovery={() => setRecoveryProject(projectId)} pauseChoices={pauseChoices} pauseDisabled={pauseTaskDisabled} onPauseTask={(task, command) => void pauseSelectedTask(task, command)} /></header>{formalReadError?.projectId === projectId && <ReadFailureNotice message="章节目录暂时无法读取。" error={formalReadError.message} retryLabel="重新读取目录" onRetry={() => setCreativeRefreshToken(value => value + 1)} />}<ConversationExecutionStatus run={statusOwnsExecution ? agentRun : conversationRun} busy={baseExecutionBusy && statusOwnsExecution} connection={connectionState} works={statusWorks} readError={taskList.error} health={(agentStatus.executionHealth ?? []).filter(item => item.projectId === projectId && Boolean(item.workTaskId) && taskList.tasks.some(task => task.id === item.workTaskId && task.threadId === selectedConversation.id))} /><>{executionBusy && !viewingExecution && <div className="conversation-other-execution" role="status">另一对话“{threads.find(thread => thread.id === executionConversationId)?.name ?? "创作任务"}”{agentRun.state === "waiting" ? "等待回答" : "正在执行"}。可浏览和编辑草稿，执行结束后可发送。<button type="button" onClick={() => selectConversation(threads.find(thread => thread.id === executionConversationId) ?? null)}>返回处理</button></div>}{selectedThread?.isArchived && <p role="status">已归档对话 · 恢复后可继续发送</p>}</>{selectedThread?.forkedFromId && <div className="conversation-branch-source">此对话从另一方案分支而来<button type="button" disabled={conversationChanging} onClick={() => { const parent = threads.find(thread => thread.id === selectedThread.forkedFromId); if (parent) void selectConversation(parent); else if (selectedThread.forkedFromId) void openBranchSource(selectedThread.forkedFromId); }}>查看原对话</button><button type="button" disabled={executionBusy} onClick={() => setComparison({ projectId, threadId: selectedThread.id, sourceId: selectedThread.forkedFromId! })}>对照两种方案</button></div>}<div className="conversation-search-slot" ref={setConversationSearchContainer} /><div className="conversation-reading-area" ref={conversationReadingAreaRef}>{showConversationReader && taskList.error && <ReadFailureNotice message="任务记录暂时无法读取。" error={taskList.error} retryLabel="重新读取任务记录" busy={taskList.reading} onRetry={() => void taskList.refresh()} />}{showConversationReader && taskList.reading && <p role="status">正在读取对话与执行记录…</p>}{!showConversationReader && <div className="conversation-rail-scroll">{earlierConversationDetail?.turns.length ? <ConversationReader detail={earlierConversationDetail} loading={false} error={null} onRetry={() => {}} onFork={turnId => void createConversation(undefined, turnId)} busy={executionBusy || conversationChanging || Boolean(selectedThread?.isArchived)} showEmpty={false} /> : null}{api && conversationProjectReady && <ChapterStructureNotice key={`structure:${projectId}`} api={api} projectId={projectId} refreshToken={creativeRefreshToken} available={agentAvailable} busy={executionBusy} dirty={creativeDirty} failures={structureFailures} />}{conversationProjectReady && !creativeConversationOpen && <WorkTaskPanel onFormalChange={refreshFormalState} key={`work:${projectId}`} conversation={conversationContent} api={api} projectId={projectId} tasks={taskList.tasks} activeCount={taskList.activeCount}
      nextCursor={taskList.nextCursor} loadingMore={taskList.loadingMore} loaded={taskList.loaded} reading={taskList.reading} readError={taskList.error}
      onRetry={() => void taskList.refresh()} onLoadMore={() => void taskList.loadMore()} revision={undefined} busy={taskControls.pending}
      agentRunning={executionBusy} dirty={creativeDirty} onDiscussResult={result => void discussSavedResult(result)} onOpenResult={result => requestNavigation({ kind: "result", result })} onAdjustCommitment={(result, detail) => { if (result.projectId === currentProjectRef.current && detail.projectId === result.projectId && !executionBusy) requestNavigation({ kind: "commitment-edit", result, detail }); }} />}{taskControlMessage?.scope === taskList.scope && taskControlMessage.error && taskList.tasks.some(task => task.id === taskControlMessage.error?.targetId && (task.threadId === selectedConversation.id || task.kind === "generation" && task.threadId === "" && selectedConversation.id === null)) && <p className="dialog-error" role="alert">{taskList.tasks.find(task => task.id === taskControlMessage.error?.targetId)?.goal.slice(0, 60) ?? "任务操作"}：{taskControlMessage.error.message}</p>}{taskControlMessage?.scope === taskList.scope && taskControlMessage.notice && taskList.tasks.some(task => task.id === taskControlMessage.notice?.targetId && (task.threadId === selectedConversation.id || task.kind === "generation" && task.threadId === "" && selectedConversation.id === null)) && <p role="status">{taskList.tasks.find(task => task.id === taskControlMessage.notice?.targetId)?.goal.slice(0, 60) ?? "任务操作"}：{taskControlMessage.notice.message}</p>}{taskResultError && <p className="dialog-error" role="alert">{taskResultError}</p>}<div className="conversation-task-records" ref={setTaskConversationContainer} /><ActionProposalList actions={selectedConversationActions} busy={Boolean(approvingActionId) || executionBusy || creativeDirty} onApprove={(action) => void approveProposedAction(action)} onDecline={(action) => void declineProposedAction(action)} onScope={(action) => setScopeAction({ projectId, action })} /></div>}{showConversationReader && <div className="conversation-history-scroll"><ConversationReader detail={activeConversationDetail?.detail ?? null} loading={Boolean(activeConversationDetail?.loading)} error={activeConversationDetail?.error ?? null} onRetry={() => { if (selectedConversation.id) void readSelectedConversation(selectedConversation.id); }} onFork={turnId => void createConversation(undefined, turnId)} busy={executionBusy || conversationChanging || Boolean(selectedThread?.isArchived)} /></div>}</div>{viewingExecution && currentSentRepair?.projectId === projectId && agentRun.state === "error" && <FailedRevisionRepair preparation={currentSentRepair} onStartOrdinaryDiscussion={() => { revisionRepair.cancel(); setSentRevisionRepair(null); }} />}{revisionRepair.prepared && <PreparedRevisionRepair preparation={revisionRepair.prepared} checking={revisionRepair.checking} error={revisionRepair.error} onCancel={revisionRepair.cancel} />}{composerDrafts.error && <p className="dialog-error" role="alert">{composerDrafts.error}</p>}<>{reconnectNotice?.scope === promptScopeRef.current && agentRun.connectionState !== "ready" && <p className="dialog-error" role="status" aria-label="原任务连接核对">{reconnectNotice.message}</p>}</>{!showConversationReader && activeConversationDetail?.error && selectedConversation.id && <button type="button" disabled={executionBusy || conversationChanging} onClick={() => { if (selectedConversation.id) void readSelectedConversation(selectedConversation.id); }}>重新读取对话历史</button>}{reasoningSelection.error && <p className="dialog-error" role="alert">{reasoningSelection.error}</p>}<ModelConnectionNotice progress={modelConnection.progress} busy={modelConnection.busy} onRetry={() => void modelConnection.retry()} /><DiscussionSourceChip source={composerDrafts.source} pending={pendingDiscussionSource?.focus.reference.projectId === projectId ? pendingDiscussionSource : null} disabled={executionBusy} onRemove={() => { composerDrafts.setSource(null); setPendingDiscussionSource(null); }} onKeep={() => setPendingDiscussionSource(null)} onReplace={() => {
      if (!pendingDiscussionSource || pendingDiscussionSourceRef.current !== pendingDiscussionSource || currentProjectRef.current !== projectId || pendingDiscussionSource.focus.reference.projectId !== projectId || executionBusy) return;
      composerDrafts.setSource(pendingDiscussionSource); setPendingDiscussionSource(null); composerRef.current?.focus({ preventScroll: true });
    }} /><DiscussionExcerptCards excerpts={composerDrafts.excerpts} disabled={executionBusy && viewingExecution} readOnly={composerDrafts.excerpts.length > 0 && (creativeDirty || composerDrafts.excerpts.some(item => item.state === "unsaved"))} onRemove={composerDrafts.removeExcerpt} /><SelectionDiscussion projectId={projectId} view={activeView} disabled={!conversationProjectReady || accountOpen || conversationChanging || executionBusy} onAdd={addDiscussionExcerpt} onNew={excerpt => createConversation(excerpt)} />{api && taskList.tasks.filter(work=>work.threadId===selectedConversation.id && work.projectId===projectId && work.status==="running").map(work=><WorkInterludePanel key={`${projectId}:${work.id}`} api={api} work={work} text={composer} focus={{...(composerDrafts.source?.focus??{view:activeView}),...(composerDrafts.excerpts.length?{excerpts:structuredClone(composerDrafts.excerpts)}:{})}} blocked={conversationChanging || Boolean(currentChapterWriting || revisionRepair.prepared || pendingDiscussionSource)} acceptDraft={composerDrafts.acceptance} refresh={taskList.refresh}/>)}<ConversationMessageQueue key={conversationScope} items={outbox.items} busy={conversationChanging} edit={outbox.edit} move={outbox.move} remove={outbox.remove} reconcile={outbox.reconcile} continueWaiting={outbox.continue} />{outbox.error && <p role="alert">{outbox.error}</p>}{executionBusy && viewingExecution && currentRunCanSteer(agentRun) && agentRun.taskId && composer.trim() && <button type="button" className="text-button" disabled={creativeDirty || Boolean(currentChapterWriting || revisionRepair.prepared || pendingDiscussionSource)} onClick={() => void queuePrompt("afterTurn")}>本轮结束后发送</button>}<Composer retryChoices={retryChoices} retrySingleAction={Boolean(retryTask)} retryScope={conversationScope} onRetryTask={task => void retrySelectedTask(task)} onRetry={composerRetry} retryDisabled={retryTaskDisabled} retryLabel={retryTask?.resumeMode === "recoverSavedResult" ? "核对保存结果" : "重试当前任务"} onQueue={executionBusy && viewingExecution ? () => void queuePrompt() : undefined} queueDisabled={!selectedConversation.id || creativeDirty || Boolean(currentChapterWriting || revisionRepair.prepared || pendingDiscussionSource)} projectId={projectId} planningSaving={planningSubmission.pending} planningSaveOnSend={creativeDirty && planningSubmissionEligible && !composerDrafts.excerpts.length} choosingSource={pendingDiscussionSource?.focus.reference.projectId === projectId} canSteer={viewingExecution && currentRunCanSteer(agentRun) && !composerDrafts.excerpts.length} writing={currentChapterWriting} writingPending={chapterWritingPending} onCancelWriting={() => selectChapterWriting(null)} repairPrepared={Boolean(revisionRepair.prepared)} checkingRepair={revisionRepair.checking} composer={composer} setComposer={setComposer} onSubmit={() => void submitPrompt()} onInterrupt={() => { if (viewingExecution) void interruptAgent(); }} composerRef={composerRef} activity={activity} agentStatus={viewingExecution ? visibleAgentStatus : { ...visibleAgentStatus, state: "idle" }} availability={{ interludeHeld: taskList.tasks.some(work=>work.projectId===projectId && work.threadId===selectedConversation.id && !!work.interlude), runtimeReady: Boolean(api && runtimeStatus.state === "online"), modelReady: agentStatus.configured, changingConversation: conversationChanging, historyState: activeConversationDetail?.loading ? "loading" : activeConversationDetail?.error ? "error" : "ready", archived: Boolean(selectedThread?.isArchived), otherConversationRunning: executionBusy && !viewingExecution }} blockedByUnsavedChanges={creativeDirty && !composerDrafts.excerpts.length && !planningSubmissionEligible && (!currentChapterWriting || executionBusy)} projectReady={Boolean(projectId && projectId === activeProjectId)} connections={agentConnections} agentConfigSaving={agentConfigSaving} onSelectConnection={(connection) => void selectAgentConnection(connection)} reasoningDisabled={!signedIn || agentConfigSaving || reasoningSelection.busy} onReasoningChange={effort => void reasoningSelection.change(effort)} />{inspectorOpen && viewingExecution && <PaneSash orientation="horizontal" label="调整功能区高度" value={panes.inspector} min={88} max={inspectorMax} nextValue={(start, _dx, dy) => start - dy} onResize={(inspector) => setPanes((current) => ({ ...current, inspector }))} />}<div className="conversation-context" id="conversation-inspector" hidden={!inspectorOpen || !viewingExecution}><AgentInspector state={state} runtimeStatus={runtimeStatus} agentRun={agentRun} connections={agentConnections} contextTab={contextTab} onContextTabChange={setContextTab} onAddCanonNote={openFactReview} onReview={() => void reviewAgent()} backgroundTerminals={backgroundTerminals} backgroundTerminalLoading={backgroundTerminalLoading} onLoadBackgroundTerminals={() => void loadBackgroundTerminals()} onTerminateBackgroundTerminal={(processId) => void terminateBackgroundTerminal(processId)} onVerifyAgent={(provider, model) => void verifyAgent(provider, model)} onConfigureAgent={(provider, model) => void configureAgent(provider, model)} agentConfigSaving={agentConfigSaving} /></div></aside></div>{recordsCurrent && moduleRecords && <ModuleRecordsPopover key={moduleRecords.requestId} source={moduleRecords}
      api={moduleRecords.api} chapter={state.novelEdition.chapters.find(chapter => chapter.id === moduleRecords.chapterId)}
      dirty={creativeDirty} blocked={executionBusy} opener={moduleRecords.opener} container={mainPanelRef.current}
      onClose={() => setModuleRecords(current => current?.requestId === moduleRecords.requestId ? null : current)} onFormalChange={refreshFormalState} />}{chapterDeletion.state && <ChapterDeleteDialog deletion={chapterDeletion} />}{pendingNavigation && <UnsavedChangesDialog saving={savingBeforeLeave || planningSubmission.pending} onSave={() => void saveAndContinueNavigation()} onDiscard={discardAndContinueNavigation} onStay={() => { if (pendingNavigation.kind === "close") void api?.app.cancelClose(); setPendingNavigation(null); }} />}{commandPaletteOpen && <CommandPalette onClose={() => setCommandPaletteOpen(false)} onFocusComposer={() => { setCommandPaletteOpen(false); composerRef.current?.focus(); }} onReview={() => { setCommandPaletteOpen(false); void reviewAgent(); }} onReconnect={() => { setCommandPaletteOpen(false); void reconnectAgent(); }} onTheme={() => { setCommandPaletteOpen(false); setTheme((current) => current === "light" ? "dark" : "light"); }} />}{recoveryProject === projectId && <ConversationRecovery key={`recovery:${projectId}`} runs={recoveryRuns.filter(run => run.projectId === projectId)} tasks={taskList.tasks} threadId={selectedConversation.id} busy={executionBusy} onInspect={inspectRecovery} onReconcile={reconcileRecovery} onClose={() => setRecoveryProject(null)} />}{comparison?.projectId === projectId && api && <ConversationComparison key={`${projectId}:${comparison.threadId}`} api={api} {...comparison} onClose={() => setComparison(null)} />}{savedAnalysis && <SavedAnalysisDialog analysis={savedAnalysis} onClose={() => setSavedAnalysis(null)} />}{exportOpen && <ExportDialog onClose={() => setExportOpen(false)} onExport={async (options) => { if (!api) throw new Error("Runtime 尚未连接，请稍后重试。"); return api.runtime.exportProject(activeProjectId, options); }} />}{scopeAction?.projectId === projectId && api && <ExecutionScopeDialog key={`${projectId}:${scopeAction.action.sourceTaskId}`} projectId={projectId} initialAction={scopeAction.action} actions={proposedActions.projectId === projectId ? proposedActions.actions : []} api={api.agent} blocked={creativeDirty} onClose={() => setScopeAction(null)} onStart={() => { scopeExecutionRef.current = { projectId, scope: beginExecutionScope() }; const previous = summarizeAgentRun(agentRun); setAgentRun(createAgentRun("按确认的有限范围继续创作", previous ? [previous, ...agentRun.history] : agentRun.history)); }} onComplete={scopeExecutionFinished} onError={(message) => { if (currentProjectRef.current === projectId && scopeExecutionRef.current?.scope === promptScopeRef.current) void reportExecutionFailure(message, scopeExecutionRef.current.scope, "有限范围执行失败"); }} />}{projectInformationOpen && conversationProjectReady && <ProjectInformation key={projectId} api={api} project={state.project} onClose={() => setProjectInformationOpen(false)} onOpenBrief={() => { setProjectInformationOpen(false); navigateView("brief"); }} />}{editionDialog}{editionError && <p className="dialog-error" role="alert">{editionError}</p>}{projectCreateOpen && <ProjectCreateDialog onClose={() => setProjectCreateOpen(false)} onSubmit={createProject} />}</div>;
}

function Sidebar({ conversationNavigation, outlineDirectory, directory, activeView, state, onNavigate, onCreateProject, runtimeConnection, busy }: { conversationNavigation?: ReactNode; outlineDirectory?: ReactNode; directory?: ReactNode; activeView: ViewId; state: RuntimeState; onNavigate: (view: ViewId) => void; onCreateProject: () => void; runtimeConnection: ReactNode; busy: boolean }) {
  return <aside className="sidebar"><div className="sidebar-heading"><span>作品</span><button className="tiny-button" title="新建作品" onClick={onCreateProject} disabled={busy}>＋</button></div>{conversationNavigation}<nav className="nav-groups">{navGroups.map((group) => <div className="nav-group" key={group.label}><div className="nav-label">{group.label}</div>{group.items.map((item) => item.id === "outline" && outlineDirectory ? <div key={item.id}>{outlineDirectory}</div> : item.id === "chapters" && directory ? <div key={item.id} className="sidebar-chapter-navigation">{directory}</div> : <button className={`nav-item ${activeView === item.id ? "active" : ""}`} key={item.id} aria-label={item.label} aria-current={activeView === item.id ? "page" : undefined} title={item.label} onClick={() => onNavigate(item.id)}><span className="nav-icon">{item.icon}</span><span>{item.label}</span>{item.id === "chapters" && <span className="nav-count">{state.novelEdition.chapters.length}</span>}</button>)}</div>)}</nav><div className="sidebar-footer">{runtimeConnection}</div></aside>;
}

function ProjectSwitcher({ projects, activeProjectId, busy, onSelect, onCreate, onInformation }: { projects: ProjectSummary[]; activeProjectId: string; busy: boolean; onSelect: (projectId: string) => void; onCreate: () => void; onInformation?: () => void }) {
  const [open, setOpen] = useState(false);
  const active = projects.find((project) => project.id === activeProjectId);
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => { if (event.key === "Escape") setOpen(false); };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);
  useEffect(() => { if (busy) setOpen(false); }, [busy]);
  return <div className="project-switcher-wrap"><button className="project-switcher" data-project-information-trigger aria-expanded={open} disabled={busy} onClick={() => setOpen((value) => !value)}><span className="project-dot" /><span title={active?.title}>{active?.title ?? "选择作品"}</span><span className="muted">⌄</span></button>{open && <div className="project-menu" role="menu" aria-label="选择作品">{onInformation && <button type="button" className="project-menu-item" role="menuitem" onClick={() => { setOpen(false); onInformation(); }}>作品信息</button>}<div className="project-menu-heading"><span>我的作品</span><button className="project-create-menu" onClick={() => { setOpen(false); onCreate(); }}>＋ 新建</button></div>{projects.map((project) => <button className={`project-menu-item ${project.id === activeProjectId ? "selected" : ""}`} role="menuitem" key={project.id} onClick={() => { setOpen(false); onSelect(project.id); }}><span className="project-menu-mark">{project.id === activeProjectId ? "✓" : "▧"}</span><span><strong>{project.title}</strong><small>{project.logline || "暂无简介"}</small></span></button>)}</div>}</div>;
}

function UnsavedChangesDialog({ saving, onSave, onDiscard, onStay }: { saving: boolean; onSave: () => void; onDiscard: () => void; onStay: () => void }) {
  const stayRef = useRef<HTMLButtonElement | null>(null);
  useEffect(() => { stayRef.current?.focus(); }, []);
  return <div className="modal-backdrop" role="presentation"><section className="project-dialog unsaved-dialog" role="dialog" aria-modal="true" aria-labelledby="unsaved-dialog-title" aria-describedby="unsaved-dialog-description"><div className="project-dialog-header"><div><span className="eyebrow">尚未保存</span><h2 id="unsaved-dialog-title">当前创作修改还没有确认保存</h2></div></div><p id="unsaved-dialog-description" className="project-dialog-description">保存会写入当前创作编辑内容；也可以明确放弃本地输入，或留在当前页面继续处理。放弃不会撤回可能已经保存的候选。</p><div className="project-dialog-actions three-way-actions"><button className="text-button danger" type="button" disabled={saving} onClick={onDiscard}>放弃修改并离开</button><button ref={stayRef} className="secondary-button" type="button" disabled={saving} onClick={onStay}>留在当前页面</button><button className="primary-button" type="button" disabled={saving} onClick={onSave}>{saving ? "正在保存…" : "保存并继续"}</button></div></section></div>;
}


function planStatusLabel(status: AgentRun["plan"][number]["status"]): string { return status === "inProgress" ? "进行中" : status === "completed" ? "已完成" : "待处理"; }
function AgentPlan({ run }: { run: AgentRun }) { if (run.plan.length === 0) return null; const completed = run.plan.filter((step) => step.status === "completed").length; return <section className="agent-plan" aria-label="Agent 执行计划"><div className="agent-plan-heading"><strong>执行计划</strong><span>{completed}/{run.plan.length} 已完成</span></div><ol>{run.plan.map((step) => <li className={step.status} key={step.id}><span className="plan-marker" aria-hidden="true">{step.status === "completed" ? "✓" : step.status === "inProgress" ? "•" : "·"}</span><span>{step.label}</span><small>{planStatusLabel(step.status)}</small></li>)}</ol></section>; }
const QUIET_RECOVERY_OPERATIONS = new Set(["read_context"]);
function authorFacingRecoveryRun(run: RecoveryRun): boolean { return run.requests.some((request) => request.recovery === "manual-reconciliation" && !QUIET_RECOVERY_OPERATIONS.has(request.operation ?? "")); }
export function ActionProposalList({ actions, busy, onApprove, onDecline, onScope }: { actions: ProposedAction[]; busy: boolean; onApprove: (action: ProposedAction) => void; onDecline: (action: ProposedAction) => void; onScope: (action: ProposedAction) => void }) {
  if (!actions.length) return null;
  return <section className="conversation-proposals" aria-label="待确认的创作建议">
    <header><strong>待确认的候选修改</strong><span>模型建议不会自行修改作品</span></header>
    {actions.map((action) => {
      const name = compactTaskName(taskOperationName(action.operation, action.target));
      return <article key={action.id} className="conversation-proposal">
      <div><strong title={name.fullLabel} aria-label={name.fullLabel}>{name.label}</strong></div>
      <ConversationPrompt text={action.instruction} detailKey={`proposal:${action.sourceTaskId}:${action.id}:instruction`} proposal />
      {action.operation === "generateChapter" && <DesiredOutcomes outcomes={action.target.desiredOutcomes} />}
      <div className="conversation-proposal-actions"><button type="button" disabled={busy} onClick={() => onDecline(action)}>拒绝建议</button><button type="button" disabled={busy} onClick={() => onScope(action)}>设置本轮范围</button><button type="button" disabled={busy} onClick={() => onApprove(action)}>批准生成候选</button></div>
    </article>;
    })}
    <p className="conversation-proposals-note">批准只允许生成可审阅候选，不会正式采用正文或事实。</p>
  </section>;
}
function SendArrowIcon() { return <svg className="composer-icon" viewBox="0 0 16 16" aria-hidden="true" focusable="false"><path d="M8 12V4m0 0L4.5 7.5M8 4l3.5 3.5" /></svg>; }
function StopIcon() { return <svg className="composer-icon composer-stop-icon" viewBox="0 0 16 16" aria-hidden="true" focusable="false"><rect x="5" y="5" width="6" height="6" rx="1" /></svg>; }
type ComposerAvailability = {
  interludeHeld?: boolean;
  runtimeReady: boolean;
  modelReady: boolean;
  changingConversation: boolean;
  historyState: "ready" | "loading" | "error";
  archived: boolean;
  otherConversationRunning: boolean;
};
function Composer({ retryChoices = [], retrySingleAction = true, retryScope = "", onRetryTask, onRetry, retryDisabled, retryLabel, onQueue, queueDisabled, projectId, planningSaving, planningSaveOnSend, choosingSource, canSteer, writing, writingPending, onCancelWriting, repairPrepared, checkingRepair, composer, setComposer, onSubmit, onInterrupt, composerRef, activity, agentStatus, availability, blockedByUnsavedChanges, projectReady, connections, agentConfigSaving, onSelectConnection, reasoningDisabled, onReasoningChange }: { retryChoices?: WorkTaskSummary[]; retrySingleAction?: boolean; retryScope?: string; onRetryTask?: (task: WorkTaskSummary) => void; onRetry?: () => void; retryDisabled: boolean; retryLabel: string; onQueue?: () => void; queueDisabled: boolean; projectId: string; planningSaving: boolean; planningSaveOnSend: boolean; choosingSource: boolean; canSteer: boolean; writing: ChapterWritingRequest | null; writingPending: boolean; onCancelWriting: () => void; repairPrepared: boolean; checkingRepair: boolean; composer: string; setComposer: (value: string) => void; onSubmit: () => void; onInterrupt: () => void; composerRef: RefObject<HTMLTextAreaElement | null>; activity: string[]; agentStatus: { configured: boolean; state: string; model?: string | null; provider?: string | null; reasoningEffort?: string | null }; availability: ComposerAvailability; blockedByUnsavedChanges: boolean; projectReady: boolean; connections: AgentConnection[]; agentConfigSaving: boolean; onSelectConnection: (connection: AgentConnection) => void; reasoningDisabled: boolean; onReasoningChange: (effort: string) => void }) {
  useLayoutEffect(() => {
    const node = composerRef.current;
    if (!node) return;
    const fit = () => {
      const top = node.scrollTop;
      node.style.height = "0px";
      node.style.height = `${Math.min(144, Math.max(48, node.scrollHeight))}px`;
      node.scrollTop = top;
    };
    fit();
    let width = node.clientWidth;
    const observer = new ResizeObserver(() => {
      if (node.clientWidth !== width) { width = node.clientWidth; fit(); }
    });
    observer.observe(node);
    return () => observer.disconnect();
  }, [composer, composerRef]);
  const inputError = useMemo(() => composer.trim() ? authorIntentError(composer) : null, [composer]);
  const submissionAvailable = availability.runtimeReady && availability.modelReady && !agentConfigSaving
    && !availability.changingConversation && availability.historyState === "ready"
    && !availability.archived && !availability.otherConversationRunning;
  const availabilityReason = availability.historyState === "error"
    ? "这条对话的历史暂时无法读取。请重试读取历史；对话草稿会保留。"
    : availability.archived ? "这条对话已归档。恢复对话后可发送；对话草稿会保留。"
    : availability.changingConversation ? "正在切换对话，完成后可发送；对话草稿会保留。"
    : availability.historyState === "loading" ? "正在读取这条对话，完成后可发送；对话草稿会保留。"
    : availability.otherConversationRunning ? "另一条对话正在执行。可返回正在执行的对话；当前草稿会保留。"
    : !availability.runtimeReady ? "创作环境暂未连接，恢复后可发送；对话草稿会保留。"
    : agentConfigSaving ? "正在应用模型配置，完成后可发送；对话草稿会保留。"
    : !availability.modelReady ? "模型连接尚不可用。请在设置中配置或验证连接；对话草稿可以先写。" : null;
  const active = agentStatus.state === "running";
  const waiting = agentStatus.state === "waiting";
  const disabled = availability.interludeHeld || planningSaving || choosingSource || (active && !canSteer) || !projectReady || (writingPending && !active) || waiting || blockedByUnsavedChanges || checkingRepair || (repairPrepared && active) || (!active && !submissionAvailable);
  const inputDisabled = !projectReady || checkingRepair || (repairPrepared && active);
  const draftReason = !projectReady ? null : availability.interludeHeld ? "原任务暂挂中：可再插入一个问题，或返回原任务。草稿会保留。" : planningSaving ? "正在保存并核对规划，完成后发送本条指令。" : choosingSource ? "请先选择保留或替换讨论来源，再发送。" : active && !canSteer ? "本轮结束后可发送；输入内容不会自动发送。" : blockedByUnsavedChanges ? "中央文档有未保存修改。保存后即可发送；可以先写下对话草稿。" : planningSaveOnSend ? "发送时先保存规划修改，再处理你的要求；不会自动采用。" : waiting ? "请先处理本轮需要回答的问题；可以先写下下一条指令。" : !submissionAvailable && !active ? availabilityReason : null;
  const latestActivity = activity[0];
  const modelLabel = agentStatus.model ?? (agentStatus.configured ? "已配置模型" : "未配置模型");
  const modelSelectionDisabled = active || waiting || writingPending || planningSaving || agentConfigSaving;
  const placeholder = !projectReady ? "请先创建或打开作品……" : active ? canSteer ? "追加指令，会在当前这一轮里处理……" : "先写下下一条指令，本轮结束后可发送……" : "想如何继续这个故事……";
  return <><div className="composer-card composer-global">{writing && <div className="chapter-writing-mode" role="status"><span>本章上下文 · 第 {writing.chapterNumber} 章 · {Array.from(writing.title).slice(0, 80).join("")}</span><button type="button" className="text-button" disabled={writingPending || active || waiting} onClick={onCancelWriting}>取消定位</button></div>}<ComposerDraftNotice reason={draftReason} /><div className="composer-body"><textarea ref={composerRef} aria-label="创作指令" aria-describedby={draftReason ? "composer-draft-notice composer-keyboard-hint" : "composer-keyboard-hint"} disabled={inputDisabled} value={composer} onChange={(event) => setComposer(event.target.value)} onKeyDown={(event) => { if (isComposerSubmit(event.nativeEvent,"enter")) { event.preventDefault(); if (!disabled && !inputError) onSubmit(); } }} placeholder={placeholder} /><div id="composer-keyboard-hint" className="composer-keyboard-hint">{!disabled && !inputError ? active && canSteer ? "Enter 追加指令 · " : "Enter 发送 · " : ""}Shift+Enter 换行</div><div className="composer-bottom"><div className="composer-choices"><ComposerModelPicker key={projectId} connections={connections} provider={agentStatus.provider} model={agentStatus.model} reasoningEffort={agentStatus.reasoningEffort} label={agentConfigSaving ? "正在配置…" : modelLabel} disabled={modelSelectionDisabled} onSelect={onSelectConnection} reasoningDisabled={reasoningDisabled} onReasoningChange={onReasoningChange} /></div><div className="composer-actions">{!active && !waiting && <ComposerRetryControl scope={retryScope} choices={retryChoices} singleAction={retrySingleAction} disabled={retryDisabled} agentAvailable={submissionAvailable} label={retryLabel} onRetry={onRetry} onSelect={onRetryTask} />}{onQueue && composer.trim() && <button type="button" className="composer-queue" disabled={queueDisabled || Boolean(inputError)} aria-label="当前任务结束后发送" title="当前任务完成后处理这条指令" onClick={onQueue}>稍后发送</button>}{active ? <><button className="composer-steer" disabled={disabled || Boolean(inputError) || !composer.trim()} aria-label={canSteer ? "追加创作指令" : "发送创作指令"} onClick={onSubmit}><SendArrowIcon /></button><button className="composer-stop" aria-label="停止当前执行" onClick={onInterrupt}><StopIcon /></button></> : waiting ? <><span className="composer-waiting">等待交互</span><button className="composer-stop" aria-label="停止当前执行" onClick={onInterrupt}><StopIcon /></button></> : <button className="composer-send" disabled={disabled || !composer.trim() || Boolean(inputError)} aria-label="发送创作指令" onClick={onSubmit}><SendArrowIcon /></button>}</div></div></div></div>{inputError && <p className="dialog-error" role="alert">{inputError}</p>}{latestActivity && <div className="activity-strip" role="status" aria-live="polite"><div><span className="activity-spark">✦</span>{latestActivity}</div></div>}</>;
}
function formatDate(value?: string) { return value ? new Date(value).toLocaleString("zh-CN", { hour: "2-digit", minute: "2-digit" }) : "—"; }

function AgentInspector({ state, runtimeStatus, agentRun, connections, contextTab, onContextTabChange, onAddCanonNote, onReview, backgroundTerminals, backgroundTerminalLoading, onLoadBackgroundTerminals, onTerminateBackgroundTerminal, onVerifyAgent, onConfigureAgent, agentConfigSaving }: { state: RuntimeState; runtimeStatus: RuntimeStatus; agentRun: AgentRun; connections: AgentConnection[]; contextTab: "story" | "task" | "changes" | "usage" | "diagnostics" | "settings"; onContextTabChange: (tab: "story" | "task" | "changes" | "usage" | "diagnostics" | "settings") => void; onAddCanonNote: () => void; onReview: () => void; backgroundTerminals: BackgroundTerminal[]; backgroundTerminalLoading: boolean; onLoadBackgroundTerminals: () => void; onTerminateBackgroundTerminal: (processId: string) => void; onVerifyAgent: (provider: string, model: string) => void; onConfigureAgent: (provider: string, model: string) => void; agentConfigSaving: boolean }) {
  const tabs: { id: typeof contextTab; label: string }[] = [{ id: "story", label: "故事" }, { id: "task", label: "任务" }, { id: "changes", label: "变更" }, { id: "usage", label: "用量" }, { id: "diagnostics", label: "诊断" }, { id: "settings", label: "设置" }];
  const status = agentRun.state === "running" ? "执行中" : agentRun.state === "waiting" ? "等待确认" : agentRun.state === "error" ? "执行失败" : agentRun.state === "completed" ? "已完成" : "暂无执行";
  const model = agentRun.model ?? "未指定";
  const provider = agentRun.provider ?? "默认 Provider";
  const currentConnection = connections.find((connection) => connection.model === agentRun.model && connection.provider === agentRun.provider) ?? connections.find((connection) => connection.isDefault) ?? connections[0];
  const [connectionDraft, setConnectionDraft] = useState("");
  useEffect(() => { if (currentConnection) setConnectionDraft(currentConnection.id); }, [currentConnection?.id]);
  const selectedConnection = connections.find((connection) => connection.id === connectionDraft) ?? currentConnection;
  const renderSettings = <div className="inspector-stack"><div className="inspector-summary-line"><strong>模型</strong><span>只用于这部作品</span></div><label className="settings-field"><span>模型连接</span><select value={connectionDraft} onChange={(event) => setConnectionDraft(event.target.value)}>{connections.map((connection) => <option value={connection.id} key={connection.id}>{connection.provider} · {connection.model}</option>)}</select></label>{selectedConnection && <><div className="inspector-facts"><span><small>协议</small><strong>{selectedConnection.selectedProtocol ?? selectedConnection.protocol}</strong></span><span><small>状态</small><strong>{selectedConnection.status === "verified" ? "基础能力已验证" : selectedConnection.status === "failed" ? "验证失败" : "等待验证"}</strong></span><span><small>声明能力</small><strong>{selectedConnection.declaredCapabilities.length} 项</strong></span><span><small>验证能力</small><strong>{selectedConnection.verifiedCapabilities.length} 项</strong></span></div>{selectedConnection.status !== "verified" && <button className="primary-button" onClick={() => onVerifyAgent(selectedConnection.provider, selectedConnection.model)} disabled={agentConfigSaving}>{agentConfigSaving ? "验证中…" : "验证连接"}</button>}<button className="secondary-button" onClick={() => onConfigureAgent(selectedConnection.provider, selectedConnection.model)} disabled={agentConfigSaving || selectedConnection.status !== "verified"}>{agentConfigSaving ? "处理中…" : "应用配置"}</button></>}<small className="inspector-note">目录只声明可能能力；应用配置前必须取得当前端点的基础探测证据。模型连接使用独立的 API Key，不会使用其他桌面应用的登录状态。密钥仅由本机读取。</small></div>;
  const renderStory = <><div className="context-section"><div className="section-title"><span>设定笔记</span><button onClick={onAddCanonNote}>＋</button></div><div className="canon-list">{state.canon.notes.map((note) => <div className="canon-note" key={note}><span className="note-pin">•</span>{note}</div>)}</div></div><div className="context-section"><div className="section-title"><span>出场角色</span><span className="section-count">{state.canon.characters.length}</span></div><div className="character-list">{state.canon.characters.map((character) => <div className="character-row" key={character.id}><div className="character-avatar" style={{ background: character.color }}>{character.name.slice(0, 1)}</div><div><strong>{character.name}</strong><span>{character.role}</span></div><span className="character-status">{character.status}</span></div>)}</div></div><div className="context-section"><div className="section-title"><span>未收的线</span><span className="section-count">{state.canon.plotThreads.length}</span></div>{state.canon.plotThreads.map((thread) => <div className="thread-row" key={thread.id}><span className={`thread-line ${thread.tone}`} /><span>{thread.title}</span><small>{thread.state}</small></div>)}</div></>;
  const renderTask = <div className="inspector-stack"><div className="inspector-status-card"><span className={`online-dot ${agentRun.connectionState === "ready" ? "" : "offline"}`} /><div><strong>{status}</strong><small>创作引擎</small></div></div><AgentPlan run={agentRun}/><div className="inspector-facts"><span><small>服务</small><strong>{provider}</strong></span><span><small>模型</small><strong>{model}</strong></span><span><small>连接</small><strong>{connectionLabel(agentRun.connectionState)}</strong></span><span><small>会话</small><strong>{agentRun.threadId ? "已连接" : "未创建"}</strong></span></div><div className="inspector-actions"><button className="secondary-button" onClick={onReview}>查看章节</button></div><div className="inspector-subsection"><div className="section-title"><span>后台终端</span><button onClick={onLoadBackgroundTerminals} disabled={!agentRun.threadId || backgroundTerminalLoading}>{backgroundTerminalLoading ? "…" : "↻"}</button></div>{backgroundTerminals.length === 0 ? <small className="inspector-empty">没有已登记的后台终端</small> : backgroundTerminals.map((terminal) => <div className="terminal-row" key={terminal.processId}><div><strong>{terminal.command ?? `进程 ${terminal.processId}`}</strong><small>{terminal.cwd ?? "工作目录未知"} · {terminal.status ?? "运行中"}</small></div><button onClick={() => onTerminateBackgroundTerminal(terminal.processId)}>终止</button></div>)}</div></div>;
  const renderChanges = <div className="inspector-stack"><div className="inspector-summary-line"><strong>文件与内容变更</strong><span>{agentRun.artifacts.length} 项</span></div>{agentRun.artifacts.length === 0 && !agentRun.diff ? <div className="context-empty">当前任务还没有报告变更。</div> : <>{agentRun.artifacts.map((artifact) => <div className="change-row" key={artifact.id}><span className={`change-status ${artifact.status}`}>{artifact.status === "applied" ? "✓" : artifact.status === "declined" ? "–" : artifact.status === "failed" ? "×" : "·"}</span><div><strong>{artifact.title}</strong><small>{artifactStatusLabel(artifact.status)}</small>{artifact.detail && <pre>{artifact.detail}</pre>}</div></div>)}{agentRun.diff && <details className="agent-diff"><summary>统一 diff</summary><pre>{agentRun.diff}</pre></details>}</>}</div>;
  const renderUsage = <div className="inspector-stack"><div className="usage-card"><span>本轮 Token</span><strong>{formatTokenCount(agentRun.tokenUsage?.totalTokens)}</strong><small>输入 {formatTokenCount(agentRun.tokenUsage?.inputTokens)} · 输出 {formatTokenCount(agentRun.tokenUsage?.outputTokens)}</small></div><div className="inspector-facts"><span><small>上下文窗口</small><strong>{formatTokenCount(agentRun.tokenUsage?.contextWindow)}</strong></span><span><small>压缩</small><strong>{agentRun.contextCompactions} 次</strong></span><span><small>开始时间</small><strong>{agentRun.startedAt ? formatDate(new Date(agentRun.startedAt).toISOString()) : "—"}</strong></span></div><small className="inspector-note">用量来自当前这一轮创作会话。</small></div>;
  const renderDiagnostics = <div className="inspector-stack"><div className="inspector-summary-line"><strong>运行告警</strong><span>{agentRun.diagnostics.length} 条</span></div><div className="diagnostic-row"><span className={`online-dot ${runtimeStatus.state === "online" ? "" : "offline"}`} /><span>创作运行时：{runtimeStatus.state === "online" ? "在线" : "离线"}</span></div><div className="diagnostic-row"><span className={`online-dot ${agentRun.connectionState === "ready" ? "" : "offline"}`} /><span>引擎连接：{connectionLabel(agentRun.connectionState)}</span></div>{agentRun.diagnostics.length === 0 ? <div className="context-empty">暂无运行告警。</div> : <div className="diagnostic-list">{agentRun.diagnostics.map((message, index) => <p key={`${message}-${index}`}>{message}</p>)}</div>}<ExecutionDiagnostics run={agentRun} /></div>;
  const content = contextTab === "story" ? renderStory : contextTab === "task" ? renderTask : contextTab === "changes" ? renderChanges : contextTab === "usage" ? renderUsage : contextTab === "diagnostics" ? renderDiagnostics : <>{renderSettings}{window.novelCodex && <MetricsPanel api={window.novelCodex} />}</>;
  return <aside className="context-panel inspector-open"><div className="panel-tabs inspector-tabs" role="tablist" aria-label="任务检查器">{tabs.map((tab) => <button key={tab.id} className={contextTab === tab.id ? "selected" : ""} role="tab" aria-selected={contextTab === tab.id} onClick={() => onContextTabChange(tab.id)}>{tab.label}</button>)}</div><div className="inspector-content">{content}</div><div className="context-footer"><span>{runtimeStatus.state === "online" ? "本地运行时在线" : "本地运行时未连接"}</span><span className={`online-dot ${runtimeStatus.state === "online" ? "" : "offline"}`} /></div></aside>;
}

function connectionLabel(state: AgentRun["connectionState"]): string { return state === "ready" ? "已连接" : state === "connecting" ? "连接中" : state === "reconnecting" ? "恢复中" : state === "error" ? "异常" : "离线"; }
function artifactStatusLabel(status: AgentRun["artifacts"][number]["status"]): string { return status === "applied" ? "已应用" : status === "declined" ? "已拒绝" : status === "failed" ? "失败" : "待确认"; }
function formatTokenCount(value: number | null | undefined): string { return value === null || value === undefined ? "—" : value.toLocaleString("zh-CN"); }

function messageClock(run: AgentRun): string { return new Date(run.startedAt ?? Date.now()).toLocaleTimeString("zh-CN", { hour: "2-digit", minute: "2-digit" }); }
function renderConversationAnswer(text: string) { return <MarkdownContent text={text}/>; }

type ChangeStats = { added: number; removed: number; known: boolean };

function changeStats(text: string): ChangeStats {
  let added = 0;
  let removed = 0;
  for (const line of text.split("\n")) {
    if (line.startsWith("+++") || line.startsWith("---")) continue;
    if (line.startsWith("+")) added += 1;
    if (line.startsWith("-")) removed += 1;
  }
  return { added, removed, known: added > 0 || removed > 0 };
}

function totalChangeStats(run: AgentRun): ChangeStats {
  const source = run.diff || run.artifacts.map((artifact) => artifact.detail ?? "").filter(Boolean).join("\n");
  return changeStats(source);
}

function ChangeStatsLabel({ stats }: { stats: ChangeStats }) {
  if (!stats.known) return null;
  return <span className="change-stats"><span className="change-stat-add">+{stats.added}</span><span className="change-stat-remove">−{stats.removed}</span></span>;
}

function ConversationChanges({ run, onReview }: { run: AgentRun; onReview: () => void }) {
  const [open, setOpen] = useState(true);
  if (run.artifacts.length === 0 && !run.diff) return null;
  const pending = run.artifacts.some((artifact) => artifact.status === "proposed");
  const stats = totalChangeStats(run);
  const fileLabel = run.artifacts.length > 0 ? `${run.artifacts.length} 个文件已更改` : "内容已更改";
  return <section className={`conversation-changes ${pending ? "pending" : ""}`} aria-label="文件变更"><div className="conversation-change-header"><div className="conversation-change-title"><span className="conversation-change-icon" aria-hidden="true">▧</span><div><strong>{fileLabel}</strong><ChangeStatsLabel stats={stats} /></div></div><div className="conversation-change-actions"><button className="conversation-change-review" onClick={onReview}>查看章节</button></div></div>{open && <div className="conversation-change-list">{run.artifacts.map((artifact) => { const artifactStats = changeStats(artifact.detail ?? ""); return <article className="artifact-row" key={artifact.id}><span className="artifact-icon" aria-hidden="true">{artifact.kind === "chapter-change" ? "▤" : "◇"}</span><div className="artifact-copy"><div className="artifact-heading"><strong>{artifact.title}</strong><ChangeStatsLabel stats={artifactStats} /></div>{artifact.detail && <details className="change-detail"><summary>{artifactStatusLabel(artifact.status)} · 查看详情</summary><pre>{artifact.detail}</pre></details>}</div></article>; })}{run.diff && <details className="agent-diff"><summary>查看统一 diff</summary><pre>{run.diff}</pre></details>}</div>}<button className="conversation-change-toggle" aria-expanded={open} onClick={() => setOpen((value) => !value)}>{open ? "⌃" : "⌄"}</button></section>;
}

function EarlierTurn({ projectId, entry, taskBound, finalReplyState }: { projectId: string; entry: AgentRun["history"][number] & { commentary?: string }; taskBound: boolean; finalReplyState?: FinalReplyDisplayState }) {
  const text = entry.answer.trim();
  const clock = new Date(entry.createdAt).toLocaleTimeString("zh-CN", { hour: "2-digit", minute: "2-digit" });
  return <div className="conversation-turn is-earlier">{!taskBound && entry.prompt.trim().length > 0 && <article className={`conversation-message ${entry.origin === "chapterStructure" ? "system-message" : "user-message"}`}><div className="conversation-message-body">{entry.origin === "chapterStructure" && <strong>自动检查</strong>}{entry.prompt && <ConversationPrompt text={entry.prompt} detailKey={`turn:${entry.id}:prompt`} />}<div className="conversation-user-meta">{entry.createdAt > 0 && <span>{clock}</span>}<button className="conversation-copy" title={entry.origin === "chapterStructure" ? "复制检查说明" : "复制我的创作指令"} aria-label={entry.origin === "chapterStructure" ? "复制检查说明" : "复制我的创作指令"} onClick={() => { if (navigator.clipboard) void navigator.clipboard.writeText(entry.prompt).catch(() => undefined); }}>▢</button></div></div></article>}<DiscussionExcerptCards excerpts={entry.excerpts ?? []} disabled={false} readOnly={false} />{entry.displayExecution && <ConversationTimeline key={`${entry.threadId}:${entry.turnId ?? entry.id}`} run={{ ...entry, ...entry.displayExecution }} retained={entry.displayExecution} />}{entry.displayExecution === null && <small>较早的执行过程未保留。</small>}<article className="conversation-message assistant-message"><div className="conversation-message-body">{entry.commentary && !entry.displayExecution && <div className="conversation-progress-notes"><MarkdownContent text={entry.commentary} /></div>}{text ? <ConversationOutput state={entry.state} identity={`turn:${entry.threadId}:${entry.turnId ?? entry.id}:answer`} text={text} finalReplyState={finalReplyState}><div className="conversation-answer">{renderConversationAnswer(text)}</div><ConversationReplyActions identity={`${projectId}:${entry.threadId}:${entry.id}`} text={entry.answer} label={isUnfinishedConversationTurn(entry.state) ? "复制未完成内容" : "复制回复"} /></ConversationOutput> : <div className="conversation-answer-empty">{entry.summary}</div>}</div></article></div>;
}

function EnhancedAgentTranscript({ projectId, run, taskBound, finalReplyState, userInputRequest, queuedUserInputCount, onResolveUserInput, onReview }: { projectId: string; run: AgentRun; taskBound: boolean; finalReplyState?: FinalReplyDisplayState; userInputRequest: UserInputRequest | null; queuedUserInputCount: number; onResolveUserInput: ResolveUserInput; onReview: () => void }) {
  if (run.state === "idle") return run.history.length ? <section className="agent-transcript conversation-panel" aria-label="恢复的创作对话">{[...run.history].reverse().map(entry => <EarlierTurn projectId={projectId} entry={entry} taskBound={false} key={entry.id} />)}</section> : null;
  const answer = run.answer.trim();
  const showAssistant = Boolean(answer) || (run.state !== "running" && !(run.state === "waiting" && userInputRequest));
  const earlier = [...run.history].reverse();
  return <section className="agent-transcript conversation-panel" id="agent-transcript" aria-label="创作引擎对话">{earlier.map((entry) => <EarlierTurn projectId={projectId} entry={entry} taskBound={false} key={entry.id} />)}<div className={`conversation-turn is-${run.state}`}>{!taskBound && run.prompt.trim().length > 0 && <article className={`conversation-message ${run.origin === "chapterStructure" ? "system-message" : "user-message"}`}><span className="transcript-avatar user">{run.origin === "chapterStructure" ? "·" : "你"}</span><div className="conversation-message-body"><div className="conversation-message-meta"><strong>{run.origin === "chapterStructure" ? "自动检查" : "你的创作指令"}</strong><span>{run.origin === "chapterStructure" ? "故事衔接" : "本轮输入"}</span></div><ConversationPrompt text={run.prompt} detailKey={`turn:${run.threadId}:${run.turnId ?? run.startedAt}:prompt`} /><div className="conversation-user-meta"><span>{messageClock(run)}</span><button className="conversation-copy" title={run.origin === "chapterStructure" ? "复制检查说明" : "复制我的创作指令"} aria-label={run.origin === "chapterStructure" ? "复制检查说明" : "复制我的创作指令"} onClick={() => { if (navigator.clipboard) void navigator.clipboard.writeText(run.prompt).catch(() => undefined); }}>▢</button></div></div></article>}<DiscussionExcerptCards excerpts={run.excerpts ?? []} disabled={false} readOnly={false} /><ConversationTimeline key={`${run.threadId}:${run.turnId ?? run.startedAt}`} run={run} showFailure={!run.error}/>{showAssistant && <article className="conversation-message assistant-message"><span className="conversation-avatar assistant" aria-hidden="true">✦</span><div className="conversation-message-body"><div className="conversation-message-meta"><strong>Novel Engine</strong><span>{answer ? isUnfinishedConversationTurn(run.state) ? "未完成内容" : "回复" : run.state === "running" ? "正在处理" : "暂无文本回复"}</span></div>{answer ? <ConversationOutput state={run.state} identity={`turn:${run.threadId}:${run.turnId ?? run.startedAt}:answer`} text={answer} finalReplyState={finalReplyState}><div className="conversation-answer">{renderConversationAnswer(answer)}{run.state === "running" && <span className="stream-cursor" aria-label="正在接收内容">▋</span>}</div><ConversationReplyActions identity={`${projectId}:${run.threadId}:${run.turnId ?? run.startedAt}`} text={run.answer} label={isUnfinishedConversationTurn(run.state) ? "复制未完成内容" : "复制回复"} /></ConversationOutput> : <div className="conversation-answer-empty" aria-live="polite"><span className="conversation-answer-pulse" aria-hidden="true">•</span>{run.state === "waiting" ? "等待你的回答后继续。" : run.state === "running" ? "正在读取故事上下文并生成回复……" : "本轮没有文本回复。"}</div>}</div></article>}{userInputRequest && <div className="conversation-interaction"><div className="conversation-interaction-heading"><span>需要你的回答</span><small>回答后创作引擎会继续当前回合{queuedUserInputCount > 0 ? ` · 之后还有 ${queuedUserInputCount} 个问题` : ""}</small></div><AuthorQuestionCard requestIdentity={JSON.stringify([projectId, userInputRequest.threadId ?? run.threadId, userInputRequest.turnId ?? run.turnId, userInputRequest.itemId ?? null, typeof userInputRequest.requestId, userInputRequest.requestId])} questions={userInputQuestions(userInputRequest.params)} onResolve={onResolveUserInput}/></div>}{run.error && <div className="transcript-error" role="alert"><strong>本轮未完成</strong><span>{describeAgentError(run.error)}</span>{describeAgentError(run.error) !== run.error && <details><summary>查看错误详情</summary><pre>{run.error}</pre></details>}</div>}<ConversationChanges run={run} onReview={onReview}/>{run.summary && !answer && <div className="conversation-result"><strong>{run.state === "completed" ? "本轮完成" : "本轮状态"}</strong><p>{run.summary}</p></div>}</div></section>;
}

function CommandPalette({ onClose, onFocusComposer, onReview, onReconnect, onTheme }: { onClose: () => void; onFocusComposer: () => void; onReview: () => void; onReconnect: () => void; onTheme: () => void }) {
  const commands = [{ label: "聚焦创作输入", hint: "⌘↵", action: onFocusComposer }, { label: "查看作品目录", hint: "", action: onReview }, { label: "恢复创作引擎连接", hint: "", action: onReconnect }, { label: "切换主题", hint: "", action: onTheme }];
  return <div className="command-palette-backdrop" role="presentation" onMouseDown={(event) => { if (event.currentTarget === event.target) onClose(); }}><section className="command-palette" role="dialog" aria-modal="true" aria-label="命令面板"><div className="command-palette-heading"><strong>命令面板</strong><button onClick={onClose} aria-label="关闭命令面板">Esc</button></div><div className="command-palette-list">{commands.map((command) => <button key={command.label} onClick={command.action}><span>✦</span><strong>{command.label}</strong><small>{command.hint}</small></button>)}</div></section></div>;
}

export default App;
