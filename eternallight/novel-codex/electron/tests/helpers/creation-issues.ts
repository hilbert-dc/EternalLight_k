import type { CreationIssue, CreationIssueSummary, RevisionOutcome, RevisionOutcomeReport } from "../../../src/types";

export const emptyCreationIssueSummary = (): CreationIssueSummary => ({ totalCount: 0, openCount: 0, staleCount: 0, disputedCount: 0, entries: [], hasMore: false });

export function deliveryIssuesFixture(reasons: string[]) {
  const entries: CreationIssue[] = reasons.map((reason, index) => ({ issueId: `delivery-issue-${index}`, summary: reason,
    status: "open", chapterId: null, currentDraftRevisionId: null, observationCount: 1, hasMoreObservations: false,
    observations: [{ issueId: `delivery-issue-${index}`, observationId: `delivery-observation-${index}`, taskId: "delivery-attempt", reportRevisionId: null,
      draftRevisionId: null, criterionId: null, status: "unmet", reason, source: "deliveryReview", current: true, recheckRequired: false, superseded: false, resolutionOf: [], evidence: [] }] }));
  return { issues: { totalCount: entries.length, openCount: entries.length, staleCount: 0, disputedCount: 0,
      entries: entries.slice(-2).map(entry => ({ ...entry, observations: [], hasMoreObservations: true })), hasMore: entries.length > 2 },
    unresolved: entries.map(entry => ({ issue: { issueId: entry.issueId, newIssue: { summary: entry.summary }, resolutionOf: [] },
      observationId: entry.observations[0].observationId, reason: entry.summary })) };
}

type ReportInput = Omit<RevisionOutcomeReport, "outcomes"> & {
  outcomes: Omit<RevisionOutcomeReport["outcomes"][number], "issue">[];
};
export function revisionOutcomeFixture(input: Omit<RevisionOutcome, "assessment" | "issues"> & { assessment: ReportInput }): RevisionOutcome {
  const issues: CreationIssue[] = [];
  const outcomes = input.assessment.outcomes.map(outcome => {
    if (outcome.status === "supported") return { ...outcome, issue: null };
    const issueId = `issue-${outcome.id}`;
    issues.push({ issueId, summary: outcome.reason, status: input.state === "stale" ? "stale" : "open",
      chapterId: input.assessment.chapterId, currentDraftRevisionId: input.assessment.draftRevisionId,
      observationCount: 1, hasMoreObservations: false, observations: [{ issueId, observationId: `observation-${outcome.id}`,
        taskId: input.assessment.writerTaskId, reportRevisionId: input.revisionId, draftRevisionId: input.assessment.draftRevisionId,
        criterionId: outcome.id, status: outcome.status, reason: outcome.reason, source: "revisionOutcome", current: input.state === "current", recheckRequired: input.state === "stale", superseded: false, resolutionOf: [], evidence: outcome.evidence }] });
    return { ...outcome, issue: { issueId, newIssue: { summary: outcome.reason }, resolutionOf: [] } };
  });
  return { ...input, assessment: { ...input.assessment, outcomes }, issues };
}
