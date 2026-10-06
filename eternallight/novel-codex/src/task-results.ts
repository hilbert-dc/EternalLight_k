import type { PlanningReviewRole, WorkTaskSummary } from "./types";

type ResultIdentity = { projectId: string; workTaskId: string; stepId: string; attemptId: string; referenceId: string };
type ChapterTarget = { chapterId: string; chapterNumber: number | null };
export type TaskResultDestination = ResultIdentity & (
  | ({ kind: "chapter" } & ChapterTarget)
  | ({ kind: "knowledge"; sourceRevisionId: string | null } & ChapterTarget)
  | ({ kind: "commitmentExtraction"; sourceRevisionId: string | null } & ChapterTarget)
  | { kind: "brief" } | { kind: "outline" } | { kind: "plan" } | { kind: "propertyRules" }
  | { kind: "commitment"; commitmentId: string }
  | { kind: "planningReview"; runId: string; role: PlanningReviewRole; artifactKind: "outline" | "plan"; sourceRevisionId: string }
  | { kind: "hypothesis" }
);
const contracts: Record<string, { kind: TaskResultDestination["kind"]; evidence: string }> = {
  generateChapter: { kind: "chapter", evidence: "draft" },
  modelKnowledge: { kind: "knowledge", evidence: "knowledgeBatch" },
  generateBrief: { kind: "brief", evidence: "brief" },
  generateOutline: { kind: "outline", evidence: "outline" },
  generatePlan: { kind: "plan", evidence: "plan" },
  generatePropertyRules: { kind: "propertyRules", evidence: "propertyRules" },
  extractCommitments: { kind: "commitmentExtraction", evidence: "commitmentExtraction" },
  reviseCommitment: { kind: "commitment", evidence: "commitment" },
  reviewPlanningSeat: { kind: "planningReview", evidence: "planningReview" },
};
const identifier = (value: unknown): value is string => typeof value === "string" && value.trim() === value
  && value.length > 0 && new TextEncoder().encode(value).length <= 160 && !/[\p{Cc}]/u.test(value);

/** Navigation describes verified delivery, never permissions or a guess from a model reply. */
export function getTaskResults(task: WorkTaskSummary): TaskResultDestination[] {
  if (!identifier(task.projectId) || !identifier(task.id)) return [];
  const results: TaskResultDestination[] = [];
  for (const step of task.steps) {
    if (step.operation === "advisory" && identifier(step.id) && identifier(step.currentAttemptId)
      && step.status !== "superseded" && step.attempts.filter(attempt => attempt.id === step.currentAttemptId).length === 1
      && step.attempts.some(attempt => attempt.id === step.currentAttemptId && ["completed", "failed", "cancelled"].includes(attempt.status))) {
      for (const evidence of task.deliveryReport.evidence.filter(item => item.stepId === step.id && item.attemptId === step.currentAttemptId
        && item.kind === "hypothesis" && item.state === "recorded" && identifier(item.referenceId))) {
        if (task.deliveryReport.evidence.filter(item => item.kind === "hypothesis" && item.referenceId === evidence.referenceId).length !== 1) continue;
        results.push({ projectId: task.projectId, workTaskId: task.id, stepId: step.id, attemptId: step.currentAttemptId,
          referenceId: evidence.referenceId!, kind: "hypothesis" });
      }
    }
    const contract = Object.hasOwn(contracts, step.operation) ? contracts[step.operation] : null;
    if (!contract || !identifier(step.id) || !identifier(step.currentAttemptId) || step.status === "superseded") continue;
    const attempts = step.attempts.filter(attempt => attempt.id === step.currentAttemptId);
    if (attempts.length !== 1 || !["completed", "failed", "cancelled"].includes(attempts[0].status)) continue;
    const evidence = task.deliveryReport.evidence.filter(item => item.stepId === step.id);
    if (evidence.length !== 1 || evidence[0].state !== "recorded" || evidence[0].kind !== contract.evidence
      || evidence[0].attemptId !== step.currentAttemptId || !identifier(evidence[0].referenceId)) continue;
    const identity = { projectId: task.projectId, workTaskId: task.id, stepId: step.id,
      attemptId: step.currentAttemptId, referenceId: evidence[0].referenceId };
    const kind = contract.kind;
    if (kind === "chapter" || kind === "knowledge" || kind === "commitmentExtraction") {
      if (!identifier(step.target.chapterId)) continue;
      const chapter = { chapterId: step.target.chapterId,
        chapterNumber: Number.isSafeInteger(step.target.chapterNumber) && Number(step.target.chapterNumber) > 0 ? Number(step.target.chapterNumber) : null };
      if (kind === "chapter") results.push({ ...identity, ...chapter, kind });
      else {
        const source = step.target.sourceRevisionId;
        if (source !== undefined && source !== null && !identifier(source)) continue;
        results.push({ ...identity, ...chapter, kind, sourceRevisionId: identifier(source) ? source : null });
      }
    } else if (kind === "commitment") {
      if (identifier(step.target.commitmentId)) results.push({ ...identity, kind, commitmentId: step.target.commitmentId });
    } else if (kind === "planningReview") {
      const { reviewRunId, role, artifactKind, revisionId } = step.target;
      if (identifier(reviewRunId) && identifier(revisionId) && (artifactKind === "outline" || artifactKind === "plan")
        && (role === "structure" || role === "character" || role === "experience")) {
        results.push({ ...identity, kind, runId: reviewRunId, role, artifactKind, sourceRevisionId: revisionId });
      }
    } else if (kind !== "hypothesis") results.push({ ...identity, kind });
  }
  return results;
}

export function taskResultLabel(result: TaskResultDestination): string {
  const chapter = "chapterNumber" in result && result.chapterNumber !== null ? `第 ${result.chapterNumber} 章` : "对应章节";
  switch (result.kind) {
    case "chapter": return `查看${chapter}正文`;
    case "knowledge": return `检查${chapter}故事设定`;
    case "commitmentExtraction": return `查看${chapter}伏笔建议`;
    case "commitment": return "检查这条伏笔修订";
    case "brief": return "检查创作简报";
    case "outline": return "检查全书大纲";
    case "plan": return "检查近章规划";
    case "propertyRules": return "检查世界规则";
    case "planningReview": return `查看${{ structure: "结构与因果", character: "人物与主题", experience: "阅读体验" }[result.role]}报告`;
    case "hypothesis": return "查看这次假设分析";
  }
}
