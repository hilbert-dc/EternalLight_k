import type { CommitmentExtractionReport, DiscussionExcerpt, NovelCodexApi, ViewId } from "./types";
import { resolveTaskResult } from "./task-result-navigation";
import { getCommitmentDecisionResults, getTaskResults, taskResultSummary, type CommitmentDecisionResult, type TaskResultDestination } from "./task-results";
import { discussionExcerptsError } from "../shared/discussion-excerpts.mjs";

export type TaskResultDiscussion = { result: TaskResultDestination | CommitmentDecisionResult; proposalId?: string };
type Capture = { text: string; snapshotId: string | null; objectId: string | null; view: ViewId; label: string; locator: string };
const identifier = (value: unknown): value is string => typeof value === "string" && value.trim() === value && value.length > 0
  && new TextEncoder().encode(value).length <= 160 && !/[\p{Cc}]/u.test(value);
const sameResult = (left: object, right: object) => Object.keys(left).length === Object.keys(right).length
  && Object.entries(left).every(([key, value]) => value === (right as Record<string, unknown>)[key]);
const changed = () => new Error("这项任务结果的来源已变化或不属于本次任务，请重新选择；未用其他版本代替。");

export const canDiscussTaskResult = (result: TaskResultDestination) => ["chapter", "brief", "outline", "plan", "propertyRules", "hypothesis", "planningReview", "commitmentExtraction", "commitment"].includes(result.kind);

async function currentProof(api: NovelCodexApi, result: TaskResultDestination) {
  const task = await api.runtime.getWorkTask(result.projectId, result.workTaskId);
  if (task.projectId !== result.projectId || task.id !== result.workTaskId
    || getTaskResults(task).filter(current => sameResult(current, result)).length !== 1) throw changed();
}
async function extractionProof(api: NovelCodexApi, result: Extract<TaskResultDestination, { kind: "commitmentExtraction" }> | CommitmentDecisionResult) {
  const task = await api.runtime.getWorkTask(result.projectId, result.workTaskId);
  if (task.projectId !== result.projectId || task.id !== result.workTaskId) throw changed();
  const matches = getCommitmentDecisionResults(task).filter(current => {
    if ("deliveryState" in result) return sameResult(current, result);
    const { deliveryState: _deliveryState, ...destination } = current;
    return sameResult(destination, result) && current.deliveryState === "recorded";
  });
  if (matches.length !== 1 || !("deliveryState" in result) && getTaskResults(task).filter(current => sameResult(current, result)).length !== 1) throw changed();
  return matches[0];
}
function exactReport(report: CommitmentExtractionReport | null, result: Extract<TaskResultDestination, { kind: "commitmentExtraction" }>): CommitmentExtractionReport {
  if (!report || report.authority !== "candidateProposalBatch" || report.revisionId !== result.referenceId || !identifier(report.snapshotId)
    || !Number.isSafeInteger(report.projectRevision) || report.projectRevision < 0
    || typeof report.sourceCurrent !== "boolean" || typeof report.reportCurrent !== "boolean"
    || report.payload?.chapterId !== result.chapterId || !identifier(report.payload.sourceRevisionId)
    || result.sourceRevisionId !== null && report.payload.sourceRevisionId !== result.sourceRevisionId
    || !Array.isArray(report.payload.candidates) || !Array.isArray(report.payload.examinedRanges) || !Array.isArray(report.established)
    || report.payload.candidates.some(proposal => !proposal || !identifier(proposal.id))
    || new Set(report.payload.candidates.map(proposal => proposal.id)).size !== report.payload.candidates.length
    || report.established.some(mapping => !mapping || !identifier(mapping.commitmentId)
      || report.payload.candidates.filter(proposal => proposal.id === mapping.proposalId).length !== 1)
    || new Set(report.established.map(mapping => mapping.proposalId)).size !== report.established.length
    || report.coverage?.semanticCompleteness !== "unknown" || report.coverage.literaryFulfillmentVerified !== false
    || report.coverage.globalCompletenessClaimed !== false || report.coverage.modelReportedRanges !== true) throw changed();
  return report;
}

async function extractionCapture(api: NovelCodexApi, request: TaskResultDiscussion): Promise<Capture> {
  const result = request.result as Extract<TaskResultDestination, { kind: "commitmentExtraction" }> | CommitmentDecisionResult;
  const proof = await extractionProof(api, result);
  const report = exactReport(await api.runtime.getCommitmentExtraction(result.projectId, result.chapterId, result.referenceId), result);
  const selected = request.proposalId === undefined ? null : report.payload.candidates.find(proposal => proposal.id === request.proposalId);
  if (request.proposalId !== undefined && !selected) throw new Error("这条建议不在对应的提取报告中，请重新选择；未用其他建议代替。");
  await extractionProof(api, result);
  const { candidates, ...sourcePayload } = report.payload;
  const capture = { kind: "commitmentExtraction", projectId: result.projectId, workTaskId: result.workTaskId, stepId: result.stepId,
    attemptId: result.attemptId, referenceId: result.referenceId, chapterId: result.chapterId, sourceRevisionId: report.payload.sourceRevisionId,
    deliveryState: proof.deliveryState, scope: selected ? "singleProposal" : "completeReport", proposalId: selected?.id ?? null,
    notice: "这是已保存提取建议的引用，状态记录捕获时的情况。历史或来源变化的建议仍可讨论；不是当前正文、完整性证明或本轮写入许可。候选对象对应关系不代表作者已确认。修改前须核对当前真实来源与本轮作者要求。" };
  const text = JSON.stringify(selected ? { capture, reportMetadata: { ...report, payload: sourcePayload, candidateCount: candidates.length }, proposal: selected }
    : { capture, report }, null, 2);
  const chapter = result.chapterNumber !== null ? `第 ${result.chapterNumber} 章` : "对应章节";
  return { text, snapshotId: report.snapshotId, objectId: `commitment-extraction-${result.chapterId}`, view: "chapters",
    label: `${chapter} · ${selected ? "这条伏笔建议" : "伏笔建议"}`, locator: selected ? "原报告中的完整单条建议与捕获状态" : "完整提取报告与捕获状态" };
}

async function commitmentCapture(api: NovelCodexApi, result: Extract<TaskResultDestination, { kind: "commitment" }>): Promise<Capture> {
  await currentProof(api, result);
  const detail = await api.runtime.getCommitment(result.projectId, result.commitmentId);
  if (detail.projectId !== result.projectId || detail.objectId !== `commitment-${result.commitmentId}` || detail.payload.id !== result.commitmentId
    || detail.revisionId !== result.referenceId || !identifier(detail.snapshotId) || typeof detail.sourceCurrent !== "boolean"
    || !["candidate", "confirmed"].includes(detail.confirmationState)
    || (detail.confirmationState === "confirmed" ? !identifier(detail.confirmationCommitId) : detail.confirmationCommitId !== null)
    || detail.interpretation !== "sourceBoundNarrativeClaim" || detail.literaryFulfillmentVerified !== false) throw changed();
  await currentProof(api, result);
  return { text: JSON.stringify({ capture: { kind: "commitment", projectId: result.projectId, workTaskId: result.workTaskId,
    stepId: result.stepId, attemptId: result.attemptId, referenceId: result.referenceId, deliveryState: "recorded",
    notice: "这是独立承诺修订的完整引用，来源、候选或确认状态是捕获时的情况，不是当前正文或本轮写入许可。修改前须重新读取当前真实对象与本轮作者要求；正式确认仍需真实作者决定及回执。" }, commitment: detail }, null, 2),
    snapshotId: detail.snapshotId, objectId: detail.objectId, view: "chapters", label: "这条伏笔修订", locator: "完整独立承诺修订与捕获状态" };
}

/** Capture exact persisted content. A superseded result never silently becomes the current version. */
export async function taskResultExcerpt(api: NovelCodexApi, result: TaskResultDestination): Promise<DiscussionExcerpt> {
  return taskResultDiscussionExcerpt(api, { result });
}

export async function taskResultDiscussionExcerpt(api: NovelCodexApi, request: TaskResultDiscussion): Promise<DiscussionExcerpt> {
  if (request.proposalId !== undefined && (!identifier(request.proposalId) || request.result.kind !== "commitmentExtraction")) {
    throw new Error("单条建议引用必须来自对应提取报告，请重新选择。");
  }
  const result = request.result;
  const capture = result.kind === "commitmentExtraction" ? await extractionCapture(api, request)
    : result.kind === "commitment" ? await commitmentCapture(api, result) : await ordinaryCapture(api, result);
  const excerpt: DiscussionExcerpt = { id: crypto.randomUUID(), projectId: result.projectId, view: capture.view,
    label: capture.label, text: capture.text, state: "displayed", locator: capture.locator, snapshotId: capture.snapshotId,
    revisionId: result.referenceId, objectId: capture.objectId, start: 0, end: capture.text.length };
  if (discussionExcerptsError([excerpt])) throw Error("这份成果较长，请先查看成果，再选中需要讨论的段落或单条建议；没有截断或替换原文。");
  return excerpt;
}

async function ordinaryCapture(api: NovelCodexApi, result: TaskResultDestination): Promise<Capture> {
  const resolved = await resolveTaskResult(api, result);
  let text: string, snapshotId: string | null = null;
  if (resolved.analysis) {
    const analysis = resolved.analysis;
    text = JSON.stringify(analysis.kind === "hypothesis" ? analysis.result : analysis.run.reports.find(report => report.id === analysis.reportId), null, 2);
    if (analysis.kind === "hypothesis") snapshotId = analysis.result.baseSnapshotId;
  } else {
    const workspace = await api.runtime.getCreativeWorkspace(result.projectId);
    if (workspace.project.id !== result.projectId) throw Error("成果不属于当前作品。");
    snapshotId = workspace.headSnapshotId;
    if (result.kind === "chapter") {
      const chapter = workspace.chapters.find(chapter => chapter.id === result.chapterId);
      const source = chapter?.candidate ?? chapter?.accepted;
      if (!source || source.id !== result.referenceId) throw Error("正文版本已变化，请重新选择成果。");
      text = source.body;
    } else if (["brief", "outline", "plan", "propertyRules"].includes(result.kind)) {
      const source = workspace.artifacts[result.kind as "brief" | "outline" | "plan" | "propertyRules"].candidate;
      if (!source || source.id !== result.referenceId) throw Error("成果版本已变化，请重新选择成果。");
      text = JSON.stringify(source.payload, null, 2);
    } else throw Error("请先查看成果，再选择要讨论的原文。");
  }
  return { text, view: resolved.view ?? "chapters", label: taskResultSummary(result), locator: "已保存成果全文", snapshotId,
    objectId: result.kind === "chapter" ? `draft-${result.chapterId}` : null };
}
