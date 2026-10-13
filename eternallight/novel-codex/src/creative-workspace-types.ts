import type { ChapterPlanPayload, RevisionPredecessor, ViewId } from "./types";
export type WorkspaceView = Extract<ViewId, "brief" | "outline" | "plans" | "chapters">;
export type EditableChapterPlan = Omit<ChapterPlanPayload, "dependencyKeys"> & { id: string; dependencyKeys: string };
export type EditorSession = {
  identity: string;
  baseProjectRevision: number;
  baseCandidateRevisionId: string | null;
  baseContent: string;
};
export type DirectionDecision = "" | "return-draft" | "accept-and-open-impact" | "false-positive";
export type DirectionDecisionValue = { decision: DirectionDecision; note: string };
export type PlanningTarget = { key: string; kind: "outline" | "plan"; revisionId: string; nodeId: string; label: string; currentArrangement: string; protected: boolean; protectionLabel: string };
export type ReviewUiSnapshot = {
  factDecisions: Record<string, "confirmed" | "rejected">;
  noConfirmedChanges: boolean;
  factDependencyTargets: Record<string, string[]>;
  directionChangeDecisions: Record<string, DirectionDecisionValue>;
  manualDirectionDecisions: Record<string, DirectionDecisionValue>;
  selectedSceneGroups: string[];
};

export type GenerationPermitReview = {
  projectId: string;
  intent: string;
  targetChapters: number;
  expectedProjectRevision: number;
  expectedBaseSnapshotId: string;
  expectedSourceDigest: string;
  revisionPredecessor: RevisionPredecessor | null;
};
