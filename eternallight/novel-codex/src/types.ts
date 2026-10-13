import type { DiscussionExcerpt, ViewId } from "../shared/discussion-excerpts.mjs";
export type { DiscussionExcerpt, ViewId } from "../shared/discussion-excerpts.mjs";

export type DiscussionReference = { projectId: string; snapshotId: string } & (
  { kind: "scene"; objectId: string; revisionId: string; sceneGroupId: string }
  | { kind: "claim"; objectId: string; revisionId: string; modelingBatchId: string; assertionIndex: number }
  | { kind: "commitment"; commitmentId: string; revisionId: string }
);
export type ConversationFocus = { view: ViewId; chapterId?: string; chapterNumber?: number; reference?: DiscussionReference; chapterStructureTaskId?: string; excerpts?: readonly DiscussionExcerpt[]; readOnly?: true };
export type DiscussionSource = { focus: ConversationFocus & { reference: DiscussionReference }; label: string };
export type CommitmentNavigationRequest = { projectId: string; snapshotId: string; commitmentId: string; revisionId: string };
export type CommitmentNavigationSelection = { requestId: string; request: CommitmentNavigationRequest; chapterId: string };
export type DesiredOutcome = import("../shared/generated/DesiredOutcome").DesiredOutcome;
export type ActionTarget = Record<string, unknown> & { desiredOutcomes?: DesiredOutcome[]; chapterAcceptance?: ChapterAcceptanceReport["chapterAcceptance"] };
export interface ProposedAction {
  id: string;
  sourceTaskId: string;
  operation: string;
  target: ActionTarget;
  instruction: string;
  inputRevision: number;
  intentRevision: number;
  status: "proposed" | "queued" | "running" | "completed" | "failed" | "superseded";
}
export interface ActionPermission {
  kind: "exactAction" | "finiteScope";
  scopePermitId: string | null;
  actionId: string;
  workTaskId: string;
  grantedAt: number;
  actionStatus: string;
  evidence: {
    authorMessageId: string | null;
    projectId: string; sourceTaskId: string; intentRevision: number; inputRevision: number;
    operation: string; target: Record<string, unknown>; instruction: string; authorIntent: string;
  };
}
export type { ExecutionScopeRequest } from "../shared/generated/ExecutionScopeRequest";
import type { ExecutionScopeRequest } from "../shared/generated/ExecutionScopeRequest";
// UI interpretation of Native JSON or Host-only commands; shared fields stay generated.
export type ExecutionScopePreview = Omit<import("../shared/generated/ExecutionScopePreview").ExecutionScopePreview, "targets"> & { targets: { operation: string; target: ActionTarget }[] };
export interface ExecutionScopePermit {
  id: string; scope: ExecutionScopePreview; status: "active" | "superseded"; confirmedAt: number;
}
export interface ExecutionScopeConfirmation { permit: ExecutionScopePermit; work: WorkTaskSummary | null; }
export type AuthorExportScope = import("../shared/generated/responses/HttpAuthorExportScope").HttpAuthorExportScope;
export interface AuthorExportOptions { scopes: AuthorExportScope[]; format: "txt" | "markdown"; }
export type AuthorExportResult = import("../shared/generated/responses/HttpAuthorExportResult").HttpAuthorExportResult;

export type Character = import("../shared/generated/responses/HttpCharacter").HttpCharacter;
export type Relationship = import("../shared/generated/responses/HttpRelationship").HttpRelationship;
export type StoryEvent = import("../shared/generated/responses/HttpStoryEvent").HttpStoryEvent;
export type Location = import("../shared/generated/responses/HttpLocation").HttpLocation;
export type Beat = import("../shared/generated/responses/HttpBeat").HttpBeat;
export type Scene = import("../shared/generated/responses/HttpScene").HttpScene;
export type PlotThread = import("../shared/generated/responses/HttpPlotThread").HttpPlotThread;
export type WorldRule = import("../shared/generated/responses/HttpWorldRule").HttpWorldRule;
export type Passage = import("../shared/generated/responses/HttpPassage").HttpPassage;
export type Chapter = import("../shared/generated/responses/HttpChapter").HttpChapter;
export type StoryCanon = import("../shared/generated/responses/HttpStoryCanon").HttpStoryCanon;
export type NarrativeIr = import("../shared/generated/responses/HttpNarrativeIr").HttpNarrativeIr;
export type NovelEdition = import("../shared/generated/responses/HttpNovelEdition").HttpNovelEdition;
export type WorkVolumeSource = import("../shared/generated/responses/HttpWorkVolumeSource").HttpWorkVolumeSource;
export type WorkCatalog = import("../shared/generated/responses/HttpWorkCatalog").HttpWorkCatalog;
export type ChapterDeletionReceipt = import("../shared/generated/responses/HttpChapterDeletionReceipt").HttpChapterDeletionReceipt;
export type ChapterStructureChange = import("../shared/generated/responses/HttpChapterStructureChange").HttpChapterStructureChange;
export type WorkCatalogOperation = { type: "createVolume"; title: string; source?: WorkVolumeSource } | { type: "linkOutline"; volumeId: string; source: WorkVolumeSource } | { type: "placeChapter"; chapterId: string; volumeId: string | null };

export type RuntimeState = import("../shared/generated/responses/HttpRuntimeState").HttpRuntimeState;
import type { ServerProjectResponse } from "../shared/generated/runtime/ServerProjectResponse";
// The switcher and information panel project only these fields across novel/media views.
export type ProjectSummary = Pick<ServerProjectResponse, "id" | "title" | "logline">;
export interface RuntimeStatus { state: "starting" | "slow" | "online" | "offline"; mode: "local" | "novel-agent-engine"; error: string | null; }
export type ChapterDiff = import("../shared/generated/responses/HttpChapterDiff").HttpChapterDiff;
export type SearchKind = import("../shared/generated/responses/HttpSearchKind").HttpSearchKind;
export type SearchResult = import("../shared/generated/responses/HttpSearchResult").HttpSearchResult;
export type SearchResponse = import("../shared/generated/responses/HttpSearchResponse").HttpSearchResponse;
export type CreativeArtifactKind = import("../shared/generated/responses/HttpCreativeArtifactKind").HttpCreativeArtifactKind;
export type IdentityDecision = import("../shared/generated/responses/HttpIdentityDecision").HttpIdentityDecision;
export type IdentityCatalog = import("../shared/generated/responses/HttpIdentityCatalog").HttpIdentityCatalog;
export type StoryEvidence = import("../shared/generated/responses/HttpStoryEvidence").HttpStoryEvidence;
type StoryAssertionBase = { subjectId: string; predicate: string; evidence: StoryEvidence; storyTime?: number | null };
export type StoryAssertion = import("../shared/generated/responses/HttpStoryAssertion").HttpStoryAssertion;
export type CharacterKnowledge = import("../shared/generated/responses/HttpCharacterKnowledge").HttpCharacterKnowledge;
export type StoryBriefPayload = import("../shared/generated/responses/HttpStoryBriefPayload").HttpStoryBriefPayload;
export type OutlineNode = import("../shared/generated/responses/HttpOutlineNode").HttpOutlineNode;
export type OutlinePayload = import("../shared/generated/responses/HttpOutlinePayload").HttpOutlinePayload;
export type PlannedBeat = import("../shared/generated/responses/HttpPlannedBeat").HttpPlannedBeat;
export type PlannedCommitmentRef = import("../shared/generated/responses/HttpPlannedCommitmentRef").HttpPlannedCommitmentRef;
export type PlannedEffect = import("../shared/generated/responses/HttpPlannedEffect").HttpPlannedEffect;
export type PlannedScene = import("../shared/generated/responses/HttpPlannedScene").HttpPlannedScene;
export type ChapterPlanPayload = import("../shared/generated/responses/HttpChapterPlanPayload").HttpChapterPlanPayload;
export type RollingPlanPayload = import("../shared/generated/responses/HttpRollingPlanPayload").HttpRollingPlanPayload;
export type WriterChapterProjection = import("../shared/generated/responses/HttpWriterChapterProjection").HttpWriterChapterProjection;
export type CreativeArtifactPayload = import("../shared/generated/responses/HttpCreativeArtifactPayload").HttpCreativeArtifactPayload;
export type CreativeRevision<T = Record<string, unknown>> = Omit<import("../shared/generated/responses/HttpCreativeRevision").HttpCreativeRevision, "payload"> & { payload: T | null };
export type WorkingBaselineSelection = import("../shared/generated/responses/HttpWorkingBaselineSelection").HttpWorkingBaselineSelection;
export type DraftRevision = import("../shared/generated/responses/HttpDraftRevision").HttpDraftRevision;
export type { ChapterApiRebindPlanRequest as ChapterPlanRebindRequest } from "../shared/generated/runtime/ChapterApiRebindPlanRequest";
import type { ChapterApiRebindPlanRequest as ChapterPlanRebindRequest } from "../shared/generated/runtime/ChapterApiRebindPlanRequest";
export type ChapterPlanRebindReceipt = import("../shared/generated/responses/HttpChapterPlanRebindReceipt").HttpChapterPlanRebindReceipt;
export type EventClaimReference = import("../shared/generated/responses/HttpEventClaimReference").HttpEventClaimReference;
export type ConditionEvaluationTarget = import("../shared/generated/ConditionEvaluationTarget").ConditionEvaluationTarget;
// UI interpretation of Native JSON or Host-only commands; shared fields stay generated.
export type ConditionEvaluationJob = import("../shared/generated/responses/HttpConditionEvaluationJob").HttpConditionEvaluationJob;
export type ConditionSourceExcerpt = import("../shared/generated/responses/HttpConditionSourceExcerpt").HttpConditionSourceExcerpt;
export type ConditionExplanationPage = import("../shared/generated/responses/HttpConditionExplanationPage").HttpConditionExplanationPage;
export type StoryEventInterpretation = import("../shared/generated/responses/HttpStoryEventInterpretation").HttpStoryEventInterpretation;
export type EventConditionAssessment = import("../shared/generated/responses/HttpEventConditionAssessment").HttpEventConditionAssessment;
export type EventReferenceTiming = import("../shared/generated/responses/HttpEventReferenceTiming").HttpEventReferenceTiming;
export type EventTemporalAssessment = import("../shared/generated/responses/HttpEventTemporalAssessment").HttpEventTemporalAssessment;
export type StateProperty = import("../shared/generated/responses/HttpStateProperty").HttpStateProperty;
export type PropertyRuleDefinition = import("../shared/generated/responses/HttpPropertyRuleDefinition").HttpPropertyRuleDefinition;
export type EventRole = StoryEventInterpretation["participants"][number]["role"];
export type EventCondition = import("../shared/generated/responses/HttpEventCondition").HttpEventCondition;
export type EventRuleDefinition = import("../shared/generated/responses/HttpEventRuleDefinition").HttpEventRuleDefinition;
export type PropertyRulesPayload = import("../shared/generated/responses/HttpPropertyRulesPayload").HttpPropertyRulesPayload;
export interface PropertyRulesSaveRequest { payload: PropertyRulesPayload; expectedRevision: number; expectedCandidateRevisionId: string | null; idempotencyKey: string; }
export type FactProposal = import("../shared/generated/responses/HttpFactProposal").HttpFactProposal;
export type ProvisionalPlanningImpact = import("../shared/generated/responses/HttpProvisionalPlanningImpact").HttpProvisionalPlanningImpact;
export type CreativeReview = import("../shared/generated/responses/HttpCreativeReview").HttpCreativeReview;
export type PlanningImpact = import("../shared/generated/responses/HttpPlanningImpact").HttpPlanningImpact;
export type PlanningReviewRole = import("../shared/generated/responses/HttpPlanningReviewRole").HttpPlanningReviewRole;
export type PlanningReviewArtifactKind = import("../shared/generated/responses/HttpPlanningReviewArtifactKind").HttpPlanningReviewArtifactKind;
export type PlanningReviewFinding = import("../shared/generated/responses/HttpPlanningReviewFinding").HttpPlanningReviewFinding;
export type PlanningReviewReport = import("../shared/generated/responses/HttpPlanningReviewReport").HttpPlanningReviewReport;
export type PlanningReviewRun = import("../shared/generated/responses/HttpPlanningReviewRun").HttpPlanningReviewRun;
export interface PlanningReviewStartRequest { artifactKind: PlanningReviewArtifactKind; revisionId: string; targetNodeId: string | null; authorIntent: string; roles: PlanningReviewRole[]; }
export type WriterContextPreview = import("../shared/generated/responses/HttpWriterContextPreview").HttpWriterContextPreview;
export type CreativeWorkspace = import("../shared/generated/responses/HttpCreativeWorkspace").HttpCreativeWorkspace;

export type ProposalSetSummary = import("../shared/generated/responses/HttpProposalSetSummary").HttpProposalSetSummary;
export type ProposalSetDetail = import("../shared/generated/responses/HttpProposalSetDetail").HttpProposalSetDetail;
export type ImpactAnalysisResult = import("../shared/generated/responses/HttpImpactAnalysisResult").HttpImpactAnalysisResult;
export type SavedAnalysis = { kind: "planningReview"; run: PlanningReviewRun; reportId: string } | { kind: "hypothesis"; result: HypothesisResult };
export type HypothesisResult = import("../shared/generated/responses/HttpHypothesisResult").HttpHypothesisResult;
export type HistoricalKnowledgeState = import("../shared/generated/responses/HttpHistoricalKnowledgeState").HttpHistoricalKnowledgeState;
export type HypothesisAssumption = import("../shared/generated/responses/HttpHypothesisAssumption").HttpHypothesisAssumption;
export interface ThreadSummary {
  readState?: ThreadReadState | null;
  projectId?: string; version?: number; isArchived?: boolean; forkedFromId?: string | null; lastTurnId?: string | null;
  id: string; name: string | null; preview: string | null; modelProvider: string | null;
  status: string | null; createdAt: number | null; updatedAt: number | null;
  isPinned: boolean; ephemeral: boolean;
}
export interface ThreadReadState { revision: string; firstUnread: { type: "threadStart" } | { type: "turn"; turnId: string } | null; }
export interface ThreadItemSummary { id: string; type: string | null; status: string | null; text: string | null; phase: "commentary" | "final_answer" | null; mediaAttachments?: { assetRevisionId: string; name: string; previewUrl: string }[]; }
export interface ThreadTurnSummary { id: string; status: string | null; startedAt: number | null; completedAt: number | null; items: ThreadItemSummary[]; }
export interface ThreadDetail { thread: ThreadSummary; turns: ThreadTurnSummary[]; }
export interface RecoveryRequest { requestId: string; turnId: string | null; kind: "model" | "read" | "write"; operation: string | null; state: string; receiptRef: string | null; hasIdempotencyKey: boolean; recovery: "none" | "resume-core" | "manual-reconciliation"; }
export interface RecoveryRun { runId: string; planId: string; threadId: string; projectId: string | null; operation: string | null; runtimeStatus: string | null; reconciledAt: string | null; requests: RecoveryRequest[]; }
export interface BackgroundTerminal { processId: string; command: string | null; cwd: string | null; status: string | null; }
export type LocalMetricsSummary = import("../shared/generated/responses/HttpLocalMetricsSummary").HttpLocalMetricsSummary;
export type CreativeIntentSource = import("../shared/generated/CreativeIntentSource").CreativeIntentSource;
export type CreativeIntentRevision = import("../shared/generated/CreativeIntentRevision").CreativeIntentRevision;
// UI interpretation of Native JSON or Host-only commands; shared fields stay generated.
export type GenerationFeedbackDecision = Omit<import("../shared/generated/GenerationFeedbackDecision").GenerationFeedbackDecision, "choice"> & { choice: "continue" | "repairDraft" };
export type GenerationChapterDelivery = import("../shared/generated/GenerationChapterDelivery").GenerationChapterDelivery;
export type GenerationRunSummary = import("../shared/generated/responses/HttpGenerationRunSummary").HttpGenerationRunSummary;
export type GenerationExecutionResult = GenerationRunSummary & {
  resumeFailure?: { code: "WORK_RESUME_NOT_STARTED"; message: string; workTaskId: string };
  pausedWork?: Pick<WorkTaskSummary, "id" | "projectId" | "threadId" | "version"> & { status: "paused" };
};
export type ReplanStage = import("../shared/generated/ReplanStage").ReplanStage;
export type GenerationReplanPreview = import("../shared/generated/responses/HttpGenerationReplanPreview").HttpGenerationReplanPreview;
export interface GenerationReplanRequest {
  expectedVersion: number;
  expectedProjectRevision: number;
  expectedSourceDigest: string;
  restartFrom: ReplanStage;
  instruction: string;
}
export type SceneAssessmentSummary = import("../shared/generated/responses/HttpSceneAssessmentSummary").HttpSceneAssessmentSummary;
export type SceneEffectAssessment = import("../shared/generated/responses/HttpSceneEffectAssessment").HttpSceneEffectAssessment;
export type SceneEventRef = import("../shared/generated/responses/HttpSceneEventRef").HttpSceneEventRef;
export type CommitmentAssessment = import("../shared/generated/responses/HttpCommitmentAssessment").HttpCommitmentAssessment;
export type AuthorReadingProgress = import("../shared/generated/AuthorReadingProgress").AuthorReadingProgress;

export type TaskFinalReply = import("../shared/generated/TaskFinalReply").TaskFinalReply;

export type CreationIssueLink = import("../shared/generated/responses/HttpCreationIssueLink").HttpCreationIssueLink;
export type CreationIssueObservation = import("../shared/generated/responses/HttpCreationIssueObservation").HttpCreationIssueObservation;
export type CreationIssue = import("../shared/generated/responses/HttpCreationIssue").HttpCreationIssue;
export type CreationIssueSummary = import("../shared/generated/responses/HttpCreationIssueSummary").HttpCreationIssueSummary;
export type CreationIssuePage = import("../shared/generated/responses/HttpCreationIssuePage").HttpCreationIssuePage;
export type CreationIssueObservationPage = import("../shared/generated/responses/HttpCreationIssueObservationPage").HttpCreationIssueObservationPage;
export type ChapterAcceptanceRequirement = import("../shared/generated/responses/HttpChapterAcceptanceRequirement").HttpChapterAcceptanceRequirement;

export type RevisionOutcomeReport = import("../shared/generated/responses/HttpRevisionOutcomeReport").HttpRevisionOutcomeReport;
export type RevisionOutcome = import("../shared/generated/responses/HttpRevisionOutcome").HttpRevisionOutcome;

export type RevisionOutcomeChoice = import("../shared/generated/RevisionOutcomeChoice").RevisionOutcomeChoice;
export type RevisionOutcomeDecisionRequest = import("../shared/generated/RevisionOutcomeDecisionRequest").RevisionOutcomeDecisionRequest;
export type RevisionOutcomeDecision = import("../shared/generated/RevisionOutcomeDecision").RevisionOutcomeDecision;
export type { RevisionPredecessor } from "../shared/generated/RevisionPredecessor";
import type { RevisionPredecessor } from "../shared/generated/RevisionPredecessor";
export interface RevisionRepairPreparation {
  projectId: string;
  source: RevisionPredecessor;
  chapterId: string;
  draftRevisionId: string;
  knowledgeBatchId: string | null;
  note: string;
}
export interface RevisionContinuationPreparation {
  projectId: string;
  intent: string;
  expectedProjectRevision: number;
  expectedBaseSnapshotId: string;
  expectedSourceDigest: string;
  revisionPredecessor: RevisionPredecessor;
}

export type ChapterAcceptanceReport = import("../shared/generated/responses/HttpChapterAcceptanceReport").HttpChapterAcceptanceReport;

export type WorkTaskSummary = import("../shared/generated/responses/HttpWorkTaskSummary").HttpWorkTaskSummary;

export type SearchAuditJobSummary = import("../shared/generated/SearchAuditJob").SearchAuditJob;

export type EntityAuditJobSummary = import("../shared/generated/EntityAuditJob").EntityAuditJob;

export type RelationAuditJobSummary = import("../shared/generated/RelationAuditJob").RelationAuditJob;

export type KnowledgeAuditJobSummary = import("../shared/generated/KnowledgeAuditJob").KnowledgeAuditJob;

export type SnapshotKnowledgeAuditJobSummary = import("../shared/generated/SnapshotKnowledgeAuditJob").SnapshotKnowledgeAuditJob;

export type FactAuditJobSummary = import("../shared/generated/FactAuditJob").FactAuditJob;

export type CommitmentExtractionOrigin = import("../shared/generated/CommitmentExtractionOrigin").CommitmentExtractionOrigin;
export type NarrativeCommitment = import("../shared/generated/NarrativeCommitment").NarrativeCommitment;
export type CommitmentEvidence = import("../shared/generated/CommitmentEvidence").CommitmentEvidence;
export type CommitmentExtractionReport = import("../shared/generated/responses/HttpCommitmentExtractionReport").HttpCommitmentExtractionReport;
export type CommitmentDetail = import("../shared/generated/responses/HttpCommitmentDetail").HttpCommitmentDetail;
export type CommitmentPage = import("../shared/generated/responses/HttpCommitmentPage").HttpCommitmentPage;

export interface GenerationHandoffProposal {
  resumeRun?: { id: string; expectedVersion: number } | null;
  proposalId: string; projectId: string; sourceTaskId: string; threadId: string;
  authorIntent: string; targetChapters: number; expectedRevision: number;
  sourceDigest: string; baseSnapshotId: string; state: "proposed";
}

// UI interpretation of Native JSON or Host-only commands; shared fields stay generated.
export type MessageTrigger = import("../shared/generated/MessageTrigger").MessageTrigger | { kind: "steer"; taskId: string };
export type ConversationMessageInput = { text: string; focus: ConversationFocus | null; connectionId: string | null; media?: import("../shared/multimedia-types").MediaTurnInput | null };
export type ConversationMessage = { id: string; projectId: string; threadId: string; revision: number; position: number;
  input: ConversationMessageInput; trigger: MessageTrigger; sessionId: string; requiresContinue?: boolean; triggerReady?: boolean; holdReason?: string | null;
  status: "queued" | "claimed" | "sending" | "delivered" | "failed" | "cancelled"; taskId: string | null; turnId: string | null; error: string | null; createdAt: number };

export interface NovelCodexApi {
  multimedia: import("../shared/multimedia-types").MultimediaApi;
  platform: string; version: string;
  app: {
    confirmClose: () => Promise<{ closing: boolean }>;
    cancelClose: () => Promise<{ closing: boolean }>;
    onCloseRequested: (listener: () => void) => () => void;
  };
  runtime: {
    status: () => Promise<RuntimeStatus>;
    retryStartup: () => Promise<RuntimeStatus>;
    listProjects: () => Promise<import("../shared/generated/responses/RendererListProjectsResponse").RendererListProjectsResponse>;
    createProject: (title: string, logline: string, medium: import("../shared/multimedia-types").CreativeMedium, idempotencyKey: string) => Promise<import("../shared/generated/responses/RendererCreateProjectResponse").RendererCreateProjectResponse>;
    getState: (projectId: string) => Promise<import("../shared/generated/responses/RendererGetStateResponse").RendererGetStateResponse>;
    getLocalMetrics: () => Promise<import("../shared/generated/responses/RendererGetLocalMetricsResponse").RendererGetLocalMetricsResponse>;
    recordStageAbandonment: (stage: "brief" | "outline" | "plan" | "draft", idempotencyKey: string) => Promise<import("../shared/generated/responses/RendererRecordStageAbandonmentResponse").RendererRecordStageAbandonmentResponse>;
    exportProject: (projectId: string, options: AuthorExportOptions) => Promise<import("../shared/generated/responses/RendererExportProjectResponse").RendererExportProjectResponse>;
    getChapterDraftHistory: (projectId: string, chapterId: string, revisionId: string) => Promise<import("../shared/generated/responses/RendererGetChapterDraftHistoryResponse").RendererGetChapterDraftHistoryResponse>;
    getCreativeWorkspace: (projectId: string) => Promise<import("../shared/generated/responses/RendererGetCreativeWorkspaceResponse").RendererGetCreativeWorkspaceResponse>;
    getQualityNotebook: (projectId: string, before?: number | null) => Promise<import("../shared/generated/responses/RendererGetQualityNotebookResponse").RendererGetQualityNotebookResponse>;
    changeQualityNotebook: (projectId: string, request: import("./quality-types").QualityMutation) => Promise<import("../shared/generated/responses/RendererChangeQualityNotebookResponse").RendererChangeQualityNotebookResponse>;
    getQualityPacket: (projectId: string, packetId: string) => Promise<import("../shared/generated/responses/RendererGetQualityPacketResponse").RendererGetQualityPacketResponse>;
    getQualityReview: (projectId: string, packetId: string) => Promise<import("../shared/generated/responses/RendererGetQualityReviewResponse").RendererGetQualityReviewResponse>;
    reviewQualityPacket: (projectId: string, packetId: string) => Promise<Pick<Awaited<ReturnType<NovelCodexApi["agent"]["generatePlanning"]>>, "mode" | "message" | "projectId">>;
    listProposalSets: (projectId: string, cursor?: string | null) => Promise<import("../shared/generated/responses/RendererListProposalSetsResponse").RendererListProposalSetsResponse>;
    createProposalSet: (projectId: string, request: { baseSnapshotId: string; intent: string; operations: { operationId: string; reviewId: string; dependsOn: string[] }[]; idempotencyKey: string }) => Promise<import("../shared/generated/responses/RendererCreateProposalSetResponse").RendererCreateProposalSetResponse>;
    getProposalSet: (projectId: string, proposalSetId: string) => Promise<import("../shared/generated/responses/RendererGetProposalSetResponse").RendererGetProposalSetResponse>;
    previewProposalSet: (projectId: string, proposalSetId: string, selectedOperationIds: string[], selectionsByOperationId: Record<string, Record<string, unknown>>) => Promise<import("../shared/generated/responses/RendererPreviewProposalSetResponse").RendererPreviewProposalSetResponse>;
    applyProposalSet: (projectId: string, proposalSetId: string, request: { selectedOperationIds: string[]; selectionsByOperationId: Record<string, Record<string, unknown>>; idempotencyKey: string }) => Promise<import("../shared/generated/responses/RendererApplyProposalSetResponse").RendererApplyProposalSetResponse>;
    undoProposalSet: (projectId: string, applicationId: string, expectedProjectRevision: number, idempotencyKey: string) => Promise<import("../shared/generated/responses/RendererUndoProposalSetResponse").RendererUndoProposalSetResponse>;
    getImpactAnalysis: (projectId: string, options: { entityId: string; fromRevisionId?: string | null; toRevisionId?: string | null; maxHops?: number; limit?: number }) => Promise<import("../shared/generated/responses/RendererGetImpactAnalysisResponse").RendererGetImpactAnalysisResponse>;
    getHistoricalKnowledgeState: (projectId: string, snapshotId: string) => Promise<import("../shared/generated/responses/RendererGetHistoricalKnowledgeStateResponse").RendererGetHistoricalKnowledgeStateResponse>;
    createHypothesis: (projectId: string, request: { baseSnapshotId: string; intent: string; assumptions: HypothesisAssumption[]; idempotencyKey: string }) => Promise<import("../shared/generated/responses/RendererCreateHypothesisResponse").RendererCreateHypothesisResponse>;
    getHypothesis: (projectId: string, hypothesisId: string) => Promise<import("../shared/generated/responses/RendererGetHypothesisResponse").RendererGetHypothesisResponse>;
    getIdentityCatalog: (projectId: string) => Promise<import("../shared/generated/responses/RendererGetIdentityCatalogResponse").RendererGetIdentityCatalogResponse>;
    getCharacterKnowledge: (projectId: string, entityId: string) => Promise<import("../shared/generated/responses/RendererGetCharacterKnowledgeResponse").RendererGetCharacterKnowledgeResponse>;
    saveIdentityDecisions: (projectId: string, decisions: IdentityDecision[], expectedRevision: number, expectedCandidateRevisionId: string | null) => Promise<import("../shared/generated/responses/RendererSaveIdentityDecisionsResponse").RendererSaveIdentityDecisionsResponse>;
    savePropertyRules: (projectId: string, request: PropertyRulesSaveRequest) => Promise<import("../shared/generated/responses/RendererSavePropertyRulesResponse").RendererSavePropertyRulesResponse>;
    listGenerationRuns: (projectId: string) => Promise<import("../shared/generated/responses/RendererListGenerationRunsResponse").RendererListGenerationRunsResponse>;
    previewGenerationReplan: (projectId: string, runId: string) => Promise<import("../shared/generated/responses/RendererPreviewGenerationReplanResponse").RendererPreviewGenerationReplanResponse>;
    replanGeneration: (projectId: string, runId: string, request: GenerationReplanRequest) => Promise<import("../shared/generated/responses/RendererReplanGenerationResponse").RendererReplanGenerationResponse>;
    getSceneAssessment: (projectId: string, chapterId: string) => Promise<import("../shared/generated/responses/RendererGetSceneAssessmentResponse").RendererGetSceneAssessmentResponse>;
    getCommitmentExtraction: (projectId: string, chapterId: string, reportRevisionId?: string | null) => Promise<import("../shared/generated/responses/RendererGetCommitmentExtractionResponse").RendererGetCommitmentExtractionResponse>;
    establishCommitment: (projectId: string, chapterId: string, request: { reportRevisionId: string; proposalId: string; baseSnapshotId: string; expectedRevision: number; idempotencyKey: string }) => Promise<import("../shared/generated/responses/RendererEstablishCommitmentResponse").RendererEstablishCommitmentResponse>;
    getCommitment: (projectId: string, id: string, snapshotId?: string | null) => Promise<import("../shared/generated/responses/RendererGetCommitmentResponse").RendererGetCommitmentResponse>;
    listCommitments: (projectId: string, cursor?: string | null, limit?: number) => Promise<import("../shared/generated/responses/RendererListCommitmentsResponse").RendererListCommitmentsResponse>;
    saveCommitment: (projectId: string, id: string, request: { payload: NarrativeCommitment; extractionOrigin: CommitmentExtractionOrigin | null; expectedRevision: number; baseSnapshotId: string; expectedCandidateRevisionId: string | null; idempotencyKey: string }) => Promise<import("../shared/generated/responses/RendererSaveCommitmentResponse").RendererSaveCommitmentResponse>;
    createCommitmentReview: (projectId: string, id: string, revisionId: string, idempotencyKey: string) => Promise<import("../shared/generated/responses/RendererCreateCommitmentReviewResponse").RendererCreateCommitmentReviewResponse>;
    listWorkTasks: (projectId: string, cursor?: string | null) => Promise<import("../shared/generated/responses/RendererListWorkTasksResponse").RendererListWorkTasksResponse>;
    getSearchAudit: (projectId: string) => Promise<import("../shared/generated/responses/RendererGetSearchAuditResponse").RendererGetSearchAuditResponse>;
    startSearchAudit: (projectId: string) => Promise<import("../shared/generated/responses/RendererStartSearchAuditResponse").RendererStartSearchAuditResponse>;
    controlSearchAudit: (projectId: string, jobId: string, command: "pause" | "resume" | "cancel", expectedVersion: number) => Promise<import("../shared/generated/responses/RendererControlSearchAuditResponse").RendererControlSearchAuditResponse>;
    getEntityAudit: (projectId: string) => Promise<import("../shared/generated/responses/RendererGetEntityAuditResponse").RendererGetEntityAuditResponse>;
    startEntityAudit: (projectId: string) => Promise<import("../shared/generated/responses/RendererStartEntityAuditResponse").RendererStartEntityAuditResponse>;
    controlEntityAudit: (projectId: string, jobId: string, command: "pause" | "resume" | "cancel", expectedVersion: number) => Promise<import("../shared/generated/responses/RendererControlEntityAuditResponse").RendererControlEntityAuditResponse>;
    getRelationAudit: (projectId: string) => Promise<import("../shared/generated/responses/RendererGetRelationAuditResponse").RendererGetRelationAuditResponse>;
    startRelationAudit: (projectId: string) => Promise<import("../shared/generated/responses/RendererStartRelationAuditResponse").RendererStartRelationAuditResponse>;
    controlRelationAudit: (projectId: string, jobId: string, command: "pause" | "resume" | "cancel", expectedVersion: number) => Promise<import("../shared/generated/responses/RendererControlRelationAuditResponse").RendererControlRelationAuditResponse>;
    getKnowledgeAudit: (projectId: string) => Promise<import("../shared/generated/responses/RendererGetKnowledgeAuditResponse").RendererGetKnowledgeAuditResponse>;
    startConditionEvaluation: (projectId: string, target: ConditionEvaluationTarget) => Promise<import("../shared/generated/responses/RendererStartConditionEvaluationResponse").RendererStartConditionEvaluationResponse>;
    getConditionEvaluation: (projectId: string, jobId: string) => Promise<import("../shared/generated/responses/RendererGetConditionEvaluationResponse").RendererGetConditionEvaluationResponse>;
    controlConditionEvaluation: (projectId: string, jobId: string, command: "pause" | "resume" | "cancel", expectedVersion: number) => Promise<import("../shared/generated/responses/RendererControlConditionEvaluationResponse").RendererControlConditionEvaluationResponse>;
    getConditionExplanations: (projectId: string, jobId: string, cursor?: string | null) => Promise<import("../shared/generated/responses/RendererGetConditionExplanationsResponse").RendererGetConditionExplanationsResponse>;
    startKnowledgeAudit: (projectId: string) => Promise<import("../shared/generated/responses/RendererStartKnowledgeAuditResponse").RendererStartKnowledgeAuditResponse>;
    controlKnowledgeAudit: (projectId: string, jobId: string, command: "pause" | "resume" | "cancel", expectedVersion: number) => Promise<import("../shared/generated/responses/RendererControlKnowledgeAuditResponse").RendererControlKnowledgeAuditResponse>;
    getSnapshotKnowledgeAudit: (projectId: string) => Promise<import("../shared/generated/responses/RendererGetSnapshotKnowledgeAuditResponse").RendererGetSnapshotKnowledgeAuditResponse>;
    startSnapshotKnowledgeAudit: (projectId: string) => Promise<import("../shared/generated/responses/RendererStartSnapshotKnowledgeAuditResponse").RendererStartSnapshotKnowledgeAuditResponse>;
    controlSnapshotKnowledgeAudit: (projectId: string, jobId: string, command: "pause" | "resume" | "cancel", expectedVersion: number) => Promise<import("../shared/generated/responses/RendererControlSnapshotKnowledgeAuditResponse").RendererControlSnapshotKnowledgeAuditResponse>;
    getFactAudit: (projectId: string) => Promise<import("../shared/generated/responses/RendererGetFactAuditResponse").RendererGetFactAuditResponse>;
    startFactAudit: (projectId: string) => Promise<import("../shared/generated/responses/RendererStartFactAuditResponse").RendererStartFactAuditResponse>;
    controlFactAudit: (projectId: string, jobId: string, command: "pause" | "resume" | "cancel", expectedVersion: number) => Promise<import("../shared/generated/responses/RendererControlFactAuditResponse").RendererControlFactAuditResponse>;
    getWorkTask: (projectId: string, workTaskId: string) => Promise<import("../shared/generated/responses/RendererGetWorkTaskResponse").RendererGetWorkTaskResponse>;
    getWorkIssues: (projectId: string, workTaskId: string, input: { issueId: string | null; cursor: number; limit: number }) => Promise<import("../shared/generated/responses/RendererGetWorkIssuesResponse").RendererGetWorkIssuesResponse>;
    getTaskFinalReply: (projectId: string, taskId: string) => Promise<import("../shared/generated/responses/RendererGetTaskFinalReplyResponse").RendererGetTaskFinalReplyResponse>;
    getRevisionOutcome: (projectId: string, writerTaskId: string) => Promise<import("../shared/generated/responses/RendererGetRevisionOutcomeResponse").RendererGetRevisionOutcomeResponse>;
    decideRevisionOutcome: (projectId: string, writerTaskId: string, request: RevisionOutcomeDecisionRequest) => Promise<import("../shared/generated/responses/RendererDecideRevisionOutcomeResponse").RendererDecideRevisionOutcomeResponse>;
    getPlanningReview: (projectId: string, runId: string) => Promise<import("../shared/generated/responses/RendererGetPlanningReviewResponse").RendererGetPlanningReviewResponse>;
    cancelPlanningReview: (projectId: string, runId: string) => Promise<import("../shared/generated/responses/RendererCancelPlanningReviewResponse").RendererCancelPlanningReviewResponse>;
    setWorkingBaseline: (projectId: string, kind: CreativeArtifactKind | "draft", objectId: string, revisionId: string, expectedSelectionRevision: number) => Promise<import("../shared/generated/responses/RendererSetWorkingBaselineResponse").RendererSetWorkingBaselineResponse>;
    clearWorkingBaseline: (projectId: string, kind: CreativeArtifactKind | "draft", objectId: string, expectedSelectionRevision: number) => Promise<import("../shared/generated/responses/RendererClearWorkingBaselineResponse").RendererClearWorkingBaselineResponse>;
    saveContinuationIntent: (projectId: string, kind: CreativeArtifactKind | "draft", targetId: string, workingRevisionId: string, authorIntent: string) => Promise<import("../shared/generated/responses/RendererSaveContinuationIntentResponse").RendererSaveContinuationIntentResponse>;
    getWriterPreview: (projectId: string, projectRevision: number, readingChapter: number, sceneOrder: number, povCharacterId?: string | null, storyTime?: number | null) => Promise<import("../shared/generated/responses/RendererGetWriterPreviewResponse").RendererGetWriterPreviewResponse>;
    listPlanningImpacts: (projectId: string, cursor?: string | null, limit?: number) => Promise<import("../shared/generated/responses/RendererListPlanningImpactsResponse").RendererListPlanningImpactsResponse>;
    saveArtifact: (projectId: string, kind: CreativeArtifactKind, payload: CreativeArtifactPayload, expectedRevision: number, expectedCandidateRevisionId?: string | null, linkedImpactIds?: string[]) => Promise<import("../shared/generated/responses/RendererSaveArtifactResponse").RendererSaveArtifactResponse>;
    materializeChapterPlan: (projectId: string, chapterPlanId: string, expectedRevision: number) => Promise<import("../shared/generated/responses/RendererMaterializeChapterPlanResponse").RendererMaterializeChapterPlanResponse>;
    getWorkCatalog: (projectId: string) => Promise<import("../shared/generated/responses/RendererGetWorkCatalogResponse").RendererGetWorkCatalogResponse>;
    deleteChapter: (projectId: string, input: { chapterId: string; expectedRevision: number; idempotencyKey: string }) => Promise<import("../shared/generated/responses/RendererDeleteChapterResponse").RendererDeleteChapterResponse>;
    listChapterStructureChanges: (projectId: string, input: { cursor: string | null; limit: number; assessmentState: "pending" | null }) => Promise<import("../shared/generated/responses/RendererListChapterStructureChangesResponse").RendererListChapterStructureChangesResponse>;
    readChapterStructureChange: (projectId: string, changeId: string) => Promise<import("../shared/generated/responses/RendererReadChapterStructureChangeResponse").RendererReadChapterStructureChangeResponse>;
    updateWorkCatalog: (projectId: string, request: { expectedRevision: number; idempotencyKey: string; operation: WorkCatalogOperation }) => Promise<import("../shared/generated/responses/RendererUpdateWorkCatalogResponse").RendererUpdateWorkCatalogResponse>;
    materializeStandaloneChapter: (projectId: string, title: string, expectedRevision: number, volumeId?: string | null) => Promise<import("../shared/generated/responses/RendererMaterializeStandaloneChapterResponse").RendererMaterializeStandaloneChapterResponse>;
    associateChapterPlan: (projectId: string, chapterId: string, chapterPlanId: string, expectedRevision: number) => Promise<import("../shared/generated/responses/RendererAssociateChapterPlanResponse").RendererAssociateChapterPlanResponse>;
    rebindChapterPlan: (projectId: string, request: ChapterPlanRebindRequest) => Promise<import("../shared/generated/responses/RendererRebindChapterPlanResponse").RendererRebindChapterPlanResponse>;
    resolvePlanningImpact: (projectId: string, impactId: string, expectedRevision: number, resolution: "kept" | "false-positive", note?: string) => Promise<import("../shared/generated/responses/RendererResolvePlanningImpactResponse").RendererResolvePlanningImpactResponse>;
    setArtifactLock: (projectId: string, kind: CreativeArtifactKind, locked: boolean, expectedRevision: number) => Promise<import("../shared/generated/responses/RendererSetArtifactLockResponse").RendererSetArtifactLockResponse>;
    saveDraftCheckpoint: (projectId: string, payload: Record<string, unknown>, expectedRevision: number, expectedCandidateRevisionId?: string | null) => Promise<import("../shared/generated/responses/RendererSaveDraftCheckpointResponse").RendererSaveDraftCheckpointResponse>;
    createReview: (projectId: string, artifactKind: CreativeArtifactKind | "draft" | "identity" | "propertyRules", revisionId: string, proposals?: Record<string, unknown>[], selections?: Record<string, unknown>, modelingBatchId?: string | null) => Promise<import("../shared/generated/responses/RendererCreateReviewResponse").RendererCreateReviewResponse>;
    rejectIdentityReview: (projectId: string, reviewId: string, expectedRevision: number) => Promise<import("../shared/generated/responses/RendererRejectIdentityReviewResponse").RendererRejectIdentityReviewResponse>;
    acceptReview: (projectId: string, reviewId: string, expectedRevision: number, selections?: Record<string, unknown>) => Promise<import("../shared/generated/responses/RendererAcceptReviewResponse").RendererAcceptReviewResponse>;
    undoCommit: (projectId: string, commitId: string, expectedRevision: number) => Promise<import("../shared/generated/responses/RendererUndoCommitResponse").RendererUndoCommitResponse>;
    getDiffs: (projectId: string, chapterId: string) => Promise<import("../shared/generated/responses/RendererGetDiffsResponse").RendererGetDiffsResponse>;
    search: (projectId: string, query: string, limit?: number, kind?: SearchKind, cursor?: string) => Promise<import("../shared/generated/responses/RendererSearchResponse").RendererSearchResponse>;
  };
  agent: {
    setWorkspaceContext: (context: { projectId: string | null; dirty: boolean; chapterScope: { projectId: string; chapterId: string } | null; mediaScope: import("../shared/multimedia-types").EditionScope | null }) => Promise<{ selected: string | null }>;
    retryChapterStructureCheck: (projectId: string, changeId: string) => Promise<void>;
    status: () => Promise<{ configured: boolean; state: string; mode: "novel-agent-engine"; error: string | null; provider?: string; model?: string; reasoningEffort?: string | null; commandSource?: string | null; cliPath?: string | null; isolatedHome?: string | null }>;
    recoveryReport: () => Promise<RecoveryRun[]>;
    listConnections: (provider?: string, model?: string, reasoningEffort?: string | null) => Promise<{ connections: { id: string; provider: string; model: string; protocol: "auto" | "responses" | "chat-completions" | "messages" | "generate-content"; selectedProtocol: string | null; declaredCapabilities: string[]; verifiedCapabilities: string[]; failedCapabilities: string[]; collaborationStatus: "ready" | "unverified" | "failed"; operations: string[]; status: "unverified" | "verified" | "failed"; certificationIds: string[]; accountStatus: "configured" | "missing-credential"; isDefault: boolean; reasoning?: { effort: string; defaultEffort: string; options: { effort: string; label: string; description: string }[] } | null }[] }>;
    verifyConnection: (provider: string, model: string, reasoningEffort?: string | null) => Promise<{ certificationIds?: string[]; reportId: string; selectedProtocol: string; attempts: { protocol: string; status: string; category?: string; message?: string }[]; records: { capability: string; status: string; scope: unknown; checkedAt: string; expiresAt: string }[]; access?: { status: "available"; protocol: string } }>;
    verifyImageInput: (provider: string, model: string, reasoningEffort?: string | null) => Promise<{ reportId: string; selectedProtocol: string; attempts: { protocol: string; status: string; category?: string; message?: string }[]; records: { capability: string; status: string; scope: unknown; checkedAt: string; expiresAt: string }[] }>;
    verifyConnectionEdge: (parentProvider: string, parentModel: string, childProvider: string, childModel: string) => Promise<{ reportId: string; parentConnectionId: string; childConnectionId: string; selectedProtocols: { parent: string; child: string }; attempt: { protocol: string; status: string; category?: string; message?: string }; record: { capability: string; status: string; scope: unknown; checkedAt: string; expiresAt: string } }>;
    setReasoningEffort: (provider: string, model: string, effort: string) => Promise<{ reasoningEffort: string }>;
    configure: (provider: string, model: string, reasoningEffort?: string | null) => Promise<{ configured: boolean; state: string; mode: "novel-agent-engine"; error: string | null; provider?: string; model?: string; reasoningEffort?: string | null; commandSource?: string | null; cliPath?: string | null; isolatedHome?: string | null }>;
    listExecutions: (projectId: string) => Promise<Array<{ executionId: string; requestId: string; projectId: string; threadId: string | null;
      taskId: string | null; executionThreadId: string | null; turnId: string | null; prompt: string; workflow: unknown;
      cursor:number; events:import("../shared/execution-event-journal.mjs").ExecutionEvent[]; truncated:boolean; questions: Array<{projectId: string; executionId: string; requestId: string | number;
        method: "item/tool/requestUserInput"; params: Record<string, unknown>; threadId?: string | null; turnId?: string | null}> }>>;
    beginInterlude: (projectId:string,workId:string,input:{id:string;threadId:string;expectedControlRevision:number;parentId:string|null;autoReturn:boolean;text:string;focus:ConversationFocus|null;media?:import("../shared/multimedia-types").MediaTurnInput})=>Promise<WorkTaskSummary>;
    returnInterlude: (projectId:string,workId:string,input:{commandId:string;interludeId:string;threadId:string;expectedControlRevision:number;expectedRevision:number})=>Promise<WorkTaskSummary>;
    readMessage: (projectId: string, threadId: string, messageId: string) => Promise<ConversationMessage>;
    listMessages: (projectId: string, threadId: string) => Promise<ConversationMessage[]>;
    submitMessage: (projectId: string, threadId: string, message: { messageId: string; input: ConversationMessageInput; trigger: MessageTrigger }) => Promise<ConversationMessage>;
    changeMessage: (projectId: string, threadId: string, id: string, change: { action: "cancel" | "continue" | "move" | "edit" | "reconcile"; expectedRevision: number; [key: string]: unknown }) => Promise<ConversationMessage>;
    sendPrompt: (projectId: string, prompt: string, repairSource?: RevisionPredecessor | null, conversationFocus?: ConversationFocus | null, threadId?: string | null, mediaInput?: import("../shared/multimedia-types").MediaTurnInput | null) => Promise<{ mode: string; message: string; projectId: string; proposedActions: ProposedAction[]; generationHandoff?: GenerationHandoffProposal | null; candidateResults?: { taskId: string; action: ProposedAction }[]; executionFailure?: { message: string; code: string; sourceTaskId: string; workTaskId: string | null; stepId: string | null }; workflow?: { task?: unknown; context?: unknown; revision?: number; headSnapshotId?: string; sourceDigest?: string } }>;
    listProposedActions: (projectId: string) => Promise<{ actions: ProposedAction[] }>;
    readActionPermission: (projectId: string, sourceTaskId: string, actionId: string) => Promise<{ permission: ActionPermission | null }>;
    readExecutionScope: (projectId: string, sourceTaskId: string) => Promise<{ scope: ExecutionScopePermit | null }>;
    previewExecutionScope: (projectId: string, sourceTaskId: string, request: ExecutionScopeRequest) => Promise<ExecutionScopePreview>;
    confirmExecutionScope: (projectId: string, sourceTaskId: string, request: ExecutionScopeRequest, previewDigest: string) => Promise<ExecutionScopeConfirmation>;
    approveAction: (projectId: string, sourceTaskId: string, actionId: string) => Promise<{ action: ProposedAction; taskId: string; message: string; candidateResults?: { taskId: string; action: { operation: string; target: Record<string, unknown> } }[]; workTask?: WorkTaskSummary; executionFailure?: { message: string; code: string; sourceTaskId: string; workTaskId: string | null; stepId: string | null } }>;
    declineAction: (projectId: string, sourceTaskId: string, actionId: string) => Promise<ProposedAction>;
    generatePlanning: (projectId: string, operation: "generateBrief" | "generateOutline" | "generatePlan", intent: string) => Promise<{ mode: string; message: string; projectId: string; workflow?: { task?: unknown; context?: unknown; revision?: number } }>;
    generateWork: (projectId: string, intent: string, targetChapters: number, expectedProjectRevision: number, expectedBaseSnapshotId: string, expectedSourceDigest: string, revisionPredecessor: RevisionPredecessor | null, generationProposalId?: string | null) => Promise<GenerationExecutionResult>;
    continueWork: (projectId: string, runId: string) => Promise<GenerationExecutionResult>;
    retryWork: (projectId: string, runId: string) => Promise<GenerationExecutionResult>;
    resolveGenerationFeedback: (projectId: string, runId: string, assessmentRevisionId: string, choice: "continue" | "repairDraft", instruction: string) => Promise<GenerationExecutionResult>;
    addFutureIntent: (projectId: string, runId: string, instruction: string) => Promise<{ id: string; intentGeneration: number; intentRevisions: CreativeIntentRevision[] }>;
    reviseCurrentChapter: (projectId: string, runId: string, instruction: string) => Promise<{ id: string; intentGeneration: number; pendingCurrentIntentGeneration: number }>;
    generateChapter: (projectId: string, request: { chapterId: string; chapterNumber: number; sceneOrder?: number; povCharacterId?: string | null; storyTime?: number | null; intent: string }) => Promise<{ mode: string; message: string; projectId: string; workflow?: { task?: unknown; target?: unknown; context?: unknown; revision?: number }; completion?: { deliveryStatus: string; outputRefs: { kind: string; revisionId: string; objectId: string; workingBaseRevisionId?: string }[] } | null }>;
    resumeKnowledge: (projectId: string, chapterId: string) => Promise<{ status: "ready"; batchId?: string; skipped?: boolean }>;
    resumeWorkTask: (projectId: string, workTaskId: string) => Promise<WorkTaskSummary & {
      resumeFailure?: { code: "WORK_RESUME_NOT_STARTED"; message: string; workTaskId: string };
    }>;
    recoverWorkTask: (projectId: string, workTaskId: string) => Promise<WorkTaskSummary>;
    controlWorkTask: (projectId: string, workTaskId: string, command: "pause" | "pauseAfterStep" | "cancel", expectedControlRevision: number, expectedThreadId: string) => Promise<WorkTaskSummary>;
    reviewPlanning: (projectId: string, request: PlanningReviewStartRequest) => Promise<{ projectId: string; run: PlanningReviewRun }>;
    steer: (projectId: string, prompt: string, threadId?: string | null) => Promise<{ steered: boolean; retained?: boolean; messageId?: string }>;
    interrupt: (target: { requestId: string; projectId: string }) => Promise<{ interrupted: boolean }>;
    listThreads: (projectId: string, archived?: boolean, cursor?: string | null, scopeId?: string) => Promise<{ threads: ThreadSummary[]; nextCursor: string | null }>;
    createThread: (projectId: string, options: { sourceThreadId?: string; lastTurnId?: string; scopeId?: string }) => Promise<{ thread: ThreadSummary }>;
    renameThread: (projectId: string, threadId: string, title: string) => Promise<{ thread: ThreadSummary }>;
    readConversation: (projectId: string, threadId: string) => Promise<ThreadDetail>;
    resumeThread: (projectId: string, threadId: string) => Promise<{ thread: ThreadSummary }>;
    reconnectThread: (projectId: string, threadId: string, expectedTurnId: string | null) => Promise<{ thread: { id: string }; recovery: { turnId: string | null; state: "running" | "waiting" | "completed" | "cancelled" | "error" } | null }>;
    readThread: (projectId: string, threadId: string) => Promise<ThreadDetail>;
    updateThreadReadState: (projectId: string, threadId: string, request: { operation: "read" | "unread"; expectedRevision: string }) => Promise<{ readState: ThreadReadState }>;
    reviewThread: (projectId: string, threadId: string) => Promise<{ started: boolean; turn: unknown }>;
    compactThread: (projectId: string, threadId: string) => Promise<{ started: boolean }>;
    listBackgroundTerminals: (projectId: string, threadId: string) => Promise<{ terminals: BackgroundTerminal[] }>;
    terminateBackgroundTerminal: (projectId: string, threadId: string, processId: string) => Promise<{ terminated: boolean }>;
    setThreadPinned: (projectId: string, threadId: string, isPinned: boolean) => Promise<{ thread: ThreadSummary }>;
    archiveThread: (projectId: string, threadId: string) => Promise<{ archived: boolean }>;
    unarchiveThread: (projectId: string, threadId: string) => Promise<{ thread: ThreadSummary }>;
    resolveUserInput: (target: { projectId: string; executionId: string; requestId: string | number }, response: { answers: Record<string, string | string[]> }) => Promise<{ resolved: boolean }>;
    onEvent: (listener: (payload: unknown) => void) => () => void;
    onUserInputRequest: (listener: (payload: { projectId: string; executionId: string; requestId: string | number; method: "item/tool/requestUserInput"; params: Record<string, unknown>; threadId?: string | null; turnId?: string | null; itemId?: string | null }) => void) => () => void;
  };
}
