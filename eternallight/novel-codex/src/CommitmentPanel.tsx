import { useEffect, useRef, useState } from "react";
import "./commitment-panel.css";
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

export function CommitmentPanel({ api, projectId, chapterId, sourceRevisionId, dirty, blocked, refreshToken, onFormalChange, registerEditor, sources = [], chapters = [] }: {
  api?: NovelCodexApi; projectId: string; chapterId: string; sourceRevisionId: string | null;
  dirty: boolean; blocked: boolean; refreshToken: number; onFormalChange: () => Promise<void>;
  registerEditor?: RegisterSupplementalEditor; sources?: CommitmentSource[]; chapters?: CommitmentChapter[];
}) {
  const identity = JSON.stringify([projectId, chapterId, sourceRevisionId, dirty]);
  const current = useRef(identity);
  const epoch = useRef(0);
  if (current.current !== identity) epoch.current += 1;
  current.current = identity;
  const generation = epoch.current;
  const isCurrent = () => current.current === identity && epoch.current === generation;
  const serial = useRef(0);
  const running = useRef(false);
  const choices = useRef(new Map<string, Choice>());
  const selected = useRef<{ identity: string; epoch: number; id: string } | null>(null);
  const [open, setOpen] = useState(false);
  const [data, setData] = useState<Data | null>(null);
  const [loading, setLoading] = useState(false);
  const [busy, setBusy] = useState(false);
  const [editing, setEditing] = useState<CommitmentDetail | null>(null);
  const [message, setMessage] = useState<{ identity: string; epoch: number; text: string; error: boolean } | null>(null);
  const visible = data?.identity === identity && data.epoch === generation && !dirty ? data : null;
  const report = visible?.report;
  const detail = visible?.detail;
  const writable = !dirty && !blocked && !busy && !loading && !editing && Boolean(api);
  const reportCurrent = report?.sourceCurrent && report.payload.sourceRevisionId === sourceRevisionId;

  async function reload(id = selected.current?.identity === identity && selected.current.epoch === generation ? selected.current.id : null) {
    if (!api || dirty) return;
    const ticket = ++serial.current;
    setLoading(true);
    try {
      const [nextReport, nextDetail] = await Promise.all([
        api.runtime.getCommitmentExtraction(projectId, chapterId),
        id ? api.runtime.getCommitment(projectId, id) : Promise.resolve(null),
      ]);
      if (isCurrent() && serial.current === ticket) setData({ identity, epoch: generation, report: nextReport, detail: nextDetail });
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
  useEffect(() => { choices.current.clear(); }, [identity]);
  useEffect(() => () => { current.current = "unmounted"; epoch.current += 1; serial.current += 1; }, []);

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
    void act(async () => (await api.runtime.establishCommitment(projectId, chapterId, frozen)).commitmentId, "已建立独立候选，尚未确认。");
  }

  function confirm() {
    if (!api || !detail || !detail.authorControls || !detail.sourceCurrent || detail.confirmationState !== "candidate") return;
    const frozen = detail;
    const controls = detail.authorControls;
    void act(async () => {
      const review = await api.runtime.createCommitmentReview(projectId, frozen.payload.id, frozen.revisionId, crypto.randomUUID());
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

  return <details className="commitment-panel" onToggle={(event) => setOpen(event.currentTarget.open)}>
    <summary>叙事承诺 · 伏笔与悬念</summary>
    {open && <div className="commitment-panel-body" aria-busy={busy || loading}>
      <p>提案只是对正文的解释。建立独立承诺不等于作者确认，也不要求每个描写都兑现。</p>
      <button className="secondary-button" disabled={!writable} onClick={() => { setMessage(null); void reload().catch((reason: unknown) => { if (isCurrent()) setMessage({ identity, epoch: generation, text: String(reason), error: true }); }); }}>刷新提案</button>
      {!api && <p role="status">桌面运行时尚未连接。</p>}
      {dirty && <p role="status">请先保存正文；旧提案已隐藏。</p>}
      {blocked && <p role="status">当前工作完成后可建立、确认或撤销承诺。</p>}
      {loading && <p role="status">正在读取来源与提案…</p>}
      {api && <CommitmentCatalog key={identity} api={api} projectId={projectId} blocked={!writable} refreshToken={refreshToken} onOpen={(id) => void act(async () => id, "已从全作品目录打开独立承诺。")} />}
      {message?.identity === identity && message.epoch === generation && <p role={message.error ? "alert" : "status"}>{message.text}</p>}
      {visible && !report && <p>本章尚无提取报告。可在对话中要求“提取并保存本章伏笔候选”。</p>}
      {report && <>
        <p className="commitment-coverage">阅读范围为模型自报 · {report.payload.examinedRanges.length} 段 · 语义完整性未知</p>
        <small className="commitment-source">正文修订：{report.payload.sourceRevisionId}</small>
        {!reportCurrent && <p role="status">正文来源已变化，不能从这份报告建立承诺。请重新提取。</p>}
        {report.payload.candidates.length === 0 && <p>本次没有提出可靠候选；不代表本章没有伏笔。</p>}
        <div className="commitment-proposals">{report.payload.candidates.map((proposal) => {
          const established = report.established.find((item) => item.proposalId === proposal.id);
          return <article key={proposal.id}>
            <header><span>{kindLabels[proposal.kind]}</span><strong>{proposal.summary}</strong></header>
            <p>兑现标准：{proposal.fulfillmentCriteria}</p>
            {proposal.setupEvidenceRefs.map((anchor, index) => <blockquote key={index}>{anchor.quote}<small>{anchor.objectId} · 字符 {anchor.start}–{anchor.end}</small></blockquote>)}
            {established ? <button className="secondary-button" disabled={!writable} onClick={() => void act(async () => established.commitmentId, "已打开独立承诺。")}>打开已建立承诺</button>
              : <button className="secondary-button" disabled={!writable || !reportCurrent} onClick={() => establish(proposal.id)}>建立独立候选</button>}
          </article>;
        })}</div>
      </>}
      {detail && <section className="commitment-detail" aria-label="独立承诺详情">
        <header><strong>{detail.payload.summary}</strong><span>{detail.confirmationState === "confirmed" ? "作者已确认" : "未确认候选"} · {statusLabels[detail.payload.status]}</span></header>
        <p>兑现标准：{detail.payload.fulfillmentCriteria}</p>
        {detail.payload.decisionReason && <p>作者理由：{detail.payload.decisionReason}</p>}
        <small className="commitment-source">提取来源：{detail.extractionOrigin?.reportRevisionId ?? "作者手工建立"} · 承诺修订：{detail.revisionId}</small>
        {detail.payload.setupEvidenceRefs.map((anchor, index) => <blockquote key={`setup-${index}`}>{anchor.quote}<small>建立证据 · {anchor.revisionId} · 字符 {anchor.start}–{anchor.end}</small></blockquote>)}
        {detail.payload.fulfillmentEvidenceRefs.map((anchor, index) => <blockquote key={index}>{anchor.quote}<small>兑现证据 · {anchor.revisionId}</small></blockquote>)}
        {!detail.sourceCurrent && <p role="status">承诺证据的正文版本已失效，需先复核才能确认。</p>}
        {detail.confirmationCommitId && !detail.authorControls?.undoAvailable && <p role="status">这次确认之后已有其他变更，不能直接撤销旧确认，以免影响后续内容。</p>}
        <p>状态是可质疑的叙事判断，不是文学质量或全书完整性认证。</p>
        <div className="commitment-actions">
          {registerEditor && <button className="secondary-button" disabled={!writable || !detail.authorControls} onClick={() => setEditing(detail)}>调整这份承诺</button>}
          <button className="primary-button" disabled={!writable || !detail.authorControls || !detail.sourceCurrent || detail.confirmationState !== "candidate"} onClick={confirm}>确认这份承诺</button>
          {detail.confirmationCommitId && <button className="secondary-button" disabled={!writable || !detail.authorControls?.undoAvailable} onClick={undo}>撤销当前确认</button>}
        </div>
      </section>}
    </div>}
    {editing && api && registerEditor && <CommitmentEditor api={api} initial={editing} sources={sources} chapters={chapters} blocked={blocked || dirty} register={registerEditor} onClose={() => setEditing(null)} onSaved={async () => { await reload(editing.payload.id); await onFormalChange(); }} />}
  </details>;
}
