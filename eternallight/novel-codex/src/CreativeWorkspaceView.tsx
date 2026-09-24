import { useEffect, useMemo, useRef, useState } from "react";

import { newDraftScene, firstOutlineEditorError, formatFactValue, generationStatusLabel, outlinePathLabels, parseFactValue, rebaseEditorBaseline, selectionAwareReviewChecksPass, valueForIdentity } from "./creative-product-logic";
import type { Chapter, ChapterPlanPayload, CreativeArtifactKind, CreativeReview, CreativeWorkspace, NovelCodexApi, OutlineNode, ProvisionalPlanningImpact, RollingPlanPayload, RuntimeState, ViewId, WriterContextPreview } from "./types";
import { OutlineTreeEditor } from "./OutlineTreeEditor";
import { DraftSceneEditor } from "./DraftSceneEditor";
import { PlanningReviewPanel, type PlanningReviewStartRequest } from "./PlanningReviewPanel";
import { authorIntentError } from "../shared/author-input-budget.mjs";

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
type EditorUiSession = { planningIntent: string; planningIntentSeed: string; generationIntent: string; draftScrollTop: number; draftSelectionStart: number; draftSelectionEnd: number };
const editorUiSessions = new Map<string, EditorUiSession>();
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
  api?: NovelCodexApi;
  projectId: string;
  view: WorkspaceView;
  state: RuntimeState;
  selectedChapterId: string;
  onSelectChapter: (chapter: Chapter) => void;
  onGeneratePlanning: (operation: "generateBrief" | "generateOutline" | "generatePlan", prompt: string) => void;
  onReviewPlanning: (request: PlanningReviewStartRequest) => Promise<void>;
  onCancelPlanningReview: (runId: string) => Promise<void>;
  onGenerateChapter: (chapter: Chapter, intent: string, options: { povCharacterId: string | null; storyTime: number | null }) => Promise<void>;
  onOpenPlanningTarget: (kind: "outline" | "plan") => void;
  onReturnToFeedback: () => void;
  onFormalChange: () => Promise<void>;
  onDirtyChange: (dirty: boolean) => void;
  onSaveHandlerChange?: (handler: (() => Promise<boolean>) | null) => void;
  onDiscardHandlerChange?: (handler: (() => void) | null) => void;
  agentAvailable: boolean;
  agentRunning: boolean;
  refreshToken: number;
  planningIntentSeed?: string;
}

const labels: Record<WorkspaceView, { title: string; description: string; agentPrompt: string }> = {
  brief: { title: "创作简报", description: "这部作品承诺什么，读者会得到什么。", agentPrompt: "请基于当前故事设定与已有内容，为这部作品生成一份可审阅的创作简报候选。" },
  outline: { title: "长篇大纲", description: "按全书、卷和单元安排故事。", agentPrompt: "请读取当前创作简报和故事设定，生成结构清晰、可审阅的长篇大纲候选。" },
  plans: { title: "近章规划", description: "只规划接下来要写的几章。", agentPrompt: "请基于正式大纲、当前事实状态和最近正文，滚动规划接下来三章并保存为候选。" },
  chapters: { title: "章节", description: "直接写这一章的正文。", agentPrompt: "请读取当前章节计划和时态上下文，继续写当前章节并保存。" },
};
const DEFAULT_GENERATION_INTENT = "根据当前正式前文和章节计划，写出完整、连贯的章节候选；保留已有悬念和人物信息边界。";

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
  return workspace.reviews.find((review) => review.artifact_kind === kind && review.artifact_revision_id === revisionId && review.status === "ready");
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

export function CreativeWorkspaceView({ api, projectId, view, state, selectedChapterId, onSelectChapter, onGeneratePlanning, onReviewPlanning, onCancelPlanningReview, onGenerateChapter, onOpenPlanningTarget, onReturnToFeedback, onFormalChange, onDirtyChange, onSaveHandlerChange, onDiscardHandlerChange, agentAvailable, agentRunning, refreshToken, planningIntentSeed = "" }: Props) {
  const [workspace, setWorkspace] = useState<CreativeWorkspace | null>(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [busyAction, setBusyAction] = useState("");
  const [dirty, setDirty] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [brief, setBrief] = useState({ premise: "", promise: "", stakes: "", endingDirection: "" });
  const [outlineNodes, setOutlineNodes] = useState<OutlineNode[]>(() => [newOutlineRoot()]);
  const [plans, setPlans] = useState<EditableChapterPlan[]>(() => [newPlanRow(1)]);
  const [draftSceneRows, setDraftSceneRows] = useState<NonNullable<CreativeWorkspace["chapters"][number]["candidate"]>["sceneGroups"]>([]);
  const draft = draftSceneRows.map((scene) => scene.body).join("\n\n---\n\n");
  const [newChapterTitle, setNewChapterTitle] = useState("第一章");
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
  const [reviewStateIdentity, setReviewStateIdentity] = useState("");
  const [lockedSceneGroups, setLockedSceneGroups] = useState<string[]>([]);
  const [generationIntent, setGenerationIntent] = useState(DEFAULT_GENERATION_INTENT);
  const [planningIntent, setPlanningIntent] = useState("");
  const [conflict, setConflict] = useState<{ baseCandidateRevisionId: string | null; serverCandidateRevisionId: string | null; baseContent: string } | null>(null);
  const [generating, setGenerating] = useState(false);
  const [associationPlanId, setAssociationPlanId] = useState("");
  const [impactCursor, setImpactCursor] = useState<string | null>(null);
  const [impactCounts, setImpactCounts] = useState({ total: 0, open: 0, retained: 0, lockedConflict: 0 });
  const [writerPreviewState, setWriterPreviewState] = useState<{ identity: string; value: WriterContextPreview } | null>(null);
  const [feedbackTarget, setFeedbackTarget] = useState<{ impactId: string; targetKind: "outline" | "plan"; targetNodeId: string; targetLabel: string } | null>(null);
  const reviewRef = useRef<HTMLElement | null>(null);
  const lastSavedRevisionIdRef = useRef<string | null>(null);
  const appliedContinuationRef = useRef<string | null>(null);
  const draftEditorRef = useRef<HTMLTextAreaElement | null>(null);
  const impactRefs = useRef(new Map<string, HTMLElement>());
  const dirtyRef = useRef(false);
  const reviewDirtyRef = useRef(false);
  const loadedProjectRef = useRef<string | null>(null);
  const editorSessionRef = useRef<EditorSession | null>(null);
  const editorUiStateRef = useRef<EditorUiSession>({ planningIntent: "", planningIntentSeed, generationIntent: DEFAULT_GENERATION_INTENT, draftScrollTop: 0, draftSelectionStart: 0, draftSelectionEnd: 0 });
  const appliedIntentSeedRef = useRef(planningIntentSeed);

  const selectedChapter = state.novelEdition.chapters.find((chapter) => chapter.id === selectedChapterId) ?? state.novelEdition.chapters[0];
  const workspaceChapter = workspace?.chapters.find((chapter) => chapter.id === selectedChapter?.id);
  const latestChapterTask = workspace?.tasks.find((task) => task.target?.chapterId === selectedChapter?.id && task.purpose === "writer");
  const viewKind: CreativeArtifactKind | "draft" = view === "plans" ? "plan" : view === "chapters" ? "draft" : view;
  const latestPlanningTask = viewKind === "draft" ? undefined : workspace?.tasks.find((task) => task.task_kind === viewKind && task.purpose === "planner");
  const artifact = viewKind === "draft" ? null : workspace?.artifacts[viewKind];
  const planningReviewRevision = view === "outline" || view === "plans"
    ? artifact?.candidate ?? artifact?.working ?? artifact?.accepted ?? null
    : null;
  const planningReviewOutlineNodes = view === "outline" && Array.isArray(planningReviewRevision?.payload.nodes)
    ? planningReviewRevision.payload.nodes as OutlineNode[]
    : [];
  const artifactLocked = viewKind !== "draft" && artifact?.locked === true;
  const candidateId = viewKind === "draft" ? workspaceChapter?.candidate?.id : artifact?.candidate?.id;
  const workingSelection = viewKind === "draft" ? workspaceChapter?.workingSelection : artifact?.workingSelection;
  const workingRevision = viewKind === "draft" ? workspaceChapter?.working : artifact?.working;
  const candidateIsWorking = Boolean(candidateId && workingSelection?.revisionId === candidateId);
  const currentReview = workspace && candidateId ? reviewFor(workspace, viewKind, candidateId) : undefined;
  const reviewSessionKey = `novel-codex:review-ui:${projectId}:${selectedChapter?.id ?? "none"}:${currentReview ? `review:${currentReview.id}` : candidateId ? `candidate:${candidateId}` : "empty"}`;
  const reviewFacts = useMemo(() => workspace?.factProposals.filter((fact) => fact.draft_revision_id === candidateId) ?? [], [workspace, candidateId]);
  const provisionalImpacts = useMemo(() => provisionalImpactsFor(currentReview), [currentReview]);
  const acceptedPlanChapters = Array.isArray(workspace?.artifacts.plan.accepted?.payload.chapters) ? workspace.artifacts.plan.accepted.payload.chapters as ChapterPlanPayload[] : [];
  const workingPlanChapters = Array.isArray(workspace?.artifacts.plan.working?.payload.chapters) ? workspace.artifacts.plan.working.payload.chapters as ChapterPlanPayload[] : [];
  const boundPlanIds = new Set(workspace?.chapterPlanBindings?.map((binding) => binding.chapter_plan_id) ?? []);
  const chapterBinding = workspace?.chapterPlanBindings?.find((binding) => binding.chapter_id === selectedChapter?.id);
  const chapterPlan = workingPlanChapters.find((entry) => entry.chapterPlanId === chapterBinding?.chapter_plan_id) ?? chapterBinding?.writerProjection ?? acceptedPlanChapters.find((entry) => entry.chapterPlanId === chapterBinding?.chapter_plan_id);
  const previewSceneOrder = 2_147_483_647;
  const workingSourceIdentity = workspace?.workingBaselines.map((item) => `${item.objectKind}:${item.objectId}:${item.revisionId}:${item.selectionRevision}`).join("|") ?? "";
  const writerPreviewIdentity = view === "chapters" && workspace && selectedChapter
    ? `${projectId}:${workspace.revision}:${selectedChapter.id}:${workingSourceIdentity}:${chapterPlan?.povCharacterId ?? ""}:${chapterPlan?.storyTime ?? ""}`
    : null;
  const writerPreview = valueForIdentity(writerPreviewState, writerPreviewIdentity);
  const previewTargetSource = writerPreview?.sourceExcerpts.find((source) => source.relationship === "targetDraft");
  const previewReferenceSources = writerPreview?.sourceExcerpts.filter((source) => source.relationship !== "targetDraft") ?? [];
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
  const activeIdentityRef = useRef(editorIdentity);
  activeIdentityRef.current = editorIdentity;
  const hasPublicWriterGuidance = Boolean(
    writerPreview?.planningContext?.chapter?.writerGuidance?.trim()
    || writerPreview?.planningContext?.chapter?.publicCommitments?.some((commitment) => commitment.trim())
    || writerPreview?.planningContext?.outlinePath.some((node) => node.writerGuidance.trim()),
  );
  editorUiStateRef.current = {
    planningIntent,
    planningIntentSeed: appliedIntentSeedRef.current,
    generationIntent,
    draftScrollTop: draftEditorRef.current?.scrollTop ?? editorUiStateRef.current.draftScrollTop,
    draftSelectionStart: draftEditorRef.current?.selectionStart ?? editorUiStateRef.current.draftSelectionStart,
    draftSelectionEnd: draftEditorRef.current?.selectionEnd ?? editorUiStateRef.current.draftSelectionEnd,
  };

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
    removeReviewUi(reviewSessionKey);
    const fallback = emptyReviewUi(workspaceChapter?.candidate?.sceneGroups.map((group) => group.id) ?? []);
    restoreReviewUi(fallback);
    reviewDirtyRef.current = false;
    setReviewDirty(false);
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
    return revision ? "body" in revision ? revision.body : JSON.stringify(revision.payload, null, 2) : "（当时没有候选版本）";
  }

  function loadEditor(next: CreativeWorkspace) {
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
      const groups = revision?.sceneGroups ?? [];
      setDraftSceneRows(groups.length ? groups : [newDraftScene()]);
      setLockedSceneGroups(groups.filter((group) => group.locked).map((group) => group.id));
    }
    dirtyRef.current = false;
    setDirty(false);
  }

  async function reload() {
    if (!api) return;
    const next = await api.runtime.getCreativeWorkspace(projectId);
    const impactPage = await api.runtime.listPlanningImpacts(projectId, null, 25);
    if (activeIdentityRef.current !== editorIdentity) return;
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
    if (!projectChanged && dirtyRef.current) {
      setLoading(true);
      void Promise.all([api?.runtime.getCreativeWorkspace(projectId), api?.runtime.listPlanningImpacts(projectId, null, 25)]).then(([next, impactPage]) => {
        if (!cancelled && next && impactPage) {
          setWorkspace({ ...next, impacts: impactPage.data, openImpactCount: impactPage.counts.open });
          setImpactCursor(impactPage.nextCursor);
          setImpactCounts(impactPage.counts);
          const session = editorSessionRef.current;
          const serverCandidateRevisionId = candidateRevisionId(next);
          if (session?.identity === editorIdentity && serverCandidateRevisionId !== session.baseCandidateRevisionId) {
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
    if (!api || !agentRunning || dirty) return;
    let cancelled = false;
    const refreshGeneration = async () => {
      try {
        const next = await api.runtime.getCreativeWorkspace(projectId);
        if (!cancelled) setWorkspace((current) => current ? { ...next, impacts: current.impacts, openImpactCount: current.openImpactCount } : next);
      } catch { /* The completion refresh reports persistent Runtime failures. */ }
    };
    void refreshGeneration();
    const timer = window.setInterval(() => void refreshGeneration(), 800);
    return () => { cancelled = true; window.clearInterval(timer); };
  }, [api, agentRunning, dirty, projectId]);

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
  }, [workspace, editorIdentity, dirty]);

  useEffect(() => {
    if (!chapterBinding) setAssociationPlanId(availableAssociationPlans[0]?.chapterPlanId ?? "");
  }, [chapterBinding?.chapter_plan_id, selectedChapter?.id, availableAssociationPlans.map((entry) => entry.chapterPlanId).join("|")]);

  useEffect(() => {
    let cancelled = false;
    if (!api || view !== "chapters" || !workspace || !selectedChapter) {
      setWriterPreviewState(null);
      return;
    }
    const identity = writerPreviewIdentity;
    if (!identity) return;
    setWriterPreviewState(null);
    void api.runtime.getWriterPreview(projectId, workspace.revision, selectedChapter.number, previewSceneOrder, chapterPlan?.povCharacterId ?? null, chapterPlan?.storyTime ?? null).then((preview) => {
      if (!cancelled) setWriterPreviewState({ identity, value: preview });
    }).catch((reason) => {
      if (!cancelled) { setWriterPreviewState(null); setError(reason instanceof Error ? reason.message : "创作依据加载失败"); }
    });
    return () => { cancelled = true; };
  }, [api, writerPreviewIdentity]);

  useEffect(() => {
    setFeedbackTarget(null);
  }, [projectId, selectedChapter?.id]);

  useEffect(() => {
    const cached = editorUiSessions.get(editorIdentity);
    appliedIntentSeedRef.current = planningIntentSeed;
    if (!cached) {
      setPlanningIntent(planningIntentSeed);
      setGenerationIntent(DEFAULT_GENERATION_INTENT);
    } else {
      setPlanningIntent(cached.planningIntentSeed === planningIntentSeed ? cached.planningIntent : planningIntentSeed);
      setGenerationIntent(cached.generationIntent);
    }
    const frame = requestAnimationFrame(() => {
      if (!draftEditorRef.current || !cached) return;
      draftEditorRef.current.scrollTop = cached.draftScrollTop;
      draftEditorRef.current.setSelectionRange(cached.draftSelectionStart, cached.draftSelectionEnd);
    });
    return () => {
      cancelAnimationFrame(frame);
      editorUiSessions.set(editorIdentity, { ...editorUiStateRef.current });
    };
  }, [editorIdentity, planningIntentSeed]);

  useEffect(() => {
    if (!workspace || view !== "chapters" || !selectedChapter) return;
    const continuation = workspace.continuationIntents.find(
      (item) => item.targetKind === "draft" && item.targetId === selectedChapter.id
    );
    if (!continuation || appliedContinuationRef.current === continuation.updatedAt) return;
    appliedContinuationRef.current = continuation.updatedAt;
    setGenerationIntent(continuation.authorIntent);
  }, [workspace?.continuationIntents, view, selectedChapter?.id]);

  useEffect(() => {
    document.documentElement.dataset.creativeWorkspace = "true";
    return () => { delete document.documentElement.dataset.creativeWorkspace; };
  }, []);

  useEffect(() => {
    dirtyRef.current = dirty;
    reviewDirtyRef.current = reviewDirty;
    onDirtyChange(dirty || reviewDirty);
    return () => onDirtyChange(false);
  }, [dirty, reviewDirty, onDirtyChange]);

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
    if (currentReview) reviewRef.current?.focus();
  }, [currentReview?.id]);

  useEffect(() => {
    if (view !== "chapters" || loading || !selectedChapter) return;
    const frame = requestAnimationFrame(() => {
      draftEditorRef.current?.scrollIntoView({ block: "start" });
      draftEditorRef.current?.focus({ preventScroll: true });
    });
    return () => cancelAnimationFrame(frame);
  }, [view, loading, selectedChapter?.id]);

  useEffect(() => {
    if (!feedbackTarget || loading) return;
    const frame = requestAnimationFrame(() => {
      if (view === "plans" && feedbackTarget.targetKind === "plan") {
        const card = document.querySelector<HTMLElement>(`[data-planning-node-id="${feedbackTarget.targetNodeId}"]`);
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

  async function perform(actionName: string, action: () => Promise<void>, success: string): Promise<boolean> {
    setBusy(true);
    setBusyAction(actionName);
    setError(null);
    setNotice(null);
    try {
      await action();
      await reload();
      setNotice(success);
      return true;
    } catch (reason) {
      const message = reason instanceof Error ? reason.message : "操作失败";
      setError(message);
      if (/revision|版本|candidate|候选|conflict|冲突/i.test(message)) {
        const session = editorSessionRef.current;
        try {
          const [next, impactPage] = await Promise.all([
            api?.runtime.getCreativeWorkspace(projectId),
            api?.runtime.listPlanningImpacts(projectId, null, 25),
          ]);
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
          setConflict({
            baseCandidateRevisionId: session?.baseCandidateRevisionId ?? null,
            serverCandidateRevisionId: workspace ? candidateRevisionId(workspace) : null,
            baseContent: session?.baseContent ?? "（基线不可用）",
          });
        }
      }
      return false;
    } finally {
      setBusy(false);
      setBusyAction("");
    }
  }

  async function publishChapterRevision(revisionId: string, sceneGroupIds: string[], proposals: Record<string, unknown>[] = []) {
    if (!api || sceneGroupIds.length === 0) return;
    const review = await api.runtime.createReview(projectId, "draft", revisionId, proposals);
    const factDecisions = proposals.length > 0 ? Object.fromEntries(review.factProposals.map((fact) => [fact.id, "confirmed"])) : {};
    await api.runtime.acceptReview(projectId, review.id, review.projectRevision, {
      sceneGroupIds,
      factDecisions,
      noConfirmedChanges: proposals.length === 0,
      factDependencyLinks: [],
      directionChangeDecisions: {},
    });
  }

  async function saveCurrent(baselineWorkspace?: CreativeWorkspace): Promise<boolean> {
    const activeWorkspace = baselineWorkspace ?? workspace;
    if (!api || !activeWorkspace) return false;
    const session = editorSessionRef.current;
    if (!session || session.identity !== editorIdentity) throw new Error("编辑会话已切换，请重新打开当前内容");
    const expectedRevision = session.baseProjectRevision;
    const expectedCandidateRevisionId = session.baseCandidateRevisionId;
    if (view === "brief") {
      return perform("正在保存创作简报…", () => api.runtime.saveArtifact(projectId, "brief", brief, expectedRevision, expectedCandidateRevisionId).then((saved) => { lastSavedRevisionIdRef.current = saved.revisionId; }), "创作简报已保存为候选版本。");
    } else if (view === "outline") {
      const firstInvalid = firstOutlineEditorError(outlineNodes);
      if (firstInvalid) {
        setOutlineError(firstInvalid);
        setError("大纲还有未完成字段；已定位到第一处错误。");
        return false;
      }
      setOutlineError(null);
      const linkedImpactIds = feedbackTarget?.targetKind === "outline" ? [feedbackTarget.impactId] : [];
      return perform("正在保存层级大纲…", () => api.runtime.saveArtifact(projectId, "outline", { nodes: outlineNodes }, expectedRevision, expectedCandidateRevisionId, linkedImpactIds).then((saved) => { lastSavedRevisionIdRef.current = saved.revisionId; }), linkedImpactIds.length ? "层级大纲候选已链接原反馈；采用后将自动关闭该反馈。" : "层级大纲已保存为候选版本。");
    } else if (view === "plans") {
      const outline = activeWorkspace.artifacts.outline.working ?? activeWorkspace.artifacts.outline.accepted;
      if (!outline) throw new Error("请先保存并选用层级大纲，再保存近章规划");
      const defaultNodeId = (outline.payload.nodes as OutlineNode[]).at(-1)?.id;
      const payload: RollingPlanPayload = { sourceOutlineRevisionId: outline.id, chapters: plans.map(({ id, dependencyKeys, ...chapter }) => ({ ...chapter, dependencyKeys: dependencyKeys.split(",").map((key) => key.trim()).filter(Boolean), chapterPlanId: chapter.chapterPlanId || undefined, clientKey: chapter.chapterPlanId ? undefined : id, primaryNodeId: chapter.primaryNodeId || defaultNodeId || "", publicTitle: chapter.publicTitle ?? "", writerGuidance: chapter.writerGuidance ?? "", publicCommitments: chapter.publicCommitments ?? [], allowances: chapter.allowances ?? [] })), window: plans.length };
      const linkedImpactIds = feedbackTarget?.targetKind === "plan" ? [feedbackTarget.impactId] : [];
      return perform("正在保存近章规划…", () => api.runtime.saveArtifact(projectId, "plan", payload, expectedRevision, expectedCandidateRevisionId, linkedImpactIds).then((saved) => { lastSavedRevisionIdRef.current = saved.revisionId; }), linkedImpactIds.length ? "近章规划候选已链接原反馈；采用后将自动关闭该反馈。" : "近章规划已保存为候选版本。");
    } else {
      if (draftSceneRows.some((scene) => !scene.body.trim())) {
        setError("请填写空场景正文，或删除不需要的空场景。");
        return false;
      }
      const hasFactDraft = Boolean(factPredicate.trim() || factEvidence.trim());
      if (hasFactDraft && (!factPredicate.trim() || !factEvidence.trim())) {
        setError("新增事实必须同时填写事实键和正文证据。");
        return false;
      }
      let parsedValue: unknown = null;
      if (hasFactDraft && factValue.trim()) {
        try { parsedValue = parseFactValue(factValue); } catch (reason) {
          setError(reason instanceof Error ? reason.message : "事实值格式无效");
          return false;
        }
      }
      const sceneGroups = draftSceneRows.map((scene) => ({ id: scene.id, sceneId: scene.sceneId ?? null, body: scene.body, locked: lockedSceneGroups.includes(scene.id) }));
      const normalizedBody = sceneGroups.map((scene) => scene.body.trim()).join("\n\n---\n\n");
      const factSceneOrder = workspaceChapter?.candidate?.sceneGroups.findIndex((group) => group.id === factSceneGroupId) ?? -1;
      const factScene = sceneGroups.find((group) => group.id === factSceneGroupId) ?? sceneGroups[Math.max(0, factSceneOrder)];
      const proposals = hasFactDraft ? [{ sceneGroupId: factScene?.id ?? sceneGroups[0]?.id, sceneId: factScene?.sceneId ?? null, factKind, subjectId: factSubjectId.trim() || null, predicate: factPredicate.trim(), value: parsedValue, epistemicOwnerId: factEpistemicOwnerId.trim() || null, epistemicState: factEpistemicState, storyTime: chapterPlan?.storyTime ?? null, readingChapter: selectedChapter?.number ?? 1, sceneOrder: Math.max(1, (factScene ? sceneGroups.findIndex((group) => group.id === factScene.id) : factSceneOrder) + 1), evidence: factEvidence.trim() }] : [];
      const published = await perform("正在保存这一章…", async () => {
        const saved = await api.runtime.saveDraftCheckpoint(projectId, { ...(selectedChapter ? { chapterId: selectedChapter.id } : { chapterNumber: 1 }), title: selectedChapter?.title ?? newChapterTitle, body: normalizedBody, sceneGroups }, expectedRevision, expectedCandidateRevisionId);
        lastSavedRevisionIdRef.current = saved.revisionId;
        await publishChapterRevision(saved.revisionId, sceneGroups.map((scene) => scene.id), proposals);
        await onFormalChange();
      }, "这一章已保存。");
      if (published && proposals.length > 0) {
        setFactPredicate("");
        setFactEvidence("");
        setFactValue("");
        setFactSubjectId("");
        setFactEpistemicOwnerId("");
      }
      return published;
    }
  }

  async function savePendingChanges(): Promise<boolean> {
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

  useEffect(() => {
    if (!onDiscardHandlerChange) return;
    onDiscardHandlerChange(discardCurrentReviewUi);
    return () => onDiscardHandlerChange(null);
  });

  async function enterReview() {
    if (!api || !candidateId) return;
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
    const created = await perform("正在生成审阅包…", () => api.runtime.createReview(projectId, viewKind, candidateId, proposals).then(() => undefined), "审阅包已生成，请检查后决定是否采用。");
    if (created) {
      removeReviewUi(reviewSessionKey);
      restoreReviewUi(emptyReviewUi(workspaceChapter?.candidate?.sceneGroups.map((group) => group.id) ?? []));
      reviewDirtyRef.current = false;
      setReviewDirty(false);
    }
  }

  async function toggleArtifactLock() {
    if (!api || !workspace || viewKind === "draft" || !artifact?.accepted) return;
    const nextLocked = !artifact.locked;
    await perform(
      nextLocked ? "正在锁定正式产物…" : "正在解锁正式产物…",
      async () => {
        await api.runtime.setArtifactLock(projectId, viewKind, nextLocked, workspace.revision);
        await onFormalChange();
      },
      nextLocked ? "正式版本已锁定，不能再生成覆盖候选。" : "正式版本已解锁，可以继续修订。",
    );
  }

  async function acceptCurrent() {
    if (!api || !workspace || !currentReview) return;
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
            const value = manualDirectionDecisions[manualDecisionKey(fact.id, target.key)];
            if (value?.decision === "false-positive") resolvedDirectionDecisions[impactId] = { decision: "false-positive", note: value.note.trim() };
            else if (value?.decision) resolvedDirectionDecisions[impactId] = value.decision;
          }
        }
      }
      if (Object.values(resolvedDirectionDecisions).some((value) => value === "return-draft")) {
        setError(null);
        setNotice("草稿已按你的决定退回修改，正式正文和事实均未推进。候选与本地编辑仍保留。");
        draftEditorRef.current?.scrollIntoView({ block: "center" });
        draftEditorRef.current?.focus({ preventScroll: true });
        return;
      }
    }
    const selections = viewKind === "draft" ? { sceneGroupIds: selectedSceneGroups, factDecisions, noConfirmedChanges, factDependencyLinks, directionChangeDecisions: resolvedDirectionDecisions } : {};
    const accepted = await perform("正在采用候选版本…", async () => {
      await api.runtime.acceptReview(projectId, currentReview.id, workspace.revision, selections);
      await onFormalChange();
      if (viewKind !== "draft") setFeedbackTarget(null);
    }, "已采用候选版本，作品正式版本已推进。");
    if (accepted && viewKind === "draft") {
      removeReviewUi(reviewSessionKey);
      removeReviewUi(`novel-codex:review-ui:${projectId}:${selectedChapter?.id ?? "none"}:candidate:${candidateId}`);
      restoreReviewUi(emptyReviewUi());
      reviewDirtyRef.current = false;
      setReviewDirty(false);
    }
  }

  async function undoLatest() {
    const latest = workspace?.commits[0];
    if (!api || !workspace || !latest || latest.operation_kind === "undo") return;
    await perform("正在撤销最近采用…", async () => {
      await api.runtime.undoCommit(projectId, latest.id, workspace.revision);
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

  async function materializeStandalone() {
    if (!api || !workspace || dirty) return;
    await perform("正在创建独立章节…", async () => {
      await api.runtime.materializeStandaloneChapter(projectId, newChapterTitle, workspace.revision);
      await onFormalChange();
    }, "独立章节已创建；它暂时没有上级规划，可稍后关联。 ");
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
    if (!api || !workspace || !impactCursor || busy) return;
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
  const canAccept = Boolean(currentReview) && !dirty && reviewChecksPass && (viewKind !== "draft" || (selectedSceneGroups.length > 0 && factsComplete && automaticLockedDecisionsComplete && manualLockedDecisionsComplete));
  const returnsDraft = provisionalImpacts.some((impact) => confirmedFactIds.has(impact.factProposalId) && directionChangeDecisions[impact.id]?.decision === "return-draft")
    || selectedReviewFacts.some((fact) => confirmedFactIds.has(fact.id) && (factDependencyTargets[fact.id] ?? []).some((targetKey) => manualDirectionDecisions[manualDecisionKey(fact.id, targetKey)]?.decision === "return-draft"));
  const latestTaskStatus = latestChapterTask ? generationStatusLabel(latestChapterTask) : null;
  const draftInteractionLocked = busy || generating || agentRunning;
  const info = labels[view];
  const planningPrompt = planningIntent.trim() || info.agentPrompt;
  const planningInputError = useMemo(() => authorIntentError(planningPrompt), [planningPrompt]);
  const chapterInputError = useMemo(() => authorIntentError(generationIntent), [generationIntent]);

  function updatePlan(index: number, change: Partial<(typeof plans)[number]>) {
    setPlans((rows) => rows.map((row, rowIndex) => rowIndex === index ? { ...row, ...change } : row));
    setDirty(true);
  }

  async function generateSelectedChapter() {
    if (!selectedChapter || !generationIntent.trim() || Boolean(chapterInputError) || draftInteractionLocked) return;
    const hadUnsavedChanges = dirty;
    if (hadUnsavedChanges && !(await saveCurrent())) return;
    const revisionToKeep = hadUnsavedChanges
      ? lastSavedRevisionIdRef.current
      : candidateId;
    if (!api || !revisionToKeep) {
      setError("请先保存当前章节，再继续创作。");
      return;
    }
    setGenerating(true);
    setError(null);
    setNotice(null);
    try {
      const currentWorkspace = await api.runtime.getCreativeWorkspace(projectId);
      const currentChapter = currentWorkspace.chapters.find(
        (chapter) => chapter.id === selectedChapter.id
      );
      if (currentChapter?.workingSelection?.revisionId !== revisionToKeep) {
        await api.runtime.setWorkingBaseline(
          projectId,
          "draft",
          selectedChapter.id,
          revisionToKeep,
          currentChapter?.workingSelection?.selectionRevision ?? 0,
        );
      }
      await api.runtime.saveContinuationIntent(
        projectId,
        "draft",
        selectedChapter.id,
        revisionToKeep,
        generationIntent.trim(),
      );
      await onGenerateChapter(selectedChapter, generationIntent.trim(), {
        povCharacterId: chapterPlan?.povCharacterId ?? null,
        storyTime: chapterPlan?.storyTime ?? null,
      });
      const refreshed = await api.runtime.getCreativeWorkspace(projectId);
      const written = refreshed.chapters.find((chapter) => chapter.id === selectedChapter.id);
      const candidate = written?.candidate;
      if (candidate && candidate.id !== written?.accepted?.id && candidate.sceneGroups.length > 0) {
        await publishChapterRevision(candidate.id, candidate.sceneGroups.map((group) => group.id));
        await onFormalChange();
      }
      await reload();
      setNotice(`第 ${selectedChapter.number} 章已写入。`);
    } catch (reason) {
      await reload().catch(() => undefined);
      setError(reason instanceof Error ? reason.message : "章节生成未完成；已写的正文仍会保留。");
    } finally {
      setGenerating(false);
    }
  }

  async function toggleWorkingBaseline() {
    if (!api || !workspace || !candidateId) return;
    const objectId = viewKind === "draft" ? selectedChapter?.id : viewKind;
    if (!objectId) return;
    const expectedSelectionRevision = workingSelection?.selectionRevision ?? 0;
    const changed = await perform(
      candidateIsWorking ? "正在取消工作依据…" : "正在选用工作依据…",
      () => candidateIsWorking
        ? api.runtime.clearWorkingBaseline(projectId, viewKind, objectId, expectedSelectionRevision).then(() => undefined)
        : api.runtime.setWorkingBaseline(projectId, viewKind, objectId, candidateId, expectedSelectionRevision).then(() => undefined),
      candidateIsWorking ? "已取消选用；后续任务将回到正式来源。" : "已选用该版本；后续任务会冻结这份修订。",
    );
    if (changed) await onFormalChange();
  }

  async function clearWorkingBaseline() {
    if (!api || !workspace || !workingSelection?.revisionId) return;
    const objectId = viewKind === "draft" ? selectedChapter?.id : viewKind;
    if (!objectId) return;
    const changed = await perform(
      "正在取消工作依据…",
      () => api.runtime.clearWorkingBaseline(projectId, viewKind, objectId, workingSelection.selectionRevision).then(() => undefined),
      "已取消选用；后续任务将回到正式来源。",
    );
    if (changed) await onFormalChange();
  }

  async function copyLocalEditor() {
    const local = view === "chapters" ? draft : view === "brief" ? JSON.stringify(brief, null, 2) : view === "outline" ? JSON.stringify({ nodes: outlineNodes }, null, 2) : JSON.stringify({ chapters: plans }, null, 2);
    try {
      await navigator.clipboard.writeText(local);
      setNotice("本地编辑已复制到剪贴板。");
    } catch {
      setError("无法访问剪贴板，请先在编辑器中全选复制。");
    }
  }

  async function saveLocalAsLatestCandidate() {
    if (!api || busy) return;
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

  if (!api) return <div className="creative-empty"><h2>{info.title}</h2><p>桌面 Runtime 连接后可使用结构化创作工作区。</p></div>;
  if (loading) return <div className="creative-workspace-view" aria-busy="true"><div className="creative-loading"><span className="loading-pulse" /><strong>正在加载{info.title}</strong><p>正在恢复候选、正式版本与审阅状态…</p></div></div>;
  if (error && !workspace) return <div className="creative-empty" role="alert"><h2>{info.title}暂时无法打开</h2><p>{error}</p><button className="secondary-button" onClick={() => { setLoading(true); setError(null); void reload(); }}>重新加载</button></div>;

  return (
    <div className="creative-workspace-view" aria-busy={busy}>
      <header className="creative-view-header">
        <div><span className="creative-kicker">创作</span><h2>{info.title}</h2><p>{info.description}</p></div>
        <div className="creative-header-actions"><span className="revision-badge">作品 v{workspace?.revision ?? "—"}</span>{viewKind !== "draft" && artifact?.accepted && <button className="secondary-button" disabled={busy || agentRunning} aria-pressed={artifact.locked} title={artifact.locked ? "解锁后可以继续修订" : "锁定后不能再生成覆盖"} onClick={() => void toggleArtifactLock()}>{artifact.locked ? "已锁定" : "锁定"}</button>}</div>
      </header>
      {error && <div className="creative-alert error" role="alert">{error}</div>}
      {notice && <div className="creative-alert success" role="status">{notice}</div>}
      {busyAction && <div className="creative-alert" role="status">{busyAction}</div>}
      {dirty && candidateId && <div className="creative-alert warning">{view === "chapters" ? "这一章还有未保存的修改。" : "还有未保存的修改。先保存，再进入审阅。"}</div>}
      {conflict && <div className="creative-alert warning editor-conflict" role="alert"><div><strong>服务器候选已变化，本地编辑仍被保留</strong><span>当前编辑基于 {conflict.baseCandidateRevisionId ?? "空候选"}，服务器现为 {conflict.serverCandidateRevisionId ?? "空候选"}。上方编辑器是本地版本，下方候选栏是服务器版本。</span><details><summary>查看本地编辑的基线版本</summary><pre>{conflict.baseContent}</pre></details></div><div className="editor-conflict-actions"><button className="primary-button" disabled={busy} onClick={() => void saveLocalAsLatestCandidate()}>以最新版为基线保存本地稿</button><button className="secondary-button" disabled={busy} onClick={() => void copyLocalEditor()}>复制本地稿</button><button className="secondary-button" disabled={busy} onClick={() => { if (workspace) loadEditor(workspace); }}>放弃本地稿并载入服务器版本</button></div></div>}
      {artifactLocked && <div className="creative-alert warning">正式版本已锁定。当前内容只读；解锁后才能创建新候选或进入审阅。</div>}
      {!agentAvailable && <div className="creative-alert">{view === "chapters" ? "当前未配置可用模型；仍可手动编辑并保存这一章。" : "当前未配置可用模型；仍可手动编辑并保存候选。"}</div>}
      {workspace?.index.status === "stale" && <div className="creative-alert warning">长篇索引正在追赶正式版本，检索结果可能暂时滞后。</div>}
      {workspace?.planLineageStatus === "stale" && <div className="creative-alert warning">正式大纲已经更新，当前章纲属于旧基线。请先在“近章规划”中基于新版大纲保存并采用新候选；旧章纲不会继续指导正文。</div>}
      {feedbackTarget && view !== "chapters" && <div className="feedback-return-bar"><span>正在处理反馈：{feedbackTarget.targetLabel}</span><button className="secondary-button" onClick={() => { onReturnToFeedback(); }}>返回原反馈</button></div>}

      {view === "brief" && <ArtifactEditor disabled={artifactLocked || busy || agentRunning} fields={[{ key: "premise", label: "核心命题", hint: "故事真正要讨论什么" }, { key: "promise", label: "读者承诺", hint: "读者持续阅读会得到什么体验" }, { key: "stakes", label: "核心代价", hint: "失败会失去什么" }, { key: "endingDirection", label: "结局方向", hint: "允许变化，但不能毫无方向" }]} value={brief} onChange={(key, value) => { setBrief((current) => ({ ...current, [key]: value })); setDirty(true); }} />}
      {view === "outline" && <OutlineTreeEditor disabled={artifactLocked || busy || agentRunning} error={outlineError} focusNodeId={outlineError?.nodeId ?? (feedbackTarget?.targetKind === "outline" ? feedbackTarget.targetNodeId : null)} nodes={outlineNodes} onChange={(nodes) => { setOutlineNodes(nodes); setOutlineError(null); setDirty(true); }} />}
      {view === "plans" && <fieldset className="plan-editor-list" disabled={artifactLocked || busy || agentRunning}>
        {acceptedPlanChapters.length > 0 && <details className="plan-materialization"><summary>已采用的章纲 · {acceptedPlanChapters.length}</summary><small>{workspace?.planLineageStatus === "stale" ? "章纲基线已过期，采用新版章纲后才能继续开写。" : "创建后才会出现可写的章节。还没开写的章可以继续改。"}</small>{acceptedPlanChapters.map((entry) => <div key={entry.chapterPlanId}><span>第 {entry.chapterNumber} 章 · {entry.publicTitle || entry.title || "未命名"}</span>{entry.chapterPlanId && <button type="button" className="secondary-button" disabled={dirty || workspace?.planLineageStatus !== "current" || boundPlanIds.has(entry.chapterPlanId)} onClick={() => void materializePlanChapter(entry.chapterPlanId!, entry.chapterNumber)}>{boundPlanIds.has(entry.chapterPlanId) ? "已创建" : "创建正文"}</button>}</div>)}</details>}
        {plans.map((item, index) => <div className="plan-editor-card" data-planning-node-id={item.chapterPlanId ?? item.id} key={item.id}>
          <div className="plan-card-head">
            <span className="plan-number">第 <input aria-label={`第 ${item.chapterNumber} 章序号`} type="number" min="1" value={item.chapterNumber} onChange={(event) => updatePlan(index, { chapterNumber: Number(event.target.value) })} /> 章</span>
            <input className="plan-title" aria-label={`第 ${item.chapterNumber} 章标题`} placeholder="章节标题" value={item.title} onChange={(event) => updatePlan(index, { title: event.target.value })} />
            <label className="plan-lock"><input type="checkbox" checked={item.locked === true} onChange={(event) => updatePlan(index, { locked: event.target.checked })} /><span>锁定</span></label>
          </div>
          <label className="plan-field"><span>本章目标</span><textarea aria-label={`第 ${item.chapterNumber} 章目标`} placeholder="这一章要完成什么" value={item.goal} onChange={(event) => updatePlan(index, { goal: event.target.value })} /></label>
          <label className="plan-field"><span>核心冲突</span><input aria-label={`第 ${item.chapterNumber} 章核心冲突`} placeholder="谁和什么对着干" value={item.conflict} onChange={(event) => updatePlan(index, { conflict: event.target.value })} /></label>
          <label className="plan-field"><span>正文标题</span><input aria-label={`第 ${item.chapterNumber} 章公开标题`} placeholder="写正文时使用的标题，可留空" value={item.publicTitle ?? ""} onChange={(event) => updatePlan(index, { publicTitle: event.target.value })} /></label>
          <label className="plan-field"><span>给正文的指引</span><textarea aria-label={`第 ${item.chapterNumber} 章写作指引`} placeholder="写这一章时需要知道的安排" value={item.writerGuidance ?? ""} onChange={(event) => updatePlan(index, { writerGuidance: event.target.value })} /></label>
          <details className="plan-advanced"><summary>承诺、余地和依据</summary>
            <label><span>必须兑现</span><input aria-label={`第 ${item.chapterNumber} 章公开承诺`} placeholder="以分号分隔" value={(item.publicCommitments ?? []).join("；")} onChange={(event) => updatePlan(index, { publicCommitments: event.target.value.split(/[；;]/).map((value) => value.trim()).filter(Boolean) })} /></label>
            <label><span>可以自由发挥</span><input aria-label={`第 ${item.chapterNumber} 章创作余地`} placeholder="以分号分隔" value={(item.allowances ?? []).join("；")} onChange={(event) => updatePlan(index, { allowances: event.target.value.split(/[；;]/).map((value) => value.trim()).filter(Boolean) })} /></label>
            {outlineForPlan && <label><span>服务的上级规划</span><select value={item.primaryNodeId} onChange={(event) => updatePlan(index, { primaryNodeId: event.target.value })}><option value="">请选择全书／卷／单元</option>{(outlineForPlan.payload.nodes as OutlineNode[]).map((node) => <option key={node.id} value={node.id}>{node.id ? outlinePaths.get(node.id) : undefined}</option>)}</select></label>}
            <label><span>故事时间</span><input aria-label={`第 ${item.chapterNumber} 章故事时间`} type="number" min="0" placeholder="未指定" value={item.storyTime ?? ""} onChange={(event) => updatePlan(index, { storyTime: event.target.value === "" ? null : Number(event.target.value) })} /></label>
            <label><span>依赖事实键</span><input aria-label={`第 ${item.chapterNumber} 章依赖事实`} placeholder="以逗号分隔" value={item.dependencyKeys} onChange={(event) => updatePlan(index, { dependencyKeys: event.target.value })} /></label>
            <label><span>视角人物</span><select aria-label={`第 ${item.chapterNumber} 章视角人物`} value={item.povCharacterId ?? ""} onChange={(event) => updatePlan(index, { povCharacterId: event.target.value || null })}><option value="">未指定</option>{item.povCharacterId && !state.canon.characters.some((character) => character.id === item.povCharacterId) && <option value={item.povCharacterId}>未知角色 · {item.povCharacterId}</option>}{state.canon.characters.map((character) => <option key={character.id} value={character.id}>{character.name} · {character.role}</option>)}</select></label>
            {item.povCharacterId && <small>稳定 ID：{item.povCharacterId}</small>}
          </details>
          <details className="plan-writer-note"><summary>写正文时会看到</summary><div aria-label={`第 ${item.chapterNumber} 章正文可见内容`}><span>{item.publicTitle || "（无公开标题）"}</span><p>{item.writerGuidance || "（无公开写作指引）"}</p><small>{(item.publicCommitments ?? []).length > 0 ? `公开承诺：${item.publicCommitments?.join("；")}` : "无公开承诺"}</small></div></details>
          {(item.scenes?.length ?? 0) > 0 && <div className="plan-scenes"><strong>场景与节拍</strong>{item.scenes?.map((scene) => <article key={scene.id}><div><b>{scene.title || scene.id}</b>{scene.locked && <span>已锁定</span>}</div><p>{scene.goal}</p>{scene.beats.map((beat) => <small key={beat.id}>· {beat.description}{beat.locked ? " · 已锁定" : ""}</small>)}</article>)}</div>}
        </div>)}
        <button className="text-button" type="button" onClick={() => { setPlans((rows) => [...rows, newPlanRow((rows.at(-1)?.chapterNumber ?? 0) + 1)]); setDirty(true); }}>＋ 添加下一章</button>
      </fieldset>}
      {viewKind !== "draft" && <section className="planning-intent-card">
        <label className="sr-only" htmlFor="planning-intent">本轮要改什么</label>
        <textarea id="planning-intent" aria-label="规划生成意图" value={planningIntent} placeholder="这一轮想调整什么……" onChange={(event) => setPlanningIntent(event.target.value)} disabled={busy || agentRunning} />
        {planningIntentSeed.trim() && <small>已带入最近一次构想。</small>}
        {planningInputError && <p role="alert">{planningInputError}</p>}
        <button className="primary-button" disabled={busy || agentRunning || dirty || artifactLocked || !agentAvailable || !planningPrompt || Boolean(planningInputError)} title={!agentAvailable ? "请先配置可用模型" : agentRunning ? "当前已有 Agent 任务正在执行" : dirty ? "请先保存当前修改" : "生成一份可审阅的规划，不会直接覆盖已采用的版本"} onClick={() => onGeneratePlanning(view === "brief" ? "generateBrief" : view === "outline" ? "generateOutline" : "generatePlan", planningPrompt)}>生成</button>
        {latestPlanningTask && <GenerationTaskStatus task={latestPlanningTask} outputName="规划稿" />}
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
      {view === "chapters" && (selectedChapter ? <div className="draft-workspace">
        <ChapterSwitcher chapters={state.novelEdition.chapters} selectedId={selectedChapter.id} disabled={draftInteractionLocked} onSelect={onSelectChapter} />
        <div className="draft-main-column">
        <DraftSceneEditor scenes={draftSceneRows} lockedIds={lockedSceneGroups} disabled={draftInteractionLocked} firstEditorRef={draftEditorRef} onChange={(scenes) => { setDraftSceneRows(scenes); dirtyRef.current = true; setDirty(true); }} />
        {draftInteractionLocked && <small role="status">{busy ? `${busyAction}完成后可继续编辑。` : "生成期间正文已锁定；停止或完成后可继续编辑。"}</small>}
        <section className="chapter-generation-card" aria-busy={generating}>
          <label className="sr-only" htmlFor={`chapter-intent-${selectedChapter.id}`}>续写这一章</label>
          <textarea id={`chapter-intent-${selectedChapter.id}`} aria-label={`第 ${selectedChapter.number} 章创作意图`} placeholder="这一章接下来要发生什么……" value={generationIntent} onChange={(event) => setGenerationIntent(event.target.value)} disabled={draftInteractionLocked} />
          {chapterInputError && <p role="alert">{chapterInputError}</p>}
          {!hasPublicWriterGuidance && <small className="planning-basis">当前没有公开规划指导；将按作者意图与已选工作来源创作。</small>}
          <button className="primary-button" disabled={draftInteractionLocked || !generationIntent.trim() || Boolean(chapterInputError)} title={agentRunning ? "当前已有 Agent 任务正在执行" : !agentAvailable ? "会先保存这一章；配置模型后可再次继续" : undefined} onClick={() => void generateSelectedChapter()}>{busy ? "正在保存…" : draftInteractionLocked ? "正在生成并保存…" : "保留并按此意图继续"}</button>
          {latestChapterTask && latestTaskStatus && <GenerationTaskStatus task={latestChapterTask} outputName="本章正文" />}
        </section>
        <details className="writer-runtime-preview"><summary>本次创作依据 · {chapterPlan ? `第 ${chapterPlan.chapterNumber} 章${chapterPlan.publicTitle ? ` · ${chapterPlan.publicTitle}` : ""}` : "独立章节"}</summary><div><strong>{writerPreview?.planningContext?.chapter?.publicTitle || "无公开标题"}</strong><small>{writerPreview?.planningContext?.lineageStatus === "working" ? "使用作者选定的工作规划" : writerPreview?.planningContext?.lineageStatus === "historical" ? "使用本章绑定的历史规划快照" : writerPreview?.planningContext ? "使用当前正式规划" : "无规划指导"}</small></div><p>{writerPreview?.planningContext?.chapter?.writerGuidance || "还没有给正文的公开写作指引。内部目标不会发给正文。"}</p>{(writerPreview?.planningContext?.outlinePath ?? []).length > 0 && <small>公开路径：{writerPreview?.planningContext?.outlinePath.map((node) => node.publicTitle || "无公开标题").join(" → ")}</small>}<small>{(writerPreview?.planningContext?.chapter?.publicCommitments ?? []).length > 0 ? `公开承诺：${writerPreview?.planningContext?.chapter?.publicCommitments?.join("；")}` : "无公开承诺"} · 视角：{writerPreview?.povCharacterId || "未指定"} · 范围：整章 · 故事时间：{writerPreview?.storyTime ?? "未指定"}</small>{previewTargetSource && <small>本次目标稿：第 {previewTargetSource.chapterNumber} 章 · {previewTargetSource.sourceRole === "workingSelected" ? "作者选定工作稿" : "正式稿"}</small>}{previewReferenceSources.length > 0 ? <details><summary>参考前文 · {previewReferenceSources.length} 项</summary>{previewReferenceSources.map((source) => <small key={source.draftRevisionId}>第 {source.chapterNumber} 章 · {source.sourceRole === "workingSelected" ? "作者选定工作稿" : "正式稿"}{source.truncated ? " · 已截取" : ""}</small>)}</details> : <small>未使用前文章节</small>}<small>上下文{writerPreview?.contextBudget.truncated ? "已截取" : "完整"} · 未带入事实 {writerPreview?.omissions.factCount ?? 0} 条</small></details>
        {!chapterPlan && <section className="standalone-planning-card"><div><strong>这是独立章节</strong><small>保持独立不会伪造大纲来源；也可以关联同章号且尚未开写的正式章纲。</small></div>{availableAssociationPlans.length > 0 ? <><select disabled={draftInteractionLocked} aria-label="可关联的正式章纲" value={associationPlanId} onChange={(event) => setAssociationPlanId(event.target.value)}>{availableAssociationPlans.map((entry) => <option key={entry.chapterPlanId} value={entry.chapterPlanId}>{entry.publicTitle || entry.title || `第 ${entry.chapterNumber} 章章纲`}</option>)}</select><button className="secondary-button" disabled={busy || draftInteractionLocked || dirty || !associationPlanId} onClick={() => void associateChapterPlan()}>关联章纲</button></> : <button className="secondary-button" disabled={draftInteractionLocked} onClick={() => onOpenPlanningTarget("plan")}>从正文建立章纲</button>}{workspaceChapter?.candidate && <small>关联会推进作品版本；已写的正文仍会保留。</small>}</section>}
        {draftSceneRows.length > 0 && <details className="chapter-extra"><summary>场景保护</summary><fieldset className="scene-lock-editor" disabled={draftInteractionLocked}><legend className="sr-only">场景保护</legend>{draftSceneRows.map((group, index) => <label key={group.id}><input type="checkbox" checked={lockedSceneGroups.includes(group.id)} onChange={(event) => { setLockedSceneGroups((current) => event.target.checked ? [...new Set([...current, group.id])] : current.filter((id) => id !== group.id)); setDirty(true); }} /><span><strong>场景 {index + 1}</strong><small>{group.body.slice(0, 72)}</small></span></label>)}</fieldset></details>}
        {lockedDraftConflict && <div className="creative-alert warning">锁定场景的正文已变化。请先恢复正文，只提交一次“取消保护”，保存后再修改。</div>}
        <details className="chapter-extra"><summary>记录本章新事实</summary><fieldset className="fact-proposal-form" disabled={draftInteractionLocked || dirty || !candidateId}>
          <h3 className="sr-only">本章新事实</h3>
          <small>保存这一章时，填写完整的事实会一起写入故事设定。</small>
          {(workspaceChapter?.candidate?.sceneGroups.length ?? 0) > 1 && <label><span>来源场景</span><select value={factSceneGroupId} onChange={(event) => { setFactSceneGroupId(event.target.value); markReviewDirty(); }}>{workspaceChapter?.candidate?.sceneGroups.map((group, index) => <option key={group.id} value={group.id}>场景 {index + 1}</option>)}</select></label>}
          <label><span>事实种类</span><select value={factKind} onChange={(event) => { setFactKind(event.target.value as FactKind); markReviewDirty(); }}><option value="world">世界事实</option><option value="reader">读者已知</option><option value="character">人物认知</option><option value="author-secret">作者秘密</option></select></label>
          <label><span>主体 ID（可选）</span><input value={factSubjectId} onChange={(event) => { setFactSubjectId(event.target.value); markReviewDirty(); }} placeholder="例如：linxi" /></label>
          <label><span>事实键</span><input value={factPredicate} onChange={(event) => { setFactPredicate(event.target.value); markReviewDirty(); }} placeholder="例如：knows-father-lied" /></label>
          <label><span>事实值（JSON）</span><input value={factValue} onChange={(event) => { setFactValue(event.target.value); markReviewDirty(); }} placeholder={'true、false、"秘密" 或 3'} /></label>
          <label><span>认知状态</span><select value={factEpistemicState} onChange={(event) => { setFactEpistemicState(event.target.value as EpistemicState); markReviewDirty(); }}><option value="unknown">未知</option><option value="suspected">怀疑</option><option value="known">已知</option><option value="false">已证伪</option></select></label>
          <label><span>认知持有者 ID（可选）</span><input value={factEpistemicOwnerId} onChange={(event) => { setFactEpistemicOwnerId(event.target.value); markReviewDirty(); }} placeholder="人物认知由谁持有" /></label>
          <label><span>正文证据</span><input value={factEvidence} onChange={(event) => { setFactEvidence(event.target.value); markReviewDirty(); }} placeholder="引用支持该事实的正文" /></label>
        </fieldset></details>
        </div>
      </div> : <div className="empty-chapter-editor"><div><h3>直接写章</h3><p>先创建一个独立章节，再手写或让引擎续写。它不会伪造上级规划，之后可再关联章纲。</p></div><label><span>章节标题</span><input disabled={agentRunning} value={newChapterTitle} onChange={(event) => setNewChapterTitle(event.target.value)} /></label><button className="primary-button" disabled={busy || agentRunning || !newChapterTitle.trim()} onClick={() => void materializeStandalone()}>创建独立章节</button></div>)}

      {view === "chapters" ? <div className="candidate-toolbar"><div><strong>{dirty ? "这一章还有未保存的修改" : "正在写这一章"}</strong></div><button className="primary-button" disabled={busy || draftInteractionLocked || lockedDraftConflict || !draft.trim()} onClick={() => void saveCurrent()}>保存章节</button></div> : <div className="candidate-toolbar"><div><strong>{artifactLocked ? "已锁定" : dirty ? "还有未保存的修改" : workingRevision ? `后续依据 v${workingRevision.version}` : "尚未采用"}</strong></div><button className="text-button" aria-pressed={candidateIsWorking} disabled={busy || draftInteractionLocked || dirty || !candidateId} onClick={() => void toggleWorkingBaseline()}>{candidateIsWorking ? "取消后续依据" : "用作后续依据"}</button>{workingRevision && !candidateIsWorking && <button className="text-button" disabled={busy || draftInteractionLocked || dirty} onClick={() => void clearWorkingBaseline()}>取消 v{workingRevision.version}</button>}<button className="secondary-button" disabled={busy || draftInteractionLocked || artifactLocked || lockedDraftConflict} onClick={() => void saveCurrent()}>保存</button><button className="primary-button" disabled={busy || draftInteractionLocked || artifactLocked || dirty || !candidateId} onClick={() => void enterReview()}>进入审阅</button></div>}

      {candidateId && view !== "chapters" && <details className="planning-compare"><summary>对照已采用的版本</summary><section className="revision-compare"><RevisionPane title="已采用" revision={artifact?.accepted} /><RevisionPane title="当前稿" revision={artifact?.candidate} /></section></details>}

      {currentReview && view !== "chapters" && <section className="review-panel" ref={reviewRef} tabIndex={-1}>
        <div><span className="creative-kicker">审阅</span><h3>采用前检查</h3></div>
        {currentReview.checks.map((check) => {
          const selectionAware = viewKind === "draft" && !check.passed && (check.id === "fact-evidence" || check.id === "fact-evidence-anchor");
          const passed = selectionAware
            ? selectionAwareReviewChecksPass([check], confirmedFacts, workspaceChapter?.candidate?.sceneGroups ?? [])
            : check.passed;
          const pending = check.id === "semantic-review" && !passed;
          return <div className={`review-check ${pending ? "pending" : passed ? "passed" : "failed"}`} key={check.id}><span>{pending ? "…" : passed ? "✓" : "!"}</span>{check.message}{selectionAware ? "；当前只检查将采用的已确认事实" : ""}</div>;
        })}
        {viewKind === "draft" && <fieldset className="scene-selection" disabled={draftInteractionLocked}><legend>选择要采用的场景</legend>{workspaceChapter?.candidate?.sceneGroups.map((group, index) => <label key={group.id}><input type="checkbox" checked={selectedSceneGroups.includes(group.id)} onChange={(event) => { setSelectedSceneGroups((current) => event.target.checked ? [...new Set([...current, group.id])] : current.filter((id) => id !== group.id)); markReviewDirty(); }} /><span><strong>场景 {index + 1}</strong><small>{group.body.slice(0, 120) || "空场景"}</small></span></label>)}</fieldset>}
        {viewKind === "draft" && reviewFacts.length === 0 && <label className="no-fact-decision"><input type="checkbox" disabled={draftInteractionLocked} checked={noConfirmedChanges} onChange={(event) => { setNoConfirmedChanges(event.target.checked); markReviewDirty(); }} /><span><strong>本章没有要确认的变化</strong><small>我已检查正文，本次采用不写入新的故事事实。</small></span></label>}
        {reviewFacts.map((fact) => {
          const sceneSelected = selectedSceneGroups.includes(fact.sceneGroupId);
          const factImpacts = provisionalImpacts.filter((impact) => impact.factProposalId === fact.id);
          const automaticTargetKeys = new Set(factImpacts.map((impact) => `${impact.targetArtifactKind}:${impact.targetNodeId}`));
          const supplementaryTargets = planningTargets.filter((target) => !automaticTargetKeys.has(`${target.kind}:${target.nodeId}`));
          const selectedManualTargets = planningTargets.filter((target) => (factDependencyTargets[fact.id] ?? []).includes(target.key));
          const confirmed = sceneSelected && factDecisions[fact.id] === "confirmed";
          return <article className={`fact-review-card ${sceneSelected ? "" : "disabled"}`} key={fact.id}>
            <header><div><small>{fact.source === "agent" ? "引擎建议" : "作者新增"} · {factKindLabel(fact.factKind)}</small><strong>{fact.predicate}</strong><small>主体：{fact.subjectId || "无"} · 值：{formatFactValue(fact.value)}</small><small>认知状态：{epistemicStateLabel(fact.epistemicState)} · 持有者：{fact.epistemicOwnerId || "无"}</small><p>{fact.evidence}</p><small>{sceneSelected ? "证据来自本次采用场景" : "所属场景未选择，不会写入正式事实"}</small></div><select disabled={!sceneSelected || draftInteractionLocked} aria-label={`${fact.predicate} 的审阅结论`} value={sceneSelected ? factDecisions[fact.id] ?? "" : "rejected"} onChange={(event) => { setFactDecisions((current) => ({ ...current, [fact.id]: event.target.value as "confirmed" | "rejected" })); markReviewDirty(); }}><option value="">请选择</option><option value="confirmed">确认事实</option><option value="rejected">拒绝</option></select></header>
            {factImpacts.length > 0 && <div className="provisional-impact-list"><strong>采用前影响预览</strong>{factImpacts.map((impact) => { const target = planningTargets.find((item) => item.kind === impact.targetArtifactKind && item.nodeId === impact.targetNodeId); const pathLabel = target?.label ?? impact.targetLabel; return <div className={`provisional-impact ${impact.severity}`} key={impact.id}><div><span>{impact.targetArtifactKind === "outline" ? "大纲" : "章纲"} · {pathLabel}</span>{target?.protected && <small>原安排：{target.currentArrangement}</small>}<small>正文变化：{impact.predicate}；证据：{impact.evidence || "未提供"}</small><small>影响范围：{pathLabel} · {impact.severity === "locked-conflict" ? "受保护，必须先决定" : "将创建待处理规划反馈"} · 自动匹配依赖</small></div>{confirmed && impact.severity === "locked-conflict" && <DirectionDecisionEditor disabled={draftInteractionLocked} value={directionChangeDecisions[impact.id]} onChange={(value) => { setDirectionChangeDecisions((current) => ({ ...current, [impact.id]: value })); markReviewDirty(); }} />}</div>; })}</div>}
            {supplementaryTargets.length > 0 && <fieldset className="manual-fact-link" disabled={!confirmed || draftInteractionLocked}><legend>补充关联遗漏的正式规划（可多选）</legend>{supplementaryTargets.map((target) => <label key={target.key}><input type="checkbox" checked={(factDependencyTargets[fact.id] ?? []).includes(target.key)} onChange={(event) => { setFactDependencyTargets((current) => ({ ...current, [fact.id]: event.target.checked ? [...new Set([...(current[fact.id] ?? []), target.key])] : (current[fact.id] ?? []).filter((key) => key !== target.key) })); markReviewDirty(); }} /><span>{target.kind === "outline" ? "大纲" : "章纲"} · {target.label} · {target.protectionLabel}</span></label>)}</fieldset>}
            {confirmed && selectedManualTargets.filter((target) => target.protected).map((target) => <div className="provisional-impact locked-conflict" key={target.key}><div><span>{target.label}</span><small>原安排：{target.currentArrangement}</small><small>正文变化：{fact.predicate}；证据：{fact.evidence || "未提供"}</small><small>影响范围：{target.label} · {target.protectionLabel}，必须先决定</small></div><DirectionDecisionEditor disabled={draftInteractionLocked} value={manualDirectionDecisions[manualDecisionKey(fact.id, target.key)]} onChange={(value) => { setManualDirectionDecisions((current) => ({ ...current, [manualDecisionKey(fact.id, target.key)]: value })); markReviewDirty(); }} /></div>)}
          </article>;
        })}
        <button className="primary-button" disabled={busy || draftInteractionLocked || !canAccept} onClick={() => void acceptCurrent()}>{returnsDraft ? "退回草稿并继续修改" : "采用并推进正式版本"}</button>
      </section>}

      {workspace && workspace.impacts.length > 0 && <section className="impact-panel">
        <div className="impact-heading"><div><h3>规划反馈</h3><small>正文采用不会自动改写大纲。打开目标并保存候选会建立精确链接，采用该候选后只关闭这条反馈。</small></div><strong>{impactCounts.open} 条待处理 · {impactCounts.lockedConflict} 条保护冲突 · {impactCounts.retained} 条知情保留</strong></div>
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
    </div>
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
    <button type="button" className="chapter-switcher-step" disabled={disabled || index === 0} onClick={() => choose(chapters[index - 1])}>上一章</button>
    <button type="button" className="chapter-switcher-current" aria-expanded={open} aria-haspopup="listbox" disabled={disabled} onClick={() => setOpen((value) => !value)}>
      <span>第 {String(selected.number).padStart(2, "0")} 章</span>
      <b>{selected.title}</b>
    </button>
    <button type="button" className="chapter-switcher-step" disabled={disabled || index === chapters.length - 1} onClick={() => choose(chapters[index + 1])}>下一章</button>
    {open && <div className="chapter-switcher-menu" role="listbox" aria-label="全部章节">{chapters.map((chapter) => <button type="button" role="option" aria-selected={chapter.id === selected.id} className={chapter.id === selected.id ? "active" : ""} key={chapter.id} onClick={() => choose(chapter)}><span>{String(chapter.number).padStart(2, "0")}</span><b>{chapter.title}</b></button>)}</div>}
  </nav>;
}

function GenerationTaskStatus({ task, outputName }: { task: CreativeTask; outputName: string }) {
  const status = generationStatusLabel(task);
  return <div className={`generation-task-status ${task.delivery_status}${status.failed ? " failed" : ""}`} role="status" aria-live="polite"><strong>{status.label}</strong><span>{task.outputRefs.length > 0 ? `已保存 ${task.outputRefs.length} 个${outputName}` : `没有保存${outputName}`}</span>{task.error && <small>{task.error}</small>}</div>;
}

function ArtifactEditor({ fields, value, onChange, disabled }: { fields: { key: string; label: string; hint: string }[]; value: Record<string, string>; onChange: (key: string, value: string) => void; disabled?: boolean }) {
  return <div className="artifact-editor-grid">{fields.map((field) => <label key={field.key}><span>{field.label}</span><small>{field.hint}</small><textarea disabled={disabled} value={value[field.key] ?? ""} onChange={(event) => onChange(field.key, event.target.value)} /></label>)}</div>;
}

function DirectionDecisionEditor({ value, onChange, disabled = false }: { value?: DirectionDecisionValue; onChange: (value: DirectionDecisionValue) => void; disabled?: boolean }) {
  const decision = value?.decision ?? "";
  return <div className="direction-decision"><select disabled={disabled} aria-label="保护冲突处理决定" value={decision} onChange={(event) => onChange({ decision: event.target.value as DirectionDecision, note: value?.note ?? "" })}><option value="">请选择处理方式</option><option value="return-draft">退回草稿修改</option><option value="accept-and-open-impact">采用正文并打开反馈</option><option value="false-positive">确认是误报</option></select>{decision === "false-positive" && <input disabled={disabled} aria-label="误报说明" value={value?.note ?? ""} onChange={(event) => onChange({ decision, note: event.target.value })} placeholder="说明为什么该事实不影响此规划" />}</div>;
}

function RevisionPane({ title, revision }: { title: string; revision: { id: string; version: number; body?: string; payload?: Record<string, unknown>; source: string } | null | undefined }) {
  return <article className="revision-pane"><header><strong>{title}</strong>{revision && <span>v{revision.version} · {revision.source === "agent" ? "引擎" : "作者"}</span>}</header>{revision ? revision.body !== undefined ? <div className="revision-prose">{revision.body.split(/\n{2,}/).map((paragraph, index) => <p key={index}>{paragraph}</p>)}</div> : <StructuredPreview value={revision.payload ?? {}} /> : <p className="muted">尚无版本</p>}</article>;
}

function StructuredPreview({ value }: { value: Record<string, unknown> }) {
  const labels: Record<string, string> = { premise: "核心命题", promise: "读者承诺", stakes: "核心代价", endingDirection: "结局方向", nodes: "层级大纲", chapters: "章节计划", title: "标题", goal: "目标", conflict: "冲突", dependencyKeys: "依赖" };
  return <div className="structured-preview">{Object.entries(value).map(([key, item]) => <section key={key}><strong>{labels[key] ?? key}</strong>{Array.isArray(item) ? <div className="structured-list">{item.map((entry, index) => <div className="structured-card" key={index}>{entry && typeof entry === "object" && !Array.isArray(entry) ? <StructuredPreview value={entry as Record<string, unknown>} /> : <p>{String(entry)}</p>}</div>)}</div> : item && typeof item === "object" ? <StructuredPreview value={item as Record<string, unknown>} /> : <p>{String(item ?? "—")}</p>}</section>)}</div>;
}
