import { useEffect,useMemo,useRef,useState } from "react";
import { createPortal } from "react-dom";
import { generationUsageBlocked,generationUsageDescription,workStopDescription } from "../shared/work-recovery-presentation.mjs";
import { ChapterPlanEditor } from "./ChapterPlanEditor";
import { CreationDirectionSummary } from "./CreationDirectionSummary";
import type { CreativeWorkspaceProps } from "./creative-workspace-props";
import type { PlanningTarget,WorkspaceView } from './creative-workspace-types';
import { GenerationPermitDialog } from "./GenerationPermitDialog";
import { GlobalStoryDesign } from "./GlobalStoryDesign";
import { ModuleRecordsButton } from "./ModuleRecordsButton";
import { OutlineLinkPanel } from "./OutlineLinkPanel";
import { PlanningReadingView } from "./PlanningReadingView";
import { PublicationReview } from "./PublicationReview";
import { manualDecisionKey } from "./PublicationReviewFields";
import { emptyReviewUi,readReviewUi,removeReviewUi,writeReviewUi } from "./review-ui-storage";
import type { CommitmentDecisionResult } from "./task-results";
import type { CommitmentDetail } from "./types";
import { useChapterCreation } from "./useChapterCreation";
import { useGenerationProgress } from "./useGenerationProgress";
import { useReviewChoices } from "./useReviewChoices";
import { useWorkspaceEditor } from "./useWorkspaceEditor";
import { WorkOverview } from "./WorkOverview";

import { authorIntentError } from "../shared/author-intent-validation.mjs";
import { NEW_CHAPTER_WRITING_INTENT } from "../shared/chapter-writing-policy.mjs";
import { describeAgentError } from "./agent-error-description";
import { ChapterDocumentView,ChapterPublicationStatus } from "./ChapterDocumentView";
import { ChapterPlanRebindPanel } from "./ChapterPlanRebindPanel";
import { CommitmentManagementDialog } from "./CommitmentManagementDialog";
import { CommitmentPanel } from "./CommitmentPanel";
import { outlinePathLabels,selectionAwareReviewChecksPass } from "./creative-product-logic";
import { StoppedExecutionError } from "./execution-stop";
import { GenerationReplanPanel } from "./GenerationReplanPanel";
import { OutlineTreeEditor } from "./OutlineTreeEditor";
import { PlanningReviewPanel } from "./PlanningReviewPanel";
import { PlanningWorkspaceHeader } from "./PlanningWorkspaceHeader";
import { SceneFeedbackDetails } from "./SceneFeedbackDetails";
import { StandaloneChapterForm } from "./StandaloneChapterForm";
import { useSupplementalEditor } from "./supplemental-editor";
import type { Chapter,ChapterPlanPayload,ChapterPlanRebindRequest,CreativeArtifactKind,CreativeReview,CreativeWorkspace,GenerationRunSummary,OutlineNode,ProvisionalPlanningImpact } from "./types";
import { useChapterPublication,type ChapterPublicationTarget } from "./useChapterPublication";
import { sameConversationFailure } from "./work-conversation";
import { WorkTaskPanel } from "./WorkTaskPanel";


const labels: Record<WorkspaceView, { title: string; description: string }> = {
  brief: { title: "作品概况", description: "明确创作方向，随写作逐步完善。尚未决定的内容可以留空。" },
  outline: { title: "全书大纲", description: "串起整部故事的起因、推进、关键转折与收束。各卷展开阶段事件，近章规划安排具体章节。" },
  plans: { title: "近章规划", description: "安排接下来几章的目标、冲突和写作指引，不必提前规划全书每一章。" },
  chapters: { title: "作品目录", description: "直接写这一章的正文。" },
};
const generationStageLabels: Record<GenerationRunSummary["stage"], string> = { coordinate: "安排创作步骤", finished: "完成" };

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

export function CreativeWorkspaceView({ authorKey, moduleActionsContainer, recordsOpen = false, outlineCatalogError = null, outlineVolume = null, onCloseOutlineVolume, onOpenVersions, onOutlineRecordScopeChange, onOpenOutlineNode, onLinkOutline, requestedChapterCreation = null, onChapterCreationPrepared, requestedCommitment = null, requestedCommitmentEdit = null, onCommitmentEditPrepared, taskResultSelection = null, onOpenTaskResult, onDiscussTaskResult, onDiscuss, api, taskList, taskControls, taskConversationContainer, conversation = null, projectId, view, state, selectedChapterId, onSelectChapter, onGenerateWork, onContinueWork, onRetryWork, onResolveFeedback, onReviewPlanning, onCancelPlanningReview, onExport, onPrepareChapterWriting, onGenerateChapter, onOpenPlanningTarget, onReturnToFeedback, onFormalChange, onDirtyChange, onChapterTitlePreview, onSaveHandlerChange, onPlanningSaveHandlerChange, onDiscardHandlerChange, agentAvailable, agentRunning, refreshToken, revisionContinuation = null, onPrepareRevisionContinuation, onPrepareRevisionRepair, onDismissRevisionContinuation }: CreativeWorkspaceProps) {
  const supplementalEditor = useSupplementalEditor();
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const handledResult = useRef<string | null>(null);
  const { factDecisions, noConfirmedChanges, factDependencyTargets, directionChangeDecisions, manualDirectionDecisions, selectedSceneGroups, setFactDecisions, setNoConfirmedChanges, setFactDependencyTargets, setDirectionChangeDecisions, setManualDirectionDecisions, setSelectedSceneGroups, reviewUiSnapshot, restoreReviewUi } = useReviewChoices();
  const [reviewDirty, setReviewDirty] = useState(false);
  const [commitmentResultsScope, setCommitmentResultsScope] = useState<string | null>(null);
  const [inlineCommitmentEdit, setInlineCommitmentEdit] = useState<{ requestId: string; result: CommitmentDecisionResult; detail: CommitmentDetail } | null>(null);
  const [commitmentManagementBusy, setCommitmentManagementBusy] = useState(false);
  useEffect(() => { setInlineCommitmentEdit(null); setCommitmentManagementBusy(false); }, [projectId]);
  const [publicationTarget, setPublicationTarget] = useState<ChapterPublicationTarget | null>(null);
  const [publicationOpen, setPublicationOpen] = useState(false);
  const [publicationPreparing, setPublicationPreparing] = useState(false);
  const pendingChapterAccept = useRef<{ scope: string; reviewId: string; revisionId: string } | null>(null);
  const chapterAcceptSubmission = useRef(false);
  const [chapterAcceptActivity, setChapterAcceptActivity] = useState<object | null>(null);
  const [reviewStateIdentity, setReviewStateIdentity] = useState("");
  const chapterWritingSubmitRef = useRef<(intent: string) => Promise<boolean>>(async () => false);
  const chapterDiscussionPrepareRef = useRef<() => Promise<boolean>>(async () => false);
  const [generating, setGenerating] = useState(false);
  const chapterGenerationSubmission = useRef(false);
  const [firstWriteTitleNotice, setFirstWriteTitleNotice] = useState<{ identity: string; savedTitle: string } | null>(null);
  const [chapterSyncError, setChapterSyncError] = useState<{ identity: string; message: string } | null>(null);
  const [associationPlanId, setAssociationPlanId] = useState("");
  const [feedbackTarget, setFeedbackTarget] = useState<{ impactId: string; targetKind: "outline" | "plan"; targetNodeId: string; targetLabel: string } | null>(null);
  const reviewRef = useRef<HTMLElement | null>(null);
  const planningEditorRef = useRef<HTMLDetailsElement | null>(null);
  const impactRefs = useRef(new Map<string, HTMLElement>());
  const reviewDirtyRef = useRef(false);

  const selectedChapter = state.novelEdition.chapters.find((chapter) => chapter.id === selectedChapterId) ?? state.novelEdition.chapters[0];
  const editor = useWorkspaceEditor({ authorKey, api, projectId, view, state, selectedChapter, refreshToken, agentRunning, taskControls, supplementalEditor, publicationPreparing, chapterAcceptActivity, reviewDirty, reviewDirtyRef, reviewUiSnapshot, setReviewDirty, feedbackTarget, setError, setNotice, onFormalChange, onDirtyChange, onChapterTitlePreview, onSaveHandlerChange, onPlanningSaveHandlerChange, onDiscardHandlerChange, discardCurrentReviewUi, readReviewSessionKey: () => reviewSessionKey });
  const { workspace, dirty, loading, busy, busyAction, planningDraft, draftTitle, draftSceneRows, lockedSceneGroups, conflict, editorIdentity, editorOwner, visibleChapters, impactCursor, impactCounts, saveCurrent, perform, canSavePlanningForDiscussion, reload, loadEditor, loadMoreImpacts, resolveChapterConflict, saveLocalAsLatestCandidate } = editor;
  const { brief, outlineNodes, plans, outlineError, reportOutlineError: setOutlineError } = planningDraft;
  const draft = draftSceneRows.map(scene => scene.body).join("\n\n---\n\n");
  const { currentGenerationRun, generationRunsProjectId, generationReadError, undoWorkChange, generationPermitReview, setGenerationReload, setGenerationRuns, feedbackAssessment, feedbackInstruction, setFeedbackInstruction, feedbackRepairUnavailable, feedbackRepairBudgetUnavailable, futureIntent, futureIntentSaving, futureIntentBlockedByActiveStep, currentRevisionUnavailable, setFutureIntent, applyGenerationIntent, setGenerationPermitReview } = useGenerationProgress({ projectId, view, revisionContinuation, api, agentRunning, refreshToken, setError, taskList, workspace, taskControls, dirty, setNotice, onFormalChange });
  const workspaceChapter = workspace?.chapters.find((chapter) => chapter.id === selectedChapter?.id);
  const chapterModeling = workspace?.modeling?.find((item) => item.objectId === `draft-${selectedChapter?.id}`);
  const viewKind: CreativeArtifactKind | "draft" = view === "plans" ? "plan" : view === "chapters" ? "draft" : view;
  const artifact = viewKind === "draft" ? null : workspace?.artifacts[viewKind];
  const [readingOutlineNodeId, setReadingOutlineNodeId] = useState<string | null>(null);
  useEffect(() => { setReadingOutlineNodeId(null); }, [projectId, view, outlineVolume?.id]);
  const outlineRevision = view === "outline" ? artifact?.candidate ?? artifact?.accepted : null;
  const savedOutlineNodes = (outlineRevision?.payload?.nodes ?? []) as OutlineNode[];
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
  const planningReviewOutlineNodes = view === "outline" && Array.isArray(planningReviewRevision?.payload?.nodes)
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
  const acceptedPlanChapters = Array.isArray(workspace?.artifacts.plan.accepted?.payload?.chapters) ? workspace.artifacts.plan.accepted.payload.chapters as ChapterPlanPayload[] : [];
  const boundPlanIds = new Set(workspace?.chapterPlanBindings?.map((binding) => binding.chapter_plan_id) ?? []);
  const chapterBinding = workspace?.chapterPlanBindings?.find((binding) => binding.chapter_id === selectedChapter?.id);
  const chapterPlan = chapterBinding?.writerProjection;
  const availableAssociationPlans = workspace?.planLineageStatus === "current" ? acceptedPlanChapters.filter((entry) => entry.chapterPlanId && !boundPlanIds.has(entry.chapterPlanId) && entry.chapterNumber === selectedChapter?.number) : [];
  const outlineForPlan = workspace?.artifacts.outline.working ?? workspace?.artifacts.outline.accepted;
  const outlinePaths = useMemo(() => {
    const nodes = Array.isArray(outlineForPlan?.payload?.nodes) ? outlineForPlan.payload.nodes as OutlineNode[] : [];
    return outlinePathLabels(nodes);
  }, [outlineForPlan?.id]);
  const planningTargets = useMemo<PlanningTarget[]>(() => {
    if (!workspace) return [];
    const outline = workspace.artifacts.outline.accepted;
    const plan = workspace.artifacts.plan.accepted;
    const targets: PlanningTarget[] = [];
    if (outline && Array.isArray(outline.payload?.nodes)) {
      for (const node of outline.payload.nodes as OutlineNode[]) {
        if (!node.id) continue;
        targets.push({ key: `outline:${node.id}`, kind: "outline", revisionId: outline.id, nodeId: node.id, label: outlinePaths.get(node.id) || node.publicTitle || node.title || "未命名大纲节点", currentArrangement: [node.purpose, node.summary].filter(Boolean).join("；") || node.title, protected: node.protection !== "free", protectionLabel: node.protection === "preserve-arrangement" ? "保护安排" : node.protection === "preserve-intent" ? "保护意图" : "自由调整" });
      }
    }
    if (plan && Array.isArray(plan.payload?.chapters)) {
      for (const chapter of plan.payload.chapters as ChapterPlanPayload[]) {
        if (!chapter.chapterPlanId) continue;
        const parentPath = chapter.primaryNodeId ? outlinePaths.get(chapter.primaryNodeId) : undefined;
        const chapterLabel = `第 ${chapter.chapterNumber} 章 · ${chapter.publicTitle || chapter.title || "未命名"}`;
        targets.push({ key: `plan:${chapter.chapterPlanId}`, kind: "plan", revisionId: plan.id, nodeId: chapter.chapterPlanId, label: parentPath ? `${parentPath} → ${chapterLabel}` : chapterLabel, currentArrangement: [chapter.goal, chapter.conflict].filter(Boolean).join("；") || chapterLabel, protected: chapter.locked === true, protectionLabel: chapter.locked ? "已锁定" : "自由调整" });
      }
    }
    return targets;
  }, [workspace?.artifacts.outline.accepted?.id, workspace?.artifacts.plan.accepted?.id, outlinePaths]);

  const publication = useChapterPublication({ scope: editorIdentity, api, projectId, chapterId: selectedChapter?.id,
    target: publicationTarget, bodyOnlyConfirmed: noConfirmedChanges, dirty,
    isBlocked: () => editor.isWriting() || taskControls.hasPendingRequests() || generating || agentRunning || supplementalEditor.active,
    save: () => saveCurrent(), savedRevision: () => editor.savedRevision(),
    onWorkspace: editor.loadSavedWorkspace,
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

  function markReviewDirty() {
    reviewDirtyRef.current = true;
    setReviewDirty(true);
  }

  function discardCurrentReviewUi() {
    if (editor.isWriting() || busy || taskControls.hasPendingRequests() || supplementalEditor.current.current?.busy) return false;
    if (supplementalEditor.current.current && !supplementalEditor.current.current.discard()) return false;
    if (!editor.discardDraft()) return false;
    removeReviewUi(reviewSessionKey);
    const fallback = emptyReviewUi(workspaceChapter?.candidate?.sceneGroups.map((group) => group.id) ?? []);
    restoreReviewUi(fallback);
    reviewDirtyRef.current = false;
    setReviewDirty(false);
    return true;
  }

  useEffect(() => {
    if (!chapterBinding) setAssociationPlanId(availableAssociationPlans[0]?.chapterPlanId ?? "");
  }, [chapterBinding?.chapter_plan_id, selectedChapter?.id, availableAssociationPlans.map((entry) => entry.chapterPlanId).join("|")]);

  useEffect(() => {
    setFeedbackTarget(null);
  }, [projectId, selectedChapter?.id]);

  useEffect(() => {
    document.documentElement.dataset.creativeWorkspace = "true";
    return () => { delete document.documentElement.dataset.creativeWorkspace; };
  }, []);

  useEffect(() => {
    reviewDirtyRef.current = reviewDirty;
    onDirtyChange(dirty || reviewDirty || supplementalEditor.dirty || supplementalEditor.busy,
      view === "chapters" && selectedChapter && dirty && !reviewDirty && !supplementalEditor.dirty && !supplementalEditor.busy && !busy
        ? { projectId, chapterId: selectedChapter.id } : null);
    return () => onDirtyChange(false);
  }, [dirty, reviewDirty, supplementalEditor.dirty, supplementalEditor.busy, onDirtyChange, view, selectedChapter?.id, projectId, busy]);

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

  async function enterReview(expectedRevisionId?: string) {
    const reviewRevisionId = expectedRevisionId ?? candidateId;
    if (!api || !reviewRevisionId) return;
    const created = await perform("正在生成审阅包…", async () => {
      if (expectedRevisionId) {
        const fresh = await api.runtime.getCreativeWorkspace(projectId);
        if (!editor.isCurrent(editorOwner) || editor.isRunning() || editor.isDirty()) throw new Error("当前页面或编辑状态已变化，请重新打开结果。");
        if (viewKind === "draft" || fresh.project.id !== projectId || fresh.artifacts[viewKind].candidate?.id !== expectedRevisionId) throw new Error("结果修订已变化，不能检查其他候选代替。");
        const ready = fresh.reviews.find(review => review.artifactKind === viewKind && review.artifactRevisionId === expectedRevisionId && review.projectRevision === fresh.revision && review.status === "ready");
        if (ready) return;
      }
      await api.runtime.createReview(projectId, viewKind, reviewRevisionId, []);
    }, "审阅包已生成，请检查后决定是否采用。");
    if (created) {
      if (view !== "chapters" && planningEditorRef.current) {
        planningEditorRef.current.open = true;
        requestAnimationFrame(() => {
          if (!editor.isCurrent(editorOwner)) return;
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
    if (!requestedCommitmentEdit || requestedCommitmentEdit.result.projectId !== projectId || !api || loading || !workspace
      || workspace.project.id !== projectId || busy || generating || agentRunning || dirty || reviewDirty
      || supplementalEditor.active || inlineCommitmentEdit) return;
    const { result, detail } = requestedCommitmentEdit;
    if (detail.projectId !== projectId || !detail.authorControls || !detail.sourceCurrent
      || detail.extractionOrigin?.chapterId !== result.chapterId || detail.extractionOrigin.reportRevisionId !== result.referenceId) return;
    setInlineCommitmentEdit(requestedCommitmentEdit);
    onCommitmentEditPrepared?.(requestedCommitmentEdit.requestId);
  }, [requestedCommitmentEdit, projectId, api, loading, workspace, busy, generating, agentRunning, dirty, reviewDirty, supplementalEditor.active, inlineCommitmentEdit]);

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
      if (!editor.isCurrent(editorOwner)) return;
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
            if (!editor.isCurrent(editorOwner)) return;
            const value = manualDirectionDecisions[manualDecisionKey(fact.id, target.key)];
            if (value?.decision === "false-positive") resolvedDirectionDecisions[impactId] = { decision: "false-positive", note: value.note.trim() };
            else if (value?.decision) resolvedDirectionDecisions[impactId] = value.decision;
          }
        }
      }
      if (!editor.isCurrent(editorOwner)) return;
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
        if (!editor.isCurrent(editorOwner)) return;
        if (viewKind === "draft") pendingChapterAccept.current = { scope: editorIdentity, reviewId: currentReview.id, revisionId: currentReview.artifactRevisionId };
      }
      await onFormalChange();
      if (viewKind !== "draft") setFeedbackTarget(null);
    }, viewKind === "draft" ? "这一章已发布。" : "已采用候选版本，作品正式版本已推进。", viewKind === "draft" ? validatePublishedReceipt : undefined);
    if (!editor.isCurrent(editorOwner)) return;
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
    } catch (reason) { if (editor.isCurrent(editorOwner)) setError(reason instanceof Error ? reason.message : "发布前读取失败，请重试。"); }
    finally { if (viewKind === "draft" && editor.isCurrent(editorOwner)) { chapterAcceptSubmission.current = false; setChapterAcceptActivity(null); } }
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

  async function resolveImpact(impactId: string, resolution: "kept" | "false-positive") {
    if (!api || !workspace || dirty) return;
    const note = resolution === "kept" ? "作者确认保留当前规划，不因本条正文事实调整。" : "作者确认该事实与此规划节点没有实际依赖关系。";
    await perform("正在记录作者的反馈决定…", async () => {
      await api.runtime.resolvePlanningImpact(projectId, impactId, workspace.revision, resolution, note);
      await onFormalChange();
      if (feedbackTarget?.impactId === impactId) setFeedbackTarget(null);
    }, resolution === "kept" ? "已记录：保留原规划。" : "已记录：此条反馈是误报。");
  }

  async function associateChapterPlan() {
    if (!api || !workspace || !selectedChapter || !associationPlanId || dirty) return;
    await perform("正在关联正式章纲…", async () => {
      await api.runtime.associateChapterPlan(projectId, selectedChapter.id, associationPlanId, workspace.revision);
      await onFormalChange();
    }, "独立章节已关联正式章纲；后续生成会使用精确规划依据。 ");
  }

  async function submitChapterPlanRebind(request: ChapterPlanRebindRequest) {
    if (!api || dirty || reviewDirty || draftInteractionLocked || editor.isWriting() || taskControls.hasPendingRequests()) throw new Error("请先保存修改并等待当前操作完成");
    const finishWrite = editor.beginWrite("正在更新章纲关联…");
    if (!finishWrite) throw new Error("请等待当前操作完成");
    setNotice(null);
    try {
      const result = await api.runtime.rebindChapterPlan(projectId, request);
      if (editor.isCurrent(editorOwner)) {
        try { await onFormalChange(); if (editor.isCurrent(editorOwner)) await reload(); }
        catch (reason) { if (editor.isCurrent(editorOwner)) setError(`关联已成功，但刷新失败：${reason instanceof Error ? reason.message : "请重新读取作品"}`); }
      }
      return result;
    } finally { finishWrite(); }
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
  const baseChapterInteractionLocked = busy || generating || agentRunning || Boolean(viewKind === "draft" && activeChapterDelivery) || supplementalEditor.active || editor.hasPendingSave;
  const chapterCreation = useChapterCreation({
    api, projectId, view, workspace, request: requestedChapterCreation,
    canPrepare: !baseChapterInteractionLocked && !dirty && !reviewDirty && !loading
      && editor.sessionReady(),
    blocked: busy || generating || agentRunning || dirty || reviewDirty || supplementalEditor.active,
    onPrepared: onChapterCreationPrepared,
    beginWrite: () => {
      if (editor.isDirty() || reviewDirtyRef.current || editor.isWriting()) return null;
      return editor.beginWrite("正在创建独立章节…");
    },
    onFormalChange,
    onReadback: ({ chapter, workspace: next, impacts }) => {
      editor.receiveCreatedChapter(next, impacts);
      setNotice("章节已创建，可以手写或生成正文；发布后成为正式章节。");
      onSelectChapter(chapter);
    },
    onError: setError,
  });
  const draftInteractionLocked = baseChapterInteractionLocked || Boolean(chapterCreation.pending);
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
    if (!operation.allowed || operation.owner !== editorOwner || operation.target !== publicationTarget || !undecidedReviewCount || editor.isWriting() || taskControls.hasPendingRequests()) return;
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
  const discussionRevision = workspaceChapter?.candidate ?? workspaceChapter?.accepted;
  const info = labels[view];
  const analyzingChapter = taskList.tasks.some(task => task.status === "running" && task.steps.some(step =>
    step.operation === "modelKnowledge" && step.status === "running" && step.target.chapterId === selectedChapter?.id));
  const chapterBusyLabel = taskControls.resuming ? analyzingChapter ? "正文已保存，正在继续分析…" : "正在继续任务…"
    : taskControls.controlling ? "正在处理任务…" : busy ? busyAction || "正在处理当前操作…"
    : analyzingChapter ? "正文已保存，正在分析…" : generating ? "正在生成并保存…" : "正在处理当前任务…";
  const chapterBodyEmpty = !draftSceneRows.some(scene => scene.body.trim());
  const emptyChapter = !workspaceChapter?.candidate && !workspaceChapter?.accepted && !workspaceChapter?.working
    && chapterBodyEmpty;

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
      if (!editor.isCurrent(editorOwner)) return false;
      let currentChapter = currentWorkspace.chapters.find(
        (chapter) => chapter.id === selectedChapter.id
      );
      if (!currentChapter) throw new Error("当前章节已变化，请重新读取后开始写作。");
      const noSavedRevision = !currentChapter.candidate && !currentChapter.accepted && !currentChapter.working;
      if (noSavedRevision && !draftSceneRows.some(scene => scene.body.trim()) && editor.isDirty()
        && !reviewDirtyRef.current && draftSceneRows.length === 1 && !lockedSceneGroups.length && draftTitle !== currentChapter.title) {
        setFirstWriteTitleNotice({ identity: editorIdentity, savedTitle: currentChapter.title });
        return false;
      }
      const firstWrite = noSavedRevision && emptyChapter && !editor.isDirty() && !reviewDirtyRef.current;
      if (!firstWrite) {
        if (reviewDirtyRef.current) throw new Error("请先保存或明确放弃发布检查决定，再开始写作。");
        const hadUnsavedChanges = editor.isDirty();
        if (hadUnsavedChanges && !(await saveCurrent())) return false;
        const revisionToKeep = hadUnsavedChanges ? editor.savedRevision() : candidateId ?? workspaceChapter?.accepted?.id;
        if (hadUnsavedChanges) {
          currentWorkspace = await api.runtime.getCreativeWorkspace(projectId);
          if (!editor.isCurrent(editorOwner)) return false;
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
      if (!editor.isCurrent(editorOwner)) return false;
      await onGenerateChapter(selectedChapter, intent.trim(), {
        povCharacterId: chapterPlan?.povCharacterId ?? null,
        storyTime: chapterPlan?.storyTime ?? null,
      });
      executionCompleted = true;
      if (!editor.isCurrent(editorOwner)) return false;
      await reload();
      if (!editor.isCurrent(editorOwner)) return false;
      setNotice(`第 ${selectedChapter.number} 章正文已保存，可以继续编辑或发布。`);
      return true;
    } catch (reason) {
      if (!editor.isCurrent(editorOwner)) return false;
      if (executionCompleted) {
        setChapterSyncError({ identity: editorIdentity, message: reason instanceof Error ? reason.message : "工作区读取失败" });
        setNotice(`第 ${selectedChapter.number} 章生成执行已结束；正文状态待重新读取，不会自动重跑模型。`);
        return true;
      }
      if (!editor.isDirty() && !reviewDirtyRef.current) await reload().catch(() => undefined);
      if (!editor.isCurrent(editorOwner)) return false;
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
    if (!editor.isCurrent(editorOwner)) return false;
    const chapter = current.chapters.find(item => item.id === selectedChapter.id);
    if (!chapter) throw new Error("当前章节已变化，请重新读取后开始写作。");
    const noSavedRevision = !chapter.candidate && !chapter.accepted && !chapter.working;
    if (noSavedRevision && !draftSceneRows.some(scene => scene.body.trim()) && editor.isDirty()
      && draftSceneRows.length === 1 && !lockedSceneGroups.length && draftTitle !== chapter.title) {
      setFirstWriteTitleNotice({ identity: editorIdentity, savedTitle: chapter.title });
      return false;
    }
    return !editor.isDirty() || await saveCurrent();
  };

  function prepareChapterWriting() {
    if (!selectedChapter || !editor.sessionReady()) return;
    const owner = editorOwner;
    onPrepareChapterWriting?.({ projectId, chapterId: selectedChapter.id, chapterNumber: selectedChapter.number, title: draftTitle,
      defaultIntent: chapterBodyEmpty ? `${NEW_CHAPTER_WRITING_INTENT}保留章节标题《${draftTitle}》。` : "",
      prepareDiscussion: () => editor.isCurrent(owner) ? chapterDiscussionPrepareRef.current() : Promise.resolve(false),
      submit: intent => editor.isCurrent(owner) ? chapterWritingSubmitRef.current(intent) : Promise.resolve(false),
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

  const generationBlocked = busy || agentRunning || dirty || reviewDirty || supplementalEditor.active
    || !agentAvailable || Boolean(currentGenerationRun)
    || generationRunsProjectId !== projectId || generationReadError;
  const actionableTasks = Boolean(taskList.error) || taskList.activeCount > 0
    || taskList.tasks.some(task => (task.status !== "completed" && task.status !== "cancelled")
      || task.deliveryReport.pendingActionIds.length > 0 || task.deliveryReport.unsettledAttemptIds.length > 0);
  const chapterSettingsRef = useRef<HTMLDetailsElement>(null);
  useEffect(() => {
    if (chapterSettingsRef.current) chapterSettingsRef.current.open = false;
  }, [editorIdentity]);
  useEffect(() => {
    const outside = (event: PointerEvent) => {
      const settings = chapterSettingsRef.current;
      if (settings?.open && !settings.contains(event.target as Node)) {
        settings.open = false;
        requestAnimationFrame(() => {
          if (chapterSettingsRef.current === settings && !settings.open && settings.contains(document.activeElement)) {
            settings.querySelector<HTMLElement>(":scope > summary")?.focus({ preventScroll: true });
          }
        });
      }
    };
    document.addEventListener("pointerdown", outside, true);
    return () => document.removeEventListener("pointerdown", outside, true);
  }, []);
  const chapterSettings = selectedChapter ? <details ref={chapterSettingsRef} key={`settings:${editorIdentity}`} className="chapter-settings" onKeyDown={event => {
    if (event.key !== "Escape") return;
    event.preventDefault(); event.currentTarget.open = false;
    event.currentTarget.querySelector<HTMLElement>(":scope > summary")?.focus({ preventScroll: true });
  }}><summary>章节设置</summary><div className="chapter-settings-content">
        {chapterBinding && <p>关联章纲：{chapterPlan?.publicTitle || `第 ${selectedChapter.number} 章`}</p>}
        {api && chapterBinding && <ChapterPlanRebindPanel api={api} projectId={projectId} chapterId={selectedChapter.id} disabled={draftInteractionLocked || dirty || reviewDirty} hasUpdate={Boolean((workspace?.artifacts.plan.candidate ?? workspace?.artifacts.plan.accepted)?.id !== chapterBinding.plan_revision_id)} submit={submitChapterPlanRebind} refresh={async () => {
          if (dirty || reviewDirty || draftInteractionLocked) throw new Error("请先保存修改并等待当前操作完成");
          if (!await perform("正在重新读取作品…", onFormalChange, "作品已重新读取。")) throw new Error("重新读取作品失败");
        }} />}
        {!chapterBinding && <section className="standalone-planning-card">
          <div><strong>这是独立章节</strong><small>{availableAssociationPlans.length ? "可以关联同章号且尚未使用的章纲。" : "暂无可关联的章纲，可先在近章规划中安排本章。"}</small></div>
          {availableAssociationPlans.length > 0 ? <>
            <select disabled={draftInteractionLocked} aria-label="可关联的正式章纲" value={associationPlanId} onChange={event => setAssociationPlanId(event.target.value)}>{availableAssociationPlans.map(entry => <option key={entry.chapterPlanId} value={entry.chapterPlanId}>{entry.publicTitle || entry.title || `第 ${entry.chapterNumber} 章章纲`}</option>)}</select>
            <button className="secondary-button" disabled={busy || draftInteractionLocked || dirty || !associationPlanId} onClick={() => void associateChapterPlan()}>关联章纲</button>
          </> : <button className="secondary-button" disabled={draftInteractionLocked} onClick={() => onOpenPlanningTarget("plan")}>打开近章规划</button>}
          {availableAssociationPlans.length > 0 && workspaceChapter?.candidate && <small>关联章纲后，已有正文会保留。</small>}
        </section>}
    </div></details> : null;
  const hasCatalogSelection = requestedCommitment?.request.projectId === projectId && requestedCommitment.chapterId === selectedChapter?.id;
  const commitmentChapters = state.novelEdition.chapters.map(chapter => ({ id: chapter.id, title: `第 ${chapter.number} 章 · ${chapter.title}` }));
  const commitmentSources = (workspace?.chapters ?? []).flatMap(chapter => {
    const revision = chapter.candidate ?? chapter.accepted;
    return revision ? [{ chapterId: chapter.id, title: `第 ${chapter.number} 章 · ${chapter.title}`, objectId: `draft-${chapter.id}`, revisionId: revision.id, body: revision.body }] : [];
  });
  const editingCommitment = inlineCommitmentEdit?.result.projectId === projectId ? inlineCommitmentEdit : null;
  const commitmentResults = <div className="chapter-commitment-management" hidden={Boolean(editingCommitment) || !hasCatalogSelection && commitmentResultsScope !== editorIdentity}>
      {view === "chapters" && selectedChapter && <CommitmentPanel requestedCommitment={requestedCommitment?.chapterId === selectedChapter.id && requestedCommitment.request.projectId === projectId ? requestedCommitment : null} requestedResult={taskResultSelection?.result.kind === "commitment" || taskResultSelection?.result.kind === "commitmentExtraction" ? { requestId: taskResultSelection.requestId, result: taskResultSelection.result } : null} autoOpen={hasCatalogSelection || commitmentResultsScope === editorIdentity} api={api} projectId={projectId} chapterId={selectedChapter.id} sourceRevisionId={candidateId ?? workspaceChapter?.accepted?.id ?? null} dirty={dirty || reviewDirty} blocked={busy || generating || agentRunning} refreshToken={refreshToken} onFormalChange={onFormalChange} registerEditor={supplementalEditor.register}
        chapters={commitmentChapters} sources={commitmentSources} />}
  </div>;
  const renderTaskPanel = () => (
    <WorkTaskPanel onFormalChange={onFormalChange} onAdjustCommitment={(result, detail) => {
      if (busy || generating || agentRunning || dirty || reviewDirty || supplementalEditor.current.current || editingCommitment
        || result.projectId !== projectId || detail.projectId !== projectId || !detail.authorControls || !detail.sourceCurrent
        || detail.extractionOrigin?.chapterId !== result.chapterId || detail.extractionOrigin.reportRevisionId !== result.referenceId) return;
      setInlineCommitmentEdit({ requestId: crypto.randomUUID(), result, detail });
    }} onDiscussResult={onDiscussTaskResult} onOpenResult={onOpenTaskResult} key={projectId} conversation={conversation} api={api} projectId={projectId} tasks={taskList.tasks} activeCount={taskList.activeCount} nextCursor={taskList.nextCursor} loadingMore={taskList.loadingMore} loaded={taskList.loaded} reading={taskList.reading} readError={taskList.error} onRetry={() => void taskList.refresh()} revision={workspace?.revision} busy={busy} agentRunning={agentRunning} dirty={dirty || reviewDirty || supplementalEditor.active || Boolean(editingCommitment)} onUndo={(commitId) => void undoWorkChange(commitId)} onLoadMore={() => void taskList.loadMore()}
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
    ? createPortal(<>{(conversation || actionableTasks || (taskList.loaded && (taskList.tasks.length > 0 || taskList.nextCursor))) && renderTaskPanel()}</>, taskConversationContainer) : null;
  const planningHeader = view !== "chapters" ? <PlanningWorkspaceHeader view={view} title={info.title} description={info.description}
        artifact={artifact ?? undefined} dirty={dirty} blocked={busy || draftInteractionLocked || lockedDraftConflict}
        prepareDirtyDiscussion={Boolean(onPlanningSaveHandlerChange) && canSavePlanningForDiscussion}
        locked={artifactLocked} hasReview={Boolean(currentReview)} onDiscuss={view === "brief" ? onDiscuss : undefined}
        onSave={() => void saveCurrent()} onReview={() => {
          if (currentReview) { reviewRef.current?.scrollIntoView({ block: "center", behavior: "smooth" }); reviewRef.current?.focus({ preventScroll: true }); }
          else void enterReview();
        }} onOpenVersions={openRecords} /> : null;
  const publicationDialog = <PublicationReview busy={busy} agentRunning={agentRunning}
    view={view}
    currentReview={currentReview}
    publicationOpen={publicationOpen}
    publicationTarget={publicationTarget}
    editorIdentity={editorIdentity}
    closePublication={closePublication}
    pendingChapterAccept={pendingChapterAccept}
    noConfirmedChanges={noConfirmedChanges}
    setNoConfirmedChanges={setNoConfirmedChanges}
    publication={publication}
    recoverPublication={recoverPublication}
    reviewRef={reviewRef}
    viewKind={viewKind}
    confirmedFacts={confirmedFacts}
    workspaceChapter={workspaceChapter}
    reviewFacts={reviewFacts}
    undecidedReviewCount={undecidedReviewCount}
    selectedReviewFacts={selectedReviewFacts}
    bulkReviewAllowed={bulkReviewAllowed}
    decideUndecidedReviewFacts={decideUndecidedReviewFacts}
    draftInteractionLocked={draftInteractionLocked}
    markReviewDirty={markReviewDirty}
    selectedSceneGroups={selectedSceneGroups}
    provisionalImpacts={provisionalImpacts}
    planningTargets={planningTargets}
    factDependencyTargets={factDependencyTargets}
    factDecisions={factDecisions}
    publicationEntities={publicationEntities}
    state={state}
    api={api}
    projectId={projectId}
    setFactDecisions={setFactDecisions}
    directionChangeDecisions={directionChangeDecisions}
    setDirectionChangeDecisions={setDirectionChangeDecisions}
    setFactDependencyTargets={setFactDependencyTargets}
    manualDirectionDecisions={manualDirectionDecisions}
    setManualDirectionDecisions={setManualDirectionDecisions}
    reviewProgress={reviewProgress}
    canAccept={canAccept}
    acceptCurrent={acceptCurrent}
    returnsDraft={returnsDraft} />;
  if (!api) return <>{conversationPortal}<div className="creative-empty"><h2>{info.title}</h2><p>桌面 Runtime 连接后可使用结构化创作工作区。</p></div></>;
  if (loading || workspace && workspace.project.id !== projectId) return <>{conversationPortal}<div className="creative-workspace-view" aria-busy="true"><div className="creative-loading"><span className="loading-pulse" /><strong>正在加载{info.title}</strong><p>{view === "chapters" ? "正在读取章节内容与发布状态…" : "正在恢复候选、正式版本与审阅状态…"}</p></div></div></>;
  if (error && !workspace) return <>{conversationPortal}<div className="creative-empty" role="alert"><h2>{info.title}暂时无法打开</h2><p>{error}</p><button className="secondary-button" onClick={() => void editor.retryLoad()}>重新加载</button></div></>;

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
          if (refreshed && editor.isCurrent(editorOwner)) setChapterSyncError(null);
        });
      }}>重新读取章节内容</button><small>仅重新读取，不会再次生成或写入。</small></div>}
      {busyAction && <div className="creative-alert" role="status">{busyAction}</div>}

      {conflict && <div className="creative-alert warning editor-conflict" role="alert"><div><strong>服务器候选已变化，本地编辑仍被保留</strong><span>当前编辑基于 {conflict.baseCandidateRevisionId ?? "空候选"}，服务器现为 {conflict.serverCandidateRevisionId ?? "空候选"}。上方编辑器是本地版本，下方候选栏是服务器版本。</span><details><summary>查看本地编辑的基线版本</summary><pre>{conflict.baseContent}</pre></details></div><div className="editor-conflict-actions">{view === "chapters" && editor.canResolveConflict && <><span>结构冲突采用所选侧的完整场景顺序；独立修改继续保留，整合后需保存。</span><button className="secondary-button" disabled={busy} onClick={() => void resolveChapterConflict("local")}>冲突处保留我的修改</button><button className="secondary-button" disabled={busy} onClick={() => void resolveChapterConflict("remote")}>冲突处使用服务器内容</button></>}{view === "chapters" && !editor.canResolveConflict && <button className="secondary-button" disabled={busy} onClick={() => { editor.retryHistory(); }}>重新核对历史版本</button>}<button className="primary-button" disabled={busy} onClick={() => void saveLocalAsLatestCandidate()}>以最新版为基线保存本地稿</button><button className="secondary-button" disabled={busy} onClick={() => void copyLocalEditor()}>复制本地稿</button><button className="secondary-button" disabled={busy} onClick={() => { if (workspace) loadEditor(workspace); }}>放弃本地稿并载入服务器版本</button></div></div>}
      {artifactLocked && <div className="creative-alert warning">规划已锁定，当前内容只读。{onOpenVersions && <button type="button" className="text-button" onClick={openRecords}>前往修改记录解锁</button>}</div>}
      {!agentAvailable && <div className="creative-alert">{view === "chapters" ? "当前未配置可用模型；仍可手动编辑并保存这一章。" : "当前未配置可用模型；仍可手动编辑并保存候选。"}</div>}
      {workspace?.index.status === "stale" && <div className="creative-alert warning">长篇索引正在追赶正式版本，检索结果可能暂时滞后。</div>}
      {view === "chapters" && analyzingChapter && chapterModeling?.sourceRevisionId === candidateId && chapterModeling?.status === "model_pending" && <p className="chapter-analysis-status" role="status">正在分析本章的故事设定…</p>}
      {view === "chapters" && chapterModeling && chapterModeling.sourceRevisionId !== candidateId && <div className="creative-alert warning" role="status">故事设定分析来源与当前正文不同，请重新读取后再发布。</div>}
      {view === "chapters" && editor.hasPendingSave && <div className="creative-alert warning" role="status">章节已保存，正在等待读取结果。<button type="button" className="secondary-button" disabled={busy || agentRunning || generating} onClick={() => void saveCurrent()}>重新读取已保存章节</button></div>}
      {workspace?.planLineageStatus === "stale" && <div className="creative-alert warning">大纲已更新，当前章纲需要重新核对。旧章纲不会继续指导正文。{onDiscuss && <button type="button" className="secondary-button" disabled={busy || agentRunning || dirty || reviewDirty} onClick={() => onDiscuss("请基于当前大纲和已发生正文，更新近章规划并保存候选。保留一致的安排；涉及受保护内容或方向取舍时先说明，不要自动采用或发布。")}>在对话中更新规划</button>}</div>}
      {feedbackTarget && view !== "chapters" && <div className="feedback-return-bar"><span>正在处理反馈：{feedbackTarget.targetLabel}</span><button className="secondary-button" onClick={() => { onReturnToFeedback(); }}>返回原反馈</button></div>}

      {view === "brief" && workspace?.project.id === projectId && <GlobalStoryDesign workspace={workspace} blocked={busy || agentRunning || Boolean(conflict)} dirty={dirty || reviewDirty || supplementalEditor.dirty} onOpen={onOpenPlanningTarget} />}
      {view === "brief" && <section className="overview-direction" aria-label="创作方向">
        <CreationDirectionSummary artifact={workspace?.artifacts.brief} />
        <details ref={planningEditorRef} key={projectId} className="overview-direction-editor"><summary>编辑创作方向{dirty || reviewDirty ? " · 有未保存修改" : ""}</summary>
          {planningHeader}
          <ArtifactEditor disabled={artifactLocked || busy || agentRunning} fields={[{ key: "premise", label: "核心命题", hint: "故事真正要讨论什么" }, { key: "promise", label: "读者承诺", hint: "读者持续阅读会得到什么体验" }, { key: "stakes", label: "核心代价", hint: "失败会失去什么" }, { key: "endingDirection", label: "结局方向", hint: "尚未决定可以留空，后续随创作调整" }]} value={brief} onChange={planningDraft.editBrief} />
          {publicationDialog}
        </details>
      </section>}
      {(view === "outline" || view === "plans") && <details ref={planningEditorRef} key={`${projectId}:${view}`} className="planning-manual-editor" hidden={outlineFocusUnavailable} style={outlineFocusUnavailable ? { display: "none" } : undefined}>
        <summary>{view === "outline" ? <><span className="outline-editor-closed-label">{focusedOutlineNode ? "编辑本卷" : "编辑全书主线与脉络"}</span><span className="outline-editor-open-label">返回阅读</span></> : "手动编辑近章规划"}{dirty || reviewDirty ? " · 有未保存修改" : ""}</summary>
        {view === "outline" && <p className="outline-editing-scope">{focusedOutlineNode ? "聚焦本卷的阶段事件与变化，下级安排在结构中单独编辑。" : "聚焦整部故事的主线与因果脉络，各卷事件在卷纲中展开。"}保存保留其他安排；采用检查作用于整份大纲。</p>}
        {view !== "outline" && planningHeader}
      {view === "outline" && <OutlineTreeEditor disabled={artifactLocked || busy || agentRunning} error={outlineError} rootNodeId={focusedOutlineNode ? requestedOutlineNodeId : null} focusNodeId={outlineFocusUnavailable ? null : feedbackTarget?.targetKind === "outline" ? feedbackTarget.targetNodeId : null} nodes={outlineNodes} onChange={planningDraft.editOutline} />}
      {view === "plans" && <ChapterPlanEditor api={api} projectId={projectId} refreshToken={refreshToken}
        disabled={artifactLocked || busy || agentRunning} dirty={dirty} planningDraft={planningDraft}
        acceptedPlanChapters={acceptedPlanChapters} boundPlanIds={boundPlanIds} lineageStatus={workspace?.planLineageStatus}
        outlineNodes={(outlineForPlan?.payload?.nodes ?? []) as OutlineNode[]} outlinePaths={outlinePaths} onMaterialize={materializePlanChapter} />}
        {view === "outline" && <>{planningHeader}<button className="text-button outline-return-reading" type="button" onClick={() => { const editor = planningEditorRef.current; if (!editor) return; editor.open = false; const entry = editor.querySelector("summary"); entry?.scrollIntoView({ block: "center" }); entry?.focus({ preventScroll: true }); }}>返回阅读{dirty ? "（修改尚未保存）" : ""}</button></>}
        {publicationDialog}
      </details>}
      {view === "brief" && (generationReadError || currentGenerationRun) && <section className="planning-intent-card">
        {view === "brief" && generationReadError && !currentGenerationRun && <div className="creative-alert error" role="alert">创作运行状态读取失败，暂不能确认是否有未完成任务。<button className="text-button" type="button" onClick={() => setGenerationReload(value => value + 1)}>重新读取</button></div>}
        {view === "brief" && currentGenerationRun && <div className="creative-alert" role="status">
          自动创作：{currentGenerationRun.completedChapters - currentGenerationRun.startChapterNumber + 1}/{currentGenerationRun.targetChapters - currentGenerationRun.startChapterNumber + 1} 章 · {currentGenerationRun.status === "blocked" ? workStopDescription(currentGenerationRun.blockedReason) : currentGenerationRun.status === "paused" ? "已暂停" : `进行到 ${generationStageLabels[currentGenerationRun.stage]}`}
          {generationUsageBlocked(currentGenerationRun) && <small>{generationUsageDescription(currentGenerationRun)}</small>}
          {generationReadError && <small>创作运行状态暂未刷新，确认最新状态前不能继续。</small>}
          {(generationUsageBlocked(currentGenerationRun) || generationReadError) && <button className="text-button" type="button" onClick={() => setGenerationReload(value => value + 1)}>重新核对状态</button>}
          {currentGenerationRun.status === "running" && <button className="secondary-button" type="button" disabled={busy || agentRunning || dirty || !agentAvailable || generationUsageBlocked(currentGenerationRun) || generationReadError} onClick={() => void onContinueWork(currentGenerationRun.id)}>继续创作</button>}
          {currentGenerationRun.status === "blocked" && ["agent_task_failed", "agent_task_cancelled", "agent_task_failed_after_output", "agent_task_cancelled_after_output"].includes(currentGenerationRun.blockedReason ?? "") && <button className="secondary-button" type="button" disabled={busy || agentRunning || dirty || !agentAvailable || generationUsageBlocked(currentGenerationRun) || generationReadError} onClick={() => void onRetryWork(currentGenerationRun.id)}>{currentGenerationRun.feedbackRepairPrompt !== null ? "按评估要求重试本章" : currentGenerationRun.pendingCurrentIntentGeneration !== null ? "按新要求重写本章" : currentGenerationRun.blockedReason?.endsWith("_after_output") ? "从已保存候选继续" : "重试这一步"}</button>}
        </div>}
      {view === "brief" && api && currentGenerationRun && ["paused", "blocked"].includes(currentGenerationRun.status) && <GenerationReplanPanel
          api={api} projectId={projectId} runId={currentGenerationRun.id} disabled={busy || agentRunning || dirty || generationUsageBlocked(currentGenerationRun) || generationReadError}
          onConfirmed={run => {
            setGenerationRuns(previous => previous.map(item => item.id === run.id ? run : item));
            setError(null); setNotice("重规划已保存，可继续创作。");
            void Promise.resolve().then(onFormalChange).catch(reason => {
              if (editor.isCurrent(editorOwner) && editor.isCurrent(editorOwner)) {
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
      {(view === "outline" || view === "plans") && <PlanningReviewPanel agentAvailable={agentAvailable}
        key={view}
        runs={workspace?.planningReviews ?? []}
        artifactKind={view === "outline" ? "outline" : "plan"}
        revisionId={planningReviewRevision?.id ?? null}
        outlineNodes={planningReviewOutlineNodes}
        busy={busy || agentRunning || dirty}
            onStart={onReviewPlanning}
        onCancel={onCancelPlanningReview}
      />}
      {view === "chapters" && chapterCreation.pending && <div className="creative-alert warning" role="alert">章节已创建，工作区读取失败：{chapterCreation.pending.error}。重新读取不会重复创建章节。<button type="button" className="secondary-button" disabled={busy || generating || agentRunning || dirty || reviewDirty || supplementalEditor.active} onClick={() => void chapterCreation.recover()}>重新读取已创建章节</button></div>}
      {view === "chapters" && (selectedChapter ? <div className="draft-workspace">
        <div className="chapter-page-toolbar"><ChapterSwitcher chapters={visibleChapters} selectedId={selectedChapter.id} disabled={draftInteractionLocked} onSelect={chapter => { const original = state.novelEdition.chapters.find(item => item.id === chapter.id); if (original) onSelectChapter(original); }} />
        <div className="chapter-page-actions">{chapterSettings}{onOpenVersions && <ModuleRecordsButton label={`第 ${selectedChapter.number} 章 · ${selectedChapter.title}`} open={recordsOpen} onClick={() => onOpenVersions()} />}{onExport && <button type="button" className="text-button" onClick={onExport}>导出</button>}</div></div>
        {chapterCreation.open && <StandaloneChapterForm key={`${projectId}:${chapterCreation.requestId}`} requestId={chapterCreation.requestId} volumeTitle={chapterCreation.volumeTitle} nextNumber={nextChapterNumber} disabled={standaloneCreationLocked} busy={busy} onCreate={chapterCreation.create} onCancel={chapterCreation.close} />}
        <div className="draft-main-column">
        <div data-discussion-revision={discussionRevision?.id ?? ""} data-discussion-object={`draft-${selectedChapter.id}`}>
        {editor.sessionReady() ? <ChapterDocumentView key={`${projectId}:${selectedChapter.id}`} scope={`${projectId}:${selectedChapter.id}`} scenes={draftSceneRows} accepted={workspaceChapter?.accepted ?? null} title={draftTitle} sourcePlanRefs={(workspaceChapter?.candidate ?? workspaceChapter?.accepted)?.sourcePlanRefs ?? null} onTitleChange={title => { editor.changeTitle(title); }} lockedIds={lockedSceneGroups} disabled={draftInteractionLocked} dirty={dirty} onChange={(scenes) => { editor.changeScenes(scenes); }} emptyWriting={chapterBodyEmpty ? { disabled: !onPrepareChapterWriting, onPrepare: prepareChapterWriting } : undefined} onLocksChange={locked => { editor.changeLocks(locked); }} /> : <p role="status">正在恢复当前章节内容…</p>}
        </div>
        {draftInteractionLocked && <small role="status">{chapterBusyLabel}当前操作结束后可继续编辑。</small>}
          {firstWriteTitleNotice?.identity === editorIdentity && <div role="alert"><p>章节标题尚未保存，当前正文为空。首次生成将使用已创建章节的标题“{firstWriteTitleNotice.savedTitle}”；本地标题仍保留。可以恢复章节标题后开始，或先手写正文并一起保存。</p><button className="secondary-button" type="button" disabled={draftInteractionLocked || reviewDirty || draftSceneRows.length !== 1 || lockedSceneGroups.length > 0 || draftSceneRows.some(scene => scene.body.trim())} onClick={() => { editor.restoreEmptyChapterTitle(firstWriteTitleNotice.savedTitle); setFirstWriteTitleNotice(null); }}>恢复章节标题</button></div>}
        {lockedDraftConflict && <div className="creative-alert warning" role="alert">锁定场景的正文已变化。请先恢复正文，只提交一次“取消保护”，保存后再修改。</div>}

        </div>
      </div> : <StandaloneChapterForm key={`${projectId}:${chapterCreation.requestId}`} requestId={chapterCreation.requestId} volumeTitle={chapterCreation.volumeTitle} nextNumber={nextChapterNumber} disabled={standaloneCreationLocked} busy={busy} onCreate={chapterCreation.create} />)}

      {view === "chapters" ? selectedChapter && <div className="candidate-toolbar chapter-save-toolbar"><div className="chapter-save-status"><span className="chapter-word-count">{Array.from(draftSceneRows.map(scene => scene.body).join("").replace(/\s/g, "")).length} 字符</span><span className="chapter-save-state">{dirty ? "正文未保存" : candidateId || workspaceChapter?.accepted ? "正文已保存" : "尚未写作"}</span>{(reviewDirty || supplementalEditor.dirty) && <span className="chapter-review-dirty" role="status">设定或检查有未保存修改</span>}<ChapterPublicationStatus published={workspaceChapter?.accepted} saved={workspaceChapter?.candidate} dirty={dirty} /></div><button className="secondary-button" disabled={draftInteractionLocked || lockedDraftConflict || !draft.trim()} onClick={() => void saveCurrent()}>保存</button><button data-publish-chapter type="button" className="primary-button" disabled={draftInteractionLocked || lockedDraftConflict || !draft.trim() || (!dirty && (!candidateId || candidateId === workspaceChapter?.accepted?.id))} onClick={() => void publication.prepare()}>发布章节</button>{pendingChapterKnowledge && candidateId === workspaceChapter?.accepted?.id && <details><summary>故事设定变化</summary><button className="secondary-button" disabled={draftInteractionLocked || dirty} onClick={() => void publication.prepare()}>更新故事设定</button></details>}</div> : null}

      {view === "chapters" && commitmentResults}
      {editingCommitment && <CommitmentManagementDialog key={editingCommitment.requestId}
        dirty={supplementalEditor.dirty}
        protectedInput={supplementalEditor.dirty || supplementalEditor.busy || commitmentManagementBusy || busy || agentRunning}
        onClose={() => { if (!supplementalEditor.current.current?.dirty && !supplementalEditor.current.current?.busy && !commitmentManagementBusy && !busy && !agentRunning) setInlineCommitmentEdit(null); }}>
        <CommitmentPanel api={api} projectId={projectId} chapterId={editingCommitment.result.chapterId}
          sourceRevisionId={editingCommitment.result.sourceRevisionId ?? commitmentSources.find(source => source.chapterId === editingCommitment.result.chapterId)?.revisionId ?? null}
          dirty={dirty || reviewDirty} blocked={busy || generating || agentRunning} refreshToken={refreshToken} onFormalChange={onFormalChange}
          registerEditor={supplementalEditor.register} onBusyChange={setCommitmentManagementBusy} autoOpen initialEdit={{ requestId: editingCommitment.requestId, detail: editingCommitment.detail }}
          requestedResult={{ requestId: editingCommitment.requestId, result: editingCommitment.result }} chapters={commitmentChapters} sources={commitmentSources} />
      </CommitmentManagementDialog>}
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
      {<GenerationPermitDialog blocked={generationBlocked}
    generationPermitReview={generationPermitReview}
    projectId={projectId}
    setGenerationPermitReview={setGenerationPermitReview}
    onDismissRevisionContinuation={onDismissRevisionContinuation}
    generationReadError={generationReadError}
    setGenerationReload={setGenerationReload}
    workspace={workspace}
    api={api}
    view={view}
    onGenerateWork={onGenerateWork}
    setError={setError} />}
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

function ArtifactEditor({ fields, value, onChange, disabled }: { fields: { key: string; label: string; hint: string }[]; value: Record<string, string>; onChange: (key: string, value: string) => void; disabled?: boolean }) {
  return <div className="artifact-editor-grid">{fields.map((field) => <label key={field.key}><span>{field.label}</span><small>{field.hint}</small><textarea disabled={disabled} value={value[field.key] ?? ""} onChange={(event) => onChange(field.key, event.target.value)} /></label>)}</div>;
}
