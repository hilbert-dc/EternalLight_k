import { useEffect, useRef, useState } from "react";
import { revisionPredecessorIsCurrent } from "./revision-outcome-decision";
import "./revision-outcome-decision.css";
import type { NovelCodexApi, RevisionOutcome, RevisionOutcomeChoice, RevisionOutcomeDecision, RevisionOutcomeDecisionRequest, RevisionPredecessor, RevisionRepairPreparation } from "./types";

const labels: Record<RevisionOutcomeChoice, string> = {
  acceptRevision: "作为后继创作依据", repairDraft: "还需改稿", defer: "暂缓",
};

export function RevisionOutcomeDecisionPanel({ api, projectId, receipt, disabled, onPrepareContinuation, onPrepareRepair }: {
  api: NovelCodexApi | undefined;
  projectId: string;
  receipt: RevisionOutcome;
  disabled: boolean;
  onPrepareContinuation?: (predecessor: RevisionPredecessor) => void;
  onPrepareRepair?: (preparation: RevisionRepairPreparation) => void;
}) {
  const [note, setNote] = useState("");
  const [saved, setSaved] = useState<RevisionOutcomeDecision | null>(null);
  const [pending, setPending] = useState<RevisionOutcomeDecisionRequest | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [sourceChanged, setSourceChanged] = useState(false);
  const [historyOpen, setHistoryOpen] = useState(false);
  const active = useRef(true);
  const inFlight = useRef(false);
  const scope = JSON.stringify([projectId, receipt.revisionId, receipt.assessment, receipt.decisions]);
  const controlsRef = useRef({ disabled, onPrepareContinuation, onPrepareRepair, scope, token: {} });
  const token = controlsRef.current.disabled !== disabled || controlsRef.current.scope !== scope ? {} : controlsRef.current.token;
  controlsRef.current = { disabled, onPrepareContinuation, onPrepareRepair, scope, token };
  useEffect(() => { active.current = true; return () => { active.current = false; }; }, []);
  const recordsValid = Array.isArray(receipt.decisions) && receipt.decisions.length <= 20;
  const history = recordsValid ? (saved ? [saved, ...receipt.decisions.filter(item => item.id !== saved.id)] : receipt.decisions).slice(0, 20) : [];
  const latest = history[0];
  const predecessor = latest ? { writerTaskId: receipt.assessment.writerTaskId,
    assessmentRevisionId: receipt.revisionId, decisionId: latest.id } : null;
  const accepted = recordsValid && predecessor && revisionPredecessorIsCurrent(
    { ...receipt, decisions: history }, predecessor);
  const repair = recordsValid && predecessor && revisionPredecessorIsCurrent(
    { ...receipt, decisions: history }, predecessor, "repairDraft");
  const noteError = [...note].length > 1000 ? "决策备注最多 1000 个字符。" : null;
  const blocked = disabled || busy || sourceChanged || !recordsValid || receipt.state !== "current";

  async function decide(request: RevisionOutcomeDecisionRequest) {
    if (blocked || inFlight.current) return;
    inFlight.current = true; setBusy(true); setError(null); setPending(request);
    try {
      if (!api?.runtime.decideRevisionOutcome) throw new Error("作者决策接口不可用，请重新读取后重试。");
      const decision = await api.runtime.decideRevisionOutcome(projectId, receipt.assessment.writerTaskId, request);
      if (!active.current) return;
      if (decision.id !== request.decisionId || decision.writerTaskId !== receipt.assessment.writerTaskId
        || decision.assessmentRevisionId !== request.expectedAssessmentRevisionId
        || decision.draftRevisionId !== receipt.assessment.draftRevisionId
        || decision.knowledgeBatchId !== receipt.assessment.knowledgeBatchId
        || decision.choice !== request.choice || decision.note !== request.note) throw new Error("决策回执与本次来源不匹配，请重新读取修改结果。");
      setSaved(decision); setPending(null); setNote("");
    } catch (reason) {
      if (active.current) setError(reason instanceof Error ? reason.message : "作者决策未能保存，请重新读取或重试原请求。");
    } finally {
      if (active.current) { inFlight.current = false; setBusy(false); }
    }
  }

  async function prepare() {
    if (blocked || inFlight.current || !predecessor || !accepted || !onPrepareContinuation) return;
    inFlight.current = true; setBusy(true); setError(null);
    try {
      if (!api) throw new Error("修改结果读取接口不可用");
      const current = await api.runtime.getRevisionOutcome(projectId, predecessor.writerTaskId);
      if (!active.current) return;
      if (!revisionPredecessorIsCurrent(current, predecessor)) {
        setSourceChanged(true); throw new Error("修改结果或最新作者决策已变化，请重新读取后准备后继创作。");
      }
      if (!controlsRef.current.disabled) controlsRef.current.onPrepareContinuation?.(predecessor);
    } catch (reason) {
      if (active.current) setError(reason instanceof Error ? reason.message : "后继创作来源核对失败");
    } finally {
      if (active.current) { inFlight.current = false; setBusy(false); }
    }
  }

  async function prepareRepair() {
    if (blocked || inFlight.current || !predecessor || !repair || !onPrepareRepair) return;
    inFlight.current = true; setBusy(true); setError(null);
    try {
      if (!api) throw new Error("修改结果读取接口不可用");
      const current = await api.runtime.getRevisionOutcome(projectId, predecessor.writerTaskId);
      if (!active.current || controlsRef.current.token !== token) return;
      if (!revisionPredecessorIsCurrent(current, predecessor, "repairDraft")
        || current?.assessment.draftRevisionId !== receipt.assessment.draftRevisionId
        || current.assessment.knowledgeBatchId !== receipt.assessment.knowledgeBatchId
        || current.assessment.chapterId !== receipt.assessment.chapterId) {
        setSourceChanged(true); throw new Error("修改结果或最新作者决策已变化，请重新读取后讨论下一次改稿。");
      }
      if (!controlsRef.current.disabled) controlsRef.current.onPrepareRepair?.({
        projectId, source: predecessor, chapterId: current.assessment.chapterId,
        draftRevisionId: current.assessment.draftRevisionId, knowledgeBatchId: current.assessment.knowledgeBatchId,
        note: current.decisions[0].note,
      });
    } catch (reason) {
      if (active.current && controlsRef.current.token === token) setError(reason instanceof Error ? reason.message : "改稿讨论来源核对失败");
    } finally {
      if (active.current) { inFlight.current = false; setBusy(false); }
    }
  }

  return <section className="revision-outcome-decision" aria-label="作者修改结果决策" aria-busy={busy}>
    <p>由作者决定本次修改结果。“作为后继创作依据”只记录决定；正式采用正文和事实仍需分别确认，后继创作也需另行确认范围与预算。</p>
    {latest && <p role="status">{saved ? "作者决策已保存" : "最新作者决策"}：{labels[latest.choice]}{latest.note && ` · ${latest.note}`}</p>}
    {!recordsValid && <p role="alert">作者决策记录不可用，请重新读取修改结果。</p>}
    {recordsValid && <details className="revision-decision-history" onToggle={event => setHistoryOpen(event.currentTarget.open)}>
      <summary>作者决策历史 · {history.length} 条（最多 20 条）</summary>
      {historyOpen && <><p>历史决定只用于追溯。后继创作以当前报告对应的最新作者决定为准。</p>
        <ol aria-label="作者决策历史">{history.map(item => <li key={item.id}>
          <strong>{labels[item.choice]}</strong>{item.note && <p>{item.note}</p>}
          <small>决定 {item.id} · 写作尝试 {item.writerTaskId} · 判断 {item.assessmentRevisionId} · 正文 {item.draftRevisionId} · 知识批次 {item.knowledgeBatchId} · 时间 {item.createdAt}</small>
        </li>)}</ol></>}
    </details>}
    <label>决策备注（可选）<textarea aria-label="修改结果决策备注" value={note} disabled={blocked || Boolean(pending)} onChange={event => setNote(event.target.value)} /></label>
    {noteError && <p role="alert">{noteError}</p>}
    <div className="revision-decision-actions">{(Object.keys(labels) as RevisionOutcomeChoice[]).map(choice => <button className="secondary-button" type="button" key={choice}
      disabled={blocked || Boolean(noteError) || Boolean(pending)} onClick={() => void decide({
        decisionId: `revision-decision-${crypto.randomUUID()}`, expectedAssessmentRevisionId: receipt.revisionId,
        expectedDecisionId: latest?.id ?? null, choice, note,
      })}>{labels[choice]}</button>)}</div>
    {busy && <p role="status">正在核对并保存…</p>}
    {error && <p role="alert">{error}</p>}
    {pending && error && <button className="secondary-button" type="button" disabled={blocked} onClick={() => void decide(pending)}>重试保存原决定</button>}
    {accepted && onPrepareContinuation && <button className="secondary-button" type="button" disabled={blocked || Boolean(pending)} onClick={() => void prepare()}>准备后继创作</button>}
    {repair && onPrepareRepair && <button className="secondary-button" type="button" disabled={blocked || Boolean(pending)} onClick={() => void prepareRepair()}>讨论下一次改稿</button>}
    {receipt.state === "stale" && <small>来源已变化，不能记录新决定或准备后继创作。</small>}
  </section>;
}
