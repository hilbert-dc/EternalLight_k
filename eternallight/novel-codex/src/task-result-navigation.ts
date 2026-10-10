import type { NovelCodexApi, SavedAnalysis, ViewId } from "./types";
import { getTaskResults, type TaskResultDestination } from "./task-results";

export type ResolvedTaskResult = { result: TaskResultDestination; chapterId?: string; sourceRevisionId?: string } &
  ({ view: ViewId; analysis?: never } | { view?: never; analysis: SavedAnalysis });

/** Re-read delivery and its exact source before opening an author decision. Never substitutes the latest output. */
export async function resolveTaskResult(api: NovelCodexApi, result: TaskResultDestination): Promise<ResolvedTaskResult> {
  const task = await api.runtime.getWorkTask(result.projectId, result.workTaskId);
  if (!getTaskResults(task).some(current => Object.keys(current).length === Object.keys(result).length && Object.entries(current).every(([key, value]) => value === (result as unknown as Record<string, unknown>)[key]))) {
    throw new Error("这项任务结果已变化或被后续版本替换，请从当前结果重新选择。");
  }
  if (result.kind === "planningReview") {
    const run = await api.runtime.getPlanningReview(result.projectId, result.runId);
    const reports = run.reports.filter(report => report.id === result.referenceId && report.runId === run.id && report.role === result.role);
    if (run.projectId !== result.projectId || run.id !== result.runId || run.workTaskId !== result.workTaskId
      || run.artifactKind !== result.artifactKind || run.revisionId !== result.sourceRevisionId || reports.length !== 1
      || !run.seats.some(seat => seat.role === result.role && seat.taskId === result.attemptId)) {
      throw new Error("这份规划报告的任务或来源不一致，未打开其他报告代替。");
    }
    return { result, analysis: { kind: "planningReview", run, reportId: result.referenceId } };
  }
  if (result.kind === "hypothesis") {
    const analysis = await api.runtime.getHypothesis(result.projectId, result.referenceId);
    if (analysis.projectId !== result.projectId || analysis.id !== result.referenceId || analysis.authority !== "hypothesisOnly"
      || typeof analysis.baseSnapshotId !== "string" || !analysis.baseSnapshotId.trim()) {
      throw new Error("这份假设分析的作品或固定快照无效。");
    }
    return { result, analysis: { kind: "hypothesis", result: analysis } };
  }
  const workspace = await api.runtime.getCreativeWorkspace(result.projectId);
  if (workspace.project.id !== result.projectId) throw new Error("结果不属于当前作品。");
  if (result.kind === "brief" || result.kind === "outline" || result.kind === "plan" || result.kind === "propertyRules") {
    const artifact = workspace.artifacts[result.kind];
    if (artifact.candidate?.id !== result.referenceId) throw new Error("这份结果已不是当前候选，未打开其他版本代替。");
    return { result, view: result.kind === "plan" ? "plans" : result.kind === "propertyRules" ? "world" : result.kind };
  }
  if (result.kind === "commitment") {
    const detail = await api.runtime.getCommitment(result.projectId, result.commitmentId);
    if (detail.projectId !== result.projectId || detail.payload.id !== result.commitmentId || detail.revisionId !== result.referenceId || !detail.sourceCurrent) throw new Error("这条伏笔修订或原文来源已变化，请重新读取。");
    const origin = detail.extractionOrigin?.chapterId;
    const chapters = workspace.chapters.filter(item => item.id === origin || (!origin && detail.payload.setupEvidenceRefs.some(anchor => anchor.objectId === `draft-${item.id}`)));
    const chapter = chapters.length === 1 ? chapters[0] : null;
    if (!chapter) throw new Error("未找到这条伏笔对应的章节，请在伏笔目录查看来源。");
    return { result, view: "chapters", chapterId: chapter.id };
  }
  const chapter = workspace.chapters.find(item => item.id === result.chapterId);
  const source = chapter?.candidate ?? chapter?.accepted;
  if (!chapter || !source) throw new Error("结果对应的章节正文不存在，请重新读取作品。");
  if (result.kind === "chapter" && source.id !== result.referenceId) throw new Error("这份正文已被后续修订替换，未打开新版本代替。");
  if (result.kind === "knowledge") {
    const batch = workspace.modeling.find(item => item.objectId === `draft-${result.chapterId}`);
    if (batch?.status !== "ready" || batch.batchId !== result.referenceId || batch.sourceRevisionId !== source.id
      || result.sourceRevisionId && result.sourceRevisionId !== source.id) throw new Error("这批故事设定的正文来源已变化，不能检查另一批结果代替。");
  }
  if (result.kind === "commitmentExtraction") {
    const report = await api.runtime.getCommitmentExtraction(result.projectId, result.chapterId, result.referenceId);
    if (report?.reportCurrent !== true || !report.sourceCurrent || report.revisionId !== result.referenceId || report.payload.chapterId !== result.chapterId
      || report.payload.sourceRevisionId !== source.id || result.sourceRevisionId && result.sourceRevisionId !== source.id) {
      throw new Error("这份伏笔报告或原文已变化，未打开最新报告代替。");
    }
  }
  return { result, view: "chapters", chapterId: chapter.id, sourceRevisionId: source.id };
}
