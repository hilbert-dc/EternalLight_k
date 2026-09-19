export type ViewId = "overview" | "brief" | "outline" | "plans" | "chapters" | "characters" | "scenes" | "plot" | "timeline" | "world" | "diffs";
export type AuthorExportScope = "manuscript" | "planning" | "candidates" | "private";
export interface AuthorExportOptions { scopes: AuthorExportScope[]; format: "txt" | "markdown"; }
export interface AuthorExportResult { fileName: string; mimeType: string; content: string; projectRevision: number; exportedAt: string; scopes: AuthorExportScope[]; }

export interface Character { id: string; name: string; role: string; status: string; color: string; }
export interface Relationship { id: string; sourceId: string; targetId: string; kind: string; state: string; }
export interface StoryEvent { id: string; title: string; outcome: string; occurredAt: string; }
export interface Location { id: string; name: string; constraints: string; }
export interface Beat { id: string; position: number; description: string; }
export interface Scene {
  id: string; title: string; location: string; time: string; summary: string;
  status: "draft" | "review" | "ready"; beats: Beat[];
  participants: { id: string; name: string }[];
  stateChanges: { id: string; description: string }[];
  canonConstraints: { id: string; statement: string }[];
}
export interface PlotThread { id: string; title: string; state: string; tone: "amber" | "blue" | "violet"; }
export interface WorldRule { id: string; statement: string; severity: string; }
export interface Passage { id: string; position: number; body: string; scene_id: string; beatIds: string[]; }
export interface Chapter { id: string; number: number; title: string; status: string; summary: string; updated_at: string; passages: Passage[]; }
export interface StoryCanon {
  characters: Character[]; relationships: Relationship[]; events: StoryEvent[]; locations: Location[];
  worldRules: WorldRule[]; plotThreads: PlotThread[]; notes: string[];
}
export interface NarrativeIr { version: string; scenes: Scene[]; anchors: { chapterId: string; passageId: string; sceneId: string | null; beatIds: string[] }[]; }
export interface NovelEdition { chapters: Chapter[]; }
export interface RuntimeState {
  project: { id: string; title: string; logline: string; updated_at: string };
  canon: StoryCanon; narrativeIr: NarrativeIr; novelEdition: NovelEdition;
}
export interface ProjectSummary { id: string; title: string; logline: string; created_at?: string; updated_at: string; }
export interface RuntimeStatus { state: "online" | "offline"; mode: "local" | "novel-agent-engine"; error: string | null; }
export interface StoryTestResult { passed: boolean; projectId: string; checks: { id: string; label: string; passed: boolean; severity: "error" | "warning"; details: string }[]; }
export interface ChapterDiff { id: number; chapter_id: string; fromText: string; toText: string; summary: string; createdAt: string; addedChars: number; removedChars: number; }
export type SearchKind = "project" | "character" | "relationship" | "event" | "location" | "worldRule" | "plotThread" | "scene" | "chapter";
export interface SearchResult { id: string; title: string; kind: SearchKind; snippet: string; source: SearchKind; }
export type CreativeArtifactKind = "brief" | "outline" | "plan";
export interface StoryBriefPayload { premise: string; promise?: string; stakes?: string; endingDirection?: string; }
export interface OutlineNode { id?: string; clientKey?: string; parentId?: string | null; parentClientKey?: string; kind: "book" | "volume" | "unit"; position: number; title: string; publicTitle?: string; purpose: string; summary: string; dependencyKeys: string[]; certainty: "open" | "tentative" | "settled"; protection: "free" | "preserve-intent" | "preserve-arrangement"; writerGuidance?: string; }
export interface OutlinePayload { nodes: OutlineNode[]; }
export interface PlannedBeat { id: string; description: string; locked?: boolean; }
export interface PlannedScene { id: string; title?: string; goal: string; conflict?: string; beats: PlannedBeat[]; locked?: boolean; }
export interface ChapterPlanPayload {
  chapterPlanId?: string;
  clientKey?: string;
  primaryNodeId: string;
  chapterNumber: number;
  title?: string;
  goal: string;
  conflict?: string;
  dependencyKeys: string[];
  scenes?: PlannedScene[];
  locked?: boolean;
  publicTitle?: string;
  writerGuidance?: string;
  publicCommitments?: string[];
  allowances?: string[];
  povCharacterId?: string | null;
  storyTime?: number | null;
}
export interface RollingPlanPayload { sourceOutlineRevisionId: string; window?: number; chapters: ChapterPlanPayload[]; }
export interface WriterChapterProjection { chapterPlanId: string; primaryNodeId?: string | null; chapterNumber?: number | null; publicTitle: string; writerGuidance: string; publicCommitments: string[]; povCharacterId?: string | null; storyTime?: number | null; }
export type CreativeArtifactPayload = StoryBriefPayload | OutlinePayload | RollingPlanPayload;
export interface CreativeRevision<T = Record<string, unknown>> { id: string; version: number; parentRevisionId: string | null; baseProjectRevision: number; payload: T; source: string; createdAt: string; }
export interface WorkingBaselineSelection { objectKind: CreativeArtifactKind | "draft"; objectId: string; revisionId: string | null; selectionRevision: number; selectedAt: string; }
export interface DraftRevision { id: string; chapterId: string; version: number; parentRevisionId: string | null; sourcePlanRefs: { lineageStatus: "current" | "historical"; planRevisionId?: string; outlineRevisionId?: string; chapterPlanId?: string; localOnly: boolean }; baseProjectRevision: number; body: string; sceneGroups: { id: string; sceneId?: string | null; body: string; locked?: boolean }[]; source: string; createdAt: string; }
export interface FactProposal { id: string; sceneGroupId: string; sceneId?: string | null; factKind: "world" | "reader" | "character" | "author-secret"; subjectId: string | null; predicate: string; value: unknown; epistemicOwnerId: string | null; epistemicState: "unknown" | "suspected" | "known" | "false"; storyTime?: number | null; evidence: string; source: "author" | "agent"; reviewDecision: "candidate" | "confirmed" | "rejected"; readingChapter: number; sceneOrder: number; }
export interface ProvisionalPlanningImpact { id: string; factProposalId: string; predicate: string; evidence: string; targetArtifactKind: "outline" | "plan"; targetRevisionId: string; targetNodeId: string; targetLabel: string; dependencyKey: string; severity: "affected" | "locked-conflict"; matchedBy: "dependency-key" | "author-link"; }
export interface CreativeReview { id: string; artifact_kind: CreativeArtifactKind | "draft"; artifact_revision_id: string; project_revision: number; status: "ready" | "stale" | "accepted" | "rejected"; checks: { id: string; passed: boolean; blocking: boolean; message: string }[]; selections: Record<string, unknown>; }
export interface PlanningImpact { id: string; status: "affected" | "locked-conflict"; reason: string; evidence: string; state: "open" | "resolved-kept"; severity: "affected" | "locked-conflict"; targetArtifactKind: "outline" | "plan"; targetRevisionId: string; targetNodeId: string; targetLabel: string; dependencyKey: string; resolutionNote?: string | null; sourceChapterId?: string | null; sourceChapterNumber?: number | null; sourceChapterTitle?: string | null; }
export type PlanningReviewRole = "structure" | "character" | "experience";
export type PlanningReviewArtifactKind = "outline" | "plan";
export interface PlanningReviewFinding { quote: string; observation: string; impact: string; suggestion: string; }
export interface PlanningReviewReport {
  id: string;
  runId: string;
  role: PlanningReviewRole;
  status: "complete" | "partial";
  report: {
    summary: string;
    strengths: { quote: string; reason: string }[];
    findings: PlanningReviewFinding[];
    coverage: { checkId: string; status: "checked" | "unknown" | "notChecked"; reason: string }[];
  };
}
export interface PlanningReviewRun {
  id: string;
  projectId: string;
  artifactKind: PlanningReviewArtifactKind;
  revisionId: string;
  targetNodeId: string | null;
  authorIntent: string;
  inputRevision: number;
  roles: PlanningReviewRole[];
  sourcePacketHash: string;
  status: "running" | "completed" | "partial" | "cancelled";
  coverageStatus: "pending" | "complete" | "partial";
  checksByRole: Partial<Record<PlanningReviewRole, string[]>>;
  seats: { role: PlanningReviewRole; taskId: string | null; status: string }[];
  reports: PlanningReviewReport[];
  synthesis: { priorities: { role: PlanningReviewRole; reportId: string; findingIndex: number }[]; disagreements: string[]; incompleteRoles: PlanningReviewRole[] } | null;
  createdAt: string;
  updatedAt: string;
}
export interface PlanningReviewStartRequest { artifactKind: PlanningReviewArtifactKind; revisionId: string; targetNodeId: string | null; authorIntent: string; roles: PlanningReviewRole[]; }
export interface WriterContextPreview { projectRevision: number; readingChapter: number; sceneOrder: number; storyTime: number | null; povCharacterId: string | null; sourceFingerprint: string; planningContext: { lineageStatus: "current" | "historical" | "working"; outlineRevisionId: string; planRevisionId: string; chapter: WriterChapterProjection | null; outlinePath: { id: string; kind: string; publicTitle: string; writerGuidance: string }[] } | null; sourceExcerpts: { chapterId: string; chapterNumber: number; chapterTitle: string; draftRevisionId: string; relationship?: "targetDraft" | "referenceDraft"; sourceRole?: "workingSelected" | "accepted"; body: string; truncated: boolean }[]; omissions: { factCount: number; message: string }; contextBudget: { usedBytes: number; maxBytes: number; truncated: boolean } }
export interface CreativeWorkspace {
  project: { id: string; title: string; logline: string };
  revision: number;
  planWindow: number;
  planLineageStatus?: "missing" | "current" | "stale";
  index: { targetRevision: number; indexedRevision: number; status: "ready" | "stale" };
  artifacts: Record<CreativeArtifactKind, { candidate: CreativeRevision | null; accepted: CreativeRevision | null; working: CreativeRevision | null; locked: boolean; workingSelection: WorkingBaselineSelection | null }>;
  chapters: ({ id: string; number: number; title: string; status: string; summary: string; updated_at: string } & { candidate: DraftRevision | null; accepted: DraftRevision | null; working: DraftRevision | null; workingSelection: WorkingBaselineSelection | null })[];
  workingBaselines: WorkingBaselineSelection[];
  continuationIntents: { targetKind: CreativeArtifactKind | "draft"; targetId: string; workingRevisionId: string; authorIntent: string; status: "ready" | "launched" | "failed"; taskId: string | null; updatedAt: string }[];
  planningReviews: PlanningReviewRun[];
  factProposals: (FactProposal & { draft_revision_id: string })[];
  reviews: CreativeReview[];
  impacts: PlanningImpact[];
  openImpactCount?: number;
  chapterPlanBindings?: { project_id: string; plan_revision_id: string; chapter_plan_id: string; chapter_id: string; lineageStatus: "current" | "historical"; writerProjection: WriterChapterProjection | null }[];
  commits: { id: string; operation_kind: "accept" | "materialize-chapter" | "materialize-standalone" | "associate-plan" | "resolve-impact" | "undo"; result_revision: number; receipt: Record<string, unknown> }[];
  tasks: {
    id: string; task_kind: string; purpose: "planner" | "writer" | "critic"; status: string;
    stage: string; delivery_status: "pending" | "ready_for_review" | "incomplete";
    thread_id: string; input_revision: number; updated_at: string; error?: string | null;
    target: { type?: string; chapterId?: string; chapterNumber?: number; chapterTitle?: string; sceneOrder?: number; scope?: { type: "wholeChapter" } | { type: "throughScene"; endSceneOrder: number }; povCharacterId?: string; storyTime?: number };
    outputRefs: { kind: string; revisionId: string; chapterId?: string }[];
  }[];
}
export interface ThreadSummary {
  id: string; name: string | null; preview: string | null; modelProvider: string | null;
  status: string | null; createdAt: number | null; updatedAt: number | null;
  isPinned: boolean; ephemeral: boolean;
}
export interface ThreadItemSummary { id: string; type: string | null; status: string | null; text: string | null; }
export interface ThreadTurnSummary { id: string; status: string | null; items: ThreadItemSummary[]; }
export interface ThreadDetail { thread: ThreadSummary; turns: ThreadTurnSummary[]; }
export interface BackgroundTerminal { processId: string; command: string | null; cwd: string | null; status: string | null; }
export interface LocalMetricsSummary {
  scope: "local-device";
  metrics: { metric: string; count: number; totalSeconds: number }[];
}
export interface NovelCodexApi {
  platform: string; version: string;
  app: {
    confirmClose: () => Promise<{ closing: boolean }>;
    cancelClose: () => Promise<{ closing: boolean }>;
    onCloseRequested: (listener: () => void) => () => void;
  };
  runtime: {
    status: () => Promise<RuntimeStatus>;
    listProjects: () => Promise<{ projects: ProjectSummary[] }>;
    createProject: (title: string, logline: string) => Promise<RuntimeState>;
    getState: (projectId: string) => Promise<RuntimeState>;
    getLocalMetrics: () => Promise<LocalMetricsSummary>;
    recordStageAbandonment: (stage: "brief" | "outline" | "plan" | "draft", idempotencyKey: string) => Promise<LocalMetricsSummary>;
    exportProject: (projectId: string, options: AuthorExportOptions) => Promise<AuthorExportResult>;
    getCreativeWorkspace: (projectId: string) => Promise<CreativeWorkspace>;
    cancelPlanningReview: (projectId: string, runId: string) => Promise<PlanningReviewRun>;
    setWorkingBaseline: (projectId: string, kind: CreativeArtifactKind | "draft", objectId: string, revisionId: string, expectedSelectionRevision: number) => Promise<WorkingBaselineSelection>;
    clearWorkingBaseline: (projectId: string, kind: CreativeArtifactKind | "draft", objectId: string, expectedSelectionRevision: number) => Promise<{ objectKind: CreativeArtifactKind | "draft"; objectId: string; revisionId: null; selectionRevision: number; cleared: true }>;
    saveContinuationIntent: (projectId: string, kind: CreativeArtifactKind | "draft", targetId: string, workingRevisionId: string, authorIntent: string) => Promise<{ targetKind: CreativeArtifactKind | "draft"; targetId: string; workingRevisionId: string; authorIntent: string; status: "ready"; taskId: null; updatedAt: string }>;
    getWriterPreview: (projectId: string, projectRevision: number, readingChapter: number, sceneOrder: number, povCharacterId?: string | null, storyTime?: number | null) => Promise<WriterContextPreview>;
    listPlanningImpacts: (projectId: string, cursor?: string | null, limit?: number) => Promise<{ data: PlanningImpact[]; nextCursor: string | null; counts: { total: number; open: number; retained: number; lockedConflict: number } }>;
    saveArtifact: (projectId: string, kind: CreativeArtifactKind, payload: CreativeArtifactPayload, expectedRevision: number, expectedCandidateRevisionId?: string | null, linkedImpactIds?: string[]) => Promise<{ artifactKind: CreativeArtifactKind; revisionId: string; version: number; payload: CreativeArtifactPayload; linkedImpactIds: string[] }>;
    materializeChapterPlan: (projectId: string, chapterPlanId: string, expectedRevision: number) => Promise<{ chapterId: string; resultRevision: number }>;
    materializeStandaloneChapter: (projectId: string, title: string, expectedRevision: number) => Promise<{ chapterId: string; resultRevision: number }>;
    associateChapterPlan: (projectId: string, chapterId: string, chapterPlanId: string, expectedRevision: number) => Promise<{ chapterId: string; chapterPlanId: string; resultRevision: number }>;
    resolvePlanningImpact: (projectId: string, impactId: string, expectedRevision: number, resolution: "kept" | "false-positive", note?: string) => Promise<{ commitId: string; resultRevision: number }>;
    setArtifactLock: (projectId: string, kind: CreativeArtifactKind, locked: boolean, expectedRevision: number) => Promise<{ artifactKind: CreativeArtifactKind; locked: boolean; resultRevision: number }>;
    saveDraftCheckpoint: (projectId: string, payload: Record<string, unknown>, expectedRevision: number, expectedCandidateRevisionId?: string | null) => Promise<{ chapterId: string; revisionId: string; version: number; body: string }>;
    createReview: (projectId: string, artifactKind: CreativeArtifactKind | "draft", revisionId: string, proposals?: Record<string, unknown>[], selections?: Record<string, unknown>) => Promise<{ id: string; artifactKind: CreativeArtifactKind | "draft"; artifactRevisionId: string; projectRevision: number; status: string; checks: CreativeReview["checks"]; factProposals: FactProposal[]; provisionalImpacts: ProvisionalPlanningImpact[] }>;
    acceptReview: (projectId: string, reviewId: string, expectedRevision: number, selections?: Record<string, unknown>) => Promise<{ commitId: string; resultRevision: number; impacts: CreativeWorkspace["impacts"] }>;
    undoCommit: (projectId: string, commitId: string, expectedRevision: number) => Promise<{ commitId: string; resultRevision: number }>;
    getDiffs: (projectId: string, chapterId: string) => Promise<{ diffs: ChapterDiff[] }>;
    runStoryTests: (projectId: string) => Promise<StoryTestResult>;
    search: (projectId: string, query: string, limit?: number, kind?: SearchKind) => Promise<{ results: SearchResult[] }>;
  };
  agent: {
    status: () => Promise<{ configured: boolean; state: string; mode: "novel-agent-engine"; error: string | null; provider?: string; model?: string; commandSource?: string | null; cliPath?: string | null; isolatedHome?: string | null }>;
    configure: (provider: string, model: string) => Promise<{ configured: boolean; state: string; mode: "novel-agent-engine"; error: string | null; provider?: string; model?: string; commandSource?: string | null; cliPath?: string | null; isolatedHome?: string | null }>;
    sendPrompt: (projectId: string, prompt: string) => Promise<{ mode: string; message: string; projectId: string; workflow?: { task?: unknown; context?: unknown; revision?: number } }>;
    generatePlanning: (projectId: string, operation: "generateBrief" | "generateOutline" | "generatePlan", intent: string) => Promise<{ mode: string; message: string; projectId: string; workflow?: { task?: unknown; context?: unknown; revision?: number } }>;
    generateChapter: (projectId: string, request: { chapterId: string; chapterNumber: number; sceneOrder?: number; povCharacterId?: string | null; storyTime?: number | null; intent: string }) => Promise<{ mode: string; message: string; projectId: string; workflow?: { task?: unknown; target?: unknown; context?: unknown; revision?: number }; completion?: { deliveryStatus: string; outputRefs: { kind: string; revisionId: string; chapterId?: string }[] } | null }>;
    reviewPlanning: (projectId: string, request: PlanningReviewStartRequest) => Promise<{ projectId: string; run: PlanningReviewRun }>;
    steer: (prompt: string) => Promise<{ steered: boolean }>;
    interrupt: () => Promise<{ interrupted: boolean }>;
    listThreads: (projectId: string, archived?: boolean, cursor?: string | null) => Promise<{ threads: ThreadSummary[]; nextCursor: string | null }>;
    resumeThread: (projectId: string, threadId: string) => Promise<{ thread: ThreadSummary }>;
    reconnectThread: (projectId: string, threadId: string) => Promise<{ thread: ThreadSummary }>;
    readThread: (projectId: string, threadId: string) => Promise<ThreadDetail>;
    reviewThread: (projectId: string, threadId: string) => Promise<{ started: boolean; turn: unknown }>;
    compactThread: (projectId: string, threadId: string) => Promise<{ started: boolean }>;
    listBackgroundTerminals: (projectId: string, threadId: string) => Promise<{ terminals: BackgroundTerminal[] }>;
    terminateBackgroundTerminal: (projectId: string, threadId: string, processId: string) => Promise<{ terminated: boolean }>;
    setThreadPinned: (projectId: string, threadId: string, isPinned: boolean) => Promise<{ thread: ThreadSummary }>;
    archiveThread: (projectId: string, threadId: string) => Promise<{ archived: boolean }>;
    unarchiveThread: (projectId: string, threadId: string) => Promise<{ thread: ThreadSummary }>;
    resolveUserInput: (requestId: string | number, response: { answers: Record<string, string | string[]> }) => Promise<{ resolved: boolean }>;
    onEvent: (listener: (payload: unknown) => void) => () => void;
    onUserInputRequest: (listener: (payload: { requestId: string | number; method: "item/tool/requestUserInput"; params: Record<string, unknown>; threadId?: string | null; turnId?: string | null; itemId?: string | null }) => void) => () => void;
  };
}
