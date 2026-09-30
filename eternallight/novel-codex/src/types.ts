export type ViewId = "overview" | "brief" | "outline" | "plans" | "chapters" | "characters" | "scenes" | "plot" | "timeline" | "world" | "diffs";
export interface DesiredOutcome {
  id: string;
  criterion: string;
  groupIds: string[];
}
export type ActionTarget = Record<string, unknown> & { desiredOutcomes?: DesiredOutcome[] };
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
export interface ExecutionScopeRequest {
  actionIds: string[]; maxActions: number; maxTaskAttempts: number; maxAdvisoryRounds: number; expectedIntentRevision: number;
}
export interface ExecutionScopePreview {
  previewDigest: string; request: ExecutionScopeRequest;
  projectId: string; workTaskId: string; sourceTaskId: string; authorMessageId: string; authorIntent: string;
  inputRevision: number; workVersion: number; baseSnapshotId: string; sourceDigest: string;
  targets: { operation: string; target: ActionTarget }[];
  remainingRequests: number; remainingTokens: number; deadlineAt: number;
}
export interface ExecutionScopePermit {
  id: string; scope: ExecutionScopePreview; status: "active" | "superseded"; confirmedAt: number;
}
export interface ExecutionScopeConfirmation { permit: ExecutionScopePermit; work: WorkTaskSummary | null; }
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
export type SearchKind = "draft" | "brief" | "outline" | "plan";
export interface SearchResult { id: string; title: string; kind: SearchKind; snippet: string; source: "story-revision"; sourceRevisionId: string; sourcePath: string; evidence: { quote: string; start: number; end: number }; }
export interface SearchResponse { results: SearchResult[]; nextCursor: string | null; coverage: { complete: boolean; snapshotId: string; scannedCandidates: number; matchedDocuments: number; candidateCapReached: boolean; truncatedMatches: boolean; indexVersion: number }; }
export type CreativeArtifactKind = "brief" | "outline" | "plan";
export type IdentityDecision = { kind: "same"; fromId: string; toId: string } | { kind: "distinct"; leftId: string; rightId: string };
export interface IdentityCatalog {
  entries: { id: string; canonicalId: string; resolutionState: "candidate" | "authorMerged"; kind: string; claims: { displayName: string; aliases: string[]; sourceObjectId: string; sourceRevisionId: string; evidence: { quote: string; start: number; end: number } }[] }[];
  ambiguousNames: { name: string; candidateIds: string[]; canonicalIds: string[]; authorDecidedDistinct: boolean }[];
  identityRevisionId: string | null; identityDecisions: IdentityDecision[];
  truncated: boolean; ambiguityScanComplete: boolean;
}
export interface StoryBriefPayload { premise: string; promise?: string; stakes?: string; endingDirection?: string; }
export interface OutlineNode { id?: string; clientKey?: string; parentId?: string | null; parentClientKey?: string; kind: "book" | "volume" | "unit"; position: number; title: string; publicTitle?: string; purpose: string; summary: string; dependencyKeys: string[]; certainty: "open" | "tentative" | "settled"; protection: "free" | "preserve-intent" | "preserve-arrangement"; writerGuidance?: string; }
export interface OutlinePayload { nodes: OutlineNode[]; }
export interface PlannedBeat { id: string; description: string; locked?: boolean; }
export type PlannedCommitmentRef = { type: "commitment"; commitmentId: string; revisionId: string } | { type: "proposal"; reportRevisionId: string; proposalId: string; sourceRevisionId: string };
export interface PlannedEffect { id: string; dimension: "world" | "knowledge" | "relationship" | "narrative"; description: string; commitmentRefs: PlannedCommitmentRef[]; }
export interface PlannedScene { id: string; title?: string; goal: string; conflict?: string; beats: PlannedBeat[]; expectedEffects: PlannedEffect[]; locked?: boolean; }
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
export interface ChapterPlanRebindRequest { chapterId: string; previousPlanRevisionId: string; previousChapterPlanId: string; sourceRevisionId: string; planRevisionId: string; chapterPlanId: string; expectedRevision: number; idempotencyKey: string; }
export interface ChapterPlanRebindReceipt { previousPlanRevisionId: string; previousChapterPlanId: string; sourceRevisionId: string; binding: { planRevisionId: string; chapterPlanId: string; chapterId: string; chapterNumber: number; resultRevision: number }; }
export interface EventClaimReference { sourceBatchId: string | null; assertionIndex: number; }
export interface ConditionEvaluationTarget { snapshotId: string | null; sourceBatchId: string; assertionIndex: number; }
export interface ConditionEvaluationJob {
  id: string; projectId: string; target: ConditionEvaluationTarget; bindingDigest: string;
  status: "queued" | "running" | "completed" | "paused" | "cancelled" | "stale" | "failed";
  version: number; scannedSources: number; sourceBytes: number; result: EventConditionAssessment | null;
}
export interface ConditionSourceExcerpt {
  sourceBatchId: string; assertionIndex: number; sourceObjectId: string; sourceRevisionId: string;
  quote: string; start: number; end: number; truncated: boolean;
}
export interface ConditionExplanationPage {
  data: { leafIndex: number; subjectId: string; operand: unknown; assessment: {
    result: EventConditionAssessment["result"]; support: EventClaimReference | null; refute: EventClaimReference | null; unresolvedChecks: number;
  }; excerpts: { support: ConditionSourceExcerpt | null; refute: ConditionSourceExcerpt | null } }[];
  nextCursor: string | null;
}
export interface StoryEventInterpretation {
  evaluationTarget: ConditionEvaluationTarget;
  eventId: string; eventType: string; storyTime: number | null; untilExclusive: number | null;
  participants: { role: "actor" | "recipient" | "affected" | "location" | "instrument" | "item" | "witness" | "beneficiary"; entityId: string }[];
  preconditions: EventClaimReference[]; effects: EventClaimReference[];
  temporalAssessment: EventTemporalAssessment;
  conditionAssessment: EventConditionAssessment;
}
export interface EventConditionAssessment {
  explanationJobId?: string;
  sourceClaimsTruncated?: boolean;
  evaluationJob?: Pick<ConditionEvaluationJob, "id" | "status" | "version" | "scannedSources">;
  basis: "authorRulesSourceClaims"; status: "noApplicableRule" | "interpreted";
  result: "supported" | "refuted" | "both" | "unknown"; observedResult: "supported" | "refuted" | "both" | "unknown";
  ruleRevisionId: string | null; identityRevisionId: string | null;
  sourceClaims?: EventClaimReference[];
  coverage?: { complete: boolean; sources: { pendingSources: number; totalSources: number; scannedSources: number; omittedSources: number; sourceBytes: number; relevantSources: number; sourceProofId: string | null; entityKindsComplete: boolean }; unresolvedChecks: number; evidenceTruncated: boolean };
  stateChangesExecuted: false; worldTruthCertified: false;
}
export type EventReferenceTiming = "applicableBeforeEvent" | "inactiveBeforeEvent" | "startsAtEvent" | "beforeEvent" | "atEvent" | "afterEvent" | "unknownTime" | "unsupportedConditionKind" | "unsupportedEffectKind";
export interface EventTemporalAssessment {
  basis: "sourceBoundTemporalReferences";
  preconditions: { referenceIndex: number; timing: EventReferenceTiming }[];
  effects: { referenceIndex: number; timing: EventReferenceTiming }[];
  semanticTruthInferred: false; globalConditionsVerified: false; stateChangesExecuted: false;
}
export interface StateProperty {
  schema: { cardinality: "single" | "multiple"; valueType: { type: "boolean" | "integer" | "text" } | { type: "entity"; kind: "character" | "organization" | "location" | "item" | "proposition" | "event" } };
  validFrom: number | null;
  validUntilExclusive: number | null;
  schemaAuthority: "modelDeclared";
}
export interface PropertyRuleDefinition { predicate: string; subjectKind: "character" | "organization" | "location" | "item" | "proposition" | "event"; schema: StateProperty["schema"]; description: string; exceptions: string[]; }
export type EventRole = StoryEventInterpretation["participants"][number]["role"];
export type EventCondition = { type: "all" | "any"; conditions: EventCondition[] }
  | { type: "not"; condition: EventCondition }
  | { type: "state"; role: EventRole; subjectKind: PropertyRuleDefinition["subjectKind"]; predicate: string;
      quantifier: "all" | "any"; operator: "eq" | "ne" | "lt" | "le" | "gt" | "ge";
      operand: { type: "boolean"; value: boolean } | { type: "integer"; value: number } | { type: "text"; value: string }
        | { type: "participant"; role: EventRole; quantifier: "all" | "any" } };
export interface EventRuleDefinition { eventType: string; description: string; exceptions: string[]; condition: EventCondition; }
export interface PropertyRulesPayload { definitions: PropertyRuleDefinition[]; eventRules: EventRuleDefinition[]; }
export interface PropertyRulesSaveRequest { payload: PropertyRulesPayload; expectedRevision: number; expectedCandidateRevisionId: string | null; idempotencyKey: string; }
export interface FactProposal { id: string; sceneGroupId: string; sceneId?: string | null; factKind: "world" | "reader" | "character" | "author-secret"; subjectId: string | null; predicate: string; value: unknown; epistemicOwnerId: string | null; epistemicState: "unknown" | "suspected" | "known" | "false"; storyTime?: number | null; stateProperty?: StateProperty; storyEvent?: StoryEventInterpretation; evidence: string; source: "author" | "agent"; reviewDecision: "candidate" | "confirmed" | "rejected"; readingChapter: number; sceneOrder: number; }
export interface ProvisionalPlanningImpact { id: string; factProposalId: string; predicate: string; evidence: string; targetArtifactKind: "outline" | "plan"; targetRevisionId: string; targetNodeId: string; targetLabel: string; dependencyKey: string; severity: "affected" | "locked-conflict"; matchedBy: "dependency-key" | "author-link"; }
export interface CreativeReview { id: string; artifactKind: CreativeArtifactKind | "draft" | "identity" | "propertyRules"; artifactRevisionId: string; projectRevision: number; status: "ready" | "stale" | "accepted" | "rejected"; checks: { id: string; passed: boolean; blocking: boolean; message: string }[]; selections: Record<string, unknown>; }
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
  authorMessageId: string;
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
export interface WriterContextPreview {
  id: string; purpose: "writer"; taskId: null; snapshotId: string; contentHash: string;
  target: { projectId: string; projectRevision: number; position: { readingChapter: number; sceneOrder: number; storyTime: number | null; povCharacterId: string | null } };
  plannedEffects: { source: "plan"; value: { planRevisionId: string; chapters: Partial<WriterChapterProjection>[] } }[];
  relevantText: { objectId: string; sourceRevisionId: string; authority: "accepted" | "candidate" | "working"; workingSelectionRevision?: number | null;
    content: { chapterId: string; chapterNumber: number; title: string; body: string } }[];
  coverage: { complete: boolean; semanticCompleteness: "unknown"; missingScope: string[]; truncationReasons: string[];
    scannedScope: { factsIncluded: number; draftsIncluded: number } };
  budgetsUsed: { byteLimit: number; byteSize: number; entryLimit: number };
}
export interface CreativeWorkspace {
  project: { id: string; title: string; logline: string };
  revision: number;
  headSnapshotId: string;
  sourceDigest: string;
  planWindow: number;
  planLineageStatus?: "missing" | "current" | "stale";
  index: { targetRevision: number; indexedRevision: number; status: "ready" | "stale" };
  artifacts: Record<CreativeArtifactKind, { candidate: CreativeRevision | null; accepted: CreativeRevision | null; working: CreativeRevision | null; locked: boolean; workingSelection: WorkingBaselineSelection | null }> & { identity: { candidate: CreativeRevision<{ decisions: IdentityDecision[] }> | null; accepted: CreativeRevision<{ decisions: IdentityDecision[] }> | null; working: null; locked: boolean; workingSelection: null }; propertyRules: { candidate: CreativeRevision<PropertyRulesPayload> | null; accepted: CreativeRevision<PropertyRulesPayload> | null; working: null; locked: boolean; workingSelection: null } };
  chapters: ({ id: string; number: number; title: string; status: string; summary: string; updated_at: string } & { candidate: DraftRevision | null; accepted: DraftRevision | null; working: DraftRevision | null; workingSelection: WorkingBaselineSelection | null })[];
  workingBaselines: WorkingBaselineSelection[];
  continuationIntents: { targetKind: CreativeArtifactKind | "draft"; targetId: string; workingRevisionId: string; authorIntent: string; status: "ready" | "launched" | "failed"; taskId: string | null; updatedAt: string }[];
  planningReviews: PlanningReviewRun[];
  modeling: { objectId: string; sourceRevisionId: string | null; status: "ready" | "model_pending"; batchId: string | null; entities: unknown[]; assertions: unknown[]; confirmationState: "candidate" }[];
  factProposals: (FactProposal & { draft_revision_id: string })[];
  reviews: CreativeReview[];
  impacts: PlanningImpact[];
  openImpactCount?: number;
  chapterPlanBindings?: { project_id: string; plan_revision_id: string; chapter_plan_id: string; chapter_id: string; lineageStatus: "current" | "historical"; writerProjection: WriterChapterProjection }[];
  commits: { id: string; operation_kind: "accept" | "materialize-chapter" | "materialize-standalone" | "associate-plan" | "resolve-impact" | "undo"; proposalApplicationId: string | null; result_revision: number; receipt: Record<string, unknown> }[];
  tasks: {
    id: string; task_kind: string; purpose: "planner" | "writer" | "critic"; status: string;
    stage: string; delivery_status: "pending" | "ready_for_review" | "incomplete";
    thread_id: string; input_revision: number; updated_at: string; error?: string | null;
    target: { type?: string; chapterId?: string; chapterNumber?: number; chapterTitle?: string; sceneOrder?: number; scope?: { type: "wholeChapter" } | { type: "throughScene"; endSceneOrder: number }; povCharacterId?: string; storyTime?: number };
    outputRefs: { kind: string; revisionId: string; chapterId?: string; workingBaseRevisionId?: string }[];
  }[];
}

export interface ProposalSetSummary { id: string; intent: string; createdAt: number; status: "proposed" | "applied" | "undone"; applicationId: string | null }
export interface ProposalSetDetail {
  id: string; projectId: string; baseSnapshotId: string; baseProjectRevision: number; intent: string; createdAt: number;
  status: "ready" | "stale" | "applied" | "undone";
  operations: { operationId: string; reviewId: string; objectId: string; objectKind: string; candidateRevisionId: string; expectedAcceptedRevisionId: string | null; dependsOn: string[] }[];
  reviewOptions: Record<string, { sceneGroupIds: string[]; factProposals: FactProposal[]; selections: Record<string, unknown> }>;
  selection: { operationIds: string[]; missingDependencies: { operationId: string; requires: string }[]; declaredClosureReady: boolean; applyReady: boolean; selectionsByOperationId?: Record<string, Record<string, unknown>> };
  staleOperations: string[];
  application: { id: string; status: "applied" | "undone"; resultRevision: number; resultSnapshotId: string; undoneByApplicationId: string | null; selectedOperationIds: string[] } | null;
  coverage: { dependencyClosure: "declaredOnly"; semanticDependenciesComplete: false };
}
export interface ImpactAnalysisResult {
  dependencyRuleVersion: string;
  baseSnapshotId: string;
  canonicalEntityId: string;
  findings: {
    affectedEntityId: string;
    classification: "possibleImpact";
    hopCount: number;
    evidencePath: { fromId: string; toId: string; relationType: string; dependencyRuleId: string; sourceObjectId: string; sourceRevisionId: string; evidence: { quote: string } }[];
  }[];
  literalTextCandidates: { sourceObjectId: string; sourceRevisionId: string; sourcePath: string; matchedName: string; evidence: { quote: string }; snippet: string }[];
  revisionDelta: null | {
    bodyChanged: boolean;
    sceneChanges: { sceneId: string; status: string }[];
    modelCandidateDelta: {
      status: "unavailable" | "comparedCandidates";
      candidateChanges: { change: "newlyExtracted" | "noLongerExtracted"; claim: { subjectId?: string; targetId?: string; predicate?: string; value?: unknown }; sourceRevisionId: string; modelingBatchId: string; evidence: { quote: string } }[];
      coverage: { from: string; to: string; complete: boolean; truncated?: boolean; totalCandidateChanges?: number; evidenceReanchoredClaimCount?: number };
    };
  };
  coverage: { globalImpactComplete: false; explicitGraphComplete: boolean; truncatedPaths: boolean; graphCoverage: { complete: boolean; pendingSourceCount?: number }; literalSearch: { literalNameSearchComplete: boolean; namesTruncated: boolean; resultsTruncated: boolean } };
}
export interface HypothesisResult {
  id: string;
  baseSnapshotId: string;
  intent: string;
  authority: "hypothesisOnly";
  baseline: { entryCount: number };
  simulation: {
    entries: { canonicalSubjectId: string; confirmationState: string; assertion: { predicate: string; value: unknown; storyTime: number | null } }[];
    findings: ({ subjectId: string; predicate: string; confirmationState: string;
      leftSourceRevisionId: string | null; rightSourceRevisionId: string | null;
      leftEvidence: { quote: string } | null; rightEvidence: { quote: string } | null;
    } & ({ type: "polarityConflict" } | { type: "epistemicDisagreement"; ownerId: string;
      propositionRef: { batchId: string; propositionId: string }; leftState: string; rightState: string;
    }))[];
    coverage: { projectionComplete: boolean; globalImpactComplete: false };
  };
}
export interface HistoricalKnowledgeState {
  snapshotId: string;
  entries: {
    canonicalSubjectId: string | null;
    sourceObjectId: string;
    sourceRevisionId: string;
    modelingBatchId: string;
    assertionIndex: number;
    confirmationState: "candidate" | "confirmed";
    assertion: { kind: string; predicate?: string; value?: unknown; storyTime?: number | null; evidence?: { quote?: string } };
  }[];
  coverage: { complete: boolean; truncatedEntries: boolean; pendingSources: string[] };
}
export type HypothesisAssumption =
  | { kind: "suppressClaim"; modelingBatchId: string; sourceRevisionId: string; assertionIndex: number }
  | { kind: "assumeClaim"; subjectId: string; predicate: string; value: boolean; storyTime: number | null };
export interface ThreadSummary {
  id: string; name: string | null; preview: string | null; modelProvider: string | null;
  status: string | null; createdAt: number | null; updatedAt: number | null;
  isPinned: boolean; ephemeral: boolean;
}
export interface ThreadItemSummary { id: string; type: string | null; status: string | null; text: string | null; }
export interface ThreadTurnSummary { id: string; status: string | null; items: ThreadItemSummary[]; }
export interface ThreadDetail { thread: ThreadSummary; turns: ThreadTurnSummary[]; }
export interface RecoveryRequest { requestId: string; turnId: string | null; kind: "model" | "read" | "write"; operation: string | null; state: string; receiptRef: string | null; hasIdempotencyKey: boolean; recovery: "none" | "resume-core" | "manual-reconciliation"; }
export interface RecoveryRun { runId: string; planId: string; threadId: string; projectId: string | null; operation: string | null; runtimeStatus: string | null; reconciledAt: string | null; requests: RecoveryRequest[]; }
export interface BackgroundTerminal { processId: string; command: string | null; cwd: string | null; status: string | null; }
export interface LocalMetricsSummary {
  scope: "local-device";
  metrics: { metric: string; count: number; totalSeconds: number }[];
}
export type CreativeIntentSource =
  | { type: "authorMessage"; authorMessageId: string; createdAt: number }
  | { type: "feedbackDecision"; decisionId: string };
export interface CreativeIntentRevision {
  id: string; generation: number; instruction: string; effectiveFromChapter: number;
  source: CreativeIntentSource;
}
export interface GenerationFeedbackDecision {
  id: string; authorMessageId: string | null; chapterId: string; chapterNumber: number;
  assessmentRevisionId: string; choice: "continue" | "repairDraft"; instruction: string;
  sourceDigest: string; decidedAt: number;
}
export interface GenerationChapterDelivery {
  chapterId: string;
  chapterNumber: number;
  draftRevisionId: string;
  planRevisionId: string;
  knowledgeBatchId: string;
  commitmentExtractionRevisionId: string;
  assessmentRevisionId: string;
  assessmentTaskId: string;
  checkedSnapshotId: string;
  sourceDigest: string;
  checkedRunVersion: number;
  feedbackDecisionId: string | null;
}
export interface GenerationRunSummary {
  id: string;
  revisionPredecessor: RevisionPredecessor | null;
  authorMessageId: string;
  usageUnsettled?: boolean;
  currentChapterId: string | null;
  status: "running" | "blocked" | "paused" | "completed" | "cancelled";
  stage: string;
  activeTaskId: string | null;
  pendingCurrentIntentGeneration: number | null;
  completedChapters: number;
  chapterDeliveries: GenerationChapterDelivery[];
  startChapterNumber: number;
  targetChapters: number;
  blockedReason: string | null;
  feedbackRepairCount: number;
  feedbackRepairPrompt: string | null;
  maxTaskAttempts: number;
  usedTaskAttempts: number;
  providerBudget: {
    maxRequests: number;
    maxTokens: number;
    usedRequests: number;
    usedTokens: number;
    unknownTokens: number;
    overrun: boolean;
    charges: { taskId: string; budgetId: string; status: string; requests: number; tokens: number; unknownTokens: number; overrun: boolean }[];
  };
}
export type ReplanStage = "brief" | "outline" | "rollingPlan" | "currentChapter";
export interface GenerationReplanPreview {
  run: GenerationRunSummary & { version: number; projectId: string; deadlineAt: number };
  projectRevision: number;
  sourceDigest: string;
  availableFrom: ReplanStage[];
  activeAttemptStatus: string | null;
}
export interface GenerationReplanRequest {
  expectedVersion: number;
  expectedProjectRevision: number;
  expectedSourceDigest: string;
  restartFrom: ReplanStage;
  instruction: string;
}
export interface SceneAssessmentSummary {
  revisionId: string;
  authority: "candidateAnalysis";
  state: "current" | "stale";
  assessment: { assessments: { sceneId: string; verdict: "realized" | "deviated" | "insufficientEvidence";
    recommendation: "continue" | "repairDraft" | "replan" | "askAuthor"; rationale: string;
    effectAssessments: SceneEffectAssessment[] }[] };
}
export interface SceneEffectAssessment {
  effectId: string;
  verdict: "realized" | "deviated" | "insufficientEvidence";
  rationale: string;
  actualEventRefs: SceneEventRef[];
  commitmentAssessments: CommitmentAssessment[];
}
export interface SceneEventRef { modelingBatchId: string; assertionIndex: number }
export interface CommitmentAssessment {
  reference: PlannedCommitmentRef;
  referenceState: "current" | "sourceChanged" | "revisionSuperseded" | "authorEstablished";
  outcome: "advanced" | "partiallyFulfilled" | "fulfilled" | "contradicted" | "unchanged" | "insufficientEvidence";
  rationale: string;
  actualEventRefs: SceneEventRef[];
}
export type AuthorReadingProgress =
  | { status: "current"; version: number; pages: number; acknowledgedCharacters: number; traversal: "reading" | "acknowledgedEnd" | "budgetExhausted" }
  | { status: "stale" | "unavailable" };

export interface TaskFinalReply {
  taskId: string;
  projectId: string;
  binding: { threadId: string; turnId: string; intentRevision: number; itemId: string | null; text: string; truncated: boolean };
  state: "recorded" | "truncated" | "absent";
  sourceDigest: string;
  receiptDigest: string;
  recordedAt: number;
}

export interface RevisionOutcomeReport {
  writerTaskId: string;
  chapterId: string;
  originalRevisionId: string;
  draftRevisionId: string;
  knowledgeBatchId: string;
  outcomes: {
    id: string;
    status: "supported" | "unmet" | "insufficientEvidence";
    reason: string;
    evidence: { groupId: string; quote: string; start: number; end: number }[];
  }[];
}
export interface RevisionOutcome {
  revisionId: string;
  authority: "candidateAnalysis";
  state: "current" | "stale";
  assessment: RevisionOutcomeReport;
  decisions: RevisionOutcomeDecision[];
}

export type RevisionOutcomeChoice = "acceptRevision" | "repairDraft" | "defer";
export interface RevisionOutcomeDecisionRequest {
  decisionId: string;
  expectedAssessmentRevisionId: string;
  expectedDecisionId: string | null;
  choice: RevisionOutcomeChoice;
  note: string;
}
export interface RevisionOutcomeDecision {
  id: string;
  writerTaskId: string;
  assessmentRevisionId: string;
  draftRevisionId: string;
  knowledgeBatchId: string;
  choice: RevisionOutcomeChoice;
  note: string;
  createdAt: number;
}
export interface RevisionPredecessor {
  writerTaskId: string;
  assessmentRevisionId: string;
  decisionId: string;
}
export interface RevisionRepairPreparation {
  projectId: string;
  source: RevisionPredecessor;
  chapterId: string;
  draftRevisionId: string;
  knowledgeBatchId: string;
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

export interface WorkTaskSummary {
  id: string; projectId: string; threadId: string; goal: string; kind: "conversation" | "direct" | "generation";
  status: "running" | "paused" | "blocked" | "completed" | "cancelled"; deliveryStatus: "pending" | "partial" | "ready_for_review" | "discussion_ended"; version: number; error: string | null;
  deliveryReport: {
    evidence: { stepId: string; attemptId: string | null; kind: string; referenceId: string | null;
      state: "pending" | "missing" | "recorded" | "superseded" }[];
    pendingActionIds: string[]; unsettledAttemptIds: string[];
  };
  resumeBlockReason: "partialOutput" | "inputChanged" | "attemptBudget" | null;
  steps: { id: string; operation: string; expectedOutputKind: string | null; instruction: string | null; sourceTaskId: string | null; target: ActionTarget;
    status: "queued" | "running" | "completed" | "blocked" | "cancelled" | "superseded"; inputRevision: number;
    dependsOnStepId: string | null; currentAttemptId: string | null;
    readingProgress: AuthorReadingProgress | null;
    attempts: { id: string; attemptNumber: number; startedAt: number; completedAt: number | null; threadId: string; turnId: string | null; status: string; error: string | null }[];
    outputRefs: { kind: string; revisionId: string; objectId?: string; workingBaseRevisionId?: string }[];
    changes: { commitId: string; operation: string; baseRevision: number; resultRevision: number; undoneByCommitId: string | null }[] }[];
}

export interface SearchAuditJobSummary {
  id: string; projectId: string; status: "queued" | "running" | "paused" | "cancelled" | "stale" | "failed" | "completed";
  version: number; sourceCount: number; indexGeneration: number; nextRowid: number;
  verifiedConsistent: boolean | null; error: string | null;
  audit: { revision_count: number; document_count: number; gram_count: number; mismatch_count: number; orphaned_documents: number; orphaned_grams: number; mismatched_revision_ids: string[]; index_generation: number };
}

export interface EntityAuditJobSummary {
  id: string; projectId: string; status: "queued" | "running" | "paused" | "cancelled" | "stale" | "failed" | "completed";
  version: number; sourceCount: number; indexGeneration: number; nextRowid: number;
  verifiedConsistent: boolean | null; error: string | null;
  audit: { publishedBatches: number; expectedClaims: number; expectedNames: number;
    missingClaims: number; extraClaims: number; changedClaims: number; missingNames: number; extraNames: number;
    affectedBatchIds: string[] };
}

export interface RelationAuditJobSummary {
  id: string; projectId: string; status: "queued" | "running" | "paused" | "cancelled" | "stale" | "failed" | "completed";
  version: number; sourceCount: number; indexGeneration: number; nextRowid: number;
  verifiedConsistent: boolean | null; error: string | null;
  audit: { publishedBatches: number; expectedClaims: number;
    missingClaims: number; extraClaims: number; changedClaims: number; affectedBatchIds: string[] };
}

export interface KnowledgeAuditJobSummary {
  id: string; projectId: string; status: "queued" | "running" | "paused" | "cancelled" | "stale" | "failed" | "completed";
  version: number; sourceCount: number; indexGeneration: number; nextRowid: number;
  entityGeneration: number;
  verifiedConsistent: boolean | null; error: string | null;
  audit: {
    entities: EntityAuditJobSummary["audit"];
    assertions: { publishedBatches: number; expectedClaims: number; missingClaims: number; extraClaims: number; changedClaims: number; missingDependencies: number; extraDependencies: number; affectedBatchIds: string[] };
    epistemic: { publishedBatches: number; expectedClaims: number; missingClaims: number; extraClaims: number; changedClaims: number; affectedBatchIds: string[] };
    heads: { publishedBatches: number; expectedHeads: number; missingHeads: number; changedHeads: number; extraHeads: number;
      invalidReadyBatches: number; ambiguousReadyBatches: number; affectedObjectIds: string[] };
  };
}

export interface SnapshotKnowledgeAuditJobSummary {
  id: string; projectId: string; status: "queued" | "running" | "paused" | "cancelled" | "stale" | "failed" | "completed";
  version: number; sourceCount: number; sourceMaxRowid: number; sourceGeneration: number; nextRowid: number;
  verifiedConsistent: boolean | null; error: string | null;
  audit: { members: number; readyCaptures: number; pendingCaptures: number; missingCaptures: number;
    invalidCaptures: number; invalidMembers: number; orphanCaptures: number; affectedMembers: string[] };
}

export interface FactAuditJobSummary {
  id: string; projectId: string; status: "queued" | "running" | "paused" | "cancelled" | "stale" | "completed";
  version: number; sourceCount: number; sourceMaxRowid: number; sourceGeneration: number; nextRowid: number;
  verifiedConsistent: boolean | null; error: string | null;
  audit: { facts: number; invalidLinks: number; invalidPayloads: number; affectedFacts: string[] };
}

export interface CommitmentExtractionOrigin { chapterId: string; reportRevisionId: string; proposalId: string }
export interface NarrativeCommitment {
  id: string; kind: "foreshadowing" | "suspense" | "characterTransformation" | "goal" | "setPiece";
  summary: string; fulfillmentCriteria: string; relatedEntityIds: string[]; plotThreadId: string | null; characterArcId: string | null;
  setupEvidenceRefs: CommitmentEvidence[]; fulfillmentEvidenceRefs: CommitmentEvidence[];
  targetWindow: { kind: "open" } | { kind: "chapterRange"; fromChapterId: string; throughChapterId: string };
  status: "open" | "partiallyFulfilled" | "fulfilled" | "abandoned"; decisionReason: string | null;
}
export interface CommitmentEvidence { objectId: string; revisionId: string; quote: string; start: number; end: number }
export interface CommitmentExtractionReport {
  established: { proposalId: string; commitmentId: string }[];
  revisionId: string; snapshotId: string; projectRevision: number; authority: "candidateProposalBatch"; sourceCurrent: boolean;
  payload: { chapterId: string; sourceRevisionId: string; examinedRanges: { start: number; end: number }[]; candidates: NarrativeCommitment[] };
  coverage: { semanticCompleteness: "unknown"; modelReportedRanges: true; literaryFulfillmentVerified: false; globalCompletenessClaimed: false };
}
export interface CommitmentDetail {
  authorControls: { projectRevision: number; undoAvailable: boolean } | null; confirmationCommitId: string | null;
  projectId: string; snapshotId: string; objectId: string; revisionId: string; payload: NarrativeCommitment;
  extractionOrigin: CommitmentExtractionOrigin | null; sourceCurrent: boolean; confirmationState: "candidate" | "confirmed";
  interpretation: "sourceBoundNarrativeClaim"; literaryFulfillmentVerified: false;
}
export interface CommitmentPage {
  snapshotId: string; nextCursor: string | null;
  data: { id: string; summary: string; kind: NarrativeCommitment["kind"]; status: NarrativeCommitment["status"]; revisionId: string; confirmationState: "candidate" | "confirmed"; sourceCurrent: boolean }[];
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
    listProposalSets: (projectId: string, cursor?: string | null) => Promise<{ data: ProposalSetSummary[]; nextCursor: string | null }>;
    createProposalSet: (projectId: string, request: { baseSnapshotId: string; intent: string; operations: { operationId: string; reviewId: string; dependsOn: string[] }[]; idempotencyKey: string }) => Promise<{ id: string; baseSnapshotId: string; replayed: boolean }>;
    getProposalSet: (projectId: string, proposalSetId: string) => Promise<ProposalSetDetail>;
    previewProposalSet: (projectId: string, proposalSetId: string, selectedOperationIds: string[], selectionsByOperationId: Record<string, Record<string, unknown>>) => Promise<ProposalSetDetail>;
    applyProposalSet: (projectId: string, proposalSetId: string, request: { selectedOperationIds: string[]; selectionsByOperationId: Record<string, Record<string, unknown>>; idempotencyKey: string }) => Promise<{ applicationId: string; resultRevision: number; resultSnapshotId: string; replayed: boolean }>;
    undoProposalSet: (projectId: string, applicationId: string, expectedProjectRevision: number, idempotencyKey: string) => Promise<{ undoApplicationId: string; resultRevision: number; resultSnapshotId: string; replayed: boolean }>;
    getImpactAnalysis: (projectId: string, options: { entityId: string; fromRevisionId?: string | null; toRevisionId?: string | null; maxHops?: number; limit?: number }) => Promise<ImpactAnalysisResult>;
    getHistoricalKnowledgeState: (projectId: string, snapshotId: string) => Promise<HistoricalKnowledgeState>;
    createHypothesis: (projectId: string, request: { baseSnapshotId: string; intent: string; assumptions: HypothesisAssumption[]; idempotencyKey: string }) => Promise<{ id: string; baseSnapshotId: string; replayed: boolean }>;
    getHypothesis: (projectId: string, hypothesisId: string) => Promise<HypothesisResult>;
    getIdentityCatalog: (projectId: string) => Promise<IdentityCatalog>;
    saveIdentityDecisions: (projectId: string, decisions: IdentityDecision[], expectedRevision: number, expectedCandidateRevisionId: string | null) => Promise<{ artifactKind: "identity"; revisionId: string; version: number; payload: { decisions: IdentityDecision[] } }>;
    savePropertyRules: (projectId: string, request: PropertyRulesSaveRequest) => Promise<{ artifactKind: "propertyRules"; revisionId: string; version: number; payload: PropertyRulesPayload }>;
    listGenerationRuns: (projectId: string) => Promise<{ runs: GenerationRunSummary[] }>;
    previewGenerationReplan: (projectId: string, runId: string) => Promise<GenerationReplanPreview>;
    replanGeneration: (projectId: string, runId: string, request: GenerationReplanRequest) => Promise<GenerationRunSummary>;
    getSceneAssessment: (projectId: string, chapterId: string) => Promise<SceneAssessmentSummary>;
    getCommitmentExtraction: (projectId: string, chapterId: string) => Promise<CommitmentExtractionReport | null>;
    establishCommitment: (projectId: string, chapterId: string, request: { reportRevisionId: string; proposalId: string; baseSnapshotId: string; expectedRevision: number; idempotencyKey: string }) => Promise<{ commitmentId: string; revisionId: string; snapshotId: string; confirmationState: "candidate" }>;
    getCommitment: (projectId: string, id: string, snapshotId?: string | null) => Promise<CommitmentDetail>;
    listCommitments: (projectId: string, cursor?: string | null, limit?: number) => Promise<CommitmentPage>;
    saveCommitment: (projectId: string, id: string, request: { payload: NarrativeCommitment; extractionOrigin: CommitmentExtractionOrigin | null; expectedRevision: number; baseSnapshotId: string; expectedCandidateRevisionId: string | null; idempotencyKey: string }) => Promise<{ revisionId: string; snapshotId: string }>;
    createCommitmentReview: (projectId: string, id: string, revisionId: string, idempotencyKey: string) => Promise<{ id: string; artifactKind: "commitment"; artifactRevisionId: string; projectRevision: number; status: string }>;
    listWorkTasks: (projectId: string, cursor?: string | null) => Promise<{ tasks: WorkTaskSummary[]; nextCursor: string | null; activeCount: number }>;
    getSearchAudit: (projectId: string) => Promise<{ job: SearchAuditJobSummary | null }>;
    startSearchAudit: (projectId: string) => Promise<SearchAuditJobSummary>;
    controlSearchAudit: (projectId: string, jobId: string, command: "pause" | "resume" | "cancel", expectedVersion: number) => Promise<SearchAuditJobSummary>;
    getEntityAudit: (projectId: string) => Promise<{ job: EntityAuditJobSummary | null }>;
    startEntityAudit: (projectId: string) => Promise<EntityAuditJobSummary>;
    controlEntityAudit: (projectId: string, jobId: string, command: "pause" | "resume" | "cancel", expectedVersion: number) => Promise<EntityAuditJobSummary>;
    getRelationAudit: (projectId: string) => Promise<{ job: RelationAuditJobSummary | null }>;
    startRelationAudit: (projectId: string) => Promise<RelationAuditJobSummary>;
    controlRelationAudit: (projectId: string, jobId: string, command: "pause" | "resume" | "cancel", expectedVersion: number) => Promise<RelationAuditJobSummary>;
    getKnowledgeAudit: (projectId: string) => Promise<{ job: KnowledgeAuditJobSummary | null }>;
    startConditionEvaluation: (projectId: string, target: ConditionEvaluationTarget) => Promise<ConditionEvaluationJob>;
    getConditionEvaluation: (projectId: string, jobId: string) => Promise<ConditionEvaluationJob>;
    controlConditionEvaluation: (projectId: string, jobId: string, command: "pause" | "resume" | "cancel", expectedVersion: number) => Promise<ConditionEvaluationJob>;
    getConditionExplanations: (projectId: string, jobId: string, cursor?: string | null) => Promise<ConditionExplanationPage>;
    startKnowledgeAudit: (projectId: string) => Promise<KnowledgeAuditJobSummary>;
    controlKnowledgeAudit: (projectId: string, jobId: string, command: "pause" | "resume" | "cancel", expectedVersion: number) => Promise<KnowledgeAuditJobSummary>;
    getSnapshotKnowledgeAudit: (projectId: string) => Promise<{ job: SnapshotKnowledgeAuditJobSummary | null }>;
    startSnapshotKnowledgeAudit: (projectId: string) => Promise<SnapshotKnowledgeAuditJobSummary>;
    controlSnapshotKnowledgeAudit: (projectId: string, jobId: string, command: "pause" | "resume" | "cancel", expectedVersion: number) => Promise<SnapshotKnowledgeAuditJobSummary>;
    getFactAudit: (projectId: string) => Promise<{ job: FactAuditJobSummary | null }>;
    startFactAudit: (projectId: string) => Promise<FactAuditJobSummary>;
    controlFactAudit: (projectId: string, jobId: string, command: "pause" | "resume" | "cancel", expectedVersion: number) => Promise<FactAuditJobSummary>;
    getWorkTask: (projectId: string, workTaskId: string) => Promise<WorkTaskSummary>;
    getTaskFinalReply: (projectId: string, taskId: string) => Promise<TaskFinalReply | null>;
    getRevisionOutcome: (projectId: string, writerTaskId: string) => Promise<RevisionOutcome | null>;
    decideRevisionOutcome: (projectId: string, writerTaskId: string, request: RevisionOutcomeDecisionRequest) => Promise<RevisionOutcomeDecision>;
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
    rebindChapterPlan: (projectId: string, request: ChapterPlanRebindRequest) => Promise<ChapterPlanRebindReceipt>;
    resolvePlanningImpact: (projectId: string, impactId: string, expectedRevision: number, resolution: "kept" | "false-positive", note?: string) => Promise<{ commitId: string; resultRevision: number }>;
    setArtifactLock: (projectId: string, kind: CreativeArtifactKind, locked: boolean, expectedRevision: number) => Promise<{ artifactKind: CreativeArtifactKind; locked: boolean; resultRevision: number }>;
    saveDraftCheckpoint: (projectId: string, payload: Record<string, unknown>, expectedRevision: number, expectedCandidateRevisionId?: string | null) => Promise<{ chapterId: string; revisionId: string; version: number; body: string }>;
    createReview: (projectId: string, artifactKind: CreativeArtifactKind | "draft" | "identity" | "propertyRules", revisionId: string, proposals?: Record<string, unknown>[], selections?: Record<string, unknown>, modelingBatchId?: string | null) => Promise<{ id: string; artifactKind: CreativeArtifactKind | "draft" | "identity" | "propertyRules"; artifactRevisionId: string; projectRevision: number; status: string; checks: CreativeReview["checks"]; factProposals: FactProposal[]; provisionalImpacts: ProvisionalPlanningImpact[] }>;
    rejectIdentityReview: (projectId: string, reviewId: string, expectedRevision: number) => Promise<{ reviewId: string; status: "rejected" }>;
    acceptReview: (projectId: string, reviewId: string, expectedRevision: number, selections?: Record<string, unknown>) => Promise<{ commitId: string; resultRevision: number; impacts: CreativeWorkspace["impacts"] }>;
    undoCommit: (projectId: string, commitId: string, expectedRevision: number) => Promise<{ commitId: string; resultRevision: number }>;
    getDiffs: (projectId: string, chapterId: string) => Promise<{ diffs: ChapterDiff[] }>;
    runStoryTests: (projectId: string) => Promise<StoryTestResult>;
    search: (projectId: string, query: string, limit?: number, kind?: SearchKind, cursor?: string) => Promise<SearchResponse>;
  };
  agent: {
    setModelingContext: (context: { projectId: string | null; dirty: boolean }) => Promise<{ selected: string | null }>;
    status: () => Promise<{ configured: boolean; state: string; mode: "novel-agent-engine"; error: string | null; provider?: string; model?: string; commandSource?: string | null; cliPath?: string | null; isolatedHome?: string | null }>;
    recoveryReport: () => Promise<RecoveryRun[]>;
    resolveRecovery: (projectId: string, runId: string) => Promise<{ resolved: true; runId: string }>;
    listConnections: () => Promise<{ connections: { id: string; provider: string; model: string; protocol: "auto" | "responses" | "chat-completions" | "messages" | "generate-content"; selectedProtocol: string | null; declaredCapabilities: string[]; verifiedCapabilities: string[]; failedCapabilities: string[]; operations: string[]; status: "unverified" | "verified" | "failed"; certificationIds: string[]; accountStatus: "configured" | "missing-credential"; isDefault: boolean }[] }>;
    verifyConnection: (provider: string, model: string) => Promise<{ certificationIds?: string[]; reportId: string; selectedProtocol: string; attempts: { protocol: string; status: string; category?: string; message?: string }[]; records: { capability: string; status: string; scope: unknown; checkedAt: string; expiresAt: string }[]; access?: { status: "available"; protocol: string } }>;
    verifyConnectionEdge: (parentProvider: string, parentModel: string, childProvider: string, childModel: string) => Promise<{ reportId: string; parentConnectionId: string; childConnectionId: string; selectedProtocols: { parent: string; child: string }; attempt: { protocol: string; status: string; category?: string; message?: string }; record: { capability: string; status: string; scope: unknown; checkedAt: string; expiresAt: string } }>;
    configure: (provider: string, model: string) => Promise<{ configured: boolean; state: string; mode: "novel-agent-engine"; error: string | null; provider?: string; model?: string; commandSource?: string | null; cliPath?: string | null; isolatedHome?: string | null }>;
    sendPrompt: (projectId: string, prompt: string, repairSource?: RevisionPredecessor | null) => Promise<{ mode: string; message: string; projectId: string; proposedActions: ProposedAction[]; workflow?: { task?: unknown; context?: unknown; revision?: number; headSnapshotId?: string; sourceDigest?: string } }>;
    listProposedActions: (projectId: string) => Promise<{ actions: ProposedAction[] }>;
    readActionPermission: (projectId: string, sourceTaskId: string, actionId: string) => Promise<{ permission: ActionPermission | null }>;
    readExecutionScope: (projectId: string, sourceTaskId: string) => Promise<{ scope: ExecutionScopePermit | null }>;
    previewExecutionScope: (projectId: string, sourceTaskId: string, request: ExecutionScopeRequest) => Promise<ExecutionScopePreview>;
    confirmExecutionScope: (projectId: string, sourceTaskId: string, request: ExecutionScopeRequest, previewDigest: string) => Promise<ExecutionScopeConfirmation>;
    approveAction: (projectId: string, sourceTaskId: string, actionId: string) => Promise<{ action: ProposedAction; taskId: string; modeling: { status: string; error?: string } | null; outcome: { status: "recorded" | "pending" | "paused"; taskId?: string; error?: string } | null; followup: { status: "paused" | "pending"; error?: string } | null }>;
    declineAction: (projectId: string, sourceTaskId: string, actionId: string) => Promise<ProposedAction>;
    generatePlanning: (projectId: string, operation: "generateBrief" | "generateOutline" | "generatePlan", intent: string) => Promise<{ mode: string; message: string; projectId: string; workflow?: { task?: unknown; context?: unknown; revision?: number } }>;
    generateWork: (projectId: string, intent: string, targetChapters: number, expectedProjectRevision: number, expectedBaseSnapshotId: string, expectedSourceDigest: string, revisionPredecessor: RevisionPredecessor | null) => Promise<GenerationRunSummary>;
    continueWork: (projectId: string, runId: string) => Promise<GenerationRunSummary>;
    retryWork: (projectId: string, runId: string) => Promise<GenerationRunSummary>;
    resolveGenerationFeedback: (projectId: string, runId: string, assessmentRevisionId: string, choice: "continue" | "repairDraft", instruction: string) => Promise<GenerationRunSummary>;
    addFutureIntent: (projectId: string, runId: string, instruction: string) => Promise<{ id: string; intentGeneration: number; intentRevisions: CreativeIntentRevision[] }>;
    reviseCurrentChapter: (projectId: string, runId: string, instruction: string) => Promise<{ id: string; intentGeneration: number; pendingCurrentIntentGeneration: number }>;
    generateChapter: (projectId: string, request: { chapterId: string; chapterNumber: number; sceneOrder?: number; povCharacterId?: string | null; storyTime?: number | null; intent: string }) => Promise<{ mode: string; message: string; projectId: string; workflow?: { task?: unknown; target?: unknown; context?: unknown; revision?: number }; completion?: { deliveryStatus: string; outputRefs: { kind: string; revisionId: string; chapterId?: string; workingBaseRevisionId?: string }[] } | null }>;
    resumeKnowledge: (projectId: string, chapterId: string) => Promise<{ status: "ready"; batchId?: string; skipped?: boolean }>;
    resumeWorkTask: (projectId: string, workTaskId: string) => Promise<WorkTaskSummary>;
    controlWorkTask: (projectId: string, workTaskId: string, command: "pause" | "cancel") => Promise<WorkTaskSummary>;
    reviewPlanning: (projectId: string, request: PlanningReviewStartRequest) => Promise<{ projectId: string; run: PlanningReviewRun }>;
    steer: (projectId: string, prompt: string) => Promise<{ steered: boolean }>;
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
