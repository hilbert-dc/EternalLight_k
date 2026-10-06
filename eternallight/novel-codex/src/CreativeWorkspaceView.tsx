import { displayChapters, savedChapterTitle, type ChapterTitlePreview } from "./chapter-title";
import type { SidebarChapterCreationRequest } from "./SidebarWorkDirectory";
import { ModuleRecordsButton } from "./ModuleRecordsButton";
import { OutlineLinkPanel } from "./OutlineLinkPanel";
import { PlanningReadingView } from "./PlanningReadingView";
import { WorkOverview } from "./WorkOverview";
import { CreationDirectionSummary } from "./CreationDirectionSummary";
import type { CommitmentNavigationSelection, DiscussionSource } from "./types";
import type { ResolvedTaskResult } from "./task-result-navigation";
import type { TaskResultDestination } from "./task-results";
import { useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";

import { newDraftScene, firstOutlineEditorError, formatFactValue, generationStatusLabel, outlinePathLabels, parseFactValue, rebaseEditorBaseline, selectionAwareReviewChecksPass } from "./creative-product-logic";
import type { Chapter, ChapterPlanPayload, ChapterPlanRebindRequest, CreativeArtifactKind, CreativeArtifactPayload, CreativeReview, CreativeWorkspace, GenerationRunSummary, NovelCodexApi, OutlineNode, ProvisionalPlanningImpact, RollingPlanPayload, RuntimeState, SceneAssessmentSummary, ViewId } from "./types";
import { PlanningCharacterProvider, PlanningCharacterSelect } from "./PlanningCharacterContext";
import { PlanningWorkspaceHeader } from "./PlanningWorkspaceHeader";
import { OutlineTreeEditor } from "./OutlineTreeEditor";
import { ChapterDocumentView, ChapterPublicationStatus } from "./ChapterDocumentView";
import { ChapterPublicationDialog } from "./ChapterPublicationDialog";
import { useChapterPublication, type ChapterPublicationTarget } from "./useChapterPublication";
import { publicationObjectLabel } from "./publication-object-label";
import { StandaloneChapterForm } from "./StandaloneChapterForm";
import { WorkTaskPanel } from "./WorkTaskPanel";
import { StoppedExecutionError } from "./execution-stop";
import { sameConversationFailure, type WorkConversationContent } from "./work-conversation";
import type { WorkTaskList } from "./useWorkTaskList";
import { chapterEditorContent } from "./chapter-editor-content";
import type { WorkTaskControls } from "./useWorkTaskControls";
import { describeAgentError } from "./agent-error-description";
import { ImpactAnalysisPanel } from "./ImpactAnalysisPanel";
import { CommitmentPanel } from "./CommitmentPanel";
import { SceneFeedbackDetails } from "./SceneFeedbackDetails";
import { GenerationReplanPanel } from "./GenerationReplanPanel";
import { ChapterPlanRebindPanel } from "./ChapterPlanRebindPanel";
import { useWriterPreview, WriterPreviewPanel } from "./WriterPreviewPanel";
import { StatePropertyDetails } from "./StatePropertyDetails";
import { StoryEventDetails } from "./StoryEventDetails";
import { useSupplementalEditor } from "./supplemental-editor";
import { PlanningReviewPanel, type PlanningReviewStartRequest } from "./PlanningReviewPanel";
import type { ChapterWritingRequest } from "./chapter-writing";
import { NEW_CHAPTER_WRITING_INTENT } from "../shared/chapter-writing-policy.mjs";
import { authorIntentError } from "../shared/author-input-budget.mjs";
import { generationPermitForChapters, MAX_GENERATION_CHAPTERS } from "../shared/generation-permit.mjs";
import { revisionPredecessorIsCurrent } from "./revision-outcome-decision";
import type { RevisionContinuationPreparation, RevisionPredecessor, RevisionRepairPreparation } from "./types";

type WorkspaceView = Extract<ViewId, "brief" | "outline" | "plans" | "chapters">;
type CreativeTask = CreativeWorkspace["tasks"][number];
type EditableChapterPlan = Omit<ChapterPlanPayload, "dependencyKeys"> & { id: string; dependencyKeys: string };
type EditorSession = {
  identity: string;
  baseProjectRevision: number;
  baseCandidateRevisionId: string | null;
  baseContent: string;
};
type DirectionDecision = "" | "return-draft" | "accept-and-open-impact" | "false-positive";
type DirectionDecisionValue = { decision: DirectionDecision; note: string };
type FactKind = "world" | "reader" | "character" | "author-secret";
type EpistemicState = "unknown" | "suspected" | "known" | "false";
type PlanningTarget = { key: string; kind: "outline" | "plan"; revisionId: string; nodeId: string; label: string; currentArrangement: string; protected: boolean; protectionLabel: string };
type ReviewUiSnapshot = {
  factKind: FactKind;
  factSubjectId: string;
  factPredicate: string;
  factValue: string;
  factEpistemicOwnerId: string;
  factEpistemicState: EpistemicState;
  factEvidence: string;
  factSceneGroupId: string;
  factDecisions: Record<string, "confirmed" | "rejected">;
  noConfirmedChanges: boolean;
  factDependencyTargets: Record<string, string[]>;
  directionChangeDecisions: Record<string, DirectionDecisionValue>;
  manualDirectionDecisions: Record<string, DirectionDecisionValue>;
  selectedSceneGroups: string[];
};

interface Props {
  moduleActionsContainer?: HTMLElement | null;
  recordsOpen?: boolean;
  outlineCatalogError?: string | null;
  outlineVolume?: import("./types").WorkCatalog["volumes"][number] | null;
  onOpenVersions?: (outlineNodeId?: string | null) => void;
  onOutlineRecordScopeChange?: (outlineNodeId: string | null) => void;
  onCloseOutlineVolume?: () => void;
  onOpenOutlineNode?: (nodeId: string) => "volume" | "node" | "blocked";
  onLinkOutline?: (volumeId: string, source: import("./types").WorkVolumeSource) => Promise<void>;
  requestedChapterCreation?: SidebarChapterCreationRequest | null;
  onChapterCreationPrepared?: (requestId: string) => void;
  requestedCommitment?: CommitmentNavigationSelection | null;
  taskResultSelection?: (ResolvedTaskResult & { requestId: string }) | null;
  onOpenTaskResult?: (result: TaskResultDestination) => void;
  onDiscuss?: (prompt: string, source?: DiscussionSource) => void;
  api?: NovelCodexApi;
  taskConversationContainer: HTMLElement | null;
  taskList: WorkTaskList;
  taskControls: WorkTaskControls;
  conversation?: WorkConversationContent | null;
  projectId: string;
  view: WorkspaceView;
  state: RuntimeState;
  selectedChapterId: string;
  onSelectChapter: (chapter: Chapter) => void;
  onGenerateWork: (intent: string, targetChapters: number, expectedProjectRevision: number,
    expectedBaseSnapshotId: string, expectedSourceDigest: string, revisionPredecessor: RevisionPredecessor | null) => Promise<void>;
  onContinueWork: (runId: string) => Promise<void>;
  onRetryWork: (runId: string) => Promise<void>;
  onResolveFeedback: (runId: string, assessmentRevisionId: string, choice: "continue" | "repairDraft", instruction: string) => Promise<void>;
  onReviewPlanning: (request: PlanningReviewStartRequest) => Promise<void>;
  onCancelPlanningReview: (runId: string) => Promise<void>;
  onExport?: () => void;
  onPrepareChapterWriting?: (request: ChapterWritingRequest) => void;
  onGenerateChapter: (chapter: Chapter, intent: string, options: { povCharacterId: string | null; storyTime: number | null }) => Promise<void>;
  onOpenPlanningTarget: (kind: "outline" | "plan") => void;
  onReturnToFeedback: () => void;
  onFormalChange: () => Promise<void>;
  onDirtyChange: (dirty: boolean) => void;
  onChapterTitlePreview?: (preview: ChapterTitlePreview) => (() => void);
  onSaveHandlerChange?: (handler: (() => Promise<boolean>) | null) => void;
  onPlanningSaveHandlerChange?: (handler: (() => Promise<boolean>) | null) => void;
  onDiscardHandlerChange?: (handler: (() => boolean) | null) => void;
  agentAvailable: boolean;
  agentRunning: boolean;
  refreshToken: number;
  revisionContinuation?: RevisionContinuationPreparation | null;
  onPrepareRevisionContinuation?: (preparation: RevisionContinuationPreparation) => void;
  onPrepareRevisionRepair?: (preparation: RevisionRepairPreparation) => void;
  onDismissRevisionContinuation?: () => void;
}

const labels: Record<WorkspaceView, { title: string; description: string }> = {
  brief: { title: "作品概况", description: "明确创作方向，随写作逐步完善。尚未决定的内容可以留空。" },
  outline: { title: "全书大纲", description: "串起整部故事的起因、推进、关键转折与收束。各卷展开阶段事件，近章规划安排具体章节。" },
  plans: { title: "近章规划", description: "安排接下来几章的目标、冲突和写作指引，不必提前规划全书每一章。" },
  chapters: { title: "作品目录", description: "直接写这一章的正文。" },
};
const generationStageLabels: Record<GenerationRunSummary["stage"], string> = { coordinate: "安排创作步骤", finished: "完成" };

function emptyReviewUi(sceneGroupIds: string[] = []): ReviewUiSnapshot {
  return {
    factKind: "world",
    factSubjectId: "",
    factPredicate: "",
    factValue: "true",
    factEpistemicOwnerId: "",
    factEpistemicState: "known",
    factEvidence: "",
    factSceneGroupId: sceneGroupIds[0] ?? "",
    factDecisions: {},
    noConfirmedChanges: false,
    factDependencyTargets: {},
    directionChangeDecisions: {},
    manualDirectionDecisions: {},
    selectedSceneGroups: sceneGroupIds,
  };
}

function readReviewUi(key: string, fallback: ReviewUiSnapshot): ReviewUiSnapshot {
  try {
    const stored = localStorage.getItem(key);
    if (!stored) return fallback;
    return { ...fallback, ...JSON.parse(stored) as Partial<ReviewUiSnapshot> };
  } catch {
    return fallback;
  }
}

function writeReviewUi(key: string, snapshot: ReviewUiSnapshot) {
  try { localStorage.setItem(key, JSON.stringify(snapshot)); } catch { /* Local recovery is best effort. */ }
}

function removeReviewUi(key: string) {
  try { localStorage.removeItem(key); } catch { /* Local recovery is best effort. */ }
}

function asText(value: unknown): string {
  return typeof value === "string" ? value : "";
}

function factKindLabel(kind: string): string {
  return ({ world: "世界事实", reader: "读者已知", character: "人物认知", "author-secret": "作者秘密" } as Record<string, string>)[kind] ?? "正文事实";
}

function epistemicStateLabel(state: string): string {
  return ({ unknown: "未知", suspected: "怀疑", known: "已知", false: "已证伪" } as Record<string, string>)[state] ?? state;
}

function manualDecisionKey(factId: string, targetKey: string): string {
  return `${factId}\0${targetKey}`;
}

function newPlanRow(chapterNumber: number): EditableChapterPlan {
  return {
    id: `chapter-plan-${crypto.randomUUID()}`,
    primaryNodeId: "",
    chapterNumber,
    title: "",
    goal: "",
    conflict: "",
    dependencyKeys: "",
    scenes: [],
    locked: false,
    publicCommitments: [],
    allowances: [],
    writerGuidance: "",
    povCharacterId: null,
    storyTime: null,
  };
}

function newOutlineRoot(): OutlineNode { return { clientKey: `book-${crypto.randomUUID()}`, kind: "book", position: 0, title: "全书主线", publicTitle: "", purpose: "", summary: "", dependencyKeys: [], certainty: "open", protection: "free", writerGuidance: "" }; }

function editablePlanRows(payload: Record<string, unknown> | undefined): EditableChapterPlan[] {
  if (!payload || !Array.isArray(payload.chapters)) return [];
  return payload.chapters.flatMap((item) => {
    if (!item || typeof item !== "object") return [];
    const chapter = item as Partial<ChapterPlanPayload>;
    return [{
      id: asText(chapter.chapterPlanId) || `chapter-plan-${chapter.chapterNumber}`,
      chapterPlanId: asText(chapter.chapterPlanId) || undefined,
      primaryNodeId: asText(chapter.primaryNodeId),
      chapterNumber: Number(chapter.chapterNumber) || 1,
      title: asText(chapter.title),
      goal: asText(chapter.goal),
      conflict: asText(chapter.conflict),
      dependencyKeys: Array.isArray(chapter.dependencyKeys) ? chapter.dependencyKeys.join(", ") : "",
      scenes: Array.isArray(chapter.scenes) ? chapter.scenes : [],
      locked: chapter.locked === true,
      publicTitle: asText(chapter.publicTitle),
      writerGuidance: asText(chapter.writerGuidance),
      publicCommitments: Array.isArray(chapter.publicCommitments) ? chapter.publicCommitments : [],
      allowances: Array.isArray(chapter.allowances) ? chapter.allowances : [],
      povCharacterId: asText(chapter.povCharacterId) || null,
      storyTime: Number.isInteger(chapter.storyTime) ? chapter.storyTime : null,
    }];
  });
}

function reviewFor(workspace: CreativeWorkspace, kind: CreativeArtifactKind | "draft", revisionId?: string): CreativeReview | undefined {
  return workspace.reviews.find((review) => review.artifactKind === kind && review.artifactRevisionId === revisionId && review.projectRevision === workspace.revision && review.status === "ready");
}

function provisionalImpactsFor(review: CreativeReview | undefined): ProvisionalPlanningImpact[] {
  const impacts = review?.selections.provisionalImpacts;
  return Array.isArray(impacts) ? impacts as ProvisionalPlanningImpact[] : [];
}

async function provisionalImpactId(factId: string, target: PlanningTarget, predicate: string): Promise<string> {
  const source = `${factId}\0${target.kind}\0${target.revisionId}\0${target.nodeId}\0${predicate}`;
  const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(source));
  const hex = [...new Uint8Array(digest)].map((value) => value.toString(16).padStart(2, "0")).join("");
  return `provisional-impact-${hex.slice(0, 32)}`;
}

export function CreativeWorkspaceView({ moduleActionsContainer, recordsOpen = false, outlineCatalogError = null, outlineVolume = null, onCloseOutlineVolume, onOpenVersions, onOutlineRecordScopeChange, onOpenOutlineNode, onLinkOutline, requestedChapterCreation = null, onChapterCreationPrepared, requestedCommitment = null, taskResultSelection = null, onOpenTaskResult, onDiscuss, api, taskList, taskControls, taskConversationContainer, conversation = null, projectId, view, state, selectedChapterId, onSelectChapter, onGenerateWork, onContinueWork, onRetryWork, onResolveFeedback, onReviewPlanning, onCancelPlanningReview, onExport, onPrepareChapterWriting, onGenerateChapter, onOpenPlanningTarget, onReturnToFeedback, onFormalChange, onDirtyChange, onChapterTitlePreview, onSaveHandlerChange, onPlanningSaveHandlerChange, onDiscardHandlerChange, agentAvailable, agentRunning, refreshToken, revisionContinuation = null, onPrepareRevisionContinuation, onPrepareRevisionRepair, onDismissRevisionContinuation }: Props) {
  const supplementalEditor = useSupplementalEditor();
  const writingRef = useRef(false);
  const [workspace, setWorkspace] = useState<CreativeWorkspace | null>(null);
  const [generationRuns, setGenerationRuns] = useState<GenerationRunSummary[]>([]);
  const [generationRunsProjectId, setGenerationRunsProjectId] = useState<string | null>(null);
  const [generationReadError, setGenerationReadError] = useState(false);
  const [generationReload, setGenerationReload] = useState(0);
  const [generationPermitReview, setGenerationPermitReview] = useState<{ projectId: string; intent: string; targetChapters: number;
    expectedProjectRevision: number; expectedBaseSnapshotId: string; expectedSourceDigest: string;
    revisionPredecessor: RevisionPredecessor | null } | null>(null);
  const [generationConfirming, setGenerationConfirming] = useState(false);
  const [generationPermitError, setGenerationPermitError] = useState<string | null>(null);
  const confirmationRef = useRef({ review: generationPermitReview, api, projectId, view, workspace, blocked: false });
  confirmationRef.current = { review: generationPermitReview, api, projectId, view, workspace, blocked: false };
  const confirmingRef = useRef(false);
  useEffect(() => () => { confirmationRef.current.review = null; }, []);
  useEffect(() => { setGenerationPermitReview(null); }, [projectId, view]);
  useEffect(() => {
    if (revisionContinuation?.projectId === projectId) {
      setGenerationPermitReview({ ...revisionContinuation, targetChapters: 1 });
    }
  }, [revisionContinuation, projectId]);
  useEffect(() => { setGenerationPermitError(null); }, [generationPermitReview]);
  const generationPermit = generationPermitForChapters(generationPermitReview?.targetChapters ?? 1);
  const generationPermitInputError = useMemo(() => generationPermitReview ? authorIntentError(generationPermitReview.intent) : null, [generationPermitReview?.intent]);
  const [futureIntent, setFutureIntent] = useState("");
  const [futureIntentSaving, setFutureIntentSaving] = useState(false);
  const [feedbackAssessment, setFeedbackAssessment] = useState<SceneAssessmentSummary | null>(null);
  const [feedbackInstruction, setFeedbackInstruction] = useState("");
  useEffect(() => { setGenerationRuns([]); setGenerationRunsProjectId(null); setGenerationReadError(false); }, [projectId]);
  useEffect(() => { setFutureIntent(""); }, [projectId]);
  useEffect(() => {
    let cancelled = false;
    const refresh = () => { void api?.runtime.listGenerationRuns(projectId).then((result) => {
      if (!cancelled) { setGenerationRuns(result.runs); setGenerationRunsProjectId(projectId); setGenerationReadError(false); }
    }).catch(() => { if (!cancelled) setGenerationReadError(true); }); };
    refresh();
    const interval = agentRunning ? window.setInterval(refresh, 1500) : undefined;
    return () => { cancelled = true; if (interval !== undefined) window.clearInterval(interval); };
  }, [api, projectId, refreshToken, agentRunning, generationReload]);
  const currentGenerationRun = generationRunsProjectId === projectId
    ? generationRuns.find((run) => run.status === "running" || run.status === "paused" || run.status === "blocked")
    : undefined;
  const feedbackRepairBudgetUnavailable = currentGenerationRun
    ? currentGenerationRun.providerBudget.overrun
      || (currentGenerationRun.providerBudget.maxRequests !== null && currentGenerationRun.providerBudget.maxRequests <= currentGenerationRun.providerBudget.usedRequests)
      || (currentGenerationRun.providerBudget.maxTokens !== null && currentGenerationRun.providerBudget.maxTokens <= currentGenerationRun.providerBudget.usedTokens + currentGenerationRun.providerBudget.unknownTokens)
      || (currentGenerationRun.maxTaskAttempts !== null && currentGenerationRun.usedTaskAttempts >= currentGenerationRun.maxTaskAttempts)
    : true;
  const feedbackRepairUnavailable = feedbackRepairBudgetUnavailable || currentGenerationRun?.stage !== "coordinate"
    || currentGenerationRun.status !== "blocked" || Boolean(currentGenerationRun.activeTaskId);
  useEffect(() => {
    let cancelled = false;
    if (currentGenerationRun?.blockedReason !== "scene_feedback_needs_author" || !currentGenerationRun.currentChapterId || !api) {
      setFeedbackAssessment(null);
      return () => { cancelled = true; };
    }
    setFeedbackAssessment(null);
    void api.runtime.getSceneAssessment(projectId, currentGenerationRun.currentChapterId).then((result) => {
      if (!cancelled) setFeedbackAssessment(result);
    }).catch((reason) => { if (!cancelled) setError(reason instanceof Error ? reason.message : "读取场景评估失败"); });
    return () => { cancelled = true; };
  }, [api, projectId, currentGenerationRun?.id, currentGenerationRun?.blockedReason, currentGenerationRun?.currentChapterId, refreshToken]);
  const activeGenerationStep = currentGenerationRun && taskList.tasks.find(task => task.projectId === projectId
    && task.id === `work-${currentGenerationRun.id}` && task.kind === "generation")?.steps.find(step =>
      step.currentAttemptId === currentGenerationRun.activeTaskId);
  const futureIntentBlockedByActiveStep = !currentGenerationRun || currentGenerationRun.stage !== "coordinate"
    || currentGenerationRun.pendingCurrentIntentGeneration !== null
    || Boolean(currentGenerationRun.activeTaskId && !currentGenerationRun.currentChapterId)
    || currentGenerationRun.completedChapters + (currentGenerationRun.currentChapterId ? 2 : 1) > currentGenerationRun.targetChapters;
  const currentRevisionUnavailable = currentGenerationRun?.stage !== "coordinate"
    || !currentGenerationRun.activeTaskId || !currentGenerationRun.currentChapterId || !currentGenerationRun.currentChapterNumber
    || activeGenerationStep?.operation !== "generateChapter"
    || activeGenerationStep.target.chapterId !== currentGenerationRun.currentChapterId
    || currentGenerationRun.pendingCurrentIntentGeneration !== null
    || (currentGenerationRun.maxTaskAttempts !== null && currentGenerationRun.usedTaskAttempts >= currentGenerationRun.maxTaskAttempts);
  async function updateWorkTask(workTaskId: string, command: "pause" | "cancel" | "resume" | "recoverSavedResult") {
    if (!api || workspaceBusy || writingRef.current || dirty || reviewDirty || supplementalEditor.active
      || ((command === "resume" || command === "recoverSavedResult") && agentRunning)) return;
    await taskControls.control(api, workTaskId, command);
  }
  async function undoWorkChange(commitId: string) {
    if (!api || !workspace || agentRunning) return;
    await taskControls.run("undo", commitId, () => api.runtime.undoCommit(projectId, commitId, workspace.revision),
      "撤销失败：请检查后续修改是否依赖这个版本", "已撤销这次采用，作品版本已更新。");
  }
  async function applyGenerationIntent(target: "current" | "future") {
    if (!api || !currentGenerationRun || currentGenerationRun.status !== "running" || futureIntentSaving || dirty
      || (target === "current" ? currentRevisionUnavailable : futureIntentBlockedByActiveStep)) return;
    const instruction = futureIntent.trim();
    if (!instruction || Array.from(instruction).length > 1_000) return;
    setFutureIntentSaving(true);
    setError(null);
    try {
      const run = target === "current"
        ? await api.agent.reviseCurrentChapter(projectId, currentGenerationRun.id, instruction)
        : await api.agent.addFutureIntent(projectId, currentGenerationRun.id, instruction);
      setFutureIntent("");
      setNotice(target === "current"
        ? `当前章修订要求已保存为第 ${run.intentGeneration} 版；本步结束后将基于候选稿重写。`
        : `后续创作要求已保存为第 ${run.intentGeneration} 版；已有章节不会重写。`);
      await onFormalChange();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : "保存创作要求失败");
    } finally {
      setFutureIntentSaving(false);
    }
  }
  const runningNow = useRef(agentRunning);
  runningNow.current = agentRunning;
  const handledResult = useRef<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [workspaceBusy, setBusy] = useState(false);
  const [busyAction, setBusyAction] = useState("");
  const [dirty, setDirty] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [brief, setBrief] = useState({ premise: "", promise: "", stakes: "", endingDirection: "" });
  const [outlineNodes, setOutlineNodes] = useState<OutlineNode[]>(() => [newOutlineRoot()]);
  const [plans, setPlans] = useState<EditableChapterPlan[]>(() => [newPlanRow(1)]);
  const [draftSceneRows, setDraftSceneRows] = useState<NonNullable<CreativeWorkspace["chapters"][number]["candidate"]>["sceneGroups"]>([]);
  const [draftTitle, setDraftTitle] = useState("");
  const draft = draftSceneRows.map((scene) => scene.body).join("\n\n---\n\n");
  const [newChapterOpen, setNewChapterOpen] = useState(false);
  const [chapterCreationRequest, setChapterCreationRequest] = useState(0);
  const [newChapterVolume, setNewChapterVolume] = useState<{ projectId: string; volumeId: string | null; volumeTitle?: string | null }>({ projectId, volumeId: null });
  const consumedChapterCreation = useRef<string | null>(null);
  const [createdChapterPending, setCreatedChapterPending] = useState<{ projectId: string; chapterId: string; error: string } | null>(null);
  const createdChapterRef = useRef<{ projectId: string; chapterId: string } | null>(null);
  const creationScopeRef = useRef<string | null>(`${projectId}:${view}`);
  creationScopeRef.current = `${projectId}:${view}`;
  useEffect(() => () => { creationScopeRef.current = null; }, []);
  const [factKind, setFactKind] = useState<FactKind>("world");
  const [factSubjectId, setFactSubjectId] = useState("");
  const [factPredicate, setFactPredicate] = useState("");
  const [factValue, setFactValue] = useState("true");
  const [factEpistemicOwnerId, setFactEpistemicOwnerId] = useState("");
  const [factEpistemicState, setFactEpistemicState] = useState<EpistemicState>("known");
  const [factEvidence, setFactEvidence] = useState("");
  const [factSceneGroupId, setFactSceneGroupId] = useState("");
  const [factDecisions, setFactDecisions] = useState<Record<string, "confirmed" | "rejected">>({});
  const [noConfirmedChanges, setNoConfirmedChanges] = useState(false);
  const [factDependencyTargets, setFactDependencyTargets] = useState<Record<string, string[]>>({});
  const [directionChangeDecisions, setDirectionChangeDecisions] = useState<Record<string, DirectionDecisionValue>>({});
  const [manualDirectionDecisions, setManualDirectionDecisions] = useState<Record<string, DirectionDecisionValue>>({});
  const [outlineError, setOutlineError] = useState<{ nodeId: string; message: string } | null>(null);
  const [selectedSceneGroups, setSelectedSceneGroups] = useState<string[]>([]);
  const [reviewDirty, setReviewDirty] = useState(false);
  const [commitmentResultsScope, setCommitmentResultsScope] = useState<string | null>(null);
  const [publicationTarget, setPublicationTarget] = useState<ChapterPublicationTarget | null>(null);
  const [publicationOpen, setPublicationOpen] = useState(false);
  const [publicationPreparing, setPublicationPreparing] = useState(false);
  const pendingChapterSave = useRef<{ scope: string; signature: string; revisionId: string } | null>(null);
  const pendingChapterAccept = useRef<{ scope: string; reviewId: string; revisionId: string } | null>(null);
  const chapterAcceptSubmission = useRef(false);
  const [chapterAcceptActivity, setChapterAcceptActivity] = useState<object | null>(null);
  const busy = workspaceBusy || taskControls.pending || publicationPreparing || chapterAcceptActivity !== null;
  const [reviewStateIdentity, setReviewStateIdentity] = useState("");
  const [lockedSceneGroups, setLockedSceneGroups] = useState<string[]>([]);
  const chapterWritingSubmitRef = useRef<(intent: string) => Promise<boolean>>(async () => false);
  const chapterDiscussionPrepareRef = useRef<() => Promise<boolean>>(async () => false);
  const [conflict, setConflict] = useState<{ baseCandidateRevisionId: string | null; serverCandidateRevisionId: string | null; baseContent: string } | null>(null);
  const [generating, setGenerating] = useState(false);
  const chapterGenerationSubmission = useRef(false);
  const [firstWriteTitleNotice, setFirstWriteTitleNotice] = useState<{ identity: string; savedTitle: string } | null>(null);
  const [chapterSyncError, setChapterSyncError] = useState<{ identity: string; message: string } | null>(null);
  const [associationPlanId, setAssociationPlanId] = useState("");
  const [impactCursor, setImpactCursor] = useState<string | null>(null);
  const [impactCounts, setImpactCounts] = useState({ total: 0, open: 0, retained: 0, lockedConflict: 0 });
  const [activeSceneId, setActiveSceneId] = useState<string | null>(null);
  const [previewScope, setPreviewScope] = useState<"chapter" | "scene">("chapter");
  const [feedbackTarget, setFeedbackTarget] = useState<{ impactId: string; targetKind: "outline" | "plan"; targetNodeId: string; targetLabel: string } | null>(null);
  const reviewRef = useRef<HTMLElement | null>(null);
  const planningEditorRef = useRef<HTMLDetailsElement | null>(null);
  const lastSavedRevisionIdRef = useRef<string | null>(null);
  const pendingPlanningSave = useRef<{ owner: object; signature: string; revisionId: string } | null>(null);
  const impactRefs = useRef(new Map<string, HTMLElement>());
  const dirtyRef = useRef(false);
  const reviewDirtyRef = useRef(false);
  const loadedProjectRef = useRef<string | null>(null);
  const editorSessionRef = useRef<EditorSession | null>(null);
  const chapterTitleOwnerRef = useRef<object | null>(null);
  const chapterBaselineRef = useRef<{ identity: string; content: string } | null>(null);

  const selectedChapter = state.novelEdition.chapters.find((chapter) => chapter.id === selectedChapterId) ?? state.novelEdition.chapters[0];
  const workspaceChapter = workspace?.chapters.find((chapter) => chapter.id === selectedChapter?.id);
  const chapterModeling = workspace?.modeling?.find((item) => item.objectId === `draft-${selectedChapter?.id}`);
  const latestChapterTask = workspace?.tasks.find((task) => task.target?.chapterId === selectedChapter?.id && task.purpose === "writer");
  const viewKind: CreativeArtifactKind | "draft" = view === "plans" ? "plan" : view === "chapters" ? "draft" : view;
  const artifact = viewKind === "draft" ? null : workspace?.artifacts[viewKind];
  const [readingOutlineNodeId, setReadingOutlineNodeId] = useState<string | null>(null);
  useEffect(() => { setReadingOutlineNodeId(null); }, [projectId, view, outlineVolume?.id]);
  const outlineRevision = view === "outline" ? artifact?.candidate ?? artifact?.accepted : null;
  const savedOutlineNodes = (outlineRevision?.payload.nodes ?? []) as OutlineNode[];
  const requestedOutlineNodeId = outlineVolume ? outlineVolume.source?.nodeId ?? null : readingOutlineNodeId;
  useEffect(() => {
    if (view === "outline") onOutlineRecordScopeChange?.(requestedOutlineNodeId);
  }, [view, requestedOutlineNodeId, onOutlineRecordScopeChange]);
  const matchedOutlineNodes = requestedOutlineNodeId ? savedOutlineNodes.filter(node => (node.id ?? node.clientKey) === requestedOutlineNodeId) : [];
  const focusedOutlineNode = matchedOutlineNodes.length === 1 && matchedOutlineNodes[0].kind === "volume" ? matchedOutlineNodes[0] : null;
  const openRecords = onOpenVersions ? () => onOpenVersions(view === "outline" ? requestedOutlineNodeId : null) : undefined;
  const outlineFocusRequested = view === "outline" && Boolean(outlineVolume || readingOutlineNodeId);
  const outlineFocusUnavailable = outlineFocusRequested && !focusedOutlineNode;
  const outlineHeadingRef = useRef<HTMLHeadingElement>(null);
  useEffect(() => {
    if (!outlineFocusRequested) return;
    const frame = requestAnimationFrame(() => outlineHeadingRef.current?.focus());
    return () => cancelAnimationFrame(frame);
  }, [outlineFocusRequested, requestedOutlineNodeId, outlineVolume?.id]);
  const closeOutlineFocus = () => {
    if (outlineVolume) { onCloseOutlineVolume?.(); return; }
    if (!dirty && !reviewDirty) setReadingOutlineNodeId(null);
  };

  const planningReviewRevision = view === "outline" || view === "plans"
    ? artifact?.candidate ?? artifact?.working ?? artifact?.accepted ?? null
    : null;
  const planningReviewOutlineNodes = view === "outline" && Array.isArray(planningReviewRevision?.payload.nodes)
    ? planningReviewRevision.payload.nodes as OutlineNode[]
    : [];
  const artifactLocked = viewKind !== "draft" && artifact?.locked === true;
  const candidateId = viewKind === "draft" ? workspaceChapter?.candidate?.id : artifact?.candidate?.id;
  const currentReview = workspace && candidateId ? viewKind === "draft"
    ? workspace.reviews.find(review => publicationTarget?.scope === `${projectId}:${view}:${selectedChapter?.id ?? "new"}` && review.id === publicationTarget?.reviewId && review.artifactRevisionId === candidateId && review.status === "ready")
    : reviewFor(workspace, viewKind, candidateId) : undefined;
  const resolvedKnowledgeReview = chapterModeling?.sourceRevisionId === candidateId && workspace?.reviews.find(review => review.artifactKind === "draft"
    && review.artifactRevisionId === chapterModeling?.sourceRevisionId && review.status === "accepted"
    && review.selections.modelingBatchId === chapterModeling?.batchId);
  const pendingChapterKnowledge = chapterModeling?.status === "ready" && chapterModeling.sourceRevisionId === candidateId
    && !resolvedKnowledgeReview;
  const reviewSessionKey = `novel-codex:review-ui:${projectId}:${selectedChapter?.id ?? "none"}:${currentReview ? `review:${currentReview.id}` : candidateId ? `candidate:${candidateId}` : "empty"}`;
  const reviewFacts = useMemo(() => [...new Map((workspace?.factProposals.filter((fact) => fact.draft_revision_id === candidateId && (viewKind !== "draft" || publicationTarget?.factIds.includes(fact.id))) ?? []).map(fact => [fact.id, fact])).values()], [workspace, candidateId, publicationTarget, viewKind]);
  const publicationEntities = chapterModeling?.status === "ready" && chapterModeling.sourceRevisionId === candidateId ? chapterModeling.entities : [];
  const provisionalImpacts = useMemo(() => provisionalImpactsFor(currentReview), [currentReview]);
  const acceptedPlanChapters = Array.isArray(workspace?.artifacts.plan.accepted?.payload.chapters) ? workspace.artifacts.plan.accepted.payload.chapters as ChapterPlanPayload[] : [];
  const boundPlanIds = new Set(workspace?.chapterPlanBindings?.map((binding) => binding.chapter_plan_id) ?? []);
  const chapterBinding = workspace?.chapterPlanBindings?.find((binding) => binding.chapter_id === selectedChapter?.id);
  const chapterPlan = chapterBinding?.writerProjection;
  const activeSceneIndex = draftSceneRows.findIndex(scene => scene.id === activeSceneId);
  const activeSceneOrder = activeSceneIndex < 0 ? null : activeSceneIndex + 1;
  const previewSceneOrder = previewScope === "scene" && activeSceneOrder !== null ? activeSceneOrder : 2_147_483_647;
  const workingSourceIdentity = workspace?.workingBaselines.map((item) => `${item.objectKind}:${item.objectId}:${item.revisionId}:${item.selectionRevision}`).join("|") ?? "";
  const writerPreviewIdentity = view === "chapters" && workspace && selectedChapter
    ? `${projectId}:${workspace.revision}:${selectedChapter.id}:${previewSceneOrder}:${workingSourceIdentity}:${chapterBinding?.plan_revision_id ?? ""}:${chapterBinding?.chapter_plan_id ?? ""}:${chapterPlan?.povCharacterId ?? ""}:${chapterPlan?.storyTime ?? ""}`
    : null;
  const writerPreviewRead = useWriterPreview({ api, projectId, revision: workspace?.revision ?? null,
    chapterNumber: selectedChapter?.number ?? null, sceneOrder: previewSceneOrder, binding: chapterBinding, identity: writerPreviewIdentity });
  const writerPreview = writerPreviewRead.read?.status === "current" ? writerPreviewRead.read.value : null;
  const availableAssociationPlans = workspace?.planLineageStatus === "current" ? acceptedPlanChapters.filter((entry) => entry.chapterPlanId && !boundPlanIds.has(entry.chapterPlanId) && entry.chapterNumber === selectedChapter?.number) : [];
  const outlineForPlan = workspace?.artifacts.outline.working ?? workspace?.artifacts.outline.accepted;
  const outlinePaths = useMemo(() => {
    const nodes = Array.isArray(outlineForPlan?.payload.nodes) ? outlineForPlan.payload.nodes as OutlineNode[] : [];
    return outlinePathLabels(nodes);
  }, [outlineForPlan?.id]);
  const planningTargets = useMemo<PlanningTarget[]>(() => {
    if (!workspace) return [];
    const outline = workspace.artifacts.outline.accepted;
    const plan = workspace.artifacts.plan.accepted;
    const targets: PlanningTarget[] = [];
    if (outline && Array.isArray(outline.payload.nodes)) {
      for (const node of outline.payload.nodes as OutlineNode[]) {
        if (!node.id) continue;
        targets.push({ key: `outline:${node.id}`, kind: "outline", revisionId: outline.id, nodeId: node.id, label: outlinePaths.get(node.id) || node.publicTitle || node.title || "未命名大纲节点", currentArrangement: [node.purpose, node.summary].filter(Boolean).join("；") || node.title, protected: node.protection !== "free", protectionLabel: node.protection === "preserve-arrangement" ? "保护安排" : node.protection === "preserve-intent" ? "保护意图" : "自由调整" });
      }
    }
    if (plan && Array.isArray(plan.payload.chapters)) {
      for (const chapter of plan.payload.chapters as ChapterPlanPayload[]) {
        if (!chapter.chapterPlanId) continue;
        const parentPath = chapter.primaryNodeId ? outlinePaths.get(chapter.primaryNodeId) : undefined;
        const chapterLabel = `第 ${chapter.chapterNumber} 章 · ${chapter.publicTitle || chapter.title || "未命名"}`;
        targets.push({ key: `plan:${chapter.chapterPlanId}`, kind: "plan", revisionId: plan.id, nodeId: chapter.chapterPlanId, label: parentPath ? `${parentPath} → ${chapterLabel}` : chapterLabel, currentArrangement: [chapter.goal, chapter.conflict].filter(Boolean).join("；") || chapterLabel, protected: chapter.locked === true, protectionLabel: chapter.locked ? "已锁定" : "自由调整" });
      }
    }
    return targets;
  }, [workspace?.artifacts.outline.accepted?.id, workspace?.artifacts.plan.accepted?.id, outlinePaths]);
  const reviewUiSnapshot = useMemo<ReviewUiSnapshot>(() => ({
    factKind,
    factSubjectId,
    factPredicate,
    factValue,
    factEpistemicOwnerId,
    factEpistemicState,
    factEvidence,
    factSceneGroupId,
    factDecisions,
    noConfirmedChanges,
    factDependencyTargets,
    directionChangeDecisions,
    manualDirectionDecisions,
    selectedSceneGroups,
  }), [factKind, factSubjectId, factPredicate, factValue, factEpistemicOwnerId, factEpistemicState, factEvidence, factSceneGroupId, factDecisions, noConfirmedChanges, factDependencyTargets, directionChangeDecisions, manualDirectionDecisions, selectedSceneGroups]);
  const editorIdentity = `${projectId}:${view}:${view === "chapters" ? selectedChapter?.id ?? "new" : viewKind}`;
  const activeIdentityRef = useRef({ scope: editorIdentity, api });
  if (activeIdentityRef.current.scope !== editorIdentity || activeIdentityRef.current.api !== api) activeIdentityRef.current = { scope: editorIdentity, api };
  const editorOwner = activeIdentityRef.current;
  if (pendingPlanningSave.current && pendingPlanningSave.current.owner !== editorOwner) pendingPlanningSave.current = null;
  useEffect(() => {
    activeIdentityRef.current = editorOwner;
    return () => {
      if (activeIdentityRef.current === editorOwner) activeIdentityRef.current = { ...editorOwner };
    };
  }, [editorOwner]);
  const publication = useChapterPublication({ scope: editorIdentity, api, projectId, chapterId: selectedChapter?.id,
    target: publicationTarget, bodyOnlyConfirmed: noConfirmedChanges, dirty,
    isBlocked: () => writingRef.current || taskControls.hasPendingRequests() || generating || agentRunning || supplementalEditor.active,
    save: saveCurrent, savedRevision: () => lastSavedRevisionIdRef.current,
    manualProposals: next => manualFactProposals(next),
    onWorkspace: next => { setWorkspace(next); loadEditor(next); },
    onTarget: target => {
      if (target.revisionId !== publicationTarget?.revisionId) { setNoConfirmedChanges(false); setFactDecisions({}); }
      setPublicationTarget(target); setPublicationOpen(true);
      if (target.reviewId) {
        const key = `novel-codex:review-ui:${projectId}:${selectedChapter?.id ?? "none"}:review:${target.reviewId}`;
        const choices = readReviewUi(key, { ...reviewUiSnapshot, factDecisions: {}, noConfirmedChanges: target.bodyOnlyConfirmed, selectedSceneGroups: target.sceneIds });
        restoreReviewUi({ ...choices, selectedSceneGroups: target.sceneIds, noConfirmedChanges: target.knowledge === "modeled" ? false : choices.noConfirmedChanges });
        setReviewStateIdentity(key);
      }
      setSelectedSceneGroups(target.sceneIds);
    }, onError: setError });
  useEffect(() => { setPublicationPreparing(publication.preparing); }, [publication.preparing]);
  useEffect(() => { setPublicationOpen(false); setChapterAcceptActivity(null); chapterAcceptSubmission.current = false; }, [editorOwner]);
  const closePublication = () => { setPublicationOpen(false); document.querySelector<HTMLButtonElement>('[data-publish-chapter]')?.focus({ preventScroll: true }); };

  function restoreReviewUi(snapshot: ReviewUiSnapshot) {
    setFactKind(snapshot.factKind);
    setFactSubjectId(snapshot.factSubjectId);
    setFactPredicate(snapshot.factPredicate);
    setFactValue(snapshot.factValue);
    setFactEpistemicOwnerId(snapshot.factEpistemicOwnerId);
    setFactEpistemicState(snapshot.factEpistemicState);
    setFactEvidence(snapshot.factEvidence);
    setFactSceneGroupId(snapshot.factSceneGroupId);
    setFactDecisions(snapshot.factDecisions);
    setNoConfirmedChanges(snapshot.noConfirmedChanges);
    setFactDependencyTargets(snapshot.factDependencyTargets);
    setDirectionChangeDecisions(snapshot.directionChangeDecisions);
    setManualDirectionDecisions(snapshot.manualDirectionDecisions);
    setSelectedSceneGroups(snapshot.selectedSceneGroups);
  }

  function markReviewDirty() {
    reviewDirtyRef.current = true;
    setReviewDirty(true);
  }

  function discardCurrentReviewUi() {
    if (writingRef.current || busy || taskControls.hasPendingRequests() || supplementalEditor.current.current?.busy) return false;
    if (supplementalEditor.current.current && !supplementalEditor.current.current.discard()) return false;
    if (workspace) loadEditor(workspace);
    removeReviewUi(reviewSessionKey);
    const fallback = emptyReviewUi(workspaceChapter?.candidate?.sceneGroups.map((group) => group.id) ?? []);
    restoreReviewUi(fallback);
    reviewDirtyRef.current = false;
    setReviewDirty(false);
    return true;
  }

  function candidateRevision(next: CreativeWorkspace) {
    return viewKind === "draft"
      ? next.chapters.find((chapter) => chapter.id === selectedChapter?.id)?.candidate ?? null
      : next.artifacts[viewKind].candidate;
  }

  function candidateRevisionId(next: CreativeWorkspace): string | null {
    return candidateRevision(next)?.id ?? null;
  }

  function candidateContent(next: CreativeWorkspace): string {
    const revision = candidateRevision(next);
    return revision ? "body" in revision ? `${revision.title}\n\n${revision.body}` : JSON.stringify(revision.payload, null, 2) : "（当时没有候选版本）";
  }

  function loadEditor(next: CreativeWorkspace) {
    chapterTitleOwnerRef.current = next.project.id === projectId ? editorOwner : null;
    const candidate = candidateRevisionId(next);
    editorSessionRef.current = { identity: editorIdentity, baseProjectRevision: next.revision, baseCandidateRevisionId: candidate, baseContent: candidateContent(next) };
    setConflict(null);
    if (view === "brief") {
      const payload = next.artifacts.brief.candidate?.payload ?? next.artifacts.brief.accepted?.payload;
      setBrief({ premise: asText(payload?.premise), promise: asText(payload?.promise), stakes: asText(payload?.stakes), endingDirection: asText(payload?.endingDirection) });
    } else if (view === "outline") {
      const payload = next.artifacts.outline.candidate?.payload ?? next.artifacts.outline.accepted?.payload;
      setOutlineNodes(payload && Array.isArray(payload.nodes) ? payload.nodes as OutlineNode[] : [newOutlineRoot()]);
    } else if (view === "plans") {
      const payload = next.artifacts.plan.candidate?.payload ?? next.artifacts.plan.accepted?.payload;
      const rows = editablePlanRows(payload);
      setPlans(rows.length ? rows : [newPlanRow(1)]);
    } else {
      const chapter = next.chapters.find((item) => item.id === selectedChapter?.id);
      const revision = chapter?.candidate ?? chapter?.accepted;
      const title = chapter ? savedChapterTitle(chapter) : "";
      const groups = revision?.sceneGroups ?? [];
      const scenes = groups.length ? groups : [newDraftScene()];
      const locked = groups.filter((group) => group.locked).map((group) => group.id);
      chapterBaselineRef.current = { identity: editorIdentity, content: chapterEditorContent(title, scenes, locked) };
      setDraftTitle(title);
      setDraftSceneRows(scenes);
      setLockedSceneGroups(locked);
    }
    dirtyRef.current = false;
    setDirty(false);
  }

  function updateChapterDirty(title: string, scenes: typeof draftSceneRows, locked: string[]) {
    const baseline = chapterBaselineRef.current;
    const changed = baseline?.identity !== editorIdentity || chapterEditorContent(title, scenes, locked) !== baseline.content;
    dirtyRef.current = changed;
    setDirty(changed);
  }

  async function reload(validate?: (next: CreativeWorkspace) => void) {
    if (!api) return;
    const next = await api.runtime.getCreativeWorkspace(projectId);
    const impactPage = await api.runtime.listPlanningImpacts(projectId, null, 25);
    if (activeIdentityRef.current !== editorOwner) return;
    validate?.(next);
    setWorkspace({ ...next, impacts: impactPage.data, openImpactCount: impactPage.counts.open });
    setImpactCursor(impactPage.nextCursor);
    setImpactCounts(impactPage.counts);
    loadEditor(next);
    setLoading(false);
  }

  useEffect(() => {
    let cancelled = false;
    const projectChanged = loadedProjectRef.current !== projectId;
    loadedProjectRef.current = projectId;
    if (!projectChanged) {
      void Promise.all([api?.runtime.getCreativeWorkspace(projectId), api?.runtime.listPlanningImpacts(projectId, null, 25)]).then(([next, impactPage]) => {
        if (!cancelled && next && impactPage) {
          setWorkspace({ ...next, impacts: impactPage.data, openImpactCount: impactPage.counts.open });
          setImpactCursor(impactPage.nextCursor);
          setImpactCounts(impactPage.counts);
          const session = editorSessionRef.current;
          const serverCandidateRevisionId = candidateRevisionId(next);
          if ((dirtyRef.current || reviewDirtyRef.current || supplementalEditor.current.current)
            && session?.identity === editorIdentity && serverCandidateRevisionId !== session.baseCandidateRevisionId) {
            setConflict({ baseCandidateRevisionId: session.baseCandidateRevisionId, serverCandidateRevisionId, baseContent: session.baseContent });
          }
          setLoading(false);
        }
      }).catch((reason) => {
        if (!cancelled) { setError(reason instanceof Error ? reason.message : "工作区刷新失败"); setLoading(false); }
      });
      return () => { cancelled = true; };
    }
    dirtyRef.current = false;
    editorSessionRef.current = null;
    setWorkspace(null);
    setLoading(true);
    setError(null);
    setNotice(null);
    setDirty(false);
    setBrief({ premise: "", promise: "", stakes: "", endingDirection: "" });
    setOutlineNodes([newOutlineRoot()]);
    setPlans([newPlanRow(1)]);
    setDraftSceneRows([]);
    if (!api) { setLoading(false); return; }
    void Promise.all([api.runtime.getCreativeWorkspace(projectId), api.runtime.listPlanningImpacts(projectId, null, 25)]).then(([next, impactPage]) => {
      if (!cancelled) {
        setWorkspace({ ...next, impacts: impactPage.data, openImpactCount: impactPage.counts.open });
        setImpactCursor(impactPage.nextCursor);
        setImpactCounts(impactPage.counts);
        setLoading(false);
      }
    }).catch((reason) => {
      if (!cancelled) { setError(reason instanceof Error ? reason.message : "工作区加载失败"); setLoading(false); }
    });
    return () => { cancelled = true; };
  }, [api, projectId, refreshToken]);

  useEffect(() => {
    if (!api || !(agentRunning || taskControls.resuming) || dirty) return;
    let cancelled = false;
    let pending = false;
    const refreshGeneration = async () => {
      if (pending) return;
      pending = true;
      try {
        const next = await api.runtime.getCreativeWorkspace(projectId);
        if (!cancelled) setWorkspace((current) => current ? { ...next, impacts: current.impacts, openImpactCount: current.openImpactCount } : next);
      } catch { /* The completion refresh reports persistent Runtime failures. */ }
      finally { pending = false; }
    };
    void refreshGeneration();
    const timer = window.setInterval(() => void refreshGeneration(), taskControls.resuming ? 1500 : 800);
    return () => { cancelled = true; window.clearInterval(timer); };
  }, [api, agentRunning, taskControls.resuming, dirty, projectId]);

  useEffect(() => {
    if (!workspace) return;
    const session = editorSessionRef.current;
    if (!session || session.identity !== editorIdentity) {
      loadEditor(workspace);
      return;
    }
    const serverCandidateRevisionId = candidateRevisionId(workspace);
    if (dirty || dirtyRef.current) {
      if (serverCandidateRevisionId !== session.baseCandidateRevisionId) setConflict({ baseCandidateRevisionId: session.baseCandidateRevisionId, serverCandidateRevisionId, baseContent: session.baseContent });
      return;
    }
    loadEditor(workspace);
  }, [workspace, editorIdentity]);

  const chapterTitlePreviewReady = view === "chapters" && !loading && workspace?.project.id === projectId
    && chapterTitleOwnerRef.current === editorOwner && editorSessionRef.current?.identity === editorIdentity;
  useEffect(() => {
    if (!onChapterTitlePreview || !chapterTitlePreviewReady || !selectedChapter) return;
    return onChapterTitlePreview({ projectId, chapterId: selectedChapter.id, title: draftTitle });
  }, [onChapterTitlePreview, chapterTitlePreviewReady, projectId, selectedChapter?.id, draftTitle]);

  const visibleChapters = displayChapters(state.novelEdition.chapters,
    workspace?.project.id === projectId ? workspace.chapters.map(chapter => ({ id: chapter.id, title: savedChapterTitle(chapter) })) : [],
    chapterTitlePreviewReady && selectedChapter
      ? { projectId, chapterId: selectedChapter.id, title: draftTitle } : null);

  useEffect(() => {
    if (!chapterBinding) setAssociationPlanId(availableAssociationPlans[0]?.chapterPlanId ?? "");
  }, [chapterBinding?.chapter_plan_id, selectedChapter?.id, availableAssociationPlans.map((entry) => entry.chapterPlanId).join("|")]);

  useEffect(() => {
    setFeedbackTarget(null);
    setActiveSceneId(null);
    setPreviewScope("chapter");
  }, [projectId, selectedChapter?.id]);



  useEffect(() => {
    document.documentElement.dataset.creativeWorkspace = "true";
    return () => { delete document.documentElement.dataset.creativeWorkspace; };
  }, []);

  useEffect(() => {
    dirtyRef.current = dirty;
    reviewDirtyRef.current = reviewDirty;
    onDirtyChange(dirty || reviewDirty || supplementalEditor.dirty || supplementalEditor.busy);
    return () => onDirtyChange(false);
  }, [dirty, reviewDirty, supplementalEditor.dirty, supplementalEditor.busy, onDirtyChange]);

  useEffect(() => {
    if (reviewStateIdentity === reviewSessionKey) return;
    const sceneGroupIds = workspaceChapter?.candidate?.sceneGroups.map((group) => group.id) ?? [];
    restoreReviewUi(readReviewUi(reviewSessionKey, emptyReviewUi(sceneGroupIds)));
    reviewDirtyRef.current = false;
    setReviewDirty(false);
    setReviewStateIdentity(reviewSessionKey);
  }, [reviewSessionKey, reviewStateIdentity]);

  useEffect(() => {
    if (!reviewDirty || reviewStateIdentity !== reviewSessionKey) return;
    writeReviewUi(reviewSessionKey, reviewUiSnapshot);
  }, [reviewDirty, reviewStateIdentity, reviewSessionKey, reviewUiSnapshot]);

  useEffect(() => {
    if (!feedbackTarget || loading) return;
    const frame = requestAnimationFrame(() => {
      if (view === "plans" && feedbackTarget.targetKind === "plan") {
        const card = document.querySelector<HTMLElement>(`[data-planning-node-id="${feedbackTarget.targetNodeId}"]`);
        if (card && planningEditorRef.current) planningEditorRef.current.open = true;
        card?.scrollIntoView({ block: "center" });
        card?.querySelector<HTMLElement>("input, textarea, select")?.focus({ preventScroll: true });
      } else if (view === "chapters") {
        const impact = impactRefs.current.get(feedbackTarget.impactId);
        impact?.scrollIntoView({ block: "center" });
        impact?.focus({ preventScroll: true });
      }
    });
    return () => cancelAnimationFrame(frame);
  }, [feedbackTarget, loading, view]);

  async function perform(actionName: string, action: () => Promise<void>, success: string, validate?: (next: CreativeWorkspace) => void): Promise<boolean> {
    if (writingRef.current || taskControls.hasPendingRequests() || supplementalEditor.active) return false;
    writingRef.current = true;
    setBusy(true);
    setBusyAction(actionName);
    setError(null);
    setNotice(null);
    try {
      await action();
      if (activeIdentityRef.current !== editorOwner) return false;
      await reload(validate);
      if (activeIdentityRef.current !== editorOwner) return false;
      setNotice(success);
      return true;
    } catch (reason) {
      if (activeIdentityRef.current !== editorOwner) return false;
      if (reason instanceof StoppedExecutionError) {
        setNotice(reason.message);
        try { await reload(); } catch {
          if (activeIdentityRef.current === editorOwner) setNotice(`${reason.message}内容状态待重新读取。`);
        }
        return false;
      }
      const message = reason instanceof Error ? reason.message : "操作失败";
      setError(message);
      if (/revision|版本|candidate|候选|conflict|冲突/i.test(message)) {
        const session = editorSessionRef.current;
        try {
          const [next, impactPage] = await Promise.all([
            api?.runtime.getCreativeWorkspace(projectId),
            api?.runtime.listPlanningImpacts(projectId, null, 25),
          ]);
          if (activeIdentityRef.current !== editorOwner) return false;
          if (next && impactPage) {
            setWorkspace({ ...next, impacts: impactPage.data, openImpactCount: impactPage.counts.open });
            setImpactCursor(impactPage.nextCursor);
            setImpactCounts(impactPage.counts);
            setConflict({
              baseCandidateRevisionId: session?.baseCandidateRevisionId ?? null,
              serverCandidateRevisionId: candidateRevisionId(next),
              baseContent: session?.baseContent ?? "（基线不可用）",
            });
          }
        } catch {
          if (activeIdentityRef.current !== editorOwner) return false;
          setConflict({
            baseCandidateRevisionId: session?.baseCandidateRevisionId ?? null,
            serverCandidateRevisionId: workspace ? candidateRevisionId(workspace) : null,
            baseContent: session?.baseContent ?? "（基线不可用）",
          });
        }
      }
      return false;
    } finally {
      writingRef.current = false;
      setBusy(false);
      setBusyAction("");
    }
  }

  async function savePlanning(kind: CreativeArtifactKind, payload: CreativeArtifactPayload, expectedRevision: number, expectedCandidate: string | null, linkedImpacts: string[], action: string, success: string): Promise<boolean> {
    if (!api) return false;
    const signature = JSON.stringify({ payload, linkedImpacts });
    const previousReceipt = pendingPlanningSave.current;
    if (previousReceipt?.owner === editorOwner && previousReceipt.signature !== signature) {
      setError("规划已保存，请先核对保存结果；本地新修改仍保留，不会再次保存。");
      return false;
    }
    let sourceError: string | null = null;
    const completed = await perform(action, async () => {
      const pending = pendingPlanningSave.current;
      if (pending?.owner === editorOwner) return;
      const saved = await api.runtime.saveArtifact(projectId, kind, payload, expectedRevision, expectedCandidate, linkedImpacts);
      if (activeIdentityRef.current !== editorOwner) return;
      pendingPlanningSave.current = { owner: editorOwner, signature, revisionId: saved.revisionId };
      lastSavedRevisionIdRef.current = saved.revisionId;
    }, success, next => {
      const pending = pendingPlanningSave.current;
      if (!pending || pending.owner !== editorOwner || next.project.id !== projectId || next.artifacts[kind].candidate?.id !== pending.revisionId) {
        sourceError = "已保存的规划来源已变化，请重新核对；本地编辑仍保留。";
        throw new Error(sourceError);
      }
    });
    if (activeIdentityRef.current !== editorOwner) return false;
    if (completed) {
      pendingPlanningSave.current = null;
      if (kind === "outline") {
        try { await onFormalChange(); }
        catch (reason) {
          if (activeIdentityRef.current === editorOwner) setError(`大纲已保存，但目录刷新失败：${reason instanceof Error ? reason.message : "请重新读取作品"}`);
        }
      }
    }
    else if (pendingPlanningSave.current?.owner === editorOwner) {
      setError(previous => sourceError ?? `${previous ? `${previous}。` : ""}规划已保存，但读取结果尚未确认。再次保存只会核对该保存版本，不会重复写入；本地编辑仍保留。`);
    }
    return completed;
  }

  async function saveCurrent(baselineWorkspace?: CreativeWorkspace): Promise<boolean> {
    const activeWorkspace = baselineWorkspace ?? workspace;
    if (!api || !activeWorkspace) return false;
    const session = editorSessionRef.current;
    if (!session || session.identity !== editorIdentity) throw new Error("编辑会话已切换，请重新打开当前内容");
    const expectedRevision = session.baseProjectRevision;
    const expectedCandidateRevisionId = session.baseCandidateRevisionId;
    if (view === "brief") {
      return savePlanning("brief", brief, expectedRevision, expectedCandidateRevisionId, [], "正在保存创作简报…", "创作简报已保存为候选版本。");
    } else if (view === "outline") {
      const firstInvalid = firstOutlineEditorError(outlineNodes);
      if (firstInvalid) {
        setOutlineError(firstInvalid);
        setError("大纲还有未完成字段；已定位到第一处错误。");
        return false;
      }
      setOutlineError(null);
      const linkedImpactIds = feedbackTarget?.targetKind === "outline" ? [feedbackTarget.impactId] : [];
      return savePlanning("outline", { nodes: outlineNodes }, expectedRevision, expectedCandidateRevisionId, linkedImpactIds, "正在保存全书大纲…", linkedImpactIds.length ? "全书大纲候选已链接原反馈；采用后将自动关闭该反馈。" : "全书大纲已保存为候选版本。");
    } else if (view === "plans") {
      const outline = activeWorkspace.artifacts.outline.working ?? activeWorkspace.artifacts.outline.accepted;
      if (!outline) throw new Error("请先保存并选用全书大纲，再保存近章规划");
      const defaultNodeId = (outline.payload.nodes as OutlineNode[]).at(-1)?.id;
      const payload: RollingPlanPayload = { sourceOutlineRevisionId: outline.id, chapters: plans.map(({ id, dependencyKeys, ...chapter }) => ({ ...chapter, dependencyKeys: dependencyKeys.split(",").map((key) => key.trim()).filter(Boolean), chapterPlanId: chapter.chapterPlanId || undefined, clientKey: chapter.chapterPlanId ? undefined : id, primaryNodeId: chapter.primaryNodeId || defaultNodeId || "", publicTitle: chapter.publicTitle ?? "", writerGuidance: chapter.writerGuidance ?? "", publicCommitments: chapter.publicCommitments ?? [], allowances: chapter.allowances ?? [] })), window: plans.length };
      const linkedImpactIds = feedbackTarget?.targetKind === "plan" ? [feedbackTarget.impactId] : [];
      return savePlanning("plan", payload, expectedRevision, expectedCandidateRevisionId, linkedImpactIds, "正在保存近章规划…", linkedImpactIds.length ? "近章规划候选已链接原反馈；采用后将自动关闭该反馈。" : "近章规划已保存为候选版本。");
    } else {
      if (!draftTitle.trim()) {
        setError("请填写章节标题，再保存。");
        return false;
      }
      if (draftSceneRows.some((scene) => !scene.body.trim())) {
        setError("请填写空场景正文，或删除不需要的空场景。");
        return false;
      }
      const hasFactDraft = Boolean(factPredicate.trim() || factEvidence.trim());
      if (hasFactDraft && (!factPredicate.trim() || !factEvidence.trim())) {
        setError("新增事实必须同时填写事实键和正文证据。");
        return false;
      }
      if (hasFactDraft && factValue.trim()) {
        try { parseFactValue(factValue); } catch (reason) {
          setError(reason instanceof Error ? reason.message : "事实值格式无效");
          return false;
        }
      }
      const sceneGroups = draftSceneRows.map((scene) => ({ id: scene.id, sceneId: scene.sceneId ?? null, body: scene.body, locked: lockedSceneGroups.includes(scene.id) }));
      const normalizedBody = sceneGroups.map((scene) => scene.body.trim()).join("\n\n---\n\n");
      const payload = { ...(selectedChapter ? { chapterId: selectedChapter.id, chapterNumber: selectedChapter.number } : { chapterNumber: 1 }), title: draftTitle.trim(), body: normalizedBody, sceneGroups };
      const signature = JSON.stringify(payload);
      const completed = await perform("正在保存这一章…", async () => {
        const pending = pendingChapterSave.current;
        if (pending?.scope === editorIdentity && pending.signature !== signature) throw new Error("章节已保存，请先重新读取保存结果再继续编辑。");
        const saved = pending?.scope === editorIdentity ? pending : await api.runtime.saveDraftCheckpoint(projectId, payload, expectedRevision, expectedCandidateRevisionId);
        if (activeIdentityRef.current !== editorOwner) return;
        pendingChapterSave.current = { scope: editorIdentity, signature, revisionId: saved.revisionId };
        lastSavedRevisionIdRef.current = saved.revisionId;
        if (hasFactDraft) writeReviewUi(`novel-codex:review-ui:${projectId}:${selectedChapter?.id ?? "none"}:candidate:${saved.revisionId}`,
          { ...reviewUiSnapshot, selectedSceneGroups: sceneGroups.map(scene => scene.id) });
        await onFormalChange();
      }, "这一章已保存；发布后更新已发布内容。", next => {
        if (next.project.id !== projectId || candidateRevisionId(next) !== pendingChapterSave.current?.revisionId) throw new Error("已保存的章节内容已变化，请重新读取确认；本地编辑仍保留。");
      });
      if (activeIdentityRef.current === editorOwner) {
        if (completed) pendingChapterSave.current = null;
        else if (pendingChapterSave.current?.scope === editorIdentity) setError("章节已保存，但读取结果失败。重新读取不会重复保存，正文仍保留。");
      }
      return completed;
    }
  }

  async function savePendingChanges(): Promise<boolean> {
    if (writingRef.current || busy || taskControls.hasPendingRequests()) return false;
    if (supplementalEditor.current.current) return supplementalEditor.current.current.save();
    if (dirty && !(await saveCurrent())) return false;
    if (reviewDirty) {
      writeReviewUi(reviewSessionKey, reviewUiSnapshot);
      reviewDirtyRef.current = false;
      setReviewDirty(false);
    }
    return true;
  }

  useEffect(() => {
    if (!onSaveHandlerChange) return;
    onSaveHandlerChange(savePendingChanges);
    return () => onSaveHandlerChange(null);
  });

  const canSavePlanningForDiscussion = view !== "chapters" && Boolean(api && workspace) && dirty && !reviewDirty
    && !supplementalEditor.active && !busy && !loading && !agentRunning && !artifactLocked && !conflict;
  const planningSaveAvailable = useRef(false);
  planningSaveAvailable.current = canSavePlanningForDiscussion;
  useEffect(() => {
    if (!onPlanningSaveHandlerChange) return;
    onPlanningSaveHandlerChange(canSavePlanningForDiscussion ? async () => {
      if (activeIdentityRef.current !== editorOwner || !planningSaveAvailable.current || !dirtyRef.current || reviewDirtyRef.current
        || supplementalEditor.current.current || writingRef.current || runningNow.current || taskControls.hasPendingRequests()) return false;
      if (!await saveCurrent() || activeIdentityRef.current !== editorOwner || dirtyRef.current || reviewDirtyRef.current
        || supplementalEditor.current.current) return false;
      onDirtyChange(false);
      return true;
    } : null);
    return () => onPlanningSaveHandlerChange(null);
  });

  useEffect(() => {
    if (!onDiscardHandlerChange) return;
    onDiscardHandlerChange(discardCurrentReviewUi);
    return () => onDiscardHandlerChange(null);
  });

  async function enterReview(expectedRevisionId?: string) {
    const reviewRevisionId = expectedRevisionId ?? candidateId;
    if (!api || !reviewRevisionId) return;
    const hasFactDraft = Boolean(factPredicate.trim() || factEvidence.trim());
    if (hasFactDraft && (!factPredicate.trim() || !factEvidence.trim())) {
      setError("新增事实必须同时填写事实键和正文证据。");
      return;
    }
    let parsedValue: unknown = null;
    if (hasFactDraft) {
      try { parsedValue = parseFactValue(factValue); } catch (reason) {
        setError(reason instanceof Error ? reason.message : "事实值格式无效");
        return;
      }
    }
    const factSceneOrder = workspaceChapter?.candidate?.sceneGroups.findIndex((group) => group.id === factSceneGroupId) ?? -1;
    const factScene = workspaceChapter?.candidate?.sceneGroups[factSceneOrder];
    const proposals = viewKind === "draft" && hasFactDraft ? [{ sceneGroupId: factSceneGroupId, sceneId: factScene?.sceneId ?? null, factKind, subjectId: factSubjectId.trim() || null, predicate: factPredicate.trim(), value: parsedValue, epistemicOwnerId: factEpistemicOwnerId.trim() || null, epistemicState: factEpistemicState, storyTime: chapterPlan?.storyTime ?? null, readingChapter: selectedChapter?.number ?? 1, sceneOrder: Math.max(1, factSceneOrder + 1), evidence: factEvidence.trim() }] : [];
    const created = await perform("正在生成审阅包…", async () => {
      if (expectedRevisionId) {
        const fresh = await api.runtime.getCreativeWorkspace(projectId);
        if (activeIdentityRef.current !== editorOwner || runningNow.current || dirtyRef.current) throw new Error("当前页面或编辑状态已变化，请重新打开结果。");
        if (viewKind === "draft" || fresh.project.id !== projectId || fresh.artifacts[viewKind].candidate?.id !== expectedRevisionId) throw new Error("结果修订已变化，不能检查其他候选代替。");
        const ready = fresh.reviews.find(review => review.artifactKind === viewKind && review.artifactRevisionId === expectedRevisionId && review.projectRevision === fresh.revision && review.status === "ready");
        if (ready) return;
      }
      await api.runtime.createReview(projectId, viewKind, reviewRevisionId, proposals);
    }, "审阅包已生成，请检查后决定是否采用。");
    if (created) {
      if (view !== "chapters" && planningEditorRef.current) {
        planningEditorRef.current.open = true;
        requestAnimationFrame(() => {
          if (activeIdentityRef.current !== editorOwner) return;
          reviewRef.current?.scrollIntoView({ block: "center" });
          reviewRef.current?.focus({ preventScroll: true });
        });
      }
      removeReviewUi(reviewSessionKey);
      restoreReviewUi(emptyReviewUi(workspaceChapter?.candidate?.sceneGroups.map((group) => group.id) ?? []));
      reviewDirtyRef.current = false;
      setReviewDirty(false);
    }
  }

  useEffect(() => {
    const selection = taskResultSelection;
    if (!selection || selection.result.projectId !== projectId || selection.view !== view || loading || !workspace
      || workspace.project.id !== projectId || selection.chapterId && selection.chapterId !== selectedChapter?.id
      || handledResult.current === selection.requestId || busy || agentRunning || dirty || reviewDirty || supplementalEditor.active) return;
    handledResult.current = selection.requestId;
    const result = selection.result;
    if (result.kind === "knowledge" && selection.sourceRevisionId) void publication.prepare({ expectedSource: { revisionId: selection.sourceRevisionId, batchId: result.referenceId } });
    else if (result.kind === "brief" || result.kind === "outline" || result.kind === "plan") void enterReview(result.referenceId);
    else if (result.kind === "commitmentExtraction" || result.kind === "commitment") setCommitmentResultsScope(editorIdentity);
  }, [taskResultSelection, view, projectId, selectedChapter?.id, loading, workspace, busy, agentRunning, dirty, reviewDirty, supplementalEditor.active]);

  function manualFactProposals(next: CreativeWorkspace): Record<string, unknown>[] {
    if (!factPredicate.trim() && !factEvidence.trim()) return [];
    if (!factPredicate.trim() || !factEvidence.trim()) throw new Error("新增事实必须同时填写事实键和正文证据。");
    const groups = next.chapters.find(chapter => chapter.id === selectedChapter?.id)?.candidate?.sceneGroups ?? [];
    const index = groups.findIndex(group => group.id === factSceneGroupId);
    if (index < 0) throw new Error("新增事实的来源场景已变化，请重新选择。");
    return [{ sceneGroupId: groups[index].id, sceneId: groups[index].sceneId ?? null, factKind, subjectId: factSubjectId.trim() || null,
      predicate: factPredicate.trim(), value: parseFactValue(factValue), epistemicOwnerId: factEpistemicOwnerId.trim() || null, epistemicState: factEpistemicState,
      storyTime: chapterPlan?.storyTime ?? null, readingChapter: selectedChapter?.number ?? 1, sceneOrder: index + 1, evidence: factEvidence.trim() }];
  }

  async function recoverPublication() {
    if (pendingChapterAccept.current?.scope !== editorIdentity) return;
    if (await perform("正在读取发布结果…", onFormalChange, "这一章已发布。", validatePublishedReceipt)) {
      pendingChapterAccept.current = null; closePublication(); reviewDirtyRef.current = false; setReviewDirty(false);
    }
  }

  function validatePublishedReceipt(next: CreativeWorkspace) {
    const receipt = pendingChapterAccept.current;
    if (!receipt || next.project.id !== projectId || next.reviews.find(review => review.id === receipt.reviewId)?.status !== "accepted"
      || next.chapters.find(chapter => chapter.id === selectedChapter?.id)?.accepted?.id !== receipt.revisionId) throw new Error("发布结果已变化，请重新读取确认当前已发布章节。");
  }

  async function acceptCurrent() {
    if (!api || !workspace || !currentReview) return;
    if (viewKind === "draft" && (chapterAcceptSubmission.current || (!pendingChapterAccept.current && !canAccept))) return;
    if (viewKind === "draft") { chapterAcceptSubmission.current = true; setChapterAcceptActivity(editorOwner); }
    try {
    if (viewKind === "draft" && !pendingChapterAccept.current) {
      const latest = await api.runtime.getCreativeWorkspace(projectId);
      if (activeIdentityRef.current !== editorOwner) return;
      const source = latest.chapters.find(chapter => chapter.id === selectedChapter?.id)?.candidate;
      const modeling = latest.modeling.find(item => item.objectId === `draft-${selectedChapter?.id}`);
      if (latest.project.id !== projectId || source?.id !== candidateId || !modeling || modeling.sourceRevisionId !== candidateId) throw new Error("发布正文来源已变化，请重新读取确认。");
      if (latest.tasks.some(task => task.target?.chapterId === selectedChapter?.id && ["queued", "running", "waiting"].includes(task.status))) throw new Error("本章仍有创作任务进行中，请等待交付后再发布。");
      const batch = modeling.status === "ready" ? modeling.batchId : null;
      if ((batch ?? null) !== (currentReview.selections.modelingBatchId ?? null) || latest.revision !== workspace.revision) {
        await publication.prepare(); return;
      }
    }
    const factDependencyLinks: { factProposalId: string; targetArtifactKind: "outline" | "plan"; targetRevisionId: string; targetNodeId: string }[] = [];
    const resolvedDirectionDecisions: Record<string, "return-draft" | "accept-and-open-impact" | { decision: "false-positive"; note: string }> = {};
    if (viewKind === "draft") {
      for (const impact of provisionalImpacts) {
        if (factDecisions[impact.factProposalId] !== "confirmed" || impact.severity !== "locked-conflict") continue;
        const value = directionChangeDecisions[impact.id];
        if (value?.decision === "false-positive") resolvedDirectionDecisions[impact.id] = { decision: "false-positive", note: value.note.trim() };
        else if (value?.decision) resolvedDirectionDecisions[impact.id] = value.decision;
      }
      for (const fact of reviewFacts) {
        if (factDecisions[fact.id] !== "confirmed") continue;
        for (const targetKey of factDependencyTargets[fact.id] ?? []) {
          const target = planningTargets.find((item) => item.key === targetKey);
          if (!target) continue;
          factDependencyLinks.push({ factProposalId: fact.id, targetArtifactKind: target.kind, targetRevisionId: target.revisionId, targetNodeId: target.nodeId });
          if (target.protected) {
            const impactId = await provisionalImpactId(fact.id, target, fact.predicate);
            if (activeIdentityRef.current !== editorOwner) return;
            const value = manualDirectionDecisions[manualDecisionKey(fact.id, target.key)];
            if (value?.decision === "false-positive") resolvedDirectionDecisions[impactId] = { decision: "false-positive", note: value.note.trim() };
            else if (value?.decision) resolvedDirectionDecisions[impactId] = value.decision;
          }
        }
      }
      if (activeIdentityRef.current !== editorOwner) return;
      if (Object.values(resolvedDirectionDecisions).some((value) => value === "return-draft")) {
        setError(null);
        setNotice("已返回编辑，正文与检查决定已保留；尚未发布。");
        closePublication();
        document.querySelector<HTMLElement>("[data-chapter-draft-anchor]")?.scrollIntoView({ block: "center" });
        return;
      }
    }
    const selections = viewKind === "draft" ? { sceneGroupIds: workspaceChapter?.candidate?.sceneGroups.map(group => group.id) ?? [], factDecisions, noConfirmedChanges, factDependencyLinks, directionChangeDecisions: resolvedDirectionDecisions,
      ...(typeof currentReview.selections.modelingBatchId === "string" ? { modelingBatchId: currentReview.selections.modelingBatchId } : {}) } : {};
    const accepted = await perform(viewKind === "draft" ? "正在发布章节…" : "正在采用候选版本…", async () => {
      if (viewKind !== "draft" || pendingChapterAccept.current?.scope !== editorIdentity || pendingChapterAccept.current.reviewId !== currentReview.id) {
        await api.runtime.acceptReview(projectId, currentReview.id, workspace.revision, selections);
        if (activeIdentityRef.current !== editorOwner) return;
        if (viewKind === "draft") pendingChapterAccept.current = { scope: editorIdentity, reviewId: currentReview.id, revisionId: currentReview.artifactRevisionId };
      }
      await onFormalChange();
      if (viewKind !== "draft") setFeedbackTarget(null);
    }, viewKind === "draft" ? "这一章已发布。" : "已采用候选版本，作品正式版本已推进。", viewKind === "draft" ? validatePublishedReceipt : undefined);
    if (activeIdentityRef.current !== editorOwner) return;
    if (accepted && viewKind === "draft") {
      pendingChapterAccept.current = null;
      closePublication();
      removeReviewUi(reviewSessionKey);
      removeReviewUi(`novel-codex:review-ui:${projectId}:${selectedChapter?.id ?? "none"}:candidate:${candidateId}`);
      restoreReviewUi(emptyReviewUi());
      reviewDirtyRef.current = false;
      setReviewDirty(false);
    }
    if (!accepted && viewKind === "draft" && pendingChapterAccept.current?.scope === editorIdentity) setError("章节已发布，但读取结果失败。请重新读取发布结果，不会重复发布。");
    } catch (reason) { if (activeIdentityRef.current === editorOwner) setError(reason instanceof Error ? reason.message : "发布前读取失败，请重试。"); }
    finally { if (viewKind === "draft" && activeIdentityRef.current === editorOwner) { chapterAcceptSubmission.current = false; setChapterAcceptActivity(null); } }
  }

  async function undoLatest() {
    const latest = workspace?.commits[0];
    if (!api || !workspace || !latest || latest.operation_kind === "undo") return;
    await perform("正在撤销最近采用…", async () => {
      if (latest.proposalApplicationId) {
        await api.runtime.undoProposalSet(projectId, latest.proposalApplicationId, workspace.revision, `desktop-proposal-undo-${crypto.randomUUID()}`);
      } else {
        await api.runtime.undoCommit(projectId, latest.id, workspace.revision);
      }
      await onFormalChange();
    }, "最近一次采用已撤销，历史记录仍被保留。");
  }

  async function materializePlanChapter(chapterPlanId: string, chapterNumber: number) {
    if (!api || !workspace || dirty) return;
    await perform(`正在创建第 ${chapterNumber} 章…`, async () => {
      await api.runtime.materializeChapterPlan(projectId, chapterPlanId, workspace.revision);
      await onFormalChange();
    }, `第 ${chapterNumber} 章已从正式章纲创建，可以开始写作。`);
  }

  async function materializeStandalone(title?: string): Promise<boolean> {
    if (!api || !workspace || dirtyRef.current || reviewDirtyRef.current || busy || generating || agentRunning
      || supplementalEditor.active || writingRef.current) return false;
    const scope = `${projectId}:${view}`;
    const isCurrent = () => creationScopeRef.current === scope;
    writingRef.current = true;
    setBusy(true);
    setBusyAction("正在创建独立章节…");
    setError(null);
    try {
      let receipt = createdChapterRef.current?.projectId === projectId ? createdChapterRef.current : null;
      if (!receipt) {
        if (title === undefined) return false;
        const created = await api.runtime.materializeStandaloneChapter(projectId, title.trim(), workspace.revision, newChapterVolume.projectId === projectId ? newChapterVolume.volumeId : null);
        if (!isCurrent()) return false;
        receipt = { projectId, chapterId: created.chapterId };
        createdChapterRef.current = receipt;
      }
      await onFormalChange();
      const [next, nextWorkspace, impactPage] = await Promise.all([
        api.runtime.getState(projectId),
        api.runtime.getCreativeWorkspace(projectId),
        api.runtime.listPlanningImpacts(projectId, null, 25),
      ]);
      if (!isCurrent()) return false;
      const chapter = next.novelEdition.chapters.find(item => item.id === receipt.chapterId);
      if (!chapter || !nextWorkspace.chapters.some(item => item.id === receipt.chapterId)) throw new Error("已创建章节尚未出现在工作区，请重新读取。");
      setWorkspace({ ...nextWorkspace, impacts: impactPage.data, openImpactCount: impactPage.counts.open });
      setImpactCursor(impactPage.nextCursor);
      setImpactCounts(impactPage.counts);
      setLoading(false);
      createdChapterRef.current = null;
      setCreatedChapterPending(null);
      setNewChapterOpen(false);
      setNotice("章节已创建，可以手写或生成正文；发布后成为正式章节。");
      onSelectChapter(chapter);
      return true;
    } catch (reason) {
      if (!isCurrent()) return false;
      const message = reason instanceof Error ? reason.message : "章节创建失败";
      const receipt = createdChapterRef.current;
      if (receipt?.projectId === projectId) setCreatedChapterPending({ ...receipt, error: message });
      else setError(message);
      return false;
    } finally {
      writingRef.current = false;
      setBusy(false);
      setBusyAction("");
    }
  }

  async function resolveImpact(impactId: string, resolution: "kept" | "false-positive") {
    if (!api || !workspace || dirty) return;
    const note = resolution === "kept" ? "作者确认保留当前规划，不因本条正文事实调整。" : "作者确认该事实与此规划节点没有实际依赖关系。";
    await perform("正在记录作者的反馈决定…", async () => {
      await api.runtime.resolvePlanningImpact(projectId, impactId, workspace.revision, resolution, note);
      await onFormalChange();
      if (feedbackTarget?.impactId === impactId) setFeedbackTarget(null);
    }, resolution === "kept" ? "已记录：保留原规划。" : "已记录：此条反馈是误报。");
  }

  async function loadMoreImpacts() {
    if (!api || !workspace || !impactCursor || busy || taskControls.hasPendingRequests()) return;
    setBusy(true);
    try {
      const page = await api.runtime.listPlanningImpacts(projectId, impactCursor, 25);
      setWorkspace((current) => current ? { ...current, impacts: [...current.impacts, ...page.data.filter((item) => !current.impacts.some((existing) => existing.id === item.id))] } : current);
      setImpactCursor(page.nextCursor);
      setImpactCounts(page.counts);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : "规划反馈加载失败");
    } finally {
      setBusy(false);
    }
  }

  async function associateChapterPlan() {
    if (!api || !workspace || !selectedChapter || !associationPlanId || dirty) return;
    await perform("正在关联正式章纲…", async () => {
      await api.runtime.associateChapterPlan(projectId, selectedChapter.id, associationPlanId, workspace.revision);
      await onFormalChange();
    }, "独立章节已关联正式章纲；后续生成会使用精确规划依据。 ");
  }

  async function submitChapterPlanRebind(request: ChapterPlanRebindRequest) {
    if (!api || dirty || reviewDirty || draftInteractionLocked || writingRef.current || taskControls.hasPendingRequests()) throw new Error("请先保存修改并等待当前操作完成");
    writingRef.current = true; setBusy(true); setBusyAction("正在更新章纲关联…"); setError(null); setNotice(null);
    try {
      const result = await api.runtime.rebindChapterPlan(projectId, request);
      if (loadedProjectRef.current === projectId) {
        try { await onFormalChange(); if (loadedProjectRef.current === projectId) await reload(); }
        catch (reason) { if (activeIdentityRef.current === editorOwner) setError(`关联已成功，但刷新失败：${reason instanceof Error ? reason.message : "请重新读取作品"}`); }
      }
      return result;
    } finally { writingRef.current = false; setBusy(false); setBusyAction(""); }
  }

  const acceptedLockedGroups = new Map((workspaceChapter?.accepted?.sceneGroups ?? []).flatMap((group, index) => group.locked ? [[group.id, { group, index }] as const] : []));
  const draftGroupsById = new Map(draftSceneRows.map((group, index) => [group.id, { group, index }]));
  const lockedDraftConflict = [...acceptedLockedGroups].some(([id, accepted]) => {
    const candidate = draftGroupsById.get(id);
    return !candidate || candidate.index !== accepted.index || candidate.group.body !== accepted.group.body;
  });
  const selectedReviewFacts = reviewFacts.filter((fact) => selectedSceneGroups.includes(fact.sceneGroupId));
  const confirmedFactIds = new Set(selectedReviewFacts.filter((fact) => factDecisions[fact.id] === "confirmed").map((fact) => fact.id));
  const confirmedFacts = selectedReviewFacts.filter((fact) => confirmedFactIds.has(fact.id));
  const automaticLockedDecisionsComplete = provisionalImpacts.filter((impact) => impact.severity === "locked-conflict" && confirmedFactIds.has(impact.factProposalId)).every((impact) => {
    const value = directionChangeDecisions[impact.id];
    return Boolean(value?.decision && (value.decision !== "false-positive" || value.note.trim()));
  });
  const manualLockedDecisionsComplete = selectedReviewFacts.filter((fact) => confirmedFactIds.has(fact.id)).every((fact) => {
    return (factDependencyTargets[fact.id] ?? []).every((targetKey) => {
      const target = planningTargets.find((item) => item.key === targetKey);
      if (!target?.protected) return true;
      const value = manualDirectionDecisions[manualDecisionKey(fact.id, target.key)];
      return Boolean(value?.decision && (value.decision !== "false-positive" || value.note.trim()));
    });
  });
  const factsComplete = reviewFacts.length === 0 ? noConfirmedChanges : selectedReviewFacts.every((fact) => factDecisions[fact.id]);
  const reviewChecksPass = Boolean(currentReview) && (viewKind === "draft"
    ? selectionAwareReviewChecksPass(currentReview?.checks ?? [], confirmedFacts, workspaceChapter?.candidate?.sceneGroups ?? [])
    : currentReview?.checks.every((check) => !check.blocking || check.passed));
  const activeChapterDelivery = workspace?.tasks.some(task => task.target?.chapterId === selectedChapter?.id && ["queued", "running", "waiting"].includes(task.status));
  const publicationSourceMatches = Boolean(chapterModeling && chapterModeling.sourceRevisionId === candidateId && (publicationTarget?.knowledge === "modeled"
    ? chapterModeling.status === "ready" && chapterModeling.batchId === currentReview?.selections.modelingBatchId
    : chapterModeling.status === "model_pending"));
  const canAccept = Boolean(currentReview) && !dirty && reviewChecksPass && (viewKind !== "draft" || (publicationTarget?.scope === editorIdentity && publicationTarget.revisionId === candidateId && publicationSourceMatches && !activeChapterDelivery && selectedSceneGroups.length > 0 && factsComplete && automaticLockedDecisionsComplete && manualLockedDecisionsComplete));
  const returnsDraft = provisionalImpacts.some((impact) => confirmedFactIds.has(impact.factProposalId) && directionChangeDecisions[impact.id]?.decision === "return-draft")
    || selectedReviewFacts.some((fact) => confirmedFactIds.has(fact.id) && (factDependencyTargets[fact.id] ?? []).some((targetKey) => manualDirectionDecisions[manualDecisionKey(fact.id, targetKey)]?.decision === "return-draft"));
  const latestTaskStatus = latestChapterTask ? generationStatusLabel(latestChapterTask) : null;
  const draftInteractionLocked = busy || generating || agentRunning || (viewKind === "draft" && activeChapterDelivery) || supplementalEditor.active || createdChapterPending?.projectId === projectId || pendingChapterSave.current?.scope === editorIdentity;
  const undecidedReviewCount = selectedReviewFacts.filter(fact => !factDecisions[fact.id]).length;
  const bulkReviewAllowed = view === "chapters" && publicationOpen && publicationTarget?.scope === editorIdentity
    && publicationTarget.revisionId === candidateId && publicationTarget.reviewId === currentReview?.id
    && currentReview?.projectRevision === workspace?.revision && publicationSourceMatches
    && publicationTarget.factIds.length === selectedReviewFacts.length
    && selectedReviewFacts.every(fact => fact.draft_revision_id === candidateId && workspaceChapter?.candidate?.sceneGroups.some(scene => scene.id === fact.sceneGroupId))
    && !dirty && !draftInteractionLocked && !loading && !publication.preparing
    && taskList.loaded && !taskList.reading && !taskList.error && !chapterAcceptSubmission.current;
  const bulkReview = useRef({ owner: editorOwner, target: publicationTarget, allowed: bulkReviewAllowed, factIds: selectedReviewFacts.map(fact => fact.id) });
  bulkReview.current = { owner: editorOwner, target: publicationTarget, allowed: bulkReviewAllowed, factIds: selectedReviewFacts.map(fact => fact.id) };
  function decideUndecidedReviewFacts(decision: "confirmed" | "rejected") {
    const operation = bulkReview.current;
    if (!operation.allowed || operation.owner !== editorOwner || operation.target !== publicationTarget || !undecidedReviewCount || writingRef.current || taskControls.hasPendingRequests()) return;
    setFactDecisions(previous => {
      const current = bulkReview.current;
      if (!current.allowed || current.owner !== operation.owner || current.target !== operation.target) return previous;
      const undecided = current.factIds.filter(id => !previous[id]);
      return undecided.length ? { ...previous, ...Object.fromEntries(undecided.map(id => [id, decision])) } : previous;
    });
    markReviewDirty();
  }
  const reviewBlockers: string[] = [];
  if (dirty) reviewBlockers.push(viewKind === "draft" ? "先保存章节" : "先保存工作稿");
  if (viewKind === "draft") {
    if (!selectedSceneGroups.length) reviewBlockers.push("章节尚无可发布正文");
    const undecidedFacts = selectedReviewFacts.filter(fact => !factDecisions[fact.id]).length;
    if (undecidedFacts) reviewBlockers.push(`还有 ${undecidedFacts} 项事实未决定`);
    if (!reviewFacts.length && !noConfirmedChanges) reviewBlockers.push("确认本章无需写入新的故事事实");
    if (!automaticLockedDecisionsComplete || !manualLockedDecisionsComplete) reviewBlockers.push("完成保护冲突处理决定");
  }
  if (!reviewChecksPass) reviewBlockers.push(viewKind === "draft" ? "发布前检查尚未通过" : "采用前检查尚未通过");
  if (viewKind === "draft" && !publicationSourceMatches) reviewBlockers.push("正文或故事设定来源已变化，请重新准备发布检查");
  if (draftInteractionLocked) reviewBlockers.push("等待当前操作结束");
  const reviewProgress = reviewBlockers.length ? `${reviewBlockers.join("；")}。` : canAccept ? returnsDraft ? "已完成检查，可按决定返回编辑。" : viewKind === "draft" ? "已完成检查，可以发布这一章。" : "已完成检查，可采用。" : "等待检查。";
  const nextChapterNumber = Math.max(0, ...(workspace?.chapters ?? []).map(chapter => chapter.number)) + 1;
  const standaloneCreationLocked = draftInteractionLocked || dirty || reviewDirty || !workspace || loading;
  useEffect(() => {
    if (!requestedChapterCreation || requestedChapterCreation.projectId !== projectId || view !== "chapters"
      || standaloneCreationLocked || workspace?.project.id !== projectId
      || editorSessionRef.current?.identity !== editorIdentity
      || consumedChapterCreation.current === requestedChapterCreation.requestId) return;
    consumedChapterCreation.current = requestedChapterCreation.requestId;
    setNewChapterVolume(requestedChapterCreation);
    setNewChapterOpen(true);
    setChapterCreationRequest(value => value + 1);
    onChapterCreationPrepared?.(requestedChapterCreation.requestId);
  }, [requestedChapterCreation, projectId, view, standaloneCreationLocked, workspace, editorIdentity, onChapterCreationPrepared]);
  const discussionRevision = workspaceChapter?.candidate ?? workspaceChapter?.accepted;
  const sceneDiscussionDisabled = draftInteractionLocked || dirty || reviewDirty || loading || !discussionRevision;
  function prepareSceneDiscussion(sceneGroupId: string) {
    if (!onDiscuss || !workspace || !selectedChapter || !discussionRevision || sceneDiscussionDisabled
      || activeIdentityRef.current !== editorOwner || dirtyRef.current || reviewDirtyRef.current || runningNow.current
      || workspace.project.id !== projectId || editorSessionRef.current?.identity !== editorIdentity) return;
    const groups = discussionRevision.sceneGroups.filter(group => group.id === sceneGroupId);
    const local = draftSceneRows.filter(group => group.id === sceneGroupId);
    if (groups.length !== 1 || local.length !== 1 || groups[0].body !== local[0].body) {
      setError("场景正文与已保存来源不一致，请保存后再讨论。");
      return;
    }
    const index = discussionRevision.sceneGroups.indexOf(groups[0]);
    onDiscuss("请讨论这个场景的节奏，先不要改写。", {
      label: `第 ${selectedChapter.number} 章 · 场景 ${index + 1} · v${discussionRevision.version}`,
      focus: { view: "chapters", chapterId: selectedChapter.id, chapterNumber: selectedChapter.number,
        reference: { kind: "scene", projectId, snapshotId: workspace.headSnapshotId,
          objectId: `draft-${selectedChapter.id}`, revisionId: discussionRevision.id, sceneGroupId } },
    });
  }
  const info = labels[view];
  const analyzingChapter = taskList.tasks.some(task => task.status === "running" && task.steps.some(step =>
    step.operation === "modelKnowledge" && step.status === "running" && step.target.chapterId === selectedChapter?.id));
  const chapterBusyLabel = taskControls.resuming ? analyzingChapter ? "正文已保存，正在继续分析…" : "正在继续任务…"
    : taskControls.controlling ? "正在处理任务…" : busy ? busyAction || "正在处理当前操作…"
    : analyzingChapter ? "正文已保存，正在分析…" : generating ? "正在生成并保存…" : "正在处理当前任务…";
  const chapterBodyEmpty = !draftSceneRows.some(scene => scene.body.trim());
  const emptyChapter = !workspaceChapter?.candidate && !workspaceChapter?.accepted && !workspaceChapter?.working
    && chapterBodyEmpty;

  function updatePlan(index: number, change: Partial<(typeof plans)[number]>) {
    setPlans((rows) => rows.map((row, rowIndex) => rowIndex === index ? { ...row, ...change } : row));
    setDirty(true);
  }

  async function generateSelectedChapter(intent: string): Promise<boolean> {
    if (!api || !selectedChapter || !intent.trim() || Boolean(authorIntentError(intent)) || draftInteractionLocked || chapterGenerationSubmission.current) return false;
    if (!agentAvailable) {
      setError("当前没有可用模型。创作意图已保留，请配置模型后再开始写作。");
      return false;
    }
    chapterGenerationSubmission.current = true;
    setGenerating(true);
    setError(null);
    setNotice(null);
    setChapterSyncError(null);
    setFirstWriteTitleNotice(null);
    let executionCompleted = false;
    try {
      let currentWorkspace = await api.runtime.getCreativeWorkspace(projectId);
      if (activeIdentityRef.current !== editorOwner) return false;
      let currentChapter = currentWorkspace.chapters.find(
        (chapter) => chapter.id === selectedChapter.id
      );
      if (!currentChapter) throw new Error("当前章节已变化，请重新读取后开始写作。");
      const noSavedRevision = !currentChapter.candidate && !currentChapter.accepted && !currentChapter.working;
      if (noSavedRevision && !draftSceneRows.some(scene => scene.body.trim()) && dirtyRef.current
        && !reviewDirtyRef.current && draftSceneRows.length === 1 && !lockedSceneGroups.length && draftTitle !== currentChapter.title) {
        setFirstWriteTitleNotice({ identity: editorIdentity, savedTitle: currentChapter.title });
        return false;
      }
      const firstWrite = noSavedRevision && emptyChapter && !dirtyRef.current && !reviewDirtyRef.current;
      if (!firstWrite) {
        if (reviewDirtyRef.current) throw new Error("请先保存或明确放弃发布检查决定，再开始写作。");
        const hadUnsavedChanges = dirtyRef.current;
        if (hadUnsavedChanges && !(await saveCurrent())) return false;
        const revisionToKeep = hadUnsavedChanges ? lastSavedRevisionIdRef.current : candidateId ?? workspaceChapter?.accepted?.id;
        if (hadUnsavedChanges) {
          currentWorkspace = await api.runtime.getCreativeWorkspace(projectId);
          if (activeIdentityRef.current !== editorOwner) return false;
          currentChapter = currentWorkspace.chapters.find(chapter => chapter.id === selectedChapter.id);
        }
        if (!revisionToKeep || !currentChapter || ![currentChapter.candidate?.id, currentChapter.accepted?.id, currentChapter.working?.id].includes(revisionToKeep)) throw new Error("当前章节已有版本变化，请重新读取章节后再继续创作。");
        if (currentChapter.workingSelection?.revisionId !== revisionToKeep) await api.runtime.setWorkingBaseline(
          projectId,
          "draft",
          selectedChapter.id,
          revisionToKeep,
          currentChapter?.workingSelection?.selectionRevision ?? 0,
        );
        await api.runtime.saveContinuationIntent(projectId, "draft", selectedChapter.id, revisionToKeep, intent.trim());
      }
      if (activeIdentityRef.current !== editorOwner) return false;
      await onGenerateChapter(selectedChapter, intent.trim(), {
        povCharacterId: chapterPlan?.povCharacterId ?? null,
        storyTime: chapterPlan?.storyTime ?? null,
      });
      executionCompleted = true;
      if (activeIdentityRef.current !== editorOwner) return false;
      await reload();
      if (activeIdentityRef.current !== editorOwner) return false;
      setNotice(`第 ${selectedChapter.number} 章正文已保存，可以继续编辑或发布。`);
      return true;
    } catch (reason) {
      if (activeIdentityRef.current !== editorOwner) return false;
      if (executionCompleted) {
        setChapterSyncError({ identity: editorIdentity, message: reason instanceof Error ? reason.message : "工作区读取失败" });
        setNotice(`第 ${selectedChapter.number} 章生成执行已结束；正文状态待重新读取，不会自动重跑模型。`);
        return true;
      }
      if (!dirtyRef.current && !reviewDirtyRef.current) await reload().catch(() => undefined);
      if (activeIdentityRef.current !== editorOwner) return false;
      if (reason instanceof StoppedExecutionError) { setNotice(reason.message); return false; }
      setError(reason instanceof Error ? reason.message : "章节生成未完成；已写的正文仍会保留。");
      return false;
    } finally {
      chapterGenerationSubmission.current = false;
      setGenerating(false);
    }
  }

  chapterWritingSubmitRef.current = generateSelectedChapter;
  chapterDiscussionPrepareRef.current = async () => {
    if (!api || !selectedChapter || draftInteractionLocked || reviewDirtyRef.current || supplementalEditor.active) {
      setError("请先处理未保存的设定或检查，再讨论本章。");
      return false;
    }
    const current = await api.runtime.getCreativeWorkspace(projectId);
    if (activeIdentityRef.current !== editorOwner) return false;
    const chapter = current.chapters.find(item => item.id === selectedChapter.id);
    if (!chapter) throw new Error("当前章节已变化，请重新读取后开始写作。");
    const noSavedRevision = !chapter.candidate && !chapter.accepted && !chapter.working;
    if (noSavedRevision && !draftSceneRows.some(scene => scene.body.trim()) && dirtyRef.current
      && draftSceneRows.length === 1 && !lockedSceneGroups.length && draftTitle !== chapter.title) {
      setFirstWriteTitleNotice({ identity: editorIdentity, savedTitle: chapter.title });
      return false;
    }
    return !dirtyRef.current || await saveCurrent();
  };

  function prepareChapterWriting() {
    if (!selectedChapter || editorSessionRef.current?.identity !== editorIdentity) return;
    const owner = editorOwner;
    onPrepareChapterWriting?.({ projectId, chapterId: selectedChapter.id, chapterNumber: selectedChapter.number, title: draftTitle,
      defaultIntent: chapterBodyEmpty ? `${NEW_CHAPTER_WRITING_INTENT}保留章节标题《${draftTitle}》。` : "",
      prepareDiscussion: () => activeIdentityRef.current === owner ? chapterDiscussionPrepareRef.current() : Promise.resolve(false),
      submit: intent => activeIdentityRef.current === owner ? chapterWritingSubmitRef.current(intent) : Promise.resolve(false),
    });
  }

  async function copyLocalEditor() {
    const local = view === "chapters" ? `${draftTitle}\n\n${draft}` : view === "brief" ? JSON.stringify(brief, null, 2) : view === "outline" ? JSON.stringify({ nodes: outlineNodes }, null, 2) : JSON.stringify({ chapters: plans }, null, 2);
    try {
      await navigator.clipboard.writeText(local);
      setNotice("本地编辑已复制到剪贴板。");
    } catch {
      setError("无法访问剪贴板，请先在编辑器中全选复制。");
    }
  }

  async function saveLocalAsLatestCandidate() {
    if (!api || busy || taskControls.hasPendingRequests()) return;
    const session = editorSessionRef.current;
    if (!session || session.identity !== editorIdentity) return;
    setBusy(true);
    setBusyAction("正在载入最新版并保存本地稿…");
    setError(null);
    try {
      const next = await api.runtime.getCreativeWorkspace(projectId);
      editorSessionRef.current = rebaseEditorBaseline(session, {
        baseProjectRevision: next.revision,
        baseCandidateRevisionId: candidateRevisionId(next),
        baseContent: candidateContent(next),
      });
      setWorkspace((current) => current ? { ...next, impacts: current.impacts, openImpactCount: current.openImpactCount } : next);
      setConflict(null);
      setBusy(false);
      setBusyAction("");
      await saveCurrent(next);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : "无法基于服务器最新版保存本地稿");
    } finally {
      setBusy(false);
      setBusyAction("");
    }
  }

  confirmationRef.current.blocked = busy || agentRunning || dirty || reviewDirty || supplementalEditor.active
    || Boolean(generationPermitInputError) || !agentAvailable || Boolean(currentGenerationRun)
    || generationRunsProjectId !== projectId || generationReadError;
  const actionableTasks = Boolean(taskList.error) || taskList.activeCount > 0
    || taskList.tasks.some(task => (task.status !== "completed" && task.status !== "cancelled")
      || task.deliveryReport.pendingActionIds.length > 0 || task.deliveryReport.unsettledAttemptIds.length > 0);
  const chapterTools = selectedChapter ? <><button className="secondary-button" type="button" onClick={() => setCommitmentResultsScope(editorIdentity)}>查看伏笔与悬念</button><details key={`settings:${editorIdentity}`} className="chapter-settings"><summary>章节设置</summary>
        {api && chapterBinding && <ChapterPlanRebindPanel api={api} projectId={projectId} chapterId={selectedChapter.id} disabled={draftInteractionLocked || dirty || reviewDirty} hasUpdate={Boolean((workspace?.artifacts.plan.candidate ?? workspace?.artifacts.plan.accepted)?.id !== chapterBinding.plan_revision_id)} submit={submitChapterPlanRebind} refresh={async () => {
          if (dirty || reviewDirty || draftInteractionLocked) throw new Error("请先保存修改并等待当前操作完成");
          if (!await perform("正在重新读取作品…", onFormalChange, "作品已重新读取。")) throw new Error("重新读取作品失败");
        }} />}
          {latestChapterTask && latestTaskStatus && <details className="chapter-extra"><summary>查看生成记录</summary><GenerationTaskStatus task={latestChapterTask} outputName="本章正文修订" /></details>}
        <WriterPreviewPanel read={writerPreviewRead.read} retry={writerPreviewRead.retry} dirty={dirty} activeSceneOrder={activeSceneOrder} scope={previewScope} onScopeChange={setPreviewScope} />
        {!chapterBinding && <section className="standalone-planning-card"><div><strong>这是独立章节</strong><small>保持独立不会伪造大纲来源；也可以关联同章号且尚未开写的正式章纲。</small></div>{availableAssociationPlans.length > 0 ? <><select disabled={draftInteractionLocked} aria-label="可关联的正式章纲" value={associationPlanId} onChange={(event) => setAssociationPlanId(event.target.value)}>{availableAssociationPlans.map((entry) => <option key={entry.chapterPlanId} value={entry.chapterPlanId}>{entry.publicTitle || entry.title || `第 ${entry.chapterNumber} 章章纲`}</option>)}</select><button className="secondary-button" disabled={busy || draftInteractionLocked || dirty || !associationPlanId} onClick={() => void associateChapterPlan()}>关联章纲</button></> : <button className="secondary-button" disabled={draftInteractionLocked} onClick={() => onOpenPlanningTarget("plan")}>从正文建立章纲</button>}{workspaceChapter?.candidate && <small>关联会推进作品版本；已写的正文仍会保留。</small>}</section>}

        <details className="chapter-extra"><summary>记录本章新事实</summary><fieldset className="fact-proposal-form" disabled={draftInteractionLocked || dirty || !candidateId}>
          <h3 className="sr-only">本章新事实</h3>
          <small>保存时记录事实建议，发布前再确认是否更新故事设定。</small>
          {(workspaceChapter?.candidate?.sceneGroups.length ?? 0) > 1 && <label><span>来源场景</span><select value={factSceneGroupId} onChange={(event) => { setFactSceneGroupId(event.target.value); markReviewDirty(); }}>{workspaceChapter?.candidate?.sceneGroups.map((group, index) => <option key={group.id} value={group.id}>场景 {index + 1}</option>)}</select></label>}
          <label><span>事实种类</span><select value={factKind} onChange={(event) => { setFactKind(event.target.value as FactKind); markReviewDirty(); }}><option value="world">世界事实</option><option value="reader">读者已知</option><option value="character">人物认知</option><option value="author-secret">作者秘密</option></select></label>
          <label><span>主体 ID（可选）</span><input value={factSubjectId} onChange={(event) => { setFactSubjectId(event.target.value); markReviewDirty(); }} placeholder="例如：linxi" /></label>
          <label><span>事实键</span><input value={factPredicate} onChange={(event) => { setFactPredicate(event.target.value); markReviewDirty(); }} placeholder="例如：knows-father-lied" /></label>
          <label><span>事实值（JSON）</span><input value={factValue} onChange={(event) => { setFactValue(event.target.value); markReviewDirty(); }} placeholder={'true、false、"秘密" 或 3'} /></label>
          <label><span>认知状态</span><select value={factEpistemicState} onChange={(event) => { setFactEpistemicState(event.target.value as EpistemicState); markReviewDirty(); }}><option value="unknown">未知</option><option value="suspected">怀疑</option><option value="known">已知</option><option value="false">已证伪</option></select></label>
          <label><span>认知持有者 ID（可选）</span><input value={factEpistemicOwnerId} onChange={(event) => { setFactEpistemicOwnerId(event.target.value); markReviewDirty(); }} placeholder="人物认知由谁持有" /></label>
          <label><span>正文证据</span><input value={factEvidence} onChange={(event) => { setFactEvidence(event.target.value); markReviewDirty(); }} placeholder="引用支持该事实的正文" /></label>
        </fieldset></details>
        </details>
      {view === "chapters" && selectedChapter && <ImpactAnalysisPanel onDiscuss={onDiscuss} agentRunning={agentRunning} api={api ?? null} projectId={projectId} workspace={workspace} chapterId={selectedChapter.id} dirty={dirty} refreshToken={refreshToken} />}</> : null;
  const hasCatalogSelection = requestedCommitment?.request.projectId === projectId && requestedCommitment.chapterId === selectedChapter?.id;
  const hasCommitmentResult = taskList.tasks.some(task => task.steps.some(step => step.operation === "extractCommitments" && step.target.chapterId === selectedChapter?.id));
  const commitmentResults = <div className="conversation-chapter-results" hidden={!hasCatalogSelection && !hasCommitmentResult && commitmentResultsScope !== editorIdentity && !supplementalEditor.active}>
      {view === "chapters" && selectedChapter && <CommitmentPanel requestedCommitment={requestedCommitment?.chapterId === selectedChapter.id && requestedCommitment.request.projectId === projectId ? requestedCommitment : null} requestedResult={taskResultSelection?.result.kind === "commitment" || taskResultSelection?.result.kind === "commitmentExtraction" ? { requestId: taskResultSelection.requestId, result: taskResultSelection.result } : null} autoOpen={hasCommitmentResult || commitmentResultsScope === editorIdentity} api={api} projectId={projectId} chapterId={selectedChapter.id} sourceRevisionId={candidateId ?? workspaceChapter?.accepted?.id ?? null} dirty={dirty || reviewDirty} blocked={busy || generating || agentRunning} refreshToken={refreshToken} onFormalChange={onFormalChange} registerEditor={supplementalEditor.register}
        chapters={state.novelEdition.chapters.map((chapter) => ({ id: chapter.id, title: `第 ${chapter.number} 章 · ${chapter.title}` }))}
        sources={(workspace?.chapters ?? []).flatMap((chapter) => {
          const revision = chapter.candidate ?? chapter.accepted;
          return revision ? [{ chapterId: chapter.id, title: `第 ${chapter.number} 章 · ${chapter.title}`, objectId: `draft-${chapter.id}`, revisionId: revision.id, body: revision.body }] : [];
        })} />}
  </div>;
  const renderTaskPanel = () => (
    <WorkTaskPanel onOpenResult={onOpenTaskResult} key={projectId} conversation={conversation} api={api} projectId={projectId} tasks={taskList.tasks} activeCount={taskList.activeCount} nextCursor={taskList.nextCursor} loadingMore={taskList.loadingMore} loaded={taskList.loaded} reading={taskList.reading} readError={taskList.error} onRetry={() => void taskList.refresh()} revision={workspace?.revision} busy={busy} interruptionBusy={workspaceBusy || taskControls.controlling} agentRunning={agentRunning} agentAvailable={agentAvailable} dirty={dirty || reviewDirty || supplementalEditor.active} onControl={(id, command) => void updateWorkTask(id, command)} onUndo={(commitId) => void undoWorkChange(commitId)} onLoadMore={() => void taskList.loadMore()}
        onPrepareRevisionRepair={onPrepareRevisionRepair ? preparation => {
          if (busy || agentRunning || dirty || reviewDirty || supplementalEditor.active || !agentAvailable || preparation.projectId !== projectId) return;
          onPrepareRevisionRepair(preparation);
        } : undefined}
        onPrepareRevisionContinuation={onPrepareRevisionContinuation && workspace && !generationPermitReview ? (revisionPredecessor, intent) => {
          if (busy || agentRunning || dirty || reviewDirty || supplementalEditor.active || !agentAvailable
            || currentGenerationRun || generationRunsProjectId !== projectId || generationReadError) {
            setError("请先保存修改并核对创作任务状态，再准备后继创作。"); return;
          }
          onPrepareRevisionContinuation({ projectId, intent, expectedProjectRevision: workspace.revision,
            expectedBaseSnapshotId: workspace.headSnapshotId, expectedSourceDigest: workspace.sourceDigest, revisionPredecessor });
        } : undefined} />
  );
  const conversationPortal = taskConversationContainer
    ? createPortal(<>{(conversation || actionableTasks || (taskList.loaded && (taskList.tasks.length > 0 || taskList.nextCursor))) && renderTaskPanel()}{commitmentResults}</>, taskConversationContainer) : null;
  const planningHeader = view !== "chapters" ? <PlanningWorkspaceHeader view={view} title={info.title} description={info.description}
        artifact={artifact ?? undefined} dirty={dirty} blocked={busy || draftInteractionLocked || lockedDraftConflict}
        prepareDirtyDiscussion={Boolean(onPlanningSaveHandlerChange) && canSavePlanningForDiscussion}
        locked={artifactLocked} hasReview={Boolean(currentReview)} onDiscuss={view === "brief" ? onDiscuss : undefined}
        onSave={() => void saveCurrent()} onReview={() => {
          if (currentReview) { reviewRef.current?.scrollIntoView({ block: "center", behavior: "smooth" }); reviewRef.current?.focus({ preventScroll: true }); }
          else void enterReview();
        }} onOpenVersions={openRecords} /> : null;
  const publicationDialog = (view !== "chapters" && currentReview || view === "chapters" && publicationOpen && publicationTarget?.scope === editorIdentity) && <ChapterPublicationDialog inline={view !== "chapters"} busy={busy || agentRunning} onClose={closePublication}>
        {view === "chapters" && publicationTarget?.knowledge === "manual" && <p className="creative-alert warning">本版本尚未完成故事设定分析；不会使用之前版本的分析结果。</p>}
        {view === "chapters" && !currentReview && !pendingChapterAccept.current && <><label className="no-fact-decision"><input type="checkbox" disabled={busy} checked={noConfirmedChanges} onChange={event => setNoConfirmedChanges(event.target.checked)} /><span>我已检查，本次仅发布正文，不新增故事设定</span></label><button className="primary-button" type="button" disabled={busy || !noConfirmedChanges} onClick={() => void publication.prepare({ bodyOnlyConfirmed: true })}>继续发布前检查</button></>}
        {view === "chapters" && pendingChapterAccept.current?.scope === editorIdentity && <button type="button" className="primary-button" disabled={busy} onClick={() => void recoverPublication()}>重新读取发布结果</button>}
        {currentReview && <section className="review-panel" ref={reviewRef} tabIndex={-1}>
        {view !== "chapters" && <div><span className="creative-kicker">审阅</span><h3>采用前检查</h3></div>}
        {currentReview.checks.map((check) => {
          const selectionAware = viewKind === "draft" && !check.passed && (check.id === "fact-evidence" || check.id === "fact-evidence-anchor");
          const passed = selectionAware
            ? selectionAwareReviewChecksPass([check], confirmedFacts, workspaceChapter?.candidate?.sceneGroups ?? [])
            : check.passed;
          const pending = check.id === "semantic-review" && !passed;
          return <div className={`review-check ${pending ? "pending" : passed ? "passed" : "failed"}`} key={check.id}><span>{pending ? "…" : passed ? "✓" : "!"}</span>{view === "chapters" && check.id === "current-candidate" ? "内容版本与本次发布一致" : check.message}{selectionAware ? "；当前只检查已确认的故事设定" : ""}</div>;
        })}
        {viewKind === "draft" && <p>本次发布整章，包括标题、场景顺序和删除的内容。</p>}
        {view === "chapters" && reviewFacts.length > 0 && <section className="publication-bulk-decisions" aria-label="批量处理未决定建议">
          <p role="status" aria-live="polite" aria-label="建议决定进展">还有 {undecidedReviewCount} 项未决定，{selectedReviewFacts.length - undecidedReviewCount} 项已有决定。</p>
          {undecidedReviewCount > 0 && <div><button type="button" className="secondary-button" disabled={!bulkReviewAllowed} onClick={() => decideUndecidedReviewFacts("confirmed")}>确认未决定的 {undecidedReviewCount} 项</button><button type="button" className="secondary-button" disabled={!bulkReviewAllowed} onClick={() => decideUndecidedReviewFacts("rejected")}>拒绝未决定的 {undecidedReviewCount} 项</button></div>}
          <small>仅处理本次检查中未决定的建议，保留已有单项决定；发布后才更新故事设定。</small>
        </section>}
        {viewKind === "draft" && reviewFacts.length === 0 && <label className="no-fact-decision"><input type="checkbox" disabled={draftInteractionLocked} checked={noConfirmedChanges} onChange={(event) => { setNoConfirmedChanges(event.target.checked); markReviewDirty(); }} /><span><strong>{publicationTarget?.knowledge === "manual" ? "我已检查，本次仅发布正文，不新增故事设定" : "本章没有要确认的故事设定变化"}</strong></span></label>}
        {reviewFacts.map((fact) => {
          const sceneSelected = selectedSceneGroups.includes(fact.sceneGroupId);
          const factImpacts = provisionalImpacts.filter((impact) => impact.factProposalId === fact.id);
          const automaticTargetKeys = new Set(factImpacts.map((impact) => `${impact.targetArtifactKind}:${impact.targetNodeId}`));
          const supplementaryTargets = planningTargets.filter((target) => !automaticTargetKeys.has(`${target.kind}:${target.nodeId}`));
          const selectedManualTargets = planningTargets.filter((target) => (factDependencyTargets[fact.id] ?? []).includes(target.key));
          const confirmed = sceneSelected && factDecisions[fact.id] === "confirmed";
          return <article className={`fact-review-card ${sceneSelected ? "" : "disabled"}`} key={fact.id}>
            <header><div><small>{fact.source === "agent" ? "分析建议" : "作者记录"} · {factKindLabel(fact.factKind)}</small><strong>{fact.predicate}</strong><small>主体：{publicationObjectLabel(fact.subjectId, publicationEntities, state.canon)}{!fact.storyEvent && ` · 值：${formatFactValue(fact.value)}`}</small><StoryEventDetails event={fact.storyEvent} api={api} projectId={projectId} />{fact.stateProperty ? <StatePropertyDetails property={fact.stateProperty} /> : fact.storyEvent ? null : <small>认知状态：{epistemicStateLabel(fact.epistemicState)} · 持有者：{publicationObjectLabel(fact.epistemicOwnerId, publicationEntities, state.canon)}</small>}<p>{fact.evidence}</p><small>证据来自本章正文</small><details><summary>查看来源标识</summary><small>主体：{fact.subjectId || "无"} · 持有者：{fact.epistemicOwnerId || "无"}</small></details></div><select disabled={!sceneSelected || draftInteractionLocked} aria-label={`${fact.predicate} 的确认结论`} value={sceneSelected ? factDecisions[fact.id] ?? "" : "rejected"} onChange={(event) => { setFactDecisions((current) => ({ ...current, [fact.id]: event.target.value as "confirmed" | "rejected" })); markReviewDirty(); }}><option value="">请选择</option><option value="confirmed">确认变化</option><option value="rejected">不采纳</option></select></header>
            {factImpacts.length > 0 && <div className="provisional-impact-list"><strong>采用前影响预览</strong>{factImpacts.map((impact) => { const target = planningTargets.find((item) => item.kind === impact.targetArtifactKind && item.nodeId === impact.targetNodeId); const pathLabel = target?.label ?? impact.targetLabel; return <div className={`provisional-impact ${impact.severity}`} key={impact.id}><div><span>{impact.targetArtifactKind === "outline" ? "大纲" : "章纲"} · {pathLabel}</span>{target?.protected && <small>原安排：{target.currentArrangement}</small>}<small>正文变化：{impact.predicate}；证据：{impact.evidence || "未提供"}</small><small>影响范围：{pathLabel} · {impact.severity === "locked-conflict" ? "受保护，必须先决定" : "将创建待处理规划反馈"} · 自动匹配依赖</small></div>{confirmed && impact.severity === "locked-conflict" && <DirectionDecisionEditor disabled={draftInteractionLocked} value={directionChangeDecisions[impact.id]} onChange={(value) => { setDirectionChangeDecisions((current) => ({ ...current, [impact.id]: value })); markReviewDirty(); }} />}</div>; })}</div>}
            {supplementaryTargets.length > 0 && <fieldset className="manual-fact-link" disabled={!confirmed || draftInteractionLocked}><legend>补充关联遗漏的正式规划（可多选）</legend>{supplementaryTargets.map((target) => <label key={target.key}><input type="checkbox" checked={(factDependencyTargets[fact.id] ?? []).includes(target.key)} onChange={(event) => { setFactDependencyTargets((current) => ({ ...current, [fact.id]: event.target.checked ? [...new Set([...(current[fact.id] ?? []), target.key])] : (current[fact.id] ?? []).filter((key) => key !== target.key) })); markReviewDirty(); }} /><span>{target.kind === "outline" ? "大纲" : "章纲"} · {target.label} · {target.protectionLabel}</span></label>)}</fieldset>}
            {confirmed && selectedManualTargets.filter((target) => target.protected).map((target) => <div className="provisional-impact locked-conflict" key={target.key}><div><span>{target.label}</span><small>原安排：{target.currentArrangement}</small><small>正文变化：{fact.predicate}；证据：{fact.evidence || "未提供"}</small><small>影响范围：{target.label} · {target.protectionLabel}，必须先决定</small></div><DirectionDecisionEditor disabled={draftInteractionLocked} value={manualDirectionDecisions[manualDecisionKey(fact.id, target.key)]} onChange={(value) => { setManualDirectionDecisions((current) => ({ ...current, [manualDecisionKey(fact.id, target.key)]: value })); markReviewDirty(); }} /></div>)}
          </article>;
        })}
        <p role="status" aria-live="polite" aria-label={view === "chapters" ? "发布检查进展" : "审阅进展"}>{reviewProgress}</p>
        <button className="primary-button" disabled={busy || draftInteractionLocked || !canAccept || pendingChapterAccept.current?.scope === editorIdentity} onClick={() => void acceptCurrent()}>{view === "chapters" ? returnsDraft ? "返回编辑" : "确认发布章节" : "确认采用这份规划"}</button>
      </section>}</ChapterPublicationDialog>;
  if (!api) return <>{conversationPortal}<div className="creative-empty"><h2>{info.title}</h2><p>桌面 Runtime 连接后可使用结构化创作工作区。</p></div></>;
  if (loading || workspace && workspace.project.id !== projectId) return <>{conversationPortal}<div className="creative-workspace-view" aria-busy="true"><div className="creative-loading"><span className="loading-pulse" /><strong>正在加载{info.title}</strong><p>{view === "chapters" ? "正在读取章节内容与发布状态…" : "正在恢复候选、正式版本与审阅状态…"}</p></div></div></>;
  if (error && !workspace) return <>{conversationPortal}<div className="creative-empty" role="alert"><h2>{info.title}暂时无法打开</h2><p>{error}</p><button className="secondary-button" onClick={() => { setLoading(true); setError(null); void reload(); }}>重新加载</button></div></>;


  return (
    <>
      {conversationPortal}
      {view !== "chapters" && openRecords && moduleActionsContainer && createPortal(
        <ModuleRecordsButton label={view === "outline" ? focusedOutlineNode?.title ?? outlineVolume?.title ?? "全书大纲" : view === "brief" ? "创作方向" : "近章规划"}
          open={recordsOpen} onClick={openRecords} />, moduleActionsContainer)}
    <div className={`creative-workspace-view${view === "chapters" ? " chapter-workspace-view" : view === "outline" ? " outline-workspace-view" : ""}`} aria-busy={busy}
      data-discussion-document="" data-discussion-project={projectId} data-discussion-view={view}
      data-discussion-label={view === "chapters" ? `第 ${selectedChapter?.number ?? ""} 章 · ${draftTitle}` : focusedOutlineNode?.title ?? info.title}
      data-discussion-dirty={dirty || reviewDirty || supplementalEditor.dirty}
      data-discussion-snapshot={workspace?.headSnapshotId}
      data-discussion-revision={workspace && viewKind !== "draft" ? (workspace.artifacts[viewKind].candidate ?? workspace.artifacts[viewKind].accepted)?.id ?? "" : ""}
      data-discussion-object={viewKind !== "draft" ? `artifact-${viewKind}` : undefined}>
      {view === "brief" && <WorkOverview workspace={workspace?.project.id === projectId ? workspace : null} blocked={busy || agentRunning} onContinue={chapterId => {
        const chapter = state.novelEdition.chapters.find(item => item.id === chapterId);
        if (chapter) onSelectChapter(chapter); else onReturnToFeedback();
      }} onDiscuss={onDiscuss ? () => onDiscuss("请和我讨论这部小说的创作方向。依据我已经明确的要求提出建议，不要自动采用或发布。") : undefined} />}
      {outlineFocusRequested && <header className="outline-focus-heading" data-outline-volume-id={outlineVolume?.id}><div><small>卷纲 · 与全书大纲共用同一份内容</small><h2 ref={outlineHeadingRef} tabIndex={-1}>{focusedOutlineNode?.title ?? outlineVolume?.title ?? "卷纲暂不可用"}</h2>{!outlineVolume && (dirty || reviewDirty) && <p>请先保存修改再返回全书大纲。</p>}</div><button type="button" className="secondary-button" disabled={!outlineVolume && (dirty || reviewDirty)} onClick={closeOutlineFocus}>返回全书大纲</button></header>}
      {outlineFocusRequested && outlineCatalogError && <p role="alert">目录关联读取失败：{outlineCatalogError}。当前保留上次读取的关联。</p>}
      {outlineFocusUnavailable && (outlineVolume ? <OutlineLinkPanel key={`${projectId}:${outlineVolume.id}`} volume={outlineVolume} nodes={savedOutlineNodes} revisionId={outlineRevision?.id} disabled={busy || agentRunning || dirty || reviewDirty || Boolean(outlineCatalogError)} onLink={onLinkOutline} onDiscuss={onDiscuss} /> : <p role="status">这段卷纲已变化，请返回全书大纲重新选择。</p>)}
      {!outlineFocusUnavailable && (view === "outline" || view === "plans") && <PlanningReadingView view={view} artifact={artifact ?? null} onOpenVersions={openRecords} outlineNodeId={focusedOutlineNode ? requestedOutlineNodeId : null} onOpenOutlineNode={id => {
        if (dirty || reviewDirty) return;
        const destination = onOpenOutlineNode?.(id) ?? "node";
        if (destination === "node") setReadingOutlineNodeId(id);
      }}
        blocked={busy || agentRunning || draftInteractionLocked || lockedDraftConflict} dirty={dirty || reviewDirty}
        prepareDirtyDiscussion={Boolean(onPlanningSaveHandlerChange) && canSavePlanningForDiscussion && !reviewDirty} onDiscuss={onDiscuss} />}

      {error && !sameConversationFailure(error, conversation?.run.error ?? null) && <div className="creative-alert error" role="alert">{describeAgentError(error)}{describeAgentError(error) !== error && <details><summary>查看原始错误</summary><pre>{error}</pre></details>}</div>}
      {notice && <div className="creative-alert success" role="status">{notice}</div>}
      {chapterSyncError?.identity === editorIdentity && <div className="creative-alert warning" role="status">章节执行已结束，工作区读取失败：{chapterSyncError.message}。<button className="text-button" type="button" disabled={draftInteractionLocked || dirty || reviewDirty} onClick={() => {
        void perform("正在重新读取章节内容…", async () => {}, "章节内容已重新读取，可以继续编辑或发布。").then(refreshed => {
          if (refreshed && activeIdentityRef.current === editorOwner) setChapterSyncError(null);
        });
      }}>重新读取章节内容</button><small>仅重新读取，不会再次生成或写入。</small></div>}
      {busyAction && <div className="creative-alert" role="status">{busyAction}</div>}

      {conflict && <div className="creative-alert warning editor-conflict" role="alert"><div><strong>服务器候选已变化，本地编辑仍被保留</strong><span>当前编辑基于 {conflict.baseCandidateRevisionId ?? "空候选"}，服务器现为 {conflict.serverCandidateRevisionId ?? "空候选"}。上方编辑器是本地版本，下方候选栏是服务器版本。</span><details><summary>查看本地编辑的基线版本</summary><pre>{conflict.baseContent}</pre></details></div><div className="editor-conflict-actions"><button className="primary-button" disabled={busy} onClick={() => void saveLocalAsLatestCandidate()}>以最新版为基线保存本地稿</button><button className="secondary-button" disabled={busy} onClick={() => void copyLocalEditor()}>复制本地稿</button><button className="secondary-button" disabled={busy} onClick={() => { if (workspace) loadEditor(workspace); }}>放弃本地稿并载入服务器版本</button></div></div>}
      {artifactLocked && <div className="creative-alert warning">规划已锁定，当前内容只读。{onOpenVersions && <button type="button" className="text-button" onClick={openRecords}>前往修改记录解锁</button>}</div>}
      {!agentAvailable && <div className="creative-alert">{view === "chapters" ? "当前未配置可用模型；仍可手动编辑并保存这一章。" : "当前未配置可用模型；仍可手动编辑并保存候选。"}</div>}
      {workspace?.index.status === "stale" && <div className="creative-alert warning">长篇索引正在追赶正式版本，检索结果可能暂时滞后。</div>}
      {view === "chapters" && analyzingChapter && chapterModeling?.sourceRevisionId === candidateId && chapterModeling?.status === "model_pending" && <p className="chapter-analysis-status" role="status">正在分析本章的故事设定…</p>}
      {view === "chapters" && chapterModeling && chapterModeling.sourceRevisionId !== candidateId && <div className="creative-alert warning" role="status">故事设定分析来源与当前正文不同，请重新读取后再发布。</div>}
      {view === "chapters" && pendingChapterSave.current?.scope === editorIdentity && <div className="creative-alert warning" role="status">章节已保存，正在等待读取结果。<button type="button" className="secondary-button" disabled={busy || agentRunning || generating} onClick={() => void saveCurrent()}>重新读取已保存章节</button></div>}
      {workspace?.planLineageStatus === "stale" && <div className="creative-alert warning">大纲已更新，当前章纲需要重新核对。旧章纲不会继续指导正文。{onDiscuss && <button type="button" className="secondary-button" disabled={busy || agentRunning || dirty || reviewDirty} onClick={() => onDiscuss("请基于当前大纲和已发生正文，更新近章规划并保存候选。保留一致的安排；涉及受保护内容或方向取舍时先说明，不要自动采用或发布。")}>在对话中更新规划</button>}</div>}
      {feedbackTarget && view !== "chapters" && <div className="feedback-return-bar"><span>正在处理反馈：{feedbackTarget.targetLabel}</span><button className="secondary-button" onClick={() => { onReturnToFeedback(); }}>返回原反馈</button></div>}

      {view === "brief" && <section className="overview-direction" aria-label="创作方向">
        <CreationDirectionSummary artifact={workspace?.artifacts.brief} />
        <details ref={planningEditorRef} key={projectId} className="overview-direction-editor"><summary>编辑创作方向{dirty || reviewDirty ? " · 有未保存修改" : ""}</summary>
          {planningHeader}
          <ArtifactEditor disabled={artifactLocked || busy || agentRunning} fields={[{ key: "premise", label: "核心命题", hint: "故事真正要讨论什么" }, { key: "promise", label: "读者承诺", hint: "读者持续阅读会得到什么体验" }, { key: "stakes", label: "核心代价", hint: "失败会失去什么" }, { key: "endingDirection", label: "结局方向", hint: "尚未决定可以留空，后续随创作调整" }]} value={brief} onChange={(key, value) => { setBrief((current) => ({ ...current, [key]: value })); setDirty(true); }} />
          {publicationDialog}
        </details>
      </section>}
      {(view === "outline" || view === "plans") && <details ref={planningEditorRef} key={`${projectId}:${view}`} className="planning-manual-editor" hidden={outlineFocusUnavailable} style={outlineFocusUnavailable ? { display: "none" } : undefined}>
        <summary>{view === "outline" ? <><span className="outline-editor-closed-label">{focusedOutlineNode ? "编辑本卷" : "编辑全书主线与脉络"}</span><span className="outline-editor-open-label">返回阅读</span></> : "手动编辑近章规划"}{dirty || reviewDirty ? " · 有未保存修改" : ""}</summary>
        {view === "outline" && <p className="outline-editing-scope">{focusedOutlineNode ? "聚焦本卷的阶段事件与变化，下级安排在结构中单独编辑。" : "聚焦整部故事的主线与因果脉络，各卷事件在卷纲中展开。"}保存保留其他安排；采用检查作用于整份大纲。</p>}
        {view !== "outline" && planningHeader}
      {view === "outline" && <OutlineTreeEditor disabled={artifactLocked || busy || agentRunning} error={outlineError} rootNodeId={focusedOutlineNode ? requestedOutlineNodeId : null} focusNodeId={outlineFocusUnavailable ? null : feedbackTarget?.targetKind === "outline" ? feedbackTarget.targetNodeId : null} nodes={outlineNodes} onChange={(nodes) => { setOutlineNodes(nodes); setOutlineError(null); setDirty(true); }} />}
      {view === "plans" && <PlanningCharacterProvider api={api} projectId={projectId} refreshToken={refreshToken}><fieldset className="plan-editor-list" disabled={artifactLocked || busy || agentRunning}>
        {acceptedPlanChapters.length > 0 && <details className="plan-materialization"><summary>已采用的章纲 · {acceptedPlanChapters.length}</summary><small>{workspace?.planLineageStatus === "stale" ? "章纲基线已过期，采用新版章纲后才能继续开写。" : "创建后才会出现可写的章节。还没开写的章可以继续改。"}</small>{acceptedPlanChapters.map((entry) => <div key={entry.chapterPlanId}><span>第 {entry.chapterNumber} 章 · {entry.publicTitle || entry.title || "未命名"}</span>{entry.chapterPlanId && <button type="button" className="secondary-button" disabled={dirty || workspace?.planLineageStatus !== "current" || boundPlanIds.has(entry.chapterPlanId)} onClick={() => void materializePlanChapter(entry.chapterPlanId!, entry.chapterNumber)}>{boundPlanIds.has(entry.chapterPlanId) ? "已创建" : "创建正文"}</button>}</div>)}</details>}
        {plans.map((item, index) => <div className="plan-editor-card" data-planning-node-id={item.chapterPlanId ?? item.id} key={item.id}>
          <div className="plan-card-head">
            <span className="plan-number">第 <input aria-label={`第 ${item.chapterNumber} 章序号`} type="number" min="1" value={item.chapterNumber} onChange={(event) => updatePlan(index, { chapterNumber: Number(event.target.value) })} /> 章</span>
            <input className="plan-title" aria-label={`第 ${item.chapterNumber} 章标题`} placeholder="章节标题" value={item.title} onChange={(event) => updatePlan(index, { title: event.target.value })} />
            <label className="plan-lock"><input type="checkbox" checked={item.locked === true} onChange={(event) => updatePlan(index, { locked: event.target.checked })} /><span>锁定</span></label>
          </div>
          <label className="plan-field"><span>本章目标</span><textarea aria-label={`第 ${item.chapterNumber} 章目标`} placeholder="这一章要完成什么" value={item.goal} onChange={(event) => updatePlan(index, { goal: event.target.value })} /></label>
          <label className="plan-field"><span>核心冲突</span><input aria-label={`第 ${item.chapterNumber} 章核心冲突`} placeholder="谁和什么对着干" value={item.conflict} onChange={(event) => updatePlan(index, { conflict: event.target.value })} /></label>
          <label className="plan-field"><span>给正文的指引</span><textarea aria-label={`第 ${item.chapterNumber} 章写作指引`} placeholder="写这一章时需要知道的安排" value={item.writerGuidance ?? ""} onChange={(event) => updatePlan(index, { writerGuidance: event.target.value })} /></label>
          <details className="plan-advanced"><summary>正文标题、承诺与更多设置</summary><div className="plan-advanced-fields">
          <label className="plan-field"><span>正文标题</span><input aria-label={`第 ${item.chapterNumber} 章公开标题`} placeholder="写正文时使用的标题，可留空" value={item.publicTitle ?? ""} onChange={(event) => updatePlan(index, { publicTitle: event.target.value })} /></label>
            <label><span>必须兑现</span><input aria-label={`第 ${item.chapterNumber} 章公开承诺`} placeholder="以分号分隔" value={(item.publicCommitments ?? []).join("；")} onChange={(event) => updatePlan(index, { publicCommitments: event.target.value.split(/[；;]/).map((value) => value.trim()).filter(Boolean) })} /></label>
            <label><span>可以自由发挥</span><input aria-label={`第 ${item.chapterNumber} 章创作余地`} placeholder="以分号分隔" value={(item.allowances ?? []).join("；")} onChange={(event) => updatePlan(index, { allowances: event.target.value.split(/[；;]/).map((value) => value.trim()).filter(Boolean) })} /></label>
            {outlineForPlan && <label><span>服务的上级规划</span><select value={item.primaryNodeId} onChange={(event) => updatePlan(index, { primaryNodeId: event.target.value })}><option value="">请选择全书／卷／单元</option>{(outlineForPlan.payload.nodes as OutlineNode[]).map((node) => <option key={node.id} value={node.id}>{node.id ? outlinePaths.get(node.id) : undefined}</option>)}</select></label>}
            <label><span>故事时间</span><input aria-label={`第 ${item.chapterNumber} 章故事时间`} type="number" min="0" placeholder="未指定" value={item.storyTime ?? ""} onChange={(event) => updatePlan(index, { storyTime: event.target.value === "" ? null : Number(event.target.value) })} /></label>
            <label><span>依赖事实键</span><input aria-label={`第 ${item.chapterNumber} 章依赖事实`} placeholder="以逗号分隔" value={item.dependencyKeys} onChange={(event) => updatePlan(index, { dependencyKeys: event.target.value })} /></label>
            <PlanningCharacterSelect label={`第 ${item.chapterNumber} 章视角人物`} value={item.povCharacterId ?? null}
              disabled={artifactLocked || busy || agentRunning} onChange={value => updatePlan(index, { povCharacterId: value })} />
          </div></details>
          <details className="plan-writer-note"><summary>写正文时会看到</summary><div aria-label={`第 ${item.chapterNumber} 章正文可见内容`}><span>{item.publicTitle || "（无公开标题）"}</span><p>{item.writerGuidance || "（无公开写作指引）"}</p><small>{(item.publicCommitments ?? []).length > 0 ? `公开承诺：${item.publicCommitments?.join("；")}` : "无公开承诺"}</small></div></details>
          {(item.scenes?.length ?? 0) > 0 && <details className="plan-scenes"><summary>场景与节拍 · {item.scenes?.length}</summary>{item.scenes?.map((scene) => <article key={scene.id}><div><b>{scene.title || scene.id}</b>{scene.locked && <span>已锁定</span>}</div><p>{scene.goal}</p>{scene.beats.map((beat) => <small key={beat.id}>· {beat.description}{beat.locked ? " · 已锁定" : ""}</small>)}</article>)}</details>}
        </div>)}
        <button className="text-button" type="button" onClick={() => { setPlans((rows) => [...rows, newPlanRow((rows.at(-1)?.chapterNumber ?? 0) + 1)]); setDirty(true); }}>＋ 添加下一章</button>
      </fieldset></PlanningCharacterProvider>}
        {view === "outline" && <>{planningHeader}<button className="text-button outline-return-reading" type="button" onClick={() => { const editor = planningEditorRef.current; if (!editor) return; editor.open = false; const entry = editor.querySelector("summary"); entry?.scrollIntoView({ block: "center" }); entry?.focus({ preventScroll: true }); }}>返回阅读{dirty ? "（修改尚未保存）" : ""}</button></>}
        {publicationDialog}
      </details>}
      {view === "brief" && (generationReadError || currentGenerationRun) && <section className="planning-intent-card">
        {view === "brief" && generationReadError && !currentGenerationRun && <div className="creative-alert error" role="alert">创作运行状态读取失败，暂不能确认是否有未完成任务。<button className="text-button" type="button" onClick={() => setGenerationReload(value => value + 1)}>重新读取</button></div>}
        {view === "brief" && currentGenerationRun && <div className="creative-alert" role="status">
          自动创作：{currentGenerationRun.completedChapters - currentGenerationRun.startChapterNumber + 1}/{currentGenerationRun.targetChapters - currentGenerationRun.startChapterNumber + 1} 章 · {currentGenerationRun.status === "blocked" ? currentGenerationRun.blockedReason === "scene_feedback_needs_author" ? "场景评估待作者处理" : currentGenerationRun.blockedReason === "chapter_plan_rebound" ? "章纲已更新，需重新规划" : `已阻断（${currentGenerationRun.blockedReason ?? "需要处理"}）` : currentGenerationRun.status === "paused" ? "已暂停" : `进行到 ${generationStageLabels[currentGenerationRun.stage]}`}
          {currentGenerationRun.usageUnsettled && <small>模型用量待核账，暂不能继续；请点击重新核账或稍后重试。已保存的候选不会丢失。</small>}
          {generationReadError && <small>创作运行状态暂未刷新，确认最新状态前不能继续。</small>}
          {(currentGenerationRun.usageUnsettled || generationReadError) && <button className="text-button" type="button" onClick={() => setGenerationReload(value => value + 1)}>重新核账</button>}
          {currentGenerationRun.status === "running" && <button className="secondary-button" type="button" disabled={busy || agentRunning || dirty || !agentAvailable || currentGenerationRun.usageUnsettled || generationReadError} onClick={() => void onContinueWork(currentGenerationRun.id)}>继续创作</button>}
          {currentGenerationRun.status === "blocked" && ["agent_task_failed", "agent_task_cancelled", "agent_task_failed_after_output", "agent_task_cancelled_after_output"].includes(currentGenerationRun.blockedReason ?? "") && <button className="secondary-button" type="button" disabled={busy || agentRunning || dirty || !agentAvailable || currentGenerationRun.usageUnsettled || generationReadError} onClick={() => void onRetryWork(currentGenerationRun.id)}>{currentGenerationRun.feedbackRepairPrompt !== null ? "按评估要求重试本章" : currentGenerationRun.pendingCurrentIntentGeneration !== null ? "按新要求重写本章" : currentGenerationRun.blockedReason?.endsWith("_after_output") ? "从已保存候选继续" : "重试这一步"}</button>}
        </div>}
      {view === "brief" && api && currentGenerationRun && ["paused", "blocked"].includes(currentGenerationRun.status) && <GenerationReplanPanel
          api={api} projectId={projectId} runId={currentGenerationRun.id} disabled={busy || agentRunning || dirty || currentGenerationRun.usageUnsettled === true || generationReadError}
          onConfirmed={run => {
            setGenerationRuns(previous => previous.map(item => item.id === run.id ? run : item));
            setError(null); setNotice("重规划已保存，可继续创作。");
            void Promise.resolve().then(onFormalChange).catch(reason => {
              if (loadedProjectRef.current === projectId && activeIdentityRef.current === editorOwner) {
                setError(`重规划已保存，但刷新失败：${reason instanceof Error ? reason.message : "请重新读取作品"}。请重新打开作品概况读取持久状态，无需再次提交。`);
              }
            });
          }} />}
        {view === "brief" && currentGenerationRun?.blockedReason === "scene_feedback_needs_author" && <section className="scene-feedback-decision" aria-label="场景评估待作者处理">
          <strong>场景评估发现偏差，请决定本章如何推进</strong>
          {feedbackAssessment?.state === "current" ? <>
            <SceneFeedbackDetails assessment={feedbackAssessment} />
            <textarea aria-label="本章重写补充要求" value={feedbackInstruction} maxLength={1_000} placeholder="重写本章时希望调整什么（可选）" onChange={(event) => setFeedbackInstruction(event.target.value)} />
            <div className="scene-feedback-actions">
              <button className="secondary-button" type="button" disabled={busy || agentRunning || dirty || !agentAvailable || feedbackRepairUnavailable} onClick={() => void onResolveFeedback(currentGenerationRun.id, feedbackAssessment.revisionId, "repairDraft", feedbackInstruction)}>重写本章</button>
              <button className="secondary-button" type="button" disabled={busy || agentRunning || dirty} onClick={() => void onResolveFeedback(currentGenerationRun.id, feedbackAssessment.revisionId, "continue", "")}>接受现稿并继续</button>
            </div>
          </> : <p>正在读取当前评估；来源变化时请先检查任务状态。</p>}
          <small>评估是建议，不会自动修改正文或正式设定。重写后的检查由实际保存结果决定。{feedbackRepairBudgetUnavailable ? "任务尝试或模型额度已用尽。" : ""}</small>
        </section>}
        {view === "brief" && currentGenerationRun?.status === "running" && <div className="future-intent-control">
          <label htmlFor="future-intent">调整创作方向</label>
          <textarea id="future-intent" value={futureIntent} maxLength={1_000} placeholder="描述希望调整的情节、人物或节奏……" disabled={futureIntentSaving || dirty || (futureIntentBlockedByActiveStep && currentRevisionUnavailable)} onChange={(event) => setFutureIntent(event.target.value)} />
          <button className="secondary-button" type="button" disabled={futureIntentSaving || futureIntentBlockedByActiveStep || dirty || !futureIntent.trim() || Array.from(futureIntent.trim()).length > 1_000} onClick={() => void applyGenerationIntent("future")}>{futureIntentSaving ? "保存中…" : "应用于后续章节"}</button>
          <button className="secondary-button" type="button" disabled={futureIntentSaving || currentRevisionUnavailable || dirty || !futureIntent.trim() || Array.from(futureIntent.trim()).length > 1_000} onClick={() => void applyGenerationIntent("current")}>{currentGenerationRun.pendingCurrentIntentGeneration !== null ? "本章调整已排队" : "当前章：本步结束后重写"}</button>
        </div>}
      </section>}
      {(view === "outline" || view === "plans") && <PlanningReviewPanel
        key={view}
        runs={workspace?.planningReviews ?? []}
        artifactKind={view === "outline" ? "outline" : "plan"}
        revisionId={planningReviewRevision?.id ?? null}
        outlineNodes={planningReviewOutlineNodes}
        busy={busy || agentRunning || dirty}
        agentAvailable={agentAvailable}
        onStart={onReviewPlanning}
        onCancel={onCancelPlanningReview}
      />}
      {view === "chapters" && createdChapterPending?.projectId === projectId && <div className="creative-alert warning" role="alert">章节已创建，工作区读取失败：{createdChapterPending.error}。重新读取不会重复创建章节。<button type="button" className="secondary-button" disabled={busy || generating || agentRunning || dirty || reviewDirty || supplementalEditor.active} onClick={() => void materializeStandalone()}>重新读取已创建章节</button></div>}
      {view === "chapters" && (selectedChapter ? <div className="draft-workspace">
        <div className="chapter-page-toolbar"><ChapterSwitcher chapters={visibleChapters} selectedId={selectedChapter.id} disabled={draftInteractionLocked} onSelect={chapter => { const original = state.novelEdition.chapters.find(item => item.id === chapter.id); if (original) onSelectChapter(original); }} />
        <div className="chapter-page-actions">{onOpenVersions && <ModuleRecordsButton label={`第 ${selectedChapter.number} 章 · ${selectedChapter.title}`} open={recordsOpen} onClick={() => onOpenVersions()} />}{onExport && <button type="button" className="text-button" onClick={onExport}>导出</button>}</div></div>
        {newChapterOpen && <StandaloneChapterForm key={`${projectId}:${chapterCreationRequest}`} requestId={chapterCreationRequest} volumeTitle={newChapterVolume.projectId === projectId ? newChapterVolume.volumeTitle ?? undefined : undefined} nextNumber={nextChapterNumber} disabled={standaloneCreationLocked} busy={busy} onCreate={materializeStandalone} onCancel={() => setNewChapterOpen(false)} />}
        <div className="draft-main-column">
        <div data-discussion-revision={discussionRevision?.id ?? ""} data-discussion-object={`draft-${selectedChapter.id}`}>
        {editorSessionRef.current?.identity === editorIdentity ? <ChapterDocumentView key={`${projectId}:${selectedChapter.id}`} scope={`${projectId}:${selectedChapter.id}`} scenes={draftSceneRows} accepted={workspaceChapter?.accepted ?? null} title={draftTitle} sourcePlanRefs={(workspaceChapter?.candidate ?? workspaceChapter?.accepted)?.sourcePlanRefs ?? null} onTitleChange={title => { setDraftTitle(title); updateChapterDirty(title, draftSceneRows, lockedSceneGroups); }} lockedIds={lockedSceneGroups} disabled={draftInteractionLocked} dirty={dirty} onActiveScene={setActiveSceneId} onChange={(scenes) => { setDraftSceneRows(scenes); updateChapterDirty(draftTitle, scenes, lockedSceneGroups); }} tools={chapterTools} toolsDirty={reviewDirty || supplementalEditor.dirty} emptyWriting={chapterBodyEmpty ? { disabled: !onPrepareChapterWriting, onPrepare: prepareChapterWriting } : undefined} sceneDiscussion={onDiscuss ? { disabled: sceneDiscussionDisabled, notice: dirty || reviewDirty ? "保存后讨论此场景" : !discussionRevision ? "请先保存本章正文，再讨论场景。" : null, onPrepare: prepareSceneDiscussion } : undefined} onLocksChange={locked => { setLockedSceneGroups(locked); updateChapterDirty(draftTitle, draftSceneRows, locked); }} /> : <p role="status">正在恢复当前章节内容…</p>}
        </div>
        {draftInteractionLocked && <small role="status">{chapterBusyLabel}当前操作结束后可继续编辑。</small>}
          {firstWriteTitleNotice?.identity === editorIdentity && <div role="alert"><p>章节标题尚未保存，当前正文为空。首次生成将使用已创建章节的标题“{firstWriteTitleNotice.savedTitle}”；本地标题仍保留。可以恢复章节标题后开始，或先手写正文并一起保存。</p><button className="secondary-button" type="button" disabled={draftInteractionLocked || reviewDirty || draftSceneRows.length !== 1 || lockedSceneGroups.length > 0 || draftSceneRows.some(scene => scene.body.trim())} onClick={() => { setDraftTitle(firstWriteTitleNotice.savedTitle); dirtyRef.current = false; setDirty(false); setFirstWriteTitleNotice(null); }}>恢复章节标题</button></div>}
        {lockedDraftConflict && <div className="creative-alert warning" role="alert">锁定场景的正文已变化。请先恢复正文，只提交一次“取消保护”，保存后再修改。</div>}

        </div>
      </div> : <StandaloneChapterForm key={`${projectId}:${chapterCreationRequest}`} requestId={chapterCreationRequest} volumeTitle={newChapterVolume.projectId === projectId ? newChapterVolume.volumeTitle ?? undefined : undefined} nextNumber={nextChapterNumber} disabled={standaloneCreationLocked} busy={busy} onCreate={materializeStandalone} />)}

      {view === "chapters" ? selectedChapter && <div className="candidate-toolbar chapter-save-toolbar"><div className="chapter-save-status"><span className="chapter-word-count">{Array.from(draftSceneRows.map(scene => scene.body).join("").replace(/\s/g, "")).length} 字符</span><span className="chapter-save-state">{dirty ? "正文未保存" : candidateId || workspaceChapter?.accepted ? "正文已保存" : "尚未写作"}</span>{(reviewDirty || supplementalEditor.dirty) && <span className="chapter-review-dirty" role="status">设定或检查有未保存修改</span>}<ChapterPublicationStatus published={workspaceChapter?.accepted} saved={workspaceChapter?.candidate} dirty={dirty} /></div><button className="secondary-button" disabled={draftInteractionLocked || lockedDraftConflict || !draft.trim()} onClick={() => void saveCurrent()}>保存</button><button data-publish-chapter type="button" className="primary-button" disabled={draftInteractionLocked || lockedDraftConflict || !draft.trim() || (!dirty && (!candidateId || candidateId === workspaceChapter?.accepted?.id))} onClick={() => void publication.prepare()}>发布章节</button>{pendingChapterKnowledge && candidateId === workspaceChapter?.accepted?.id && <details><summary>故事设定变化</summary><button className="secondary-button" disabled={draftInteractionLocked || dirty} onClick={() => void publication.prepare()}>更新故事设定</button></details>}</div> : null}



      {view === "chapters" && publicationDialog}

      {workspace && workspace.impacts.length > 0 && <section className="impact-panel">
        <div className="impact-heading"><div><h3>规划反馈</h3><small>可以先在对话中讨论影响和取舍，再决定是否调整规划。正文采用不会自动改写大纲。</small></div><strong>{impactCounts.open} 条待处理 · {impactCounts.lockedConflict} 条保护冲突 · {impactCounts.retained} 条知情保留</strong></div>
        {workspace.impacts.map((impact) => {
          const targetKind = impact.targetArtifactKind;
          const retained = impact.state === "resolved-kept";
          const targetLabel = planningTargets.find((target) => target.kind === targetKind && target.nodeId === impact.targetNodeId)?.label ?? impact.targetLabel;
          const sourceChapter = state.novelEdition.chapters.find((chapter) => chapter.id === impact.sourceChapterId);
          return <article className={`impact-card ${retained ? "retained" : ""}`} ref={(element) => { if (element) impactRefs.current.set(impact.id, element); else impactRefs.current.delete(impact.id); }} tabIndex={-1} key={impact.id}>
            <header><span className={`impact-status ${impact.status}${retained ? " retained" : ""}`}>{impact.status === "locked-conflict" ? "保护冲突" : "待复核"}{retained ? " · 已知情保留" : ""}</span><strong>{targetLabel}</strong></header>
            <p>{impact.reason}</p>
            <small>{impact.sourceChapterNumber ? `来源：第 ${impact.sourceChapterNumber} 章${impact.sourceChapterTitle ? ` · ${impact.sourceChapterTitle}` : ""} · ` : ""}正文证据：{impact.evidence || "未提供"}</small>
            {retained ? <small>保留原因：{impact.resolutionNote || "作者确认维持原规划"}。此记录永久保留，不再要求处理。</small> : <div>
              {onDiscuss && <button type="button" className="secondary-button" disabled={busy || agentRunning || dirty || reviewDirty} onClick={() => onDiscuss(`请读取规划反馈 ${impact.id} 及其目标 ${targetKind}:${impact.targetNodeId}，结合来源正文讨论影响和可选调整。先说明依据与需要我决定的取舍，不修改、采用或关闭反馈。`)}>讨论这条反馈</button>}
              <button className="secondary-button" onClick={() => { setFeedbackTarget({ impactId: impact.id, targetKind, targetNodeId: impact.targetNodeId, targetLabel }); onOpenPlanningTarget(targetKind); }}>打开并修改</button>
              <button className="text-button" disabled={busy || dirty} onClick={() => void resolveImpact(impact.id, "kept")}>规划不调整</button>
              <button className="text-button" disabled={busy || dirty} onClick={() => void resolveImpact(impact.id, "false-positive")}>标记误报</button>
            </div>}
            <button className="text-button" disabled={!sourceChapter} onClick={() => { if (sourceChapter) onSelectChapter(sourceChapter); }}>返回来源正文</button>
          </article>;
        })}
        {impactCursor && <button className="secondary-button" disabled={busy} onClick={() => void loadMoreImpacts()}>加载更多反馈</button>}
      </section>}
      {workspace && workspace.commits[0]?.operation_kind !== "undo" && workspace.commits[0]?.result_revision === workspace.revision && <div className="undo-bar"><span>最近一次正式操作已记录为可追溯提交。</span><button className="text-button" disabled={busy} onClick={() => void undoLatest()}>撤销最近操作</button></div>}
      {generationPermitReview?.projectId === projectId && <div className="modal-backdrop"><section className="project-dialog generation-permit-dialog" role="dialog" aria-modal="true" aria-labelledby="generation-permit-title">
        <div className="project-dialog-header"><h2 id="generation-permit-title">确认本轮自动创作</h2><button className="dialog-close" type="button" aria-label="关闭创作范围确认" onClick={() => { setGenerationPermitReview(null); onDismissRevisionContinuation?.(); }}>×</button></div>
        {generationPermitReview.revisionPredecessor && <p className="project-dialog-description">后继创作来源：写作尝试 {generationPermitReview.revisionPredecessor.writerTaskId} · 判断 {generationPermitReview.revisionPredecessor.assessmentRevisionId} · 作者决策 {generationPermitReview.revisionPredecessor.decisionId}</p>}
        <p className="project-dialog-description">将在当前作品中生成至多 {generationPermit.targetChapters} 章可审阅工作稿，不自动采用正式版本。这里确认的是创作目标与范围，不是用量额度。</p>
        <label className="generation-permit-intent"><strong>本轮目标</strong><textarea aria-label="本轮创作目标" disabled={generationConfirming} value={generationPermitReview.intent} onChange={(event) => setGenerationPermitReview((current) => current ? { ...current, intent: event.target.value } : null)} /></label>
        {generationPermitInputError && <p className="creative-alert error" role="alert">{generationPermitInputError}</p>}
        <fieldset className="generation-scope-choice" disabled={generationConfirming}><legend>本轮创作范围</legend><label>新增章节数<input aria-label="新增章节数" type="number" min={1} max={MAX_GENERATION_CHAPTERS} step={1} value={generationPermitReview.targetChapters} onChange={(event) => { const count = Number(event.target.value); if (Number.isSafeInteger(count) && count >= 1 && count <= MAX_GENERATION_CHAPTERS) setGenerationPermitReview((current) => current ? { ...current, targetChapters: count } : null); }} /></label></fieldset>
        <p>执行至本次创作范围完成，可随时暂停或停止。模型用量按供应商实际计费。</p>
        <p className="project-dialog-description">暂停、重试与重规划会保留已保存的候选及用量记录。供应商限流、连接故障或授权失效仍可能中断执行；不会因为固定累计额度停止。</p>
        {generationReadError && <p className="creative-alert error" role="alert">创作任务状态未能核对，暂不能开始新一轮。<button className="text-button" type="button" onClick={() => setGenerationReload(value => value + 1)}>核对任务状态</button></p>}
        {(workspace?.revision !== generationPermitReview.expectedProjectRevision
          || workspace?.headSnapshotId !== generationPermitReview.expectedBaseSnapshotId
          || workspace?.sourceDigest !== generationPermitReview.expectedSourceDigest)
          && <p className="creative-alert error" role="alert">作品来源已变化，请返回编辑并重新核对创作目标。</p>}
        {generationPermitError && <p className="creative-alert error" role="alert">{generationPermitError}</p>}
        <div className="project-dialog-actions"><button className="secondary-button" type="button" onClick={() => { setGenerationPermitReview(null); onDismissRevisionContinuation?.(); }}>返回编辑</button><button className="primary-button" type="button" disabled={generationConfirming || busy || agentRunning || dirty || reviewDirty || supplementalEditor.active || Boolean(generationPermitInputError) || !agentAvailable || Boolean(currentGenerationRun) || generationRunsProjectId !== projectId || generationReadError
          || workspace?.revision !== generationPermitReview.expectedProjectRevision
          || workspace?.headSnapshotId !== generationPermitReview.expectedBaseSnapshotId
          || workspace?.sourceDigest !== generationPermitReview.expectedSourceDigest} onClick={() => {
            if (confirmingRef.current) return;
            const review = generationPermitReview;
            confirmingRef.current = true; setGenerationConfirming(true); setGenerationPermitError(null);
            void (async () => {
              let started = false;
              try {
                if (review.revisionPredecessor) {
                  if (!api) throw new Error("修改结果读取接口不可用");
                  const receipt = await api.runtime.getRevisionOutcome(projectId, review.revisionPredecessor.writerTaskId);
                  if (!revisionPredecessorIsCurrent(receipt, review.revisionPredecessor)) throw new Error("修改结果或最新作者决策已变化，请返回报告重新准备后继创作。");
                }
                const current = confirmationRef.current;
                if (current.review !== review || current.api !== api || current.projectId !== projectId || current.view !== view) return;
                if (current.blocked) throw new Error("请先保存修改并核对创作任务状态，再确认本轮创作。");
                if (current.workspace?.revision !== review.expectedProjectRevision || current.workspace.headSnapshotId !== review.expectedBaseSnapshotId
                  || current.workspace.sourceDigest !== review.expectedSourceDigest) throw new Error("作品来源已变化，请重新核对创作目标。");
                setGenerationPermitReview(null); onDismissRevisionContinuation?.();
                started = true;
                await onGenerateWork(review.intent.trim(), review.targetChapters, review.expectedProjectRevision,
                  review.expectedBaseSnapshotId, review.expectedSourceDigest, review.revisionPredecessor);
              } catch (reason) {
                if (confirmationRef.current.review === review) setGenerationPermitError(reason instanceof Error ? reason.message : "自动创作未能启动");
                else if (started && confirmationRef.current.projectId === projectId && confirmationRef.current.view === view) setError(reason instanceof Error ? reason.message : "自动创作未能启动");
              } finally { confirmingRef.current = false; setGenerationConfirming(false); }
            })();
          }}>{generationConfirming ? "正在核对来源…" : "确认开始创作"}</button></div>
      </section></div>}
    </div>
    </>
  );
}

function ChapterSwitcher({ chapters, selectedId, disabled, onSelect }: { chapters: Chapter[]; selectedId: string; disabled: boolean; onSelect: (chapter: Chapter) => void }) {
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLElement | null>(null);
  const index = Math.max(0, chapters.findIndex((chapter) => chapter.id === selectedId));
  const selected = chapters[index];
  useEffect(() => {
    if (!open) return;
    const close = (event: MouseEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", close);
    return () => document.removeEventListener("mousedown", close);
  }, [open]);
  if (!selected) return null;
  const choose = (chapter: Chapter) => {
    setOpen(false);
    if (chapter.id !== selected.id) onSelect(chapter);
  };
  return <nav className="chapter-switcher" aria-label="章节" ref={rootRef}>
    <button type="button" className="chapter-switcher-step" disabled={disabled || index === 0} onClick={() => choose(chapters[index - 1])} aria-label="上一章" title="上一章">‹</button>
    <button type="button" className="chapter-switcher-current" aria-expanded={open} aria-haspopup="listbox" disabled={disabled} onClick={() => setOpen((value) => !value)}>
      <span>第 {String(selected.number).padStart(2, "0")} 章</span>
      <b aria-hidden="true">⌄</b><span className="sr-only">切换章节</span>
    </button>
    <button type="button" className="chapter-switcher-step" disabled={disabled || index === chapters.length - 1} onClick={() => choose(chapters[index + 1])} aria-label="下一章" title="下一章">›</button>
    {open && <div className="chapter-switcher-menu" role="listbox" aria-label="全部章节">{chapters.map((chapter) => <button type="button" role="option" aria-selected={chapter.id === selected.id} className={chapter.id === selected.id ? "active" : ""} key={chapter.id} onClick={() => choose(chapter)}><span>{String(chapter.number).padStart(2, "0")}</span><b>{chapter.title}</b></button>)}</div>}
  </nav>;
}

function GenerationTaskStatus({ task, outputName }: { task: CreativeTask; outputName: string }) {
  const status = generationStatusLabel(task);
  const revisionCount = new Set(task.outputRefs.filter(reference => reference.kind === task.taskKind
    && (task.taskKind !== "draft" || reference.objectId === `draft-${task.target.chapterId}`)).map(reference => reference.revisionId)).size;
  return <div className={`generation-task-status ${task.deliveryStatus}${status.failed ? " failed" : ""}`} role="status" aria-live="polite"><strong>{status.label}</strong><span>{revisionCount > 0 ? `已保存 ${revisionCount} 个${outputName}` : `没有保存${outputName}`}</span>{task.error && <small>{task.error}</small>}</div>;
}

function ArtifactEditor({ fields, value, onChange, disabled }: { fields: { key: string; label: string; hint: string }[]; value: Record<string, string>; onChange: (key: string, value: string) => void; disabled?: boolean }) {
  return <div className="artifact-editor-grid">{fields.map((field) => <label key={field.key}><span>{field.label}</span><small>{field.hint}</small><textarea disabled={disabled} value={value[field.key] ?? ""} onChange={(event) => onChange(field.key, event.target.value)} /></label>)}</div>;
}

function DirectionDecisionEditor({ value, onChange, disabled = false }: { value?: DirectionDecisionValue; onChange: (value: DirectionDecisionValue) => void; disabled?: boolean }) {
  const decision = value?.decision ?? "";
  return <div className="direction-decision"><select disabled={disabled} aria-label="保护冲突处理决定" value={decision} onChange={(event) => onChange({ decision: event.target.value as DirectionDecision, note: value?.note ?? "" })}><option value="">请选择处理方式</option><option value="return-draft">退回草稿修改</option><option value="accept-and-open-impact">采用正文并打开反馈</option><option value="false-positive">确认是误报</option></select>{decision === "false-positive" && <input disabled={disabled} aria-label="误报说明" value={value?.note ?? ""} onChange={(event) => onChange({ decision, note: event.target.value })} placeholder="说明为什么该事实不影响此规划" />}</div>;
}
