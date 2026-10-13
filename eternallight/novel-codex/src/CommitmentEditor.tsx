import { useLayoutEffect, useRef, useState } from "react";
import type { CommitmentDetail, NovelCodexApi } from "./types";
import { CommitmentFields, type CommitmentChapter, type CommitmentSource } from "./CommitmentFields";
import type { RegisterSupplementalEditor } from "./supplemental-editor";

type SaveRequest = Parameters<NovelCodexApi["runtime"]["saveCommitment"]>[2];
export function CommitmentEditor({ api, initial, sources, chapters, blocked, register, onClose, onSaved }: {
  api: NovelCodexApi; initial: CommitmentDetail; sources: CommitmentSource[]; chapters: CommitmentChapter[]; blocked: boolean;
  register: RegisterSupplementalEditor; onClose: () => void; onSaved: () => Promise<void>;
}) {
  const [baseline, setBaseline] = useState(initial);
  const [payload, setPayload] = useState(() => structuredClone(initial.payload));
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const pending = useRef<SaveRequest | null>(null);
  const inFlight = useRef(false);
  const dirty = JSON.stringify(payload) !== JSON.stringify(baseline.payload);
  const latest = useRef(payload);
  latest.current = payload;
  const alive = useRef(true);
  useLayoutEffect(() => { alive.current = true; return () => { alive.current = false; register(null); }; }, [register]);

  async function save(): Promise<boolean> {
    if (inFlight.current || blocked || !baseline.authorControls) return false;
    if (!dirty && !pending.current) return true;
    const request = pending.current ?? {
      payload: structuredClone(payload), extractionOrigin: baseline.extractionOrigin,
      expectedRevision: baseline.authorControls.projectRevision, baseSnapshotId: baseline.snapshotId,
      expectedCandidateRevisionId: baseline.revisionId, idempotencyKey: crypto.randomUUID(),
    };
    pending.current = request;
    inFlight.current = true; setBusy(true); setMessage(null);
    try {
      const receipt = await api.runtime.saveCommitment(initial.projectId, initial.payload.id, request);
      if (!alive.current) return false;
      pending.current = null;
      setBaseline({ ...baseline, payload: request.payload, revisionId: receipt.revisionId, snapshotId: receipt.snapshotId,
        confirmationState: "candidate", confirmationCommitId: null,
        authorControls: { projectRevision: request.expectedRevision, undoAvailable: false } });
      const stillDirty = JSON.stringify(latest.current) !== JSON.stringify(request.payload);
      setMessage(stillDirty ? "原保存回执已核对；后来修改的输入仍未保存，请再次保存。" : "已保存新的候选修订，尚未确认。");
      if (!stillDirty) {
        try { await onSaved(); } catch { if (alive.current) setMessage("候选已保存；列表刷新失败，请稍后刷新。无需重复写入。"); }
      }
      return !stillDirty;
    } catch (reason) {
      if (alive.current) setMessage(`${String(reason)}。保存结果尚未核对，输入仍保留。重试使用原请求；也可明确核对当前版本。`);
      return false;
    } finally { inFlight.current = false; if (alive.current) setBusy(false); }
  }

  async function inspectCurrent() {
    if (inFlight.current || blocked) return;
    inFlight.current = true; setBusy(true);
    try {
      const detail = await api.runtime.getCommitment(initial.projectId, initial.payload.id);
      if (!alive.current) return;
      if (!detail.authorControls || detail.objectId !== initial.objectId || JSON.stringify(detail.extractionOrigin) !== JSON.stringify(initial.extractionOrigin)) throw new Error("当前对象身份或来源不匹配");
      setBaseline(detail); pending.current = null;
      setMessage("已明确选用当前服务器版本作为基线；本地输入保留，仍需保存。并发变更仍会由版本校验拦截。");
    } catch (reason) { if (alive.current) setMessage(`${String(reason)}。原输入与保存基线未改变。`); }
    finally { inFlight.current = false; if (alive.current) setBusy(false); }
  }

  useLayoutEffect(() => {
    register({ dirty: dirty || pending.current !== null || busy, busy, save, discard: () => {
      if (inFlight.current) return false;
      pending.current = null; register(null); onClose(); return true;
    } });
  });
  return <section className="commitment-editor" aria-label="承诺编辑器" aria-busy={busy}>
    <p>编辑原文解释，不修改小说正文；保存只建立候选，确认仍需另行决定。</p>
    <small className="commitment-source">保存基线：{baseline.revisionId} · {baseline.snapshotId}</small>
    {blocked && <p role="status">执行中的任务结束后才能保存。</p>}
    <CommitmentFields value={payload} sources={sources} chapters={chapters} disabled={blocked || busy} onChange={setPayload} />
    {message && <p role="status">{message}</p>}
    <div className="commitment-actions">
      <button className="primary-button" disabled={blocked || busy || !baseline.authorControls || (!dirty && !pending.current)} onClick={() => void save()}>保存承诺候选</button>
      <button className="secondary-button" disabled={blocked || busy} onClick={() => void inspectCurrent()}>核对当前版本并保留输入</button>
      <button className="text-button" disabled={busy} onClick={() => { pending.current = null; register(null); onClose(); }}>放弃本地承诺编辑</button>
    </div>
    <small>放弃只丢弃本地输入，不撤回可能已经保存的候选。</small>
  </section>;
}
