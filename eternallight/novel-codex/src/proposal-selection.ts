import type { CreativeWorkspace, ProposalSetDetail } from "./types";

export const proposalKindLabels: Record<string, string> = { brief: "创作简报", outline: "全书大纲", plan: "近章规划", draft: "章节正文", identity: "身份决议" };

export function proposalReviewCandidates(workspace: CreativeWorkspace) {
  const candidates = [
    ...Object.entries(workspace.artifacts).flatMap(([kind, artifact]) => artifact.candidate && !artifact.locked
      ? [{ id: kind, kind, revisionId: artifact.candidate.id, label: proposalKindLabels[kind] ?? kind }] : []),
    ...workspace.chapters.flatMap((chapter) => chapter.candidate
      ? [{ id: `draft-${chapter.id}`, kind: "draft", revisionId: chapter.candidate.id, label: `第 ${chapter.number} 章 · ${chapter.title}` }] : []),
  ];
  return candidates.flatMap((candidate) => {
    const review = workspace.reviews.find((item) => item.artifactKind === candidate.kind && item.artifactRevisionId === candidate.revisionId
      && item.status === "ready" && item.projectRevision === workspace.revision
      && item.checks.every((check) => !check.blocking || check.passed));
    return review ? [{ ...candidate, review }] : [];
  });
}

export function defaultProposalChoices(detail: ProposalSetDetail): Record<string, Record<string, unknown>> {
  return Object.fromEntries(detail.operations.map((op) => {
    const options = detail.reviewOptions[op.operationId];
    return [op.operationId, !options ? {} : detail.status === "applied" || detail.status === "undone" ? options.selections : op.objectKind === "draft" ? {
      sceneGroupIds: options.sceneGroupIds,
      factDecisions: Object.fromEntries(options.factProposals.map((fact) => [fact.id, "candidate"])),
    } : {}];
  }));
}

export function selectedProposalChoices(ids: string[], choices: Record<string, Record<string, unknown>>) {
  return Object.fromEntries(ids.map((id) => [id, choices[id]]));
}
