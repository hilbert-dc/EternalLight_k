import { ConversationPanel } from "./ConversationPanel";
import { useConversationSubmission } from "./conversation/useConversationSubmission";
import { useConversationProtocol } from "./conversation/useConversationProtocol";

import { useConversationExecution } from "./conversation/useConversationExecution";
import { useConversationGeneration, useConversationRecovery, useConversationTaskCommands } from "./conversation/execution-adapters";
import { useEditionNavigation } from "./multimedia/useEditionNavigation";
import { useConversationReader } from "./useConversationReader";
import { useConversationDirectory } from "./useConversationDirectory";
import { ReadonlyThreadHistory } from "./ReadonlyThreadHistory";
import { useThreadHistoryDetail } from "./useThreadHistoryDetail";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import type { CreativeEdition, CreativeMedium } from "../shared/multimedia-types";

import { createAgentRun, type AgentRun } from "./agent-events";

import { AgentStatusBar } from "./AgentStatus";
import { EarlierTurn, EnhancedAgentTranscript } from "./AgentTranscript";
import type { AgentConnection, AgentStatus } from './app-workspace-types';
import { CommandPalette, navGroups, ProjectSwitcher, Sidebar, UnsavedChangesDialog } from "./AppShell";
import { clearAuthorDrafts } from "./author-draft-storage";
import { authorInitial, hashPassword, type AuthorProfile } from "./author-session";

import { lastChapter, rememberChapter } from "./chapter-location";

import type { ChapterTitlePreview } from "./chapter-title";
import { chapterWritingMatches, type ChapterWritingRequest } from "./chapter-writing";
import { ChapterDeleteDialog } from "./ChapterDeleteDialog";
import { ChapterStructureNotice, type StructureCheckFailure } from "./ChapterStructureNotice";
import { CharacterCatalog } from "./CharacterCatalog";
import { resolveCommitmentNavigation } from "./commitment-navigation";

import "./conversation-workspace.css";
import { ConversationComparison } from "./ConversationComparison";
import { Composer } from "./ConversationComposer";
import { ConversationExecutionStatus } from "./ConversationExecutionStatus";
import { ConversationMessageQueue } from "./ConversationMessageQueue";
import { ConversationNavigator } from "./ConversationNavigator";
import { type FinalReplyDisplayState } from "./ConversationOutput";
import { ConversationPrompt } from "./ConversationPrompt";
import { ConversationReader } from "./ConversationReader";
import { ConversationRecovery } from "./ConversationRecovery";

import { CreativeWorkspaceView } from "./CreativeWorkspaceView";
import { DesiredOutcomes } from "./DesiredOutcomes";
import { DiscussionSourceChip } from "./DiscussionSourceChip";
import { DiscussionSyncNotice } from "./DiscussionSyncNotice";
import "./execution-recovery.css";

import { ExecutionScopeDialog } from "./ExecutionScopeDialog";
import { ExportDialog } from "./ExportDialog";
import { IdentityDecisionPanel } from "./IdentityDecisionPanel";
import { LoginScreen } from "./LoginScreen";
import { ModelConnectionNotice } from "./ModelConnectionNotice";
import { ModuleRecordsPopover, type ModuleRecordSource } from "./ModuleRecordsPopover";
import { CreateMediaEditionDialog } from "./multimedia/CreateMediaEditionDialog";
import { readSelectedEdition, rememberSelectedEdition } from "./multimedia/edition-selection";
import { EditionSwitcher } from "./multimedia/EditionSwitcher";
import { OutlineVolumeNavigation } from "./OutlineVolumeNavigation";
import "./page-layout.css";
import { FailedRevisionRepair, PreparedRevisionRepair } from "./PreparedRevisionRepair";
import { ProjectCreateDialog } from "./ProjectCreateDialog";
import { ProjectInformation } from "./ProjectInformation";
import { PropertyRulesPanel } from "./PropertyRulesPanel";
import { ReadFailureNotice } from "./ReadFailureNotice";
import { RuntimeConnectionStatus } from "./RuntimeConnectionStatus";
import { SavedAnalysisDialog } from "./SavedAnalysisDialog";
import { searchNavigation, SearchResults } from "./SearchResults";
import { DiscussionExcerptCards, SelectionDiscussion } from "./SelectionDiscussion";
import { SidebarWorkDirectory, type SidebarChapterCreationRequest } from "./SidebarWorkDirectory";
import { StoryReferenceView } from "./StoryReferenceView";
import { compactTaskName, taskOperationName } from "./task-operation-presentation";
import { taskResultDiscussionExcerpt, type TaskResultDiscussion } from "./task-result-excerpt";
import { resolveTaskResult, type ResolvedTaskResult } from "./task-result-navigation";
import type { CommitmentDecisionResult, TaskResultDestination } from "./task-results";
import { threadHistory } from "./thread-history";
import type { BackgroundTerminal, Chapter, CommitmentDetail, CommitmentNavigationRequest, CommitmentNavigationSelection, ConversationFocus, DiscussionExcerpt, DiscussionSource, ExecutionScopeConfirmation, ProjectSummary, ProposedAction, RecoveryRun, RuntimeState, RuntimeStatus, SavedAnalysis, ThreadSummary, ViewId } from "./types";
import { useAuthorSession } from "./use-author-session";
import { useAgentAvailability } from "./useAgentAvailability";
import { useChapterDeletion } from "./useChapterDeletion";
import { useComposerDrafts } from "./useComposerDrafts";
import { useConversationOutbox } from "./useConversationOutbox";

import { useModelConnection } from "./useModelConnection";
import { usePlanningSubmission } from "./usePlanningSubmission";
import { useReasoningSelection } from "./useReasoningSelection";
import { useRevisionRepair } from "./useRevisionRepair";
import { useSelectedConversation } from "./useSelectedConversation";
import { useWorkCatalog } from "./useWorkCatalog";
import { useWorkProjectionRefresh } from "./useWorkProjectionRefresh";
import { useWorkspaceLayout } from "./useWorkspaceLayout";
import { useWorkspaceSearch } from "./useWorkspaceSearch";
import { useWorkTaskControls } from "./useWorkTaskControls";
import { useWorkTaskList } from "./useWorkTaskList";
import { attemptContainsRun, type WorkConversationContent } from "./work-conversation";
import { WorkInterludePanel } from "./WorkInterludePanel";
import { MediaApplication, UserSettings, WorkspaceDiagnostics } from "./DeferredWorkspaceViews";
import { PaneSash } from "./WorkspaceLayout";
import { WorkTaskPanel } from "./WorkTaskPanel";

import { ConversationGenerationHandoff } from "./ConversationGenerationHandoff";

import type { RevisionContinuationPreparation } from "./types";

const DEFAULT_PROJECT_ID = "project-last-lamp";
const MAX_PENDING_INTERACTIONS = 4;

type PendingNavigation = { kind: "edition"; edition: CreativeEdition } | { kind: "new-edition" } | { kind: "delete-chapter"; projectId: string; chapterId: string } | { kind: "volume"; projectId: string; volumeId: string; expectedNodeId?: string } | { kind: "new-chapter"; request: SidebarChapterCreationRequest } | { kind: "commitment"; request: CommitmentNavigationRequest } | { kind: "commitment-edit"; result: CommitmentDecisionResult; detail: CommitmentDetail } | { kind: "result"; result: TaskResultDestination } | { kind: "view"; view: ViewId } | { kind: "chapter"; chapterId: string } | { kind: "project"; projectId: string } | { kind: "new-project" } | { kind: "close" } | { kind: "account" };
const emptyState: RuntimeState = {
  project: { id: "", title: "正在加载作品…", logline: "", created_at: 0, updated_at: 0 },
  canon: { characters: [], relationships: [], events: [], locations: [], plotThreads: [], worldRules: [], notes: [] },
  narrativeIr: { version: "1.0", scenes: [], anchors: [] },
  novelEdition: { chapters: [] },
};

function App() {
  const api = window.novelCodex;
  const execution = useConversationExecution(api);
  const { questions: userInputQueue, writing: chapterWriting, writingPending: chapterWritingPending,
    activity, run: agentRun, handoff: generationHandoff, selectWriting: selectChapterWriting } = execution;
  const [theme, setTheme] = useState<"light" | "dark">(() => {
    try { return localStorage.getItem("novel-codex-theme") === "dark" ? "dark" : "light"; } catch { return "light"; }
  });
  const authorSession = useAuthorSession();
  const { author, signedIn } = authorSession;
  const [accountOpen, setAccountOpen] = useState(false);
  const [projectInformationOpen, setProjectInformationOpen] = useState(false);
  const [activeView, setActiveView] = useState<ViewId>("chapters");
  const [state, setState] = useState<RuntimeState>(emptyState);
  const [activeProjectId, setActiveProjectId] = useState(() => {
    try { return localStorage.getItem("novel-codex-active-project") ?? DEFAULT_PROJECT_ID; } catch { return DEFAULT_PROJECT_ID; }
  });
  const [projects, setProjects] = useState<ProjectSummary[]>([]);
  const [projectLoading, setProjectLoading] = useState(true);
  const editionNavigation = useEditionNavigation({ api, projectId: activeProjectId, authorKey: author?.email ?? "",
    onBegin: () => setProjectLoading(true), onEnd: () => setProjectLoading(false),
    onLoaded: novel => { resetProjectSession(); if (novel) { setState(novel); setSelectedChapterId(lastChapter(activeProjectId, novel.novelEdition.chapters)); } },
  });
  const { context: mediaContext, busy: mediaBusy, catalog: editionCatalog, creating: editionCreateOpen,
    source: editionSource, error: editionError, epoch: editionNavigationEpoch, opening: editionOpening,
    open: openEdition, reportError: setEditionError } = editionNavigation;
  const mediaContextRef = useRef(mediaContext); mediaContextRef.current = mediaContext;

  const [exportOpen, setExportOpen] = useState(false);
  const [projectCreateOpen, setProjectCreateOpen] = useState(false);
  const [runtimeStatus, setRuntimeStatus] = useState<RuntimeStatus>({ state: "offline", mode: "local", error: "正在连接 Creative Runtime…" });
  const [agentStatus, setAgentStatus] = useState<AgentStatus>({ configured: false, state: "idle", mode: "novel-agent-engine", error: null });
  const [agentConnections, setAgentConnections] = useState<AgentConnection[]>([]);
  const [selectedChapterId, setSelectedChapterId] = useState("");
  const [proposedActions, setProposedActions] = useState<{ projectId: string; actions: ProposedAction[] }>({ projectId: "", actions: [] });
  const [approvingActionId, setApprovingActionId] = useState<string | null>(null);
  const [scopeAction, setScopeAction] = useState<{ projectId: string; action: ProposedAction } | null>(null);
  const currentProjectRef = useRef(activeProjectId);
  currentProjectRef.current = activeProjectId;
  const [recoveryRuns, setRecoveryRuns] = useState<RecoveryRun[]>([]);
  const scopeExecutionRef = useRef<{ projectId: string; scope: object } | null>(null);
  const { layoutStyle, sidebarWidth, sidebarMax, setPanes, railWidth, railMax, panes, inspectorMax } = useWorkspaceLayout();
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
  const [chapterDraftScope, setChapterDraftScope] = useState<{ projectId: string; chapterId: string } | null>(null);
  const chapterDraftScopeRef = useRef(chapterDraftScope);
  const creativeDirtyRef = useRef(creativeDirty);
  const setCreativeDirty = useCallback((value: boolean, scope: { projectId: string; chapterId: string } | null = null) => {
    creativeDirtyRef.current = value; updateCreativeDirty(value);
    chapterDraftScopeRef.current = value ? scope : null;
    setChapterDraftScope(current => current?.projectId === (value ? scope?.projectId : undefined) && current?.chapterId === (value ? scope?.chapterId : undefined) ? current : value ? scope : null);
  }, []);
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
  const currentSentRepair = execution.sentRepair;
  const [commandPaletteOpen, setCommandPaletteOpen] = useState(false);
  const composerRef = useRef<HTMLTextAreaElement | null>(null);
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
  const readonlyHistory = useThreadHistoryDetail(api, projectId);
  const readonlyHistoryOpen = Boolean(readonlyHistory.detail || readonlyHistory.loading || readonlyHistory.error);
  const conversationProjectReady = signedIn && !mediaContext && Boolean(projectId) && projectId === activeProjectId;
  const { reading: conversationReading, refresh: readSelectedConversation } = useConversationReader(api, projectId, selectedConversation.id, author?.email ?? "", conversationProjectReady);
  const directory = useConversationDirectory({ api, projectId, authorKey: author?.email ?? "", enabled: conversationProjectReady,
    configured: agentStatus.configured, changing: conversationChanging,
    isRunning: execution.isConversationRunning,
    onArchived: id => { if (id === conversationSelectionRef.current.id) selectConversation(null); },
  });
  const { threads, loading: threadLoading, error: threadAction, archived: threadArchived, report: setThreadAction,
    refresh: refreshThreads, pin: toggleThreadPin, archive: toggleThreadArchive, toggleArchived: toggleThreadArchiveView, rename: renameConversation } = directory;
  const selectedThread = threads.find(thread => thread.id === selectedConversation.id) ?? (conversationReading?.projectId === projectId && conversationReading.threadId === selectedConversation.id ? conversationReading.detail?.thread : null);
  const executionConversationId = execution.conversationId;
  const viewingExecution = execution.isViewing(projectId, selectedConversation.id);
  const activeConversationDetail = conversationReading?.projectId === projectId && conversationReading.threadId === selectedConversation.id ? conversationReading : null;
  const sharedStateOwner = useMemo(() => ({}), [api, projectId, activeProjectId, signedIn]);
  const sharedStateOwnerRef = useRef(sharedStateOwner); sharedStateOwnerRef.current = sharedStateOwner;
  const promptPipelineBusy = execution.isPipelineBusy(projectId);
  const baseExecutionBusy = Boolean(promptPipelineBusy) || agentRun.state === "running" || agentRun.state === "waiting";
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
  execution.validateWritingRoute(writingRoute);
  useEffect(() => {
    if (!chapterWritingMatches(chapterWriting, writingRoute)) selectChapterWriting(null);
  }, [projectId, activeProjectId, activeView, accountOpen, selectedChapterId, conversationProjectReady]);
  function prepareChapterWriting(request: ChapterWritingRequest) {
    if (!chapterWritingMatches(request, writingRouteRef.current) || currentProjectRef.current !== request.projectId || !execution.canPrepareWriting()) return;
    revisionRepair.cancel();
    execution.dismissRepair();
    composerDrafts.setSource(null); setPendingDiscussionSource(null);
    selectChapterWriting(request);
    setComposer(value => value.trim() ? value : request.defaultIntent);
    composerRef.current?.focus();
  }
  const taskList = useWorkTaskList(api, projectId, creativeRefreshToken, baseExecutionBusy, conversationProjectReady);
  useWorkProjectionRefresh(taskList.scope, projectId, taskList.tasks, taskList.loaded,
    () => setCreativeRefreshToken(value => value + 1));
  const liveWorkListRef = useRef(taskList.tasks); liveWorkListRef.current = taskList.tasks;
  const taskControlScopeRef = useRef(taskList.scope);
  taskControlScopeRef.current = taskList.scope;
  const [taskControlMessage, setTaskControlMessage] = useState<{ scope: object; error: { targetId: string; message: string } | null; notice: { targetId: string; message: string } | null } | null>(null);
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
      return execution.resumeWork(api, work, projectId,
        () => currentProjectRef.current === projectId && taskControlScopeRef.current === controlOwner,
        () => beginExecutionScope({ kind: "taskRecovery", operation: "继续原任务", view: activeView, intent: work.goal }));
    },
    onControlApplied: workTaskId => execution.controlApplied(api, workTaskId, currentProjectRef.current),
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
      execution.notice("章节已从作品目录和正文中删除。");
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
      execution.applyConnection(next);
      execution.notice(`已应用模型：${next.model ?? "当前模型"}。`);
    } });
  const reasoningSelection = useReasoningSelection({ api, provider: agentStatus.provider, model: agentStatus.model,
    onChange: reasoningEffort => setAgentStatus(current => ({ ...current, reasoningEffort })) });
  const agentConfigSaving = modelConnection.busy;
  const search = useWorkspaceSearch(api, projectId);
  const composerDrafts = useComposerDrafts(projectId, selectedConversation.id, author?.email ?? "");
  const conversationScope = JSON.stringify([author?.email ?? "", projectId, selectedConversation.id]);
  const outbox = useConversationOutbox(api, projectId, selectedConversation.id);
  const [comparison, setComparison] = useState<{ projectId: string; threadId: string; sourceId: string } | null>(null);
  const [recoveryProject, setRecoveryProject] = useState<string | null>(null);

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
    void composerDrafts.addExcerpt(newConversationExcerpt.excerpt).then(added => {
      if (added) requestAnimationFrame(() => composerRef.current?.focus({ preventScroll: true }));
    });
    setNewConversationExcerpt(null);
  }, [newConversationExcerpt, selectedConversation.id, projectId]);
  function selectConversation(thread: ThreadSummary | null) {
    if (!conversationProjectReady || conversationChanging || !composerDrafts.preserve()) return;
    if (thread && thread.projectId !== projectId) { setThreadAction("对话不属于当前作品"); return; }
    if ((thread?.id ?? null) !== selectedConversation.id) { resultNavigation.current += 1; setRequestedCommitmentEdit(null); }
    execution.activateConversation(projectId, thread?.id ?? null);
    selectedConversation.select(thread?.id ?? null);
    setPendingDiscussionSource(null); selectChapterWriting(null); revisionRepair.cancel();
    requestAnimationFrame(() => composerRef.current?.focus());
  }
  async function createConversation(excerpt?: DiscussionExcerpt, lastTurnId?: string) {
    if (!api || !conversationProjectReady || conversationChangeRef.current || !composerDrafts.preserve()) return false;
    const ticket = {}; conversationChangeRef.current = ticket; setConversationChanging(true); setThreadAction(null);
    try {
      const result = await api.agent.createThread(projectId, lastTurnId && selectedConversation.id ? { sourceThreadId: selectedConversation.id, lastTurnId } : {});
      if (conversationChangeRef.current !== ticket || currentProjectRef.current !== projectId) return false;
      if (result.thread.projectId !== projectId || result.thread.isArchived) throw new Error("新对话回执不属于当前作品");
      directory.register(result.thread);
      // Selection is published only after the persistent registration receipt.
      resultNavigation.current += 1; setRequestedCommitmentEdit(null);
      execution.activateConversation(projectId, result.thread.id, true);
      selectedConversation.select(result.thread.id);
      setPendingDiscussionSource(null); selectChapterWriting(null); revisionRepair.cancel();
      if (excerpt) setNewConversationExcerpt({ projectId, threadId: result.thread.id, excerpt });
      requestAnimationFrame(() => composerRef.current?.focus());
      return true;
    } catch (error) {
      if (currentProjectRef.current === projectId) setThreadAction(`创建对话失败：${error instanceof Error ? error.message : "请重试"}`);
      return false;
    } finally { if (conversationChangeRef.current === ticket) { conversationChangeRef.current = null; setConversationChanging(false); } }
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

  useConversationProtocol(execution, { api, projectId, currentProjectRef, conversationProjectReady, selectedConversation,
    conversationSelectionRef, mediaContextRef, liveWorkListRef, composerDraftsRef, directory,
    onRefresh: () => setCreativeRefreshToken(value => value + 1),
    onHealth: health => setAgentStatus(current => ({ ...current, executionHealth: health })),
    onStructureEvent: (method, params) => {
        if (method === "structure/error" && typeof params.message === "string") {
          const failure = { projectId: currentProjectRef.current, changeId: typeof params.changeId === "string" ? params.changeId : null, message: params.message.slice(0, 240) };
          setStructureFailures(current => [...current.filter(item => item.projectId !== failure.projectId || item.changeId !== failure.changeId), failure]);
          setCreativeRefreshToken(value => value + 1);
        }
        if (method === "agent/structure-updated") {
          if (params.completed === true) setStructureFailures(current => current.filter(item => item.projectId !== params.projectId || item.changeId !== params.changeId));
          setCreativeRefreshToken(value => value + 1);
        }
        if (method === "agent/structure-started") setCreativeRefreshToken(value => value + 1);
    },
  });
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
    canApply: () => !execution.isSubmitting() && !execution.isRunning(), onStatus: setAgentStatus });
  const revisionRepair = useRevisionRepair({ api, projectId, activeProjectId, composer,
    blocked: creativeDirty || !agentAvailable || executionBusy });
  resultDiscussionControls.current = { api, executionBusy, conversationChanging, writing: currentChapterWriting, repair: Boolean(revisionRepair.prepared), archived: Boolean(selectedThread?.isArchived) };
  const planningSubmissionEligible = conversationProjectReady && !accountOpen && ["brief", "outline", "plans"].includes(activeView)
    && agentAvailable && !executionBusy && !currentChapterWriting && !composerDrafts.source && !composerDrafts.excerpts.length && !pendingDiscussionSource
    && !revisionRepair.prepared && !revisionRepair.checking && !currentSentRepair;
  const planningSubmission = usePlanningSubmission({ api, projectId, view: accountOpen ? "account" : activeView,
    eligible: planningSubmissionEligible, composer, sourceIdentity: JSON.stringify(composerDrafts.source ?? null),
    canSendAfterSave: () => !creativeDirtyRef.current && !taskControls.hasPendingRequests() && !execution.isSubmitting() });
  const { beginExecutionScope, reportExecutionFailure, retryExecution, currentRunCanSteer, queuePrompt, submitPrompt, resolveUserInput } = useConversationSubmission(execution, {
    api, route: { projectId, activeProjectId, activeView, accountOpen, selectedChapter, agentAvailable, currentProjectRef },
    conversation: { selectedConversation, reading: Boolean(activeConversationDetail?.loading), archived: Boolean(selectedThread?.isArchived), conversationChanging, viewingExecution, executionBusy, executionConversationId, refreshThreads },
    composer: { composerDrafts, outbox, setComposer, pendingDiscussionSource },
    preparation: { revisionRepair, planningSubmission, planningSubmissionEligible, savePlanning: () => planningSaveHandlerRef.current, writingRouteRef },
    workspace: { taskList, taskControls, liveWorkListRef, creativeDirty, creativeDirtyRef, discussionSyncRef, canDiscussSavedChapter },
    proposals: { prepare: projectId => setProposedActions(current => current.projectId === projectId ? current : { projectId, actions: [] }), receive: (projectId, actions) => setProposedActions({ projectId, actions }) },
    onRefresh: () => setCreativeRefreshToken(value => value + 1),
    onModelUnavailable: message => setAgentStatus(status => ({ ...status, configured: false, state: "error", error: message })),
    readback: async (promptScope, promptIsCurrent) => {
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

    },
  });
  useEffect(() => {
    const pending = deferredDiscussionSync.current;
    if (!pending) return;
    if (pending.api !== api || pending.projectId !== projectId || pending.scope !== execution.scope) { deferredDiscussionSync.current = null; return; }
    if (creativeDirty || executionBusy || execution.isSubmitting() || discussionSyncRef.current || planningSubmission.isPending()) return;
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
    let cancelled = false, loaded = false, loading = false;
    setProjectLoading(true);
    async function load() {
      if (editionOpening.current || loaded || loading) return;
      loading = true;
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
          execution.applyConnection(agent, true);
          if (!recoveryNoticeShownRef.current && recoveryRuns.length) {
            recoveryNoticeShownRef.current = true;
            execution.notice("部分对话的保存结果待核对，可在对应作品的对话栏查看。");
          }
        }
        try {
          const available = await api.runtime.listProjects();
          if (!cancelled) setProjects(available.projects);
          if (cancelled) return;
          if (available.projects.length === 0) {
            editionNavigation.presentContext(null);
            setState({ ...emptyState, project: { id: "", title: "创建你的第一部作品", logline: "", created_at: 0, updated_at: 0 } });
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
        editionNavigation.receiveCatalog({ projectId: activeProjectId, editions, selectedId: edition?.id ?? null });
        setEditionError(null);
        if (!editions.length) throw new Error("作品尚无可编辑版本");
        if (!edition) return;
        if (edition.medium !== "novel") {
          const workspace = await api.multimedia.readWorkspace(activeProjectId, edition.id, edition.branchId);
          if (!cancelled && editionTicket === editionNavigationEpoch.current) { editionNavigation.presentContext({ project: workspace.project, edition: workspace.edition }); rememberSelectedEdition(author?.email ?? "", activeProjectId, edition.id); }
          return;
        }
        const next = await api.runtime.getState(activeProjectId);
        loaded = true;
        if (!cancelled && editionTicket === editionNavigationEpoch.current) editionNavigation.presentContext(null);
        if (!cancelled && editionTicket === editionNavigationEpoch.current) { rememberSelectedEdition(author?.email ?? "", activeProjectId, edition.id); setState(next); setSelectedChapterId(current => next.novelEdition.chapters.some(chapter => chapter.id === current) ? current : lastChapter(activeProjectId, next.novelEdition.chapters)); }
      } catch (error) {
        if (!cancelled) setRuntimeStatus({ state: "offline", mode: "local", error: error instanceof Error ? error.message : "Creative Runtime 不可用" });
      } finally { loading = false; if (!cancelled) setProjectLoading(false); }
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
    const selectedProjectId = signedIn && author && runtimeStatus.state === "online" && projectId === activeProjectId ? projectId : null;
    const edition = mediaContext?.project.id === selectedProjectId && !chapterDeletion.active && !workDirectory.busy ? mediaContext.edition : null;
    void api.agent.setWorkspaceContext({ projectId: selectedProjectId, dirty: creativeDirty || chapterDeletion.active || workDirectory.busy,
      chapterScope: !edition && !chapterDeletion.active && !workDirectory.busy && chapterDraftScope?.projectId === selectedProjectId ? chapterDraftScope : null,
      mediaScope: edition ? {projectId: edition.projectId, editionId: edition.editionId, scopeId: edition.scopeId, branchId: edition.branchId} : null }).catch(() => {});
    return () => { void api.agent.setWorkspaceContext({ projectId: null, dirty: false, chapterScope: null, mediaScope: null }).catch(() => {}); };
  }, [api, projectId, activeProjectId, mediaContext, creativeDirty, chapterDraftScope, chapterDeletion.active, workDirectory.busy, signedIn, author, runtimeStatus.state]);

  const registerCreativeSaveHandler = useCallback((handler: (() => Promise<boolean>) | null) => { creativeSaveHandlerRef.current = handler; }, []);
  const registerCreativeDiscardHandler = useCallback((handler: (() => boolean) | null) => { creativeDiscardHandlerRef.current = handler; }, []);
  function applyNavigation(action: PendingNavigation) {
    if (action.kind === "edition") { void openEdition(action.edition).catch(reason => setEditionError(reason instanceof Error ? reason.message : "作品版本未打开，请重试。")); return; }
    if (action.kind === "new-edition") { editionNavigation.showCreator(true); return; }
    if (action.kind === "delete-chapter") {
      if (action.projectId === currentProjectRef.current && conversationProjectReady && !executionBusy && !creativeDirtyRef.current) chapterDeletion.open(action.chapterId);
      return;
    }
    if (chapterDeletion.active) return;
    if (action.kind === "volume" && (!conversationProjectReady || action.projectId !== currentProjectRef.current
      || executionBusy || workDirectory.busy || execution.isSubmitting() || execution.isRunning()
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
        if (creativeDirtyRef.current || execution.isSubmitting() || execution.isRunning()
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
        if (creativeDirtyRef.current || execution.isSubmitting() || execution.isRunning()) {
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
        || execution.isSubmitting() || execution.isRunning()) return;
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
      execution.notice("正在打开作品…");
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
    execution.leaveProject();
    editionNavigation.clearTransient();
    setRecoveryProject(null);
    setModuleRecords(null);
    setSelectedOutlineVolume(null);
    setRequestedChapterCreation(null);
    resultNavigation.current += 1;
    setCommitmentSelection(null); setRequestedCommitmentEdit(null); setPendingDiscussionSource(null);
    setSavedAnalysis(null);
    setTaskResultSelection(null);
    setTaskResultError(null);
    revisionRepair.cancel();
    directory.reset();
    deferredDiscussionSync.current = null;
    setDiscussionSyncNotice(null);
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
  function selectEdition(edition: CreativeEdition) { if (!executionBusy && !projectLoading && edition.id !== editionCatalog.selectedId) requestNavigation({ kind: "edition", edition }); }
  async function createProject(title: string, logline: string, medium: CreativeMedium, idempotencyKey: string) {
    if (!api) throw new Error("桌面连接未加载，请重新打开应用。");
    const next = await api.runtime.createProject(title, logline, medium, idempotencyKey);
    setProjects((items) => [next.project, ...items.filter((project) => project.id !== next.project.id)]);
    const edition = next.editions.find(item => item.id === next.activeEditionId);
    if (!edition || edition.medium !== medium) throw new Error("作品创建回执未包含所选作品形式");
    editionNavigation.receiveCatalog({ projectId: next.project.id, editions: next.editions, selectedId: edition.id });
    rememberSelectedEdition(author?.email ?? "", next.project.id, edition.id);
    if (medium === "novel") {
      const projection = await api.runtime.getState(next.project.id);
      setState(projection); editionNavigation.presentContext(null);
      setSelectedChapterId(projection.novelEdition.chapters[0]?.id ?? "");
    } else editionNavigation.presentContext({ project: next.project, edition });
    setActiveProjectId(next.project.id);
    resetProjectSession();
    execution.notice(`作品“${next.project.title}”已创建。`);
  }
  useEffect(() => {
    if (!api || !conversationProjectReady) return;
    let cancelled = false;
    void api.runtime.getState(projectId).then(next => {
      if (!cancelled && currentProjectRef.current === projectId && next.project.id === projectId) { setState(next); setFormalReadError(null); }
    }).catch(() => { if (!cancelled && currentProjectRef.current === projectId) setFormalReadError({ projectId, message: "章节目录尚未刷新，已保存内容仍保留。" }); });
    return () => { cancelled = true; };
  }, [api, projectId, conversationProjectReady, formalStateRefreshToken]);
  // Refresh metadata without navigating or replacing an unsaved editor.
  useEffect(() => {
    if (executionBusy || projectLoading) return;
    void editionNavigation.refresh();
  }, [api, activeProjectId, executionBusy, projectLoading]);
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
    if (!api || discussionSyncRef.current || creativeDirtyRef.current || execution.isSubmitting()) return;
    const operation = {};
    const scope = execution.scope;
    const isCurrent = () => execution.scope === scope && currentProjectRef.current === projectId;
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
  async function syncExecutionWorkspace(label: string, scope = execution.scope) {
    const isCurrent = () => execution.scope === scope && currentProjectRef.current === projectId;
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
  function canDiscussSavedChapter(focus: ConversationFocus | null | undefined) {
    return chapterDraftScopeRef.current?.projectId === projectId && !chapterDeletion.active && !workDirectory.busy
      && focus?.view === "chapters" && Boolean(focus.chapterId);
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
  async function discussSavedResult(request: TaskResultDiscussion) {
    const { result } = request;
    if (!api || !conversationProjectReady || executionBusy || conversationChanging || selectedThread?.isArchived || result.projectId !== projectId) return;
    if (currentChapterWriting || revisionRepair.prepared) {
      setTaskResultError("输入框已有待发送的创作来源，请先发送或取消该指令，再引用成果。");
      return;
    }
    const ticket = ++resultDiscussionRequest.current, navigation = resultNavigation.current;
    const threadId = selectedConversation.id, version = composerDrafts.version(), scope = execution.scope;
    const current = () => ticket === resultDiscussionRequest.current && navigation === resultNavigation.current
      && currentProjectRef.current === projectId && conversationSelectionRef.current.id === threadId
      && execution.scope === scope && composerDraftsRef.current.version() === version
      && resultDiscussionControls.current.api === api && !resultDiscussionControls.current.executionBusy
      && !resultDiscussionControls.current.conversationChanging && !resultDiscussionControls.current.archived
      && !resultDiscussionControls.current.writing && !resultDiscussionControls.current.repair && !execution.isSubmitting();
    try {
      const excerpt = await taskResultDiscussionExcerpt(api, request);
      if (!current()) return;
      if (await composerDrafts.addExcerpt(excerpt)) {
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
    } catch { execution.notice("原对话暂时无法读取，请稍后重试。"); }
  }
  async function approveProposedAction(action: ProposedAction) {
    if (!api || approvingActionId || executionBusy || creativeDirty) return;
    setApprovingActionId(action.id);
    const executionScope = beginExecutionScope({ kind: "taskRecovery", operation: action.operation, view: action.operation === "generateBrief" ? "brief" : action.operation === "generateOutline" ? "outline" : action.operation === "generatePlan" ? "plans" : action.operation === "generatePropertyRules" ? "world" : "chapters", intent: action.instruction });
    const executionIsCurrent = () => execution.scope === executionScope && currentProjectRef.current === projectId;
    execution.startCandidate(`批准候选修改：${action.instruction}`);
    try {
      if (!executionIsCurrent()) return;
      const result = await api.agent.approveAction(projectId, action.sourceTaskId, action.id);
      if (!executionIsCurrent()) return;
      const hasSavedResults = Boolean(result.candidateResults?.length);
      const outcome = result.executionFailure
        ? hasSavedResults ? "已有候选结果保留，后续处理尚未完成。" : "本轮处理尚未完成，请查看对话说明。"
        : hasSavedResults ? "本轮候选处理已完成，尚未正式采用。" : "本轮处理已结束，请查看对话说明。";
      execution.notice(outcome);
      await syncExecutionWorkspace(outcome, executionScope);
      if (!executionIsCurrent()) return;
      if (result.executionFailure) {
        await reportExecutionFailure(result.executionFailure.message, executionScope, "候选后续处理未完成");
      } else {
        execution.completeCandidate(outcome);
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
    const receipt = scopeExecutionRef.current;
    const isCurrent = () => receipt?.projectId === projectId && receipt.scope === execution.scope && currentProjectRef.current === projectId;
    if (!isCurrent()) return;
    const outcome = !result.work ? "范围已保存；执行结果待核对，请查看创作任务。"
      : result.work.status === "completed" ? "本轮工作步骤已结束，候选仍待审阅，未正式采用。"
      : `范围已保存；创作任务当前为 ${result.work.status}，尚未完成。`;
    execution.notice(outcome);
    execution.finishScope(result.work?.status, outcome);
    const latest = await api?.agent.listProposedActions(projectId).catch(() => null);
    if (!isCurrent()) return;
    if (latest) setProposedActions({ projectId, actions: latest.actions });
    await syncExecutionWorkspace(outcome, receipt!.scope);
  }
  async function declineProposedAction(action: ProposedAction) {
    if (!api || approvingActionId || executionBusy) return;
    setApprovingActionId(action.id);
    try {
      await api.agent.declineAction(projectId, action.sourceTaskId, action.id);
      execution.notice(`已拒绝候选修改：${action.instruction}。不会创建创作任务。`);
    } catch (error) {
      const message = error instanceof Error ? error.message : "未知错误";
      execution.notice(`拒绝候选失败：${message}`);
    } finally {
      const refreshed = await api.agent.listProposedActions(projectId).catch(() => null);
      if (refreshed) setProposedActions({ projectId, actions: refreshed.actions });
      setApprovingActionId(null);
    }
  }
  const { generateWork, continueWork, reviewPlanning, cancelPlanningReview, generateChapter } = useConversationGeneration(execution, { beginExecutionScope, currentProjectRef, projectId, api, syncExecutionWorkspace, reportExecutionFailure, creativeDirtyRef, setTaskControlMessage, taskList });
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
  const { reconnectAgent, interruptAgent, reconnectNotice } = useConversationRecovery(execution, { api, taskControls, selectedConversation, taskList, currentProjectRef, projectId, taskControlScopeRef, setTaskControlMessage, promptPipelineBusy, liveWorkListRef, reportExecutionFailure, setThreadAction, refreshThreads });
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
    try { setBackgroundTerminals((await api.agent.listBackgroundTerminals(projectId, agentRun.threadId)).terminals); } catch (error) { execution.notice(`后台终端读取失败：${error instanceof Error ? error.message : "未知错误"}`); } finally { setBackgroundTerminalLoading(false); }
  }
  async function terminateBackgroundTerminal(processId: string) {
    if (!api || !agentRun.threadId) return;
    try { await api.agent.terminateBackgroundTerminal(projectId, agentRun.threadId, processId); setBackgroundTerminals((terminals) => terminals.filter((terminal) => terminal.processId !== processId)); execution.notice(`后台终端 ${processId} 已终止。`); } catch (error) { execution.notice(`终止后台终端失败：${error instanceof Error ? error.message : "未知错误"}`); }
  }
  function prepareConversation(prompt: string, source?: DiscussionSource) {
    if (!conversationProjectReady || executionBusy || source && source.focus.reference.projectId !== projectId) return;
    if (source && (currentChapterWriting || revisionRepair.prepared)) {
      execution.notice("输入框已有待发送的创作来源，请先发送或取消该指令，再引用内容。");
      return;
    }
    if (source && composerDrafts.source && JSON.stringify(source.focus) !== JSON.stringify(composerDrafts.source.focus)) {
      setPendingDiscussionSource(source); composerRef.current?.focus({ preventScroll: true }); return;
    }
    if (!source) { selectChapterWriting(null); revisionRepair.cancel(); execution.dismissRepair(); }
    const removedSource = !source && Boolean(composerDrafts.source);
    composerDrafts.setSource(source ?? null); setPendingDiscussionSource(null);
    if (composer.trim()) execution.notice(source ? "已附加讨论来源，保留你正在输入的文字。" : removedSource ? "已切换为当前页面讨论，移除原精确来源；输入文字已保留。" : "已保留你正在输入的内容；可在当前对话中补充要处理的事项。");
    else if (!source) { setComposer(prompt); if (removedSource) execution.notice("已切换为当前页面讨论，移除原精确来源。"); }
    composerRef.current?.focus({ preventScroll: true });
  }
  async function addDiscussionExcerpt(excerpt: DiscussionExcerpt) {
    if (!conversationProjectReady || executionBusy || excerpt.projectId !== projectId) return false;
    if (currentChapterWriting || revisionRepair.prepared) {
      execution.notice("输入框已有待发送的创作来源，请先发送或取消该指令，再引用内容。");
      return false;
    }
    if (!await composerDrafts.addExcerpt(excerpt)) return false;
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
  const renderedRetry = execution.retry;
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
  const { pauseChoices, pauseTaskDisabled, pauseSelectedTask, retryChoices, retryTask, retrySelectedTask, composerRetry, retryTaskDisabled } = useConversationTaskCommands(execution, { taskList, projectId, selectedConversation, conversationProjectReady, api, conversationChanging, taskControls, currentProjectRef, taskControlScopeRef, setTaskControlMessage, viewingExecution, renderedRetry, canDiscussSavedChapter, executionBusy, creativeDirty, activeConversationDetail, selectedThread, agentAvailable, creativeDirtyRef, retryExecution });
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
  const editionDialog = editionCreateOpen && api ? <CreateMediaEditionDialog key={activeProjectId} authorKey={author.email} api={api.multimedia} projectId={activeProjectId} source={editionSource} onClose={() => editionNavigation.showCreator(false)} onCreated={openEdition} /> : null;
  if (editionCatalog.projectId === activeProjectId && !editionCatalog.selectedId && editionCatalog.editions.length > 1) return <div className="app-shell"><header className="topbar"><ProjectSwitcher projects={projects} activeProjectId={activeProjectId} busy={projectLoading} onSelect={selectProject} onCreate={() => requestNavigation({ kind: "new-project" })} /></header><main className="edition-selection-required"><h2>选择要继续的作品版本</h2><p>这部作品包含多个版本，分别保存创作内容与对话。</p>{editionCatalog.editions.map(edition => <button type="button" key={edition.id} className="secondary-button" disabled={projectLoading} onClick={() => selectEdition(edition)}>{edition.title} · {edition.medium === "comic" ? "漫画" : edition.medium === "animation" ? "动画" : "小说"}</button>)}{editionError && <p role="alert">{editionError}</p>}</main>{projectCreateOpen && <ProjectCreateDialog onClose={() => setProjectCreateOpen(false)} onSubmit={createProject} />}</div>;
  if (mediaContext && api && mediaContext.project.id === activeProjectId) return <>
    <MediaApplication key={`${author.email}:${projectId}:${mediaContext.edition.id}`} api={api} edition={mediaContext.edition} authorKey={author.email}
      onEditionSourceChange={editionNavigation.useSource} onBusyChange={editionNavigation.reportBusy} onDirtyChange={setCreativeDirty} onSaveHandlerChange={registerCreativeSaveHandler} onDiscardHandlerChange={registerCreativeDiscardHandler}
      topbar={<header className="topbar"><div className="brand-lockup"><div className="brand-mark">✦</div><span>创作工坊</span></div><ProjectSwitcher projects={projects} activeProjectId={activeProjectId} busy={projectLoading} onSelect={selectProject} onCreate={() => requestNavigation({ kind: "new-project" })} />{editionControls}<button className="icon-button" type="button" aria-label="切换主题" onClick={() => setTheme(value => value === "light" ? "dark" : "light")}>◐</button></header>} />
    {editionDialog}{editionError && <p className="dialog-error" role="alert">{editionError}</p>}
    {projectCreateOpen && <ProjectCreateDialog onClose={() => setProjectCreateOpen(false)} onSubmit={createProject} />}
    {pendingNavigation && <UnsavedChangesDialog saving={savingBeforeLeave} onSave={() => void saveAndContinueNavigation()} onDiscard={discardAndContinueNavigation} onStay={() => { if (pendingNavigation.kind === "close") void api.app.cancelClose(); setPendingNavigation(null); }} />}
  </>;

  const creativeConversationOpen = conversationProjectReady && !accountOpen && (["brief", "outline", "plans", "chapters"] as ViewId[]).includes(activeView);
  const renderCurrentConversation = (run: AgentRun, taskBound: boolean, finalReplyState?: FinalReplyDisplayState) => <EnhancedAgentTranscript projectId={projectId} run={run} taskBound={taskBound} finalReplyState={finalReplyState} userInputRequest={userInputQueue[0] ?? null} queuedUserInputCount={Math.max(0, userInputQueue.length - 1)} onResolveUserInput={resolveUserInput} onReview={() => void reviewAgent()} />;
  const hasSelectedWorkTasks = taskList.tasks.some(task => task.projectId === projectId && task.threadId === (selectedConversation.id ?? ""));
  const showConversationReader = !hasSelectedWorkTasks && (!viewingExecution || !agentRun.prompt && !agentRun.history.length);
  const currentExecutionOwner = execution.owner;
  const sameAuthorSource = currentExecutionOwner && currentExecutionOwner.api === api && currentExecutionOwner.scope === execution.scope
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
      {api && generationHandoff?.scope === execution.scope && generationHandoff.proposal.projectId === projectId
        && agentRun.state === "completed" && !agentRun.error && <ConversationGenerationHandoff key={generationHandoff.proposal.proposalId}
          api={api} proposal={generationHandoff.proposal} scope={generationHandoff.scope}
          blocked={executionBusy || creativeDirty || !agentAvailable} onCancel={() => {
            if (execution.handoff !== generationHandoff) return;
            execution.dismissHandoff(); composerRef.current?.focus({ preventScroll: true });
          }} onConfirm={async () => {
            if (execution.handoff !== generationHandoff || generationHandoff.scope !== execution.scope
              || currentProjectRef.current !== projectId) return;
            const proposal = generationHandoff.proposal;
            await generateWork(proposal.authorIntent, proposal.targetChapters, proposal.expectedRevision,
              proposal.baseSnapshotId, proposal.sourceDigest, null, proposal.proposalId);
          }} />}</>,
    renderHistory: (entry, { taskBound, finalReplyState }) => <><EarlierTurn projectId={projectId} entry={entry} taskBound={taskBound} finalReplyState={finalReplyState} />{entry.state === "completed" && entry.turnId && entry.threadId === selectedConversation.id && <button type="button" className="text-button" disabled={executionBusy || conversationChanging || Boolean(selectedThread?.isArchived)} onClick={() => void createConversation(undefined, entry.turnId!)}>从此回合新建分支对话</button>}</> };

  return <div className="app-shell"><header className="topbar"><div className="brand-lockup"><div className="brand-mark">✦</div><span>小说工坊</span><span className="beta-pill">本地</span></div><div className="topbar-center"><ProjectSwitcher projects={projects} activeProjectId={activeProjectId} busy={projectLoading} onInformation={conversationProjectReady ? () => setProjectInformationOpen(true) : undefined} onSelect={selectProject} onCreate={() => requestNavigation({ kind: "new-project" })} />{editionControls}<span className="save-state"><span className={`save-dot ${creativeDirty || discussionSyncNotice ? "pending" : ""}`} />{creativeDirty ? "创作修改未保存" : discussionSyncNotice ? "工作区待同步" : "工作区已同步"}</span></div><div className="topbar-actions">{search.open && <input autoFocus className="top-search" aria-label="搜索故事内容" value={search.query} onChange={(event) => search.setQuery(event.target.value)} onKeyDown={(event) => { if (event.key === "Enter" && !event.nativeEvent.isComposing) { event.preventDefault(); void search.submit(); } }} placeholder="搜索角色、场景、章节" />}<button className="icon-button" title="搜索" onClick={search.toggle}>⌕</button><button className="icon-button" title="命令面板 ⌘⇧P" onClick={() => setCommandPaletteOpen(true)}>⌘</button><button className="icon-button theme-toggle" title={theme === "light" ? "切换深色主题" : "切换浅色主题"} aria-label={theme === "light" ? "切换深色主题" : "切换浅色主题"} onClick={() => setTheme(theme === "light" ? "dark" : "light")}>{theme === "light" ? "☾" : "☼"}</button><button type="button" className="avatar" title="用户设置" aria-label="打开用户设置" onClick={() => requestNavigation({ kind: "account" })}>{authorInitial(author.name)}</button></div></header><div className="workspace" style={layoutStyle}><Sidebar outlineDirectory={conversationProjectReady ? <OutlineVolumeNavigation key={projectId} catalog={workDirectory.catalog} error={workDirectory.error} busy={executionBusy || workDirectory.busy} active={activeView === "outline" && !accountOpen} selectedVolumeId={activeView === "outline" && !accountOpen ? outlineVolume?.id ?? null : null} onNavigate={() => navigateView("outline")} onSelect={volumeId => requestNavigation({ kind: "volume", projectId, volumeId })} onRetry={() => void workDirectory.refresh()} /> : null} directory={conversationProjectReady ? <SidebarWorkDirectory directory={workDirectory} titlePreview={currentChapterTitlePreview} key={projectId} api={api} projectId={projectId} chapters={state.novelEdition.chapters} selectedChapterId={selectedChapterId} refreshToken={creativeRefreshToken} enabled={conversationProjectReady} busy={executionBusy} dirty={creativeDirty} chapterDeletionDisabled={!taskList.loaded || Boolean(taskList.error) || taskList.activeCount > 0} onSelectChapter={selectChapter} onDeleteChapter={chapterId => requestNavigation({ kind: "delete-chapter", projectId, chapterId })} onCreateChapter={request => requestNavigation({ kind: "new-chapter", request })} /> : null} activeView={activeView} state={state} onNavigate={navigateView} onCreateProject={() => requestNavigation({ kind: "new-project" })} runtimeConnection={<RuntimeConnectionStatus status={runtimeStatus} onStatus={setRuntimeStatus} retryStartup={api?.runtime.retryStartup} />} conversationNavigation={conversationProjectReady ? <ConversationNavigator key={projectId} threads={threads} selectedId={selectedConversation.id} runningIds={[...new Set([executionBusy ? executionConversationId : null,...execution.submittingConversationIds].filter((id):id is string=>Boolean(id))) ]} busy={conversationChanging} loading={threadLoading} error={threadAction} archived={threadArchived} onSelect={selectConversation} onInspect={readonlyHistory.open} onCreate={() => void createConversation()} onRename={renameConversation} onPin={thread => void toggleThreadPin(thread)} onArchive={thread => void toggleThreadArchive(thread)} onToggleArchived={toggleThreadArchiveView} onRetry={() => void refreshThreads()} /> : null} busy={executionBusy} /><PaneSash orientation="vertical" label="调整导航宽度" value={sidebarWidth} min={72} max={sidebarMax} nextValue={(start, dx) => start + dx} onResize={(sidebar) => setPanes((current) => ({ ...current, sidebar }))} /><main ref={mainPanelRef} data-discussion-document={conversationProjectReady && !accountOpen ? "" : undefined} data-discussion-project={projectId} data-discussion-view={activeView} data-discussion-label={activeLabel} data-discussion-dirty={creativeDirty} data-view={accountOpen ? "settings" : activeView} className={`main-panel${accountOpen ? " account-open" : activeView === "chapters" ? " chapter-main-panel" : ""}`}>{(accountOpen || activeView !== "chapters") && <div className="content-header"><div className="breadcrumbs"><h1>{accountOpen ? "用户设置" : activeLabel}</h1></div><div className="header-actions"><div className="module-records-toolbar-slot" ref={setModuleActionsContainer} />{accountOpen ? <button type="button" className="secondary-button" onClick={() => setAccountOpen(false)}>返回作品</button> : <button type="button" className="secondary-button" onClick={() => setExportOpen(true)}>导出</button>}</div></div>}{accountOpen ? <UserSettings profile={author} theme={theme} diagnostics={api && projectId ? <WorkspaceDiagnostics key={projectId} api={api} projectId={projectId} projectTitle={state.project.title} refreshToken={creativeRefreshToken} /> : undefined} onTheme={setTheme} onSaveProfile={saveProfile} onChangePassword={changePassword} onSignOut={signOut} onDeleteAccount={deleteAccount} /> : null}{(agentStatus.executionHealth?.some(item => !item.projectId || item.projectId === projectId) || connectionState === "reconnecting" || connectionState === "error" || agentRun.state === "error" || agentRun.state === "waiting" || !agentStatus.configured) && <AgentStatusBar projectId={projectId} status={visibleAgentStatus} connectionState={connectionState} operationError={agentRun.error} threadId={agentRun.threadId} onReconnect={() => void reconnectAgent()} />}<div className="sr-only" role="status" aria-live="polite">{connectionState === "reconnecting" ? "正在恢复任务连接" : visibleAgentStatus.state === "running" ? "Agent 正在执行" : visibleAgentStatus.state === "waiting" ? "等待你的输入或授权" : ""}</div>{search.open && <SearchResults state={search.state} onRetry={() => void search.submit()} onSelect={(result) => {
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
{conversationProjectReady && !accountOpen && (["brief", "outline", "plans", "chapters"] as ViewId[]).includes(activeView) && <CreativeWorkspaceView key={author.email} authorKey={author.email} moduleActionsContainer={moduleActionsContainer} recordsOpen={recordsCurrent} onOpenVersions={openModuleRecords} onOutlineRecordScopeChange={syncOutlineRecordScope} outlineCatalogError={workDirectory.error} outlineVolume={activeView === "outline" ? outlineVolume : null} onCloseOutlineVolume={() => navigateView("outline")} onOpenOutlineNode={openOutlineNode} onLinkOutline={(volumeId, source) => workDirectory.update({ type: "linkOutline", volumeId, source })} onChapterCreationPrepared={requestId => setRequestedChapterCreation(current => current?.requestId === requestId ? null : current)} requestedChapterCreation={requestedChapterCreation?.projectId === projectId ? requestedChapterCreation : null} requestedCommitment={commitmentSelection?.request.projectId === projectId ? commitmentSelection : null} requestedCommitmentEdit={requestedCommitmentEdit?.result.projectId === projectId ? requestedCommitmentEdit : null} onCommitmentEditPrepared={requestId => setRequestedCommitmentEdit(current => current?.requestId === requestId ? null : current)} taskControls={taskControls} taskResultSelection={taskResultSelection?.result.projectId === projectId ? taskResultSelection : null} onDiscussTaskResult={result => void discussSavedResult(result)} onOpenTaskResult={result => requestNavigation({ kind: "result", result })} onDiscuss={prepareConversation} onExport={() => setExportOpen(true)} onPrepareChapterWriting={prepareChapterWriting} taskList={taskList} conversation={conversationContent} taskConversationContainer={showConversationReader ? null : taskConversationContainer} api={api} projectId={projectId} view={activeView as "brief" | "outline" | "plans" | "chapters"} state={state} selectedChapterId={selectedChapterId} onSelectChapter={selectChapter} onGenerateWork={generateWork} onContinueWork={(runId) => continueWork(runId)} onRetryWork={(runId) => continueWork(runId, "retry")} onResolveFeedback={(runId, revisionId, choice, instruction) => continueWork(runId, choice === "repairDraft" ? "feedbackRepair" : "feedbackContinue", revisionId, instruction)} onReviewPlanning={reviewPlanning} onCancelPlanningReview={cancelPlanningReview} onGenerateChapter={generateChapter} onOpenPlanningTarget={(kind) => navigateView(kind === "plan" ? "plans" : "outline")} onReturnToFeedback={() => navigateView("chapters")} onFormalChange={refreshFormalState} onDirtyChange={setCreativeDirty} onChapterTitlePreview={syncChapterTitlePreview} onSaveHandlerChange={registerCreativeSaveHandler} onPlanningSaveHandlerChange={registerPlanningSaveHandler} onDiscardHandlerChange={registerCreativeDiscardHandler} agentAvailable={agentAvailable} agentRunning={baseExecutionBusy || taskControls.pending} refreshToken={creativeRefreshToken}
  revisionContinuation={revisionContinuation?.projectId === projectId ? revisionContinuation : null}
  onPrepareRevisionContinuation={setRevisionContinuation} onDismissRevisionContinuation={() => setRevisionContinuation(null)}
  onPrepareRevisionRepair={preparation => {
    if (!revisionRepair.prepare(preparation)) return;
    composerDrafts.setSource(null); setPendingDiscussionSource(null);
    selectChapterWriting(null);
    execution.dismissRepair();
    setComposer(current => current.trim() ? current : "我想讨论下一次改稿的方向，请先和我核对需要调整的内容。");
    composerRef.current?.focus();
  }} />}
{!accountOpen && activeView !== "brief" && activeView !== "outline" && activeView !== "plans" && activeView !== "chapters" && <>{activeView === "characters" ? <CharacterCatalog onDiscuss={prepareConversation} api={api} projectId={projectId} refreshToken={creativeRefreshToken} chapters={state.novelEdition.chapters} onOpenChapter={selectChapter} /> : <StoryReferenceView api={api} projectId={projectId} view={activeView} refreshToken={creativeRefreshToken} onDiscuss={prepareConversation} onOpenCommitment={request => requestNavigation({ kind: "commitment", request })} onOpenChapter={chapterId => { const chapter = state.novelEdition.chapters.find(item => item.id === chapterId); if (chapter) selectChapter(chapter); }} />}{activeView === "characters" && <details className="character-identity-tools"><summary>同名与身份管理</summary><IdentityDecisionPanel agentRunning={executionBusy} key={projectId} api={api} projectId={projectId} refreshToken={creativeRefreshToken} onFormalChange={refreshFormalState} /></details>}{activeView === "world" && <PropertyRulesPanel agentRunning={executionBusy} requestedResult={taskResultSelection?.result.kind === "propertyRules" ? { requestId: taskResultSelection.requestId, result: taskResultSelection.result } : null} api={api} projectId={projectId} refreshToken={creativeRefreshToken} onFormalChange={refreshFormalState} onDirtyChange={setCreativeDirty} onSaveHandlerChange={registerCreativeSaveHandler} onDiscardHandlerChange={registerCreativeDiscardHandler} />}</>}
</main><PaneSash orientation="vertical" label="调整对话栏宽度" value={railWidth} min={300} max={railMax} nextValue={(start, dx) => start - dx} onResize={(rail) => setPanes((current) => ({ ...current, rail }))} /><ConversationPanel identity={{ id: selectedConversation.id, title: selectedThread?.name ?? "新对话", scope: conversationScope }}
    tools={{ savingCount: recoveryRuns.filter(run => run.projectId === projectId).length,
currentSavingCount: recoveryRuns.filter(run => run.projectId === projectId && Boolean(selectedConversation.id) && run.threadId === selectedConversation.id).length,
onRecovery: () => setRecoveryProject(projectId),
pauseChoices: pauseChoices,
pauseDisabled: pauseTaskDisabled,
onPauseTask: (task, command) => void pauseSelectedTask(task, command) }}
    status={<>{formalReadError?.projectId === projectId && <ReadFailureNotice message="章节目录暂时无法读取。" error={formalReadError.message} retryLabel="重新读取目录" onRetry={() => setCreativeRefreshToken(value => value + 1)} />}<ConversationExecutionStatus run={statusOwnsExecution ? agentRun : conversationRun} busy={baseExecutionBusy && statusOwnsExecution} connection={connectionState} works={statusWorks} readError={taskList.error} health={(agentStatus.executionHealth ?? []).filter(item => item.projectId === projectId && Boolean(item.workTaskId) && taskList.tasks.some(task => task.id === item.workTaskId && task.threadId === selectedConversation.id))} /><>{executionBusy && !viewingExecution && <div className="conversation-other-execution" role="status">另一对话“{threads.find(thread => thread.id === executionConversationId)?.name ?? "创作任务"}”{agentRun.state === "waiting" ? "等待回答" : "正在执行"}。可浏览和编辑草稿，执行结束后可发送。<button type="button" onClick={() => selectConversation(threads.find(thread => thread.id === executionConversationId) ?? null)}>返回处理</button></div>}{selectedThread?.isArchived && <p role="status">已归档对话 · 恢复后可继续发送</p>}</>{selectedThread?.forkedFromId && <div className="conversation-branch-source">此对话从另一方案分支而来<button type="button" disabled={conversationChanging} onClick={() => { const parent = threads.find(thread => thread.id === selectedThread.forkedFromId); if (parent) void selectConversation(parent); else if (selectedThread.forkedFromId) void openBranchSource(selectedThread.forkedFromId); }}>查看原对话</button><button type="button" disabled={executionBusy} onClick={() => setComparison({ projectId, threadId: selectedThread.id, sourceId: selectedThread.forkedFromId! })}>对照两种方案</button></div>}</>}
    reading={<>{conversationProjectReady && (!selectedConversation.id || selectedConversation.id === agentRun.threadId) && agentRun.origin === "chapterStructure" && agentRun.state === "error" && <button type="button" className="text-button" disabled={taskList.reading} onClick={() => void taskList.refresh()}>重新读取检查记录</button>}{api && conversationProjectReady && <ChapterStructureNotice key={`structure:${projectId}`} api={api} projectId={projectId} refreshToken={creativeRefreshToken} available={agentAvailable} busy={executionBusy} dirty={creativeDirty} failures={structureFailures} />}{showConversationReader && taskList.error && <ReadFailureNotice message="任务记录暂时无法读取。" error={taskList.error} retryLabel="重新读取任务记录" busy={taskList.reading} onRetry={() => void taskList.refresh()} />}{showConversationReader && taskList.reading && <p role="status">正在读取对话与执行记录…</p>}{!showConversationReader && <div className="conversation-rail-scroll" inert={readonlyHistoryOpen}>{earlierConversationDetail?.turns.length ? <ConversationReader detail={earlierConversationDetail} loading={false} error={null} onRetry={() => {}} onFork={turnId => void createConversation(undefined, turnId)} busy={executionBusy || conversationChanging || Boolean(selectedThread?.isArchived)} showEmpty={false} /> : null}{conversationProjectReady && !creativeConversationOpen && <WorkTaskPanel onFormalChange={refreshFormalState} key={`work:${projectId}`} conversation={conversationContent} api={api} projectId={projectId} tasks={taskList.tasks} activeCount={taskList.activeCount}
      nextCursor={taskList.nextCursor} loadingMore={taskList.loadingMore} loaded={taskList.loaded} reading={taskList.reading} readError={taskList.error}
      onRetry={() => void taskList.refresh()} onLoadMore={() => void taskList.loadMore()} revision={undefined} busy={taskControls.pending}
      agentRunning={executionBusy} dirty={creativeDirty} onDiscussResult={result => void discussSavedResult(result)} onOpenResult={result => requestNavigation({ kind: "result", result })} onAdjustCommitment={(result, detail) => { if (result.projectId === currentProjectRef.current && detail.projectId === result.projectId && !executionBusy) requestNavigation({ kind: "commitment-edit", result, detail }); }} />}{taskControlMessage?.scope === taskList.scope && taskControlMessage.error && taskList.tasks.some(task => task.id === taskControlMessage.error?.targetId && (task.threadId === selectedConversation.id || task.kind === "generation" && task.threadId === "" && selectedConversation.id === null)) && <p className="dialog-error" role="alert">{taskList.tasks.find(task => task.id === taskControlMessage.error?.targetId)?.goal.slice(0, 60) ?? "任务操作"}：{taskControlMessage.error.message}</p>}{taskControlMessage?.scope === taskList.scope && taskControlMessage.notice && taskList.tasks.some(task => task.id === taskControlMessage.notice?.targetId && (task.threadId === selectedConversation.id || task.kind === "generation" && task.threadId === "" && selectedConversation.id === null)) && <p role="status">{taskList.tasks.find(task => task.id === taskControlMessage.notice?.targetId)?.goal.slice(0, 60) ?? "任务操作"}：{taskControlMessage.notice.message}</p>}{taskResultError && <p className="dialog-error" role="alert">{taskResultError}</p>}<div className="conversation-task-records" ref={setTaskConversationContainer} /><ActionProposalList actions={selectedConversationActions} busy={Boolean(approvingActionId) || executionBusy || creativeDirty} onApprove={(action) => void approveProposedAction(action)} onDecline={(action) => void declineProposedAction(action)} onScope={(action) => setScopeAction({ projectId, action })} /></div>}{showConversationReader && <div className="conversation-history-scroll" hidden={readonlyHistoryOpen}><ConversationReader detail={activeConversationDetail?.detail ?? null} loading={Boolean(activeConversationDetail?.loading)} error={activeConversationDetail?.error ?? null} onRetry={() => { if (selectedConversation.id) void readSelectedConversation(); }} onFork={turnId => void createConversation(undefined, turnId)} busy={executionBusy || conversationChanging || Boolean(selectedThread?.isArchived)} /></div>}{readonlyHistoryOpen && <ReadonlyThreadHistory history={readonlyHistory} />}</>}
    composer={<>{viewingExecution && currentSentRepair?.projectId === projectId && agentRun.state === "error" && <FailedRevisionRepair preparation={currentSentRepair} onStartOrdinaryDiscussion={() => { revisionRepair.cancel(); execution.dismissRepair(); }} />}{revisionRepair.prepared && <PreparedRevisionRepair preparation={revisionRepair.prepared} checking={revisionRepair.checking} error={revisionRepair.error} onCancel={revisionRepair.cancel} />}{!composerDrafts.excerptsReady && !composerDrafts.error && <p role="status">正在校验引用内容…</p>}{composerDrafts.error && <p className="dialog-error" role="alert">{composerDrafts.error}</p>}<>{reconnectNotice?.scope === execution.scope && agentRun.connectionState !== "ready" && <p className="dialog-error" role="status" aria-label="原任务连接核对">{reconnectNotice.message}</p>}</>{!showConversationReader && activeConversationDetail?.error && selectedConversation.id && <button type="button" disabled={executionBusy || conversationChanging} onClick={() => { if (selectedConversation.id) void readSelectedConversation(); }}>重新读取对话历史</button>}{reasoningSelection.error && <p className="dialog-error" role="alert">{reasoningSelection.error}</p>}<ModelConnectionNotice progress={modelConnection.progress} busy={modelConnection.busy} onRetry={() => void modelConnection.retry()} /><DiscussionSourceChip source={composerDrafts.source} pending={pendingDiscussionSource?.focus.reference.projectId === projectId ? pendingDiscussionSource : null} disabled={executionBusy} onRemove={() => { composerDrafts.setSource(null); setPendingDiscussionSource(null); }} onKeep={() => setPendingDiscussionSource(null)} onReplace={() => {
      if (!pendingDiscussionSource || pendingDiscussionSourceRef.current !== pendingDiscussionSource || currentProjectRef.current !== projectId || pendingDiscussionSource.focus.reference.projectId !== projectId || executionBusy) return;
      composerDrafts.setSource(pendingDiscussionSource); setPendingDiscussionSource(null); composerRef.current?.focus({ preventScroll: true });
    }} /><DiscussionExcerptCards excerpts={composerDrafts.excerpts} disabled={executionBusy && viewingExecution} readOnly={composerDrafts.excerpts.length > 0 && (creativeDirty || composerDrafts.excerpts.some(item => item.state === "unsaved"))} onRemove={composerDrafts.removeExcerpt} /><SelectionDiscussion projectId={projectId} view={activeView} disabled={!conversationProjectReady || accountOpen || conversationChanging || executionBusy} onAdd={addDiscussionExcerpt} onNew={excerpt => createConversation(excerpt)} />{api && taskList.tasks.filter(work=>work.threadId===selectedConversation.id && work.projectId===projectId && work.status==="running").map(work=><WorkInterludePanel key={`${projectId}:${work.id}`} api={api} work={work} text={composer} focus={{...(composerDrafts.source?.focus??{view:activeView}),...(composerDrafts.excerpts.length?{excerpts:structuredClone(composerDrafts.excerpts)}:{})}} blocked={conversationChanging || Boolean(currentChapterWriting || revisionRepair.prepared || pendingDiscussionSource)} acceptDraft={composerDrafts.acceptance} refresh={taskList.refresh}/>)}<ConversationMessageQueue key={conversationScope} items={outbox.items} busy={conversationChanging} edit={outbox.edit} move={outbox.move} remove={outbox.remove} reconcile={outbox.reconcile} continueWaiting={outbox.continue} />{outbox.error && <p role="alert">{outbox.error}</p>}{executionBusy && viewingExecution && currentRunCanSteer(agentRun) && agentRun.taskId && composer.trim() && <button type="button" className="text-button" disabled={creativeDirty || Boolean(currentChapterWriting || revisionRepair.prepared || pendingDiscussionSource)} onClick={() => void queuePrompt("afterTurn")}>本轮结束后发送</button>}{chapterDraftScope?.projectId === projectId && <p role="status">对话使用已保存的章节内容，当前未保存正文会保留。</p>}<Composer retryChoices={retryChoices} retrySingleAction={Boolean(retryTask)} retryScope={conversationScope} onRetryTask={task => void retrySelectedTask(task)} onRetry={composerRetry} retryDisabled={retryTaskDisabled} retryLabel={retryTask?.resumeMode === "recoverSavedResult" ? "核对保存结果" : "重试当前任务"} onQueue={executionBusy && viewingExecution ? () => void queuePrompt() : undefined} queueDisabled={!composerDrafts.excerptsReady || !selectedConversation.id || creativeDirty || Boolean(currentChapterWriting || revisionRepair.prepared || pendingDiscussionSource)} projectId={projectId} planningSaving={planningSubmission.pending} planningSaveOnSend={creativeDirty && planningSubmissionEligible && !composerDrafts.excerpts.length} choosingSource={!composerDrafts.excerptsReady || pendingDiscussionSource?.focus.reference.projectId === projectId} canSteer={viewingExecution && currentRunCanSteer(agentRun) && !composerDrafts.excerpts.length} writing={currentChapterWriting} writingPending={chapterWritingPending} onCancelWriting={() => selectChapterWriting(null)} repairPrepared={Boolean(revisionRepair.prepared)} checkingRepair={revisionRepair.checking} composer={composer} setComposer={setComposer} onSubmit={() => void submitPrompt()} onInterrupt={() => { if (viewingExecution) void interruptAgent(); }} composerRef={composerRef} activity={activity} agentStatus={viewingExecution ? visibleAgentStatus : { ...visibleAgentStatus, state: "idle" }} availability={{ interludeHeld: taskList.tasks.some(work=>work.projectId===projectId && work.threadId===selectedConversation.id && !!work.interlude), runtimeReady: Boolean(api && runtimeStatus.state === "online"), modelReady: agentStatus.configured, changingConversation: conversationChanging, historyState: activeConversationDetail?.loading ? "loading" : activeConversationDetail?.error ? "error" : "ready", archived: Boolean(selectedThread?.isArchived), otherConversationRunning: executionBusy && !viewingExecution }} blockedByUnsavedChanges={creativeDirty && !(chapterDraftScope?.projectId === projectId && !chapterDeletion.active && !workDirectory.busy && activeView === "chapters" && selectedChapter && !composerDrafts.source && !revisionRepair.prepared && !pendingDiscussionSource) && !composerDrafts.excerpts.length && !planningSubmissionEligible && (!currentChapterWriting || executionBusy)} projectReady={Boolean(projectId && projectId === activeProjectId)} connections={agentConnections} agentConfigSaving={agentConfigSaving} onSelectConnection={(connection) => void selectAgentConnection(connection)} reasoningDisabled={!signedIn || agentConfigSaving || reasoningSelection.busy} onReasoningChange={effort => void reasoningSelection.change(effort)} /></>}
    viewingExecution={viewingExecution} inspector={{ state: state,
runtimeStatus: runtimeStatus,
agentRun: agentRun,
connections: agentConnections,
onAddCanonNote: openFactReview,
onReview: () => void reviewAgent(),
backgroundTerminals: backgroundTerminals,
backgroundTerminalLoading: backgroundTerminalLoading,
onLoadBackgroundTerminals: () => void loadBackgroundTerminals(),
onTerminateBackgroundTerminal: (processId) => void terminateBackgroundTerminal(processId),
onVerifyAgent: (provider, model) => void verifyAgent(provider, model),
onConfigureAgent: (provider, model) => void configureAgent(provider, model),
agentConfigSaving: agentConfigSaving }}
    inspectorSize={{ height: panes.inspector, max: inspectorMax, resize: inspector => setPanes(current => ({ ...current, inspector })) }} /></div>{recordsCurrent && moduleRecords && <ModuleRecordsPopover key={moduleRecords.requestId} source={moduleRecords}
      api={moduleRecords.api} chapter={state.novelEdition.chapters.find(chapter => chapter.id === moduleRecords.chapterId)}
      dirty={creativeDirty} blocked={executionBusy} opener={moduleRecords.opener} container={mainPanelRef.current}
      onClose={() => setModuleRecords(current => current?.requestId === moduleRecords.requestId ? null : current)} onFormalChange={refreshFormalState} />}{chapterDeletion.state && <ChapterDeleteDialog deletion={chapterDeletion} />}{pendingNavigation && <UnsavedChangesDialog saving={savingBeforeLeave || planningSubmission.pending} onSave={() => void saveAndContinueNavigation()} onDiscard={discardAndContinueNavigation} onStay={() => { if (pendingNavigation.kind === "close") void api?.app.cancelClose(); setPendingNavigation(null); }} />}{commandPaletteOpen && <CommandPalette onClose={() => setCommandPaletteOpen(false)} onFocusComposer={() => { setCommandPaletteOpen(false); composerRef.current?.focus(); }} onReview={() => { setCommandPaletteOpen(false); void reviewAgent(); }} onReconnect={() => { setCommandPaletteOpen(false); void reconnectAgent(); }} onTheme={() => { setCommandPaletteOpen(false); setTheme((current) => current === "light" ? "dark" : "light"); }} />}{recoveryProject === projectId && <ConversationRecovery key={`recovery:${projectId}`} runs={recoveryRuns.filter(run => run.projectId === projectId)} tasks={taskList.tasks} threadId={selectedConversation.id} busy={executionBusy} onInspect={inspectRecovery} onReconcile={reconcileRecovery} onClose={() => setRecoveryProject(null)} />}{comparison?.projectId === projectId && api && <ConversationComparison key={`${projectId}:${comparison.threadId}`} api={api} {...comparison} onClose={() => setComparison(null)} />}{savedAnalysis && <SavedAnalysisDialog analysis={savedAnalysis} onClose={() => setSavedAnalysis(null)} />}{exportOpen && <ExportDialog onClose={() => setExportOpen(false)} onExport={async (options) => { if (!api) throw new Error("Runtime 尚未连接，请稍后重试。"); return api.runtime.exportProject(activeProjectId, options); }} />}{scopeAction?.projectId === projectId && api && <ExecutionScopeDialog key={`${projectId}:${scopeAction.action.sourceTaskId}`} projectId={projectId} initialAction={scopeAction.action} actions={proposedActions.projectId === projectId ? proposedActions.actions : []} api={api.agent} blocked={creativeDirty} onClose={() => setScopeAction(null)} onStart={() => { scopeExecutionRef.current = { projectId, scope: beginExecutionScope() }; execution.startCandidate("按确认的有限范围继续创作"); }} onComplete={scopeExecutionFinished} onError={(message) => { if (currentProjectRef.current === projectId && scopeExecutionRef.current?.scope === execution.scope) void reportExecutionFailure(message, scopeExecutionRef.current.scope, "有限范围执行失败"); }} />}{projectInformationOpen && conversationProjectReady && <ProjectInformation key={projectId} api={api} project={state.project} onClose={() => setProjectInformationOpen(false)} onOpenBrief={() => { setProjectInformationOpen(false); navigateView("brief"); }} />}{editionDialog}{editionError && <p className="dialog-error" role="alert">{editionError}</p>}{projectCreateOpen && <ProjectCreateDialog onClose={() => setProjectCreateOpen(false)} onSubmit={createProject} />}</div>;
}

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

export default App;
