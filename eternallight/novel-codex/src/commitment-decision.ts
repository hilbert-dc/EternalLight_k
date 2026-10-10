import type { CommitmentDetail, CommitmentExtractionReport, NarrativeCommitment, NovelCodexApi } from "./types";

type DecisionInput = {
  api: NovelCodexApi; projectId: string; chapterId: string; report: CommitmentExtractionReport;
  proposalId: string; isCurrent: () => boolean;
};
type Runtime = NovelCodexApi["runtime"];
type Pending = {
  api: NovelCodexApi; proposal: NarrativeCommitment; establish: Parameters<Runtime["establishCommitment"]>[2];
  reviewKey: string; commitmentId?: string; revisionId?: string;
  review?: Awaited<ReturnType<Runtime["createCommitmentReview"]>>;
  acceptance?: Awaited<ReturnType<Runtime["acceptReview"]>>; acceptRequested?: boolean; started: boolean; settled: boolean;
};
const same = (left: unknown, right: unknown) => JSON.stringify(left, ordered) === JSON.stringify(right, ordered);
function ordered(_key: string, value: unknown) {
  return value && typeof value === "object" && !Array.isArray(value)
    ? Object.fromEntries(Object.entries(value).sort(([left], [right]) => left.localeCompare(right))) : value;
}

/** Author-only confirmation of one displayed proposal; model tasks never use this controller. */
export function createCommitmentDecision() {
  const pending = new Map<string, Pending>();
  let flight: { key: string; api: NovelCodexApi; isCurrent: () => boolean; promise: Promise<CommitmentDetail> } | null = null;
  async function execute(input: DecisionInput, key: string): Promise<CommitmentDetail> {
    const { api, projectId, chapterId, report, proposalId, isCurrent } = input;
    const assertCurrent = () => { if (!isCurrent()) throw new Error("确认来源已切换；原操作结果需在原章节核对。"); };
    assertCurrent();
    const proposals = report.payload.candidates.filter(item => item.id === proposalId);
    if (proposals.length !== 1 || report.payload.chapterId !== chapterId || report.reportCurrent !== true || !report.sourceCurrent) throw new Error("伏笔提案来源已变化，请重新读取。");
    let entry = pending.get(key);
    if (entry?.api !== api && entry) throw new Error("连接已变化，请先核对原确认结果。");
    if (entry?.settled) { pending.delete(key); entry = undefined; }
    if (!entry) {
      if (pending.size >= 16) {
        const removable = [...pending].find(([, item]) => item.settled || !item.started);
        if (!removable) throw new Error("待核对的确认已达上限，请先核对已有结果。");
        pending.delete(removable[0]);
      }
      entry = { api, proposal: structuredClone(proposals[0]), establish: {
        reportRevisionId: report.revisionId, proposalId, baseSnapshotId: report.snapshotId,
        expectedRevision: report.projectRevision, idempotencyKey: crypto.randomUUID(),
      }, reviewKey: crypto.randomUUID(), started: false, settled: false };
      pending.set(key, entry);
    }
    const saved = entry;
    const origin = { chapterId, reportRevisionId: report.revisionId, proposalId };
    const readReport = async () => {
      const fresh = await api.runtime.getCommitmentExtraction(projectId, chapterId);
      assertCurrent();
      if (!fresh || fresh.authority !== "candidateProposalBatch" || fresh.reportCurrent !== true || !fresh.sourceCurrent
        || fresh.revisionId !== origin.reportRevisionId || fresh.payload.chapterId !== chapterId
        || fresh.payload.sourceRevisionId !== report.payload.sourceRevisionId
        || fresh.payload.candidates.filter(item => item.id === proposalId).length !== 1
        || !same(fresh.payload.candidates.find(item => item.id === proposalId), saved.proposal)) {
        throw new Error("伏笔报告或正文来源已变化，请重新读取后决定。");
      }
      return fresh;
    };
    const readDetail = async () => {
      if (!saved.commitmentId) throw new Error("建立结果尚未核实，请重新读取提案后再次确认。");
      const detail = await api.runtime.getCommitment(projectId, saved.commitmentId);
      assertCurrent();
      if (detail.projectId !== projectId || detail.objectId !== `commitment-${saved.commitmentId}`
        || detail.payload.id !== saved.commitmentId || !same(detail.extractionOrigin, origin)
        || !same(detail.payload, { ...saved.proposal, id: saved.commitmentId })
        || saved.revisionId && detail.revisionId !== saved.revisionId) {
        throw new Error("这份独立伏笔已修改，不能直接确认原提案；请打开当前修订核对。");
      }
      if (!detail.sourceCurrent || !detail.authorControls) throw new Error("伏笔证据已过期或当前只能只读，请重新核对来源。");
      saved.revisionId = detail.revisionId;
      return detail;
    };
    const confirmed = (detail: CommitmentDetail) => {
      if (detail.confirmationState !== "confirmed" || !detail.confirmationCommitId) return false;
      if (saved.acceptance && detail.confirmationCommitId !== saved.acceptance.commitId) throw new Error("确认回执已变化，请重新读取当前状态。");
      saved.settled = true;
      return true;
    };
    const fresh = await readReport();
    const established = fresh.established.filter(item => item.proposalId === proposalId);
    if (established.length > 1 || saved.commitmentId && established[0]?.commitmentId !== saved.commitmentId) throw new Error("提案建立记录不一致，请重新核对。");
    if (established[0]) saved.commitmentId = established[0].commitmentId;
    if (!saved.commitmentId) {
      if (fresh.snapshotId !== saved.establish.baseSnapshotId || fresh.projectRevision !== saved.establish.expectedRevision) {
        if (!saved.started) pending.delete(key);
        throw new Error("作品已变化，不能沿用旧建立请求，请重新核对提案。");
      }
      assertCurrent(); saved.started = true;
      try {
        const receipt = await api.runtime.establishCommitment(projectId, chapterId, saved.establish);
        saved.commitmentId = receipt.commitmentId; saved.revisionId = receipt.revisionId;
        assertCurrent();
      } catch (error) {
        assertCurrent();
        try {
          const recovered = await readReport();
          const mappings = recovered.established.filter(item => item.proposalId === proposalId);
          if (mappings.length === 1) saved.commitmentId = mappings[0].commitmentId;
          if (saved.commitmentId) {
            const detail = await readDetail();
            if (confirmed(detail)) return detail;
          }
        } catch { /* Uncertain writes keep their original request; recovery never writes. */ }
        assertCurrent();
        throw new Error(saved.commitmentId ? "候选已建立，尚未确认；请再次确认以继续。" : `建立结果尚未核实：${error instanceof Error ? error.message : "读取失败"}。请重新读取后再试。`);
      }
    }
    let detail: CommitmentDetail;
    try { detail = await readDetail(); }
    catch (error) {
      assertCurrent();
      throw new Error(`候选已建立，确认状态尚未核实：${error instanceof Error ? error.message : "读取失败"}。请重新读取后继续。`);
    }
    if (confirmed(detail)) return detail;
    if (saved.acceptance) throw new Error("确认已保存，但当前结果尚未核实；请重新读取，不会重复确认。");
    if (!saved.review) {
      assertCurrent(); saved.started = true;
      try {
        saved.review = await api.runtime.createCommitmentReview(projectId, saved.commitmentId!, detail.revisionId, saved.reviewKey);
        assertCurrent();
      } catch (error) {
        assertCurrent();
        const recovered = await readDetail().catch(() => null);
        if (recovered && confirmed(recovered)) return recovered;
        throw new Error(`伏笔确认检查结果尚未核实：${error instanceof Error ? error.message : "读取失败"}。再次确认会复用原检查请求，不会自动继续写入。`);
      }
    }
    const review = saved.review;
    if (review.artifactKind !== "commitment" || review.artifactRevisionId !== saved.revisionId || review.status !== "ready") throw new Error("伏笔确认检查已变化，请重新读取当前修订。");
    await readReport();
    detail = await readDetail();
    if (confirmed(detail)) return detail;
    if (detail.authorControls!.projectRevision !== review.projectRevision) {
      if (!saved.acceptRequested) { saved.review = undefined; saved.reviewKey = crypto.randomUUID(); }
      throw new Error("作品已变化，旧确认检查已失效；请重新核对。");
    }
    assertCurrent();
    saved.acceptRequested = true;
    try {
      saved.acceptance = await api.runtime.acceptReview(projectId, review.id, review.projectRevision, {});
      assertCurrent();
      const result = await readDetail();
      if (!confirmed(result)) throw new Error("确认回执未对应当前伏笔。");
      return result;
    } catch (error) {
      assertCurrent();
      const recovered = await readDetail().catch(() => null);
      if (recovered && confirmed(recovered)) return recovered;
      throw new Error(`确认结果尚未核实：${error instanceof Error ? error.message : "读取失败"}。请重新读取后再试，期间不会自动重发确认。`);
    }
  }
  return { decide(input: DecisionInput): Promise<CommitmentDetail> {
    const key = JSON.stringify([input.projectId, input.chapterId, input.report.revisionId, input.proposalId]);
    if (flight?.isCurrent()) {
      if (flight.key === key && flight.api === input.api) return flight.promise;
      return Promise.reject(new Error("另一项伏笔确认正在进行，请等待核对完成。"));
    }
    const owner = { key, api: input.api, isCurrent: input.isCurrent, promise: execute(input, key) };
    flight = owner;
    void owner.promise.finally(() => { if (flight === owner) flight = null; }).catch(() => undefined);
    return owner.promise;
  } };
}
