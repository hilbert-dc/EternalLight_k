import { withFrozenMethods } from "./frozen-methods.mjs";

export function reviewTask(run, role, body, id) {
  const stepId = `review-seat-${run.id}-${role}`;
  return withFrozenMethods({ ...body, id, projectId: run.projectId, inputRevision: run.inputRevision,
    purpose: "critic", operation: "reviewPlanningSeat",
    target: { type: "artifact", artifactKind: run.artifactKind, revisionId: run.revisionId, reviewRunId: run.id, role, resumeStepId: stepId },
    inputManifest: { authorIntent: run.authorIntent, authorMessageId: run.authorMessageId, sourcePacketHash: run.sourcePacketHash,
      planningReviewAuthorIntentBinding: { runId: run.id, authorMessageId: run.authorMessageId, role, stepId, sourcePacketHash: run.sourcePacketHash } },
  }, run.projectId);
}
