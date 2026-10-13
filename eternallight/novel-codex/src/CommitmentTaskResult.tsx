import { useEffect, useRef, useState } from "react";
import { createCommitmentDecision } from "./commitment-decision";
import type { CommitmentDecisionResult } from "./task-results";
import type { TaskResultDiscussion } from "./task-result-excerpt";
import type { CommitmentDetail, CommitmentExtractionReport, NarrativeCommitment, NovelCodexApi } from "./types";
import "./commitment-task-result.css";

type Props = {
  api: NovelCodexApi | undefined;
  result: CommitmentDecisionResult;
  disabled: boolean;
  refreshToken: string | number;
  onFormalChange?: () => Promise<void>;
  onAdjust?: (result: CommitmentDecisionResult, commitment: CommitmentDetail) => void;
  onDiscuss?: (request: TaskResultDiscussion) => void;
  acquireInteraction?: () => boolean;
  releaseInteraction?: () => void;
};
type Check = { detail: CommitmentDetail | null; error: string | null };
type Loaded = { identity: string; epoch: number; report: CommitmentExtractionReport; checks: Record<string, Check> };
type EstablishChoice = Parameters<NovelCodexApi["runtime"]["establishCommitment"]>[2];
const kinds: Record<NarrativeCommitment["kind"], string> = {
  foreshadowing: "伏笔", suspense: "悬念", characterTransformation: "人物转变", goal: "目标", setPiece: "关键场面",
};
const identifier = (value: unknown): value is string => typeof value === "string" && value.trim() === value && value.length > 0;

/** One immutable task report, with author actions governed by today's source and controls. */
export function CommitmentTaskResult({ api, result, disabled, refreshToken, onFormalChange, onAdjust, onDiscuss, acquireInteraction, releaseInteraction }: Props) {
  const identity = JSON.stringify([result.projectId, result.workTaskId, result.stepId, result.attemptId,
    result.referenceId, result.chapterId, result.sourceRevisionId, result.deliveryState]);
  const title = Number.isSafeInteger(result.chapterNumber) && Number(result.chapterNumber) > 0
    ? `第 ${result.chapterNumber} 章 · 伏笔建议` : "本次伏笔建议";
  const owner = useRef({ identity, api, epoch: 0 });
  if (owner.current.identity !== identity || owner.current.api !== api) owner.current = { identity, api, epoch: owner.current.epoch + 1 };
  const epoch = owner.current.epoch;
  const mounted = useRef(true);
  const serial = useRef(0);
  const running = useRef<{ identity: string; epoch: number; release?: () => void; released: boolean } | null>(null);
  const decision = useRef(createCommitmentDecision());
  const choices = useRef(new Map<string, { api: NovelCodexApi; request: EstablishChoice }>());
  const [data, setData] = useState<Loaded | null>(null);
  const [loading, setLoading] = useState(false);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const visible = data?.identity === identity && data.epoch === epoch ? data : null;
  const report = visible?.report;
  const allowed = Boolean(api && report && result.deliveryState === "recorded" && report.reportCurrent && report.sourceCurrent && !disabled && !error);
  const allowedNow = useRef(allowed);
  allowedNow.current = allowed;
  const isCurrent = () => mounted.current && owner.current.identity === identity && owner.current.api === api && owner.current.epoch === epoch;
  const assertCurrent = () => { if (!isCurrent() || !allowedNow.current) throw new Error("当前来源或操作状态已变化，请重新核对建议。"); };
  const writable = allowed && !loading && !busy;
  function release(flight: NonNullable<typeof running.current>) {
    if (!flight.released) { flight.released = true; flight.release?.(); }
    if (running.current === flight) running.current = null;
  }
  function claim(proposalId: string) {
    if (running.current) return null;
    if (acquireInteraction && !acquireInteraction()) { setMessage("另一条建议正在处理，请完成后再选择。"); return null; }
    const flight = { identity, epoch, release: releaseInteraction, released: false };
    running.current = flight; setBusy(proposalId); setMessage(null);
    return flight;
  }

  function validateReport(value: CommitmentExtractionReport | null): CommitmentExtractionReport {
    if (!value || value.authority !== "candidateProposalBatch" || value.revisionId !== result.referenceId
      || value.payload.chapterId !== result.chapterId || !identifier(value.payload.sourceRevisionId)
      || result.sourceRevisionId !== null && value.payload.sourceRevisionId !== result.sourceRevisionId
      || typeof value.reportCurrent !== "boolean" || typeof value.sourceCurrent !== "boolean"
      || !Array.isArray(value.payload.candidates) || !Array.isArray(value.established)
      || value.payload.candidates.some(candidate => !identifier(candidate.id))
      || new Set(value.payload.candidates.map(candidate => candidate.id)).size !== value.payload.candidates.length
      || value.established.some(mapping => !identifier(mapping.commitmentId)
        || value.payload.candidates.filter(candidate => candidate.id === mapping.proposalId).length !== 1)
      || new Set(value.established.map(mapping => mapping.proposalId)).size !== value.established.length) {
      throw new Error("未能读取这次任务对应的伏笔报告，未用其他版本替代。");
    }
    return value;
  }
  function validateDetail(detail: CommitmentDetail, proposalId: string, commitmentId: string) {
    if (detail.projectId !== result.projectId || detail.objectId !== `commitment-${commitmentId}` || detail.payload.id !== commitmentId
      || detail.extractionOrigin?.chapterId !== result.chapterId || detail.extractionOrigin.reportRevisionId !== result.referenceId
      || detail.extractionOrigin.proposalId !== proposalId) throw new Error("这条伏笔的来源与本次建议不一致，请重新核对。");
    return detail;
  }
  async function reload() {
    if (!api) return;
    const ticket = ++serial.current;
    setLoading(true); setError(null);
    try {
      const nextReport = validateReport(await api.runtime.getCommitmentExtraction(result.projectId, result.chapterId, result.referenceId));
      if (!isCurrent() || serial.current !== ticket) return;
      const entries = await Promise.all(nextReport.established.map(async mapping => {
        try {
          const detail = validateDetail(await api.runtime.getCommitment(result.projectId, mapping.commitmentId), mapping.proposalId, mapping.commitmentId);
          return [mapping.proposalId, { detail, error: null }] as const;
        } catch { return [mapping.proposalId, { detail: null, error: "这条建议的确认状态未能核对，请刷新后重试。" }] as const; }
      }));
      if (isCurrent() && serial.current === ticket) setData({ identity, epoch, report: nextReport, checks: Object.fromEntries(entries) });
    } catch (cause) {
      if (isCurrent() && serial.current === ticket) setError(cause instanceof Error ? cause.message : "读取建议失败，请重试。");
    } finally { if (isCurrent() && serial.current === ticket) setLoading(false); }
  }
  useEffect(() => {
    mounted.current = true;
    return () => { mounted.current = false; serial.current += 1; };
  }, []);
  useEffect(() => {
    setMessage(null);
    void reload();
    return () => { serial.current += 1; };
  }, [api, identity, refreshToken]);
  useEffect(() => {
    setBusy(null);
    return () => {
      const flight = running.current;
      if (flight?.identity === identity && flight.epoch === epoch) release(flight);
    };
  }, [api, identity]);

  async function confirm(proposalId: string) {
    if (!api || !report || !writable || running.current) return;
    const flight = claim(proposalId);
    if (!flight) return;
    try {
      const confirmed = await decision.current.decide({ api, projectId: result.projectId, chapterId: result.chapterId,
        report, proposalId, isCurrent: () => isCurrent() && allowedNow.current });
      if (!isCurrent()) return;
      setData(previous => previous?.identity === identity && previous.epoch === epoch
        ? { ...previous, checks: { ...previous.checks, [proposalId]: { detail: confirmed, error: null } } } : previous);
      setMessage("已确认保留这条伏笔。");
      try { await onFormalChange?.(); }
      catch { if (isCurrent()) setMessage("伏笔已确认，但作品视图更新失败；请刷新查看已保存结果。"); }
      if (isCurrent()) await reload();
    } catch (cause) {
      if (isCurrent()) {
        setMessage(cause instanceof Error ? cause.message : "确认结果尚未核对，请刷新后重试。");
        await reload();
      }
    } finally { release(flight); if (isCurrent()) setBusy(null); }
  }
  async function adjust(proposalId: string) {
    if (!api || !onAdjust || !report || !writable || running.current) return;
    const flight = claim(proposalId);
    if (!flight) return;
    try {
      const fresh = validateReport(await api.runtime.getCommitmentExtraction(result.projectId, result.chapterId, result.referenceId));
      assertCurrent();
      if (!fresh.reportCurrent || !fresh.sourceCurrent) throw new Error("这份报告或正文来源已变化，不能建立调整候选。");
      const mapping = fresh.established.find(item => item.proposalId === proposalId);
      let id = mapping?.commitmentId;
      if (!id) {
        const key = JSON.stringify([identity, proposalId]);
        let saved = choices.current.get(key);
        if (saved?.api !== api && saved) throw new Error("连接已变化，请先核对原建立结果。");
        if (!saved) {
          saved = { api, request: { reportRevisionId: fresh.revisionId, proposalId, baseSnapshotId: fresh.snapshotId,
            expectedRevision: fresh.projectRevision, idempotencyKey: crypto.randomUUID() } };
          choices.current.set(key, saved);
        }
        assertCurrent();
        const receipt = await api.runtime.establishCommitment(result.projectId, result.chapterId, saved.request);
        assertCurrent();
        id = receipt.commitmentId;
      }
      const detail = validateDetail(await api.runtime.getCommitment(result.projectId, id), proposalId, id);
      assertCurrent();
      if (!detail.sourceCurrent || !detail.authorControls) throw new Error("这份伏笔的来源已失效或当前只能阅读，不能进入调整。");
      const currentReport = validateReport(await api.runtime.getCommitmentExtraction(result.projectId, result.chapterId, result.referenceId));
      assertCurrent();
      if (!currentReport.reportCurrent || !currentReport.sourceCurrent
        || currentReport.established.find(item => item.proposalId === proposalId)?.commitmentId !== id) {
        throw new Error("这份报告或候选来源已变化，不能进入调整。");
      }
      onAdjust(result, detail);
      await reload();
    } catch (cause) {
      if (isCurrent()) {
        setMessage(`${cause instanceof Error ? cause.message : "调整候选结果尚未核对"}。请刷新后再决定；原建立请求已保留。`);
        await reload();
      }
    } finally { release(flight); if (isCurrent()) setBusy(null); }
  }

  return <section className="commitment-task-result" data-commitment-report={result.referenceId} aria-label={title} aria-busy={loading || Boolean(busy)}>
    <header><strong>{title}</strong>{report && <span>{report.payload.candidates.length} 条</span>}</header>
    {!api && <p role="status">桌面运行时尚未连接。</p>}
    {loading && <p role="status">正在核对本次建议…</p>}
    {error && <p role="alert">{error}</p>}
    {message && <p role="status">{message}</p>}
    {api && (error || message) && <button type="button" className="text-button" disabled={loading || Boolean(busy)} onClick={() => void reload()}>重新核对本次建议</button>}
    {report && <>
      {result.deliveryState === "superseded" || !report.reportCurrent ? <p role="status">这是当时保存的建议，已有后续报告；可查看原内容。</p>
        : !report.sourceCurrent ? <p role="status">正文来源已变化，这份建议仅供回看。</p>
          : disabled ? <p role="status">当前编辑或任务结束后可选择建议。</p> : null}
      {report.payload.candidates.length === 0 && <p>本次没有提出可靠候选；不代表本章没有伏笔。</p>}
      <div className="commitment-task-proposals">{report.payload.candidates.map(proposal => {
        const check = visible?.checks[proposal.id];
        const confirmed = check?.detail?.confirmationState === "confirmed" && identifier(check.detail.confirmationCommitId);
        const unavailable = Boolean(check?.error || check?.detail && (!check.detail.sourceCurrent || !check.detail.authorControls));
        return <article className="commitment-task-proposal" data-proposal-id={proposal.id} key={proposal.id}>
          <header><span>{kinds[proposal.kind]}</span><strong>{proposal.summary}</strong></header>
          <p>兑现标准：{proposal.fulfillmentCriteria}</p>
          {confirmed && <p className="commitment-task-confirmed">已确认保留</p>}
          {check?.error && <p role="status">{check.error}</p>}
          {check?.detail && !check.detail.sourceCurrent && <p role="status">对应伏笔的正文来源待复核。</p>}
          <details data-conversation-detail={`commitment:${result.projectId}:${result.stepId}:${result.attemptId}:${result.referenceId}:${proposal.id}:evidence`}>
            <summary>原文依据与操作</summary>{proposal.setupEvidenceRefs.map((anchor, index) => <blockquote key={index}>{anchor.quote}</blockquote>)}
            <div className="commitment-task-actions">
              {onDiscuss && <button type="button" className="text-button" disabled={disabled || loading || Boolean(busy)} onClick={() => onDiscuss({ result, proposalId: proposal.id })}>引用这条建议</button>}
              {onAdjust && <button type="button" className="secondary-button" disabled={!writable || unavailable} onClick={() => void adjust(proposal.id)}>调整后再决定</button>}
              <button type="button" className="primary-button" disabled={!writable || unavailable || Boolean(confirmed)} onClick={() => void confirm(proposal.id)}>{confirmed ? "对应伏笔已确认" : "确认保留这条伏笔"}</button>
            </div>
            {(onAdjust || !confirmed) && <p className="commitment-task-operation-note">调整只保存候选；确认保留后才正式采用。</p>}
          </details>
        </article>;
      })}</div>
    </>}
  </section>;
}
