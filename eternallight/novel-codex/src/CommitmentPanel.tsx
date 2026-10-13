import type { CommitmentNavigationSelection } from "./types";
import type { TaskResultDestination } from "./task-results";
import { useEffect, useRef, useState } from "react";
import "./commitment-panel.css";
import { createCommitmentDecision } from "./commitment-decision";
import { CommitmentEditor } from "./CommitmentEditor";
import { CommitmentCatalog } from "./CommitmentCatalog";
import type { CommitmentChapter, CommitmentSource } from "./CommitmentFields";
import type { RegisterSupplementalEditor } from "./supplemental-editor";
import type { CommitmentDetail, CommitmentExtractionReport, NarrativeCommitment, NovelCodexApi } from "./types";

const kindLabels: Record<NarrativeCommitment["kind"], string> = {
  foreshadowing: "伏笔", suspense: "悬念", characterTransformation: "人物转变", goal: "目标", setPiece: "关键场面",
};
const statusLabels: Record<NarrativeCommitment["status"], string> = {
  open: "开放", partiallyFulfilled: "部分兑现", fulfilled: "已兑现", abandoned: "已放弃",
};
type Choice = Parameters<NovelCodexApi["runtime"]["establishCommitment"]>[2];
type Data = { identity: string; epoch: number; report: CommitmentExtractionReport | null; detail: CommitmentDetail | null };

export function CommitmentPanel({ api, projectId, chapterId, sourceRevisionId, dirty, blocked, refreshToken, onFormalChange, onBusyChange, registerEditor, requestedCommitment = null, requestedResult = null, initialEdit = null, autoOpen = false, sources = [], chapters = [] }: {
  api?: NovelCodexApi; projectId: string; chapterId: string; sourceRevisionId: string | null;
  dirty: boolean; blocked: boolean; refreshToken: number; onFormalChange: () => Promise<void>;
  requestedCommitment?: CommitmentNavigationSelection | null;
  requestedResult?: { requestId: string; result: TaskResultDestination } | null;
  initialEdit?: { requestId: string; detail: CommitmentDetail } | null;
  onBusyChange?: (busy: boolean) => void;
  autoOpen?: boolean; registerEditor?: RegisterSupplementalEditor; sources?: CommitmentSource[]; chapters?: CommitmentChapter[];
}) {
  const identity = JSON.stringify([projectId, chapterId, sourceRevisionId, dirty, requestedResult?.result.referenceId ?? null, initialEdit?.requestId ?? null]);
  const current = useRef(identity);
  const mounted = useRef(true);
  const epoch = useRef(0);
  if (current.current !== identity) epoch.current += 1;
  current.current = identity;
  const generation = epoch.current;
  const isCurrent = () => mounted.current && current.current === identity && epoch.current === generation;
  useEffect(() => {
    mounted.current = true;
    return () => { mounted.current = false; serial.current += 1; };
  }, []);
  const serial = useRef(0);
  const consumedCatalogRequest = useRef<string | null>(null);
  const shownCatalogRequest = useRef<string | null>(null);
  const consumedEditRequest = useRef<string | null>(null);
  const panel = useRef<HTMLDetailsElement | null>(null);
  const running = useRef(false);
  const decision = useRef(createCommitmentDecision());
  const blockedNow = useRef(blocked);
  blockedNow.current = blocked;
  const choices = useRef(new Map<string, Choice>());
  const reviewKeys = useRef(new Map<string, string>());
  const selected = useRef<{ identity: string; epoch: number; id: string } | null>(null);
  const [open, setOpen] = useState(autoOpen);
  const [data, setData] = useState<Data | null>(null);
  const [loading, setLoading] = useState(false);
  const [busy, setBusy] = useState(false);
  const [editing, setEditing] = useState<CommitmentDetail | null>(null);
  const [message, setMessage] = useState<{ identity: string; epoch: number; text: string; error: boolean } | null>(null);
  useEffect(() => { onBusyChange?.(busy); }, [busy, onBusyChange]);
  useEffect(() => () => { onBusyChange?.(false); }, [onBusyChange]);
  const visible = data?.identity === identity && data.epoch === generation && !dirty ? data : null;
  const report = visible?.report;
  const detail = visible?.detail;
  const writable = !dirty && !blocked && !busy && !loading && !editing && Boolean(api);
  const reportCurrent = report?.reportCurrent && report.sourceCurrent && report.payload.sourceRevisionId === sourceRevisionId;

  async function reload(id = selected.current?.identity === identity && selected.current.epoch === generation ? selected.current.id : null) {
    if (!api || dirty) return;
    const ticket = ++serial.current;
    setLoading(true);
    try {
      const [nextReport, nextDetail] = await Promise.all([
        api.runtime.getCommitmentExtraction(projectId, chapterId, requestedResult?.result.kind === "commitmentExtraction" ? requestedResult.result.referenceId : null),
        id ? api.runtime.getCommitment(projectId, id) : Promise.resolve(null),
      ]);
      if (isCurrent() && serial.current === ticket) {
        const expected = requestedResult?.result;
        if (expected?.projectId === projectId && (expected.kind === "commitmentExtraction" && (nextReport?.revisionId !== expected.referenceId || !nextReport.sourceCurrent)
          || expected.kind === "commitment" && id === expected.commitmentId && (nextDetail?.revisionId !== expected.referenceId || !nextDetail.sourceCurrent))) {
          setData(null); throw new Error("这项伏笔结果或原文已变化，未用其他版本替代。");
        }
        const catalogue = requestedCommitment;
        if (catalogue && consumedCatalogRequest.current !== catalogue.requestId) {
          const request = catalogue.request;
          if (request.projectId !== projectId || catalogue.chapterId !== chapterId || id !== request.commitmentId
            || nextDetail?.projectId !== projectId || nextDetail.payload.id !== request.commitmentId
            || nextDetail.revisionId !== request.revisionId || nextDetail.snapshotId !== request.snapshotId || !nextDetail.sourceCurrent) {
            setData(null); throw new Error("这条伏笔或原文来源已变化，请从目录重新选择；未打开其他版本。");
          }
          consumedCatalogRequest.current = catalogue.requestId;
        }
        setData({ identity, epoch: generation, report: nextReport, detail: nextDetail });
      }
    } catch (reason) {
      if (isCurrent() && serial.current === ticket) throw reason;
    } finally {
      if (isCurrent() && serial.current === ticket) setLoading(false);
    }
  }

  useEffect(() => {
    if (!open || !api || dirty) return;
    void reload().catch((reason: unknown) => {
      if (isCurrent()) setMessage({ identity, epoch: generation, text: String(reason), error: true });
    });
    return () => { serial.current += 1; };
  }, [api, identity, open, refreshToken]);
  useEffect(() => {
    if (!requestedResult || requestedResult.result.projectId !== projectId) return;
    const result = requestedResult.result;
    if (result.kind !== "commitment" && result.kind !== "commitmentExtraction") return;
    const id = result.kind === "commitment" ? result.commitmentId : null;
    selected.current = id ? { identity, epoch: generation, id } : null;
    setData(null); setOpen(true);
    void reload(id).catch(reason => { if (isCurrent()) setMessage({ identity, epoch: generation, text: String(reason), error: true }); });
  }, [requestedResult?.requestId, identity]);
  useEffect(() => {
    if (!requestedCommitment || requestedCommitment.request.projectId !== projectId || requestedCommitment.chapterId !== chapterId
      || consumedCatalogRequest.current === requestedCommitment.requestId || dirty || blocked) return;
    const id = requestedCommitment.request.commitmentId;
    selected.current = { identity, epoch: generation, id };
    setData(null); setOpen(true);
    void reload(id).catch(reason => { if (isCurrent()) setMessage({ identity, epoch: generation, text: String(reason), error: true }); });
  }, [requestedCommitment?.requestId, identity, blocked]);
  useEffect(() => {
    if (!requestedCommitment || consumedCatalogRequest.current !== requestedCommitment.requestId
      || shownCatalogRequest.current === requestedCommitment.requestId || !detail) return;
    shownCatalogRequest.current = requestedCommitment.requestId;
    panel.current?.scrollIntoView({ block: "nearest" });
    panel.current?.querySelector("summary")?.focus({ preventScroll: true });
  }, [requestedCommitment?.requestId, detail]);
  useEffect(() => {
    if (!initialEdit || consumedEditRequest.current === initialEdit.requestId || dirty || blocked || !registerEditor) return;
    const editable = initialEdit.detail;
    const expected = requestedResult?.result;
    if (editable.projectId !== projectId || !editable.authorControls || !editable.sourceCurrent
      || editable.extractionOrigin?.chapterId !== chapterId || expected?.kind !== "commitmentExtraction"
      || editable.extractionOrigin.reportRevisionId !== expected.referenceId) return;
    consumedEditRequest.current = initialEdit.requestId;
    selected.current = { identity, epoch: generation, id: editable.payload.id };
    setOpen(true); setEditing(editable);
    void reload(editable.payload.id).catch(reason => { if (isCurrent()) setMessage({ identity, epoch: generation, text: String(reason), error: true }); });
  }, [initialEdit?.requestId, identity, blocked, registerEditor]);
  useEffect(() => {
    if (!initialEdit || !editing) return;
    panel.current?.querySelector<HTMLElement>('.commitment-editor input, .commitment-editor textarea')?.focus({ preventScroll: true });
  }, [initialEdit?.requestId, Boolean(editing)]);
  useEffect(() => { choices.current.clear(); reviewKeys.current.clear(); }, [identity]);
  useEffect(() => { if (autoOpen) setOpen(true); }, [autoOpen, projectId, chapterId]);

  async function act(action: () => Promise<string | null>, success: string) {
    if (!writable || running.current) return;
    running.current = true; setBusy(true); setMessage(null);
    try {
      const id = await action();
      if (!isCurrent()) return;
      if (id) selected.current = { identity, epoch: generation, id };
      await reload(id);
      if (isCurrent()) setMessage({ identity, epoch: generation, text: success, error: false });
    } catch (reason) {
      if (isCurrent()) {
        await reload().catch(() => undefined);
        if (isCurrent()) setMessage({ identity, epoch: generation, text: `${String(reason)}。请刷新核对当前状态后重试。`, error: true });
      }
    } finally { running.current = false; setBusy(false); }
  }

  function establish(proposalId: string) {
    if (!api || !report || !reportCurrent) return;
    const key = JSON.stringify([report.revisionId, proposalId]);
    let choice = choices.current.get(key);
    if (!choice) {
      choice = { reportRevisionId: report.revisionId, proposalId, baseSnapshotId: report.snapshotId,
        expectedRevision: report.projectRevision, idempotencyKey: crypto.randomUUID() };
      if (choices.current.size >= 16) choices.current.clear();
      choices.current.set(key, choice);
    }
    const frozen = choice;
    void act(async () => {
      const receipt = await api.runtime.establishCommitment(projectId, chapterId, frozen);
      if (!isCurrent()) return receipt.commitmentId;
      const editable = await api.runtime.getCommitment(projectId, receipt.commitmentId);
      if (isCurrent() && registerEditor) setEditing(editable);
      return receipt.commitmentId;
    }, "已打开伏笔建议，可调整后再确认。");
  }

  function decide(proposalId: string) {
    if (!api || !report || !reportCurrent) return;
    void act(async () => {
      const confirmed = await decision.current.decide({ api, projectId, chapterId, report, proposalId,
        isCurrent: () => isCurrent() && !blockedNow.current });
      if (isCurrent()) {
        selected.current = { identity, epoch: generation, id: confirmed.payload.id };
        try { await onFormalChange(); }
        catch { throw new Error("伏笔已确认，但作品视图更新失败；请刷新查看已保存结果。"); }
      }
      return confirmed.payload.id;
    }, "已确认保留这条伏笔。");
  }

  function confirm() {
    if (!api || !detail || !detail.authorControls || !detail.sourceCurrent || detail.confirmationState !== "candidate") return;
    const frozen = detail;
    const controls = detail.authorControls;
    void act(async () => {
      let reviewKey = reviewKeys.current.get(frozen.revisionId);
      if (!reviewKey) {
        if (reviewKeys.current.size >= 16) reviewKeys.current.clear();
        reviewKey = crypto.randomUUID();
        reviewKeys.current.set(frozen.revisionId, reviewKey);
      }
      const review = await api.runtime.createCommitmentReview(projectId, frozen.payload.id, frozen.revisionId, reviewKey);
      if (!isCurrent()) return null;
      if (blockedNow.current) throw new Error("当前任务正在执行，伏笔尚未确认；请结束后再次确认。");
      await api.runtime.acceptReview(projectId, review.id, controls.projectRevision, {});
      if (isCurrent()) await onFormalChange();
      return frozen.payload.id;
    }, "作者已确认这份承诺修订；这不是文学兑现认证。");
  }

  function undo() {
    if (!api || !detail?.confirmationCommitId || !detail.authorControls?.undoAvailable) return;
    const frozen = detail;
    const controls = detail.authorControls;
    void act(async () => {
      await api.runtime.undoCommit(projectId, frozen.confirmationCommitId!, controls.projectRevision);
      if (isCurrent()) await onFormalChange();
      return frozen.payload.id;
    }, "已撤销当前确认，历史来源仍保留。");
  }

  return <details ref={panel} className="commitment-panel" open={open} onToggle={(event) => setOpen(event.currentTarget.open)}>
    <summary>{initialEdit ? "调整这条伏笔" : "本章当前建议 · 伏笔与悬念"}</summary>
    {open && <div className="commitment-panel-body" aria-busy={busy || loading}>
      {!initialEdit && <p>选择你希望保留的伏笔。确认后可继续调整，不会改写或发布正文。</p>}
      <button className="secondary-button" disabled={!writable} onClick={() => { setMessage(null); void reload().catch((reason: unknown) => { if (isCurrent()) setMessage({ identity, epoch: generation, text: String(reason), error: true }); }); }}>刷新提案</button>
      {!api && <p role="status">桌面运行时尚未连接。</p>}
      {dirty && <p role="status">请先保存正文；旧提案已隐藏。</p>}
      {blocked && <p role="status">当前工作完成后可建立、确认或撤销承诺。</p>}
      {loading && <p role="status">正在读取来源与提案…</p>}
      {message?.identity === identity && message.epoch === generation && <p role={message.error ? "alert" : "status"}>{message.text}</p>}
      {visible && !report && <p>本章尚无提取报告。可在对话中要求“提取并保存本章伏笔候选”。</p>}
      {report && !initialEdit && <>
        <p>本次提出 {report.payload.candidates.length} 条建议</p>
        {!reportCurrent && <p role="status">{!report.reportCurrent ? "这份报告已有更新，不能从旧报告建立承诺。" : "正文来源已变化，不能从这份报告建立承诺。请重新提取。"}</p>}
        {report.payload.candidates.length === 0 && <p>本次没有提出可靠候选；不代表本章没有伏笔。</p>}
        <div className="commitment-proposals">{report.payload.candidates.map((proposal) => {
          const established = report.established.find((item) => item.proposalId === proposal.id);
          return <article key={proposal.id}>
            <header><span>{kindLabels[proposal.kind]}</span><strong>{proposal.summary}</strong></header>
            <p>兑现标准：{proposal.fulfillmentCriteria}</p>
            <details><summary>查看原文依据</summary>{proposal.setupEvidenceRefs.map((anchor, index) => <blockquote key={index}>{anchor.quote}</blockquote>)}</details>
            <div className="commitment-actions">
              {established && detail?.payload.id !== established.commitmentId
                ? <button className="secondary-button" disabled={!writable} onClick={() => void act(async () => established.commitmentId, "已读取这条伏笔的确认状态。")}>查看确认状态</button>
                : <button className="primary-button" disabled={!writable || !reportCurrent || detail?.payload.id === established?.commitmentId && detail?.confirmationState === "confirmed"} onClick={() => decide(proposal.id)}>{detail?.payload.id === established?.commitmentId && detail?.confirmationState === "confirmed" ? "作者已确认" : "确认保留这条伏笔"}</button>}
              {established ? registerEditor && <button className="secondary-button" disabled={!writable} onClick={() => void act(async () => {
                if (!api) return null;
                const editable = await api.runtime.getCommitment(projectId, established.commitmentId);
                if (isCurrent() && editable.authorControls) setEditing(editable);
                return established.commitmentId;
              }, "已打开伏笔建议，可调整后再确认。")}>调整后再决定</button>
                : registerEditor && <button className="secondary-button" disabled={!writable || !reportCurrent} onClick={() => establish(proposal.id)}>调整后再决定</button>}
            </div>
          </article>;
        })}</div>
      </>}
      {detail && (!initialEdit || !editing) && <section className="commitment-detail" aria-label="独立承诺详情">
        <header><strong>{detail.payload.summary}</strong><span>{detail.confirmationState === "confirmed" ? "作者已确认" : "未确认候选"} · {statusLabels[detail.payload.status]}</span></header>
        <p>兑现标准：{detail.payload.fulfillmentCriteria}</p>
        {detail.payload.decisionReason && <p>作者理由：{detail.payload.decisionReason}</p>}
        <details><summary>来源与记录</summary>
        <small className="commitment-source">提取来源：{detail.extractionOrigin?.reportRevisionId ?? "作者手工建立"} · 承诺修订：{detail.revisionId}</small>
        {detail.payload.setupEvidenceRefs.map((anchor, index) => <blockquote key={`setup-${index}`}>{anchor.quote}<small>建立证据 · {anchor.revisionId} · 字符 {anchor.start}–{anchor.end}</small></blockquote>)}
        {detail.payload.fulfillmentEvidenceRefs.map((anchor, index) => <blockquote key={index}>{anchor.quote}<small>兑现证据 · {anchor.revisionId}</small></blockquote>)}
        </details>
        {!detail.sourceCurrent && <p role="status">承诺证据的正文版本已失效，需先复核才能确认。</p>}
        {detail.confirmationCommitId && !detail.authorControls?.undoAvailable && <p role="status">这次确认之后已有其他变更，不能直接撤销旧确认，以免影响后续内容。</p>}
        <div className="commitment-actions">
          {registerEditor && <button className="secondary-button" disabled={!writable || !detail.authorControls} onClick={() => setEditing(detail)}>调整这份承诺</button>}
          <button className="primary-button" disabled={!writable || !detail.authorControls || !detail.sourceCurrent || detail.confirmationState !== "candidate"} onClick={confirm}>确认这份承诺</button>
          {detail.confirmationCommitId && <button className="secondary-button" disabled={!writable || !detail.authorControls?.undoAvailable} onClick={undo}>撤销当前确认</button>}
        </div>
      </section>}
      {api && !initialEdit && <CommitmentCatalog key={identity} api={api} projectId={projectId} blocked={!writable} refreshToken={refreshToken} onOpen={(id) => void act(async () => id, "已从全作品目录打开独立承诺。")} />}
    </div>}
    {editing && api && registerEditor && <CommitmentEditor api={api} initial={editing} sources={sources} chapters={chapters} blocked={blocked || dirty} register={registerEditor} onClose={() => setEditing(null)} onSaved={async () => { await reload(editing.payload.id); await onFormalChange(); }} />}
  </details>;
}
