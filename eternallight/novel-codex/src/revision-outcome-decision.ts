import type { RevisionOutcome, RevisionOutcomeChoice, RevisionPredecessor } from "./types";

export function revisionPredecessorIsCurrent(receipt: RevisionOutcome | null, predecessor: RevisionPredecessor,
  choice: RevisionOutcomeChoice = "acceptRevision"): boolean {
  const latest = receipt && Array.isArray(receipt.decisions) && receipt.decisions.length <= 20 ? receipt.decisions[0] : null;
  return receipt?.state === "current" && receipt.authority === "candidateAnalysis"
    && receipt.revisionId === predecessor.assessmentRevisionId
    && receipt.assessment.writerTaskId === predecessor.writerTaskId
    && latest?.id === predecessor.decisionId && latest.choice === choice
    && latest.writerTaskId === predecessor.writerTaskId
    && latest.assessmentRevisionId === receipt.revisionId
    && latest.draftRevisionId === receipt.assessment.draftRevisionId
    && latest.knowledgeBatchId === receipt.assessment.knowledgeBatchId;
}
