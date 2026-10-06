import { useEffect, useRef, useState } from "react";
import type { NovelCodexApi } from "./types";
import type { QualityChange, QualityKind, QualityNotebook as Notebook, QualityPacket, QualityReview } from "./quality-types";
import "./quality-notebook.css";

const names: Record<QualityKind, string> = { preference: "审美偏好", goal: "期待效果", strength: "保留亮点", observation: "质量观察", trial: "场景试写" };
const statusNames: Record<string, string> = { prepared: "待阅读", queued: "准备中", running: "阅读中", completed: "已完成", failed: "未完成", cancelled: "已停止" };
export function QualityNotebook({ api, projectId }: { api: NovelCodexApi; projectId: string }) {
  const [state, setState] = useState<Notebook | null>(null);
  const [sources, setSources] = useState<{ id: string; title: string }[]>([]);
  const [error, setError] = useState(""); const [busy, setBusy] = useState(false);
  const [kind, setKind] = useState<QualityKind>("preference"); const [scope, setScope] = useState<"work" | "task">("work");
  const [text, setText] = useState(""); const [revision, setRevision] = useState(""); const [quote, setQuote] = useState("");
  const [question, setQuestion] = useState(""); const [body, setBody] = useState("");
  const [relatesTo, setRelatesTo] = useState(""); const [verdict, setVerdict] = useState<"supported" | "unmet" | "insufficientEvidence">("insufficientEvidence");
  const [selected, setSelected] = useState<string[]>([]);
  const [opened, setOpened] = useState<{ id: string; packet: QualityPacket; review: QualityReview } | null>(null);
  const [preference, setPreference] = useState("abstain"); const [reason, setReason] = useState("");
  const mounted = useRef(true); const pending = useRef(false);
  const reload = async () => {
    const [notebook, workspace] = await Promise.all([api.runtime.getQualityNotebook(projectId), api.runtime.getCreativeWorkspace(projectId)]);
    if (!mounted.current) return;
    setState(notebook);
    setSources(workspace.chapters.flatMap(chapter => { const draft = chapter.working ?? chapter.candidate ?? chapter.accepted; return draft ? [{ id: draft.id, title: `第 ${chapter.number} 章 · ${chapter.title}` }] : []; }));
  };
  useEffect(() => { mounted.current = true; void reload().catch(e => { if (mounted.current) setError(String(e.message ?? e)); }); return () => { mounted.current = false; }; }, [api, projectId]);
  const act = async (action: () => Promise<void>) => {
    if (pending.current) return; pending.current = true; setBusy(true); setError("");
    try { await action(); } catch (e) { if (mounted.current) setError(e instanceof Error ? e.message : String(e)); }
    finally { pending.current = false; if (mounted.current) setBusy(false); }
  };
  // Reuse the identical request after an unknown result; changing input makes a new request.
  const mutation = useRef<{ fingerprint: string; request: { expectedVersion: number; idempotencyKey: string; change: QualityChange } } | null>(null);
  const save = async (change: QualityChange) => {
    if (!state) throw new Error("请先读取创作回看");
    const fingerprint = JSON.stringify({ version: state.version, change });
    if (mutation.current?.fingerprint !== fingerprint) mutation.current = { fingerprint, request: { expectedVersion: state.version, idempotencyKey: crypto.randomUUID(), change } };
    const result = await api.runtime.changeQualityNotebook(projectId, mutation.current.request);
    mutation.current = null;
    await reload(); return result;
  };
  const openPacket = async (id: string) => {
    const [packet, review] = await Promise.all([api.runtime.getQualityPacket(projectId, id), api.runtime.getQualityReview(projectId, id)]);
    if (mounted.current) { setOpened({ id, packet, review }); setPreference("abstain"); setReason(""); }
  };
  return <div className="quality-notebook">
    <p>保留这部作品的特点，用正文证据回看。观察与试写不会自动改稿，也不代表文学质量认证。</p>
    {error && <p role="alert">{error}</p>}
    <button disabled={busy} onClick={() => void act(reload)}>刷新记录</button>
    <p>创作时，智能体会按需记录与安排独立阅读；这里用于查看、纠正偏好或手动补充审阅。读者意见不会自动采用。</p>
    {!state ? <p role="status">正在读取…</p> : <>
      <details><summary>记下一项创作取向</summary>
        <form onSubmit={event => { event.preventDefault(); void act(async () => {
          await save({ action: "note", note: { kind, scope, text, evidence: revision && quote.trim() ? [{ revisionId: revision, quote, role: "current" }] : [], question: kind === "trial" ? question : null, body: kind === "trial" ? body : null, relatesTo: kind === "observation" && relatesTo ? relatesTo : null, verdict: kind === "observation" && relatesTo ? verdict : null } });
          if (mounted.current) { setText(""); setQuote(""); setBody(""); }
        }); }}>
          <label>记录类型<select value={kind} onChange={e => setKind(e.target.value as QualityKind)}>{Object.entries(names).map(([value, name]) => <option key={value} value={value}>{name}</option>)}</select></label>
          <label>适用范围<select value={scope} onChange={e => setScope(e.target.value as "work" | "task")}><option value="work">这部作品</option><option value="task">仅本次，不用于后续任务</option></select></label>
          <label>内容<textarea required maxLength={500} value={text} onChange={e => setText(e.target.value)} placeholder="例如：用动作表达关心，少用旁白解释。" /></label>
          <label>原文来源<select required={kind !== "preference"} value={revision} onChange={e => setRevision(e.target.value)}><option value="">选择已保存正文</option>{sources.map(s => <option key={s.id} value={s.id}>{s.title}</option>)}</select></label>
          <label>逐字引文{kind === "preference" && "（可选）"}<textarea required={kind !== "preference"} maxLength={600} value={quote} onChange={e => setQuote(e.target.value)} /></label>
          {kind === "observation" && <><label>回看的目标或亮点（可选）<select value={relatesTo} onChange={e => setRelatesTo(e.target.value)}><option value="">独立观察</option>{state.notes.filter(n => ["goal", "strength"].includes(n.note.kind) && n.status !== "withdrawn").map(n => <option key={n.id} value={n.id}>{n.note.text}</option>)}</select></label>{relatesTo && <label>正文中的效果<select value={verdict} onChange={e => setVerdict(e.target.value as typeof verdict)}><option value="insufficientEvidence">证据不足</option><option value="supported">有证据支持</option><option value="unmet">尚未达到</option></select></label>}</>}
          {kind === "trial" && <><label>共同探索的问题<input required maxLength={250} value={question} onChange={e => setQuestion(e.target.value)} /></label><label>实际试写片段<textarea required maxLength={2400} value={body} onChange={e => setBody(e.target.value)} /></label><p>试写只作为比较材料，不写入章节。</p></>}
          <button disabled={busy} type="submit">保存记录</button>
        </form>
      </details>
      <div className="quality-notes">{state.notes.length === 0 && <p>暂无记录。可以先记下你希望保留的作品特点。</p>}{state.notes.map(item => <article key={item.id}>
        <div className="quality-meta">{names[item.note.kind]} · {item.authorKind === "model" ? "模型建议" : "作者记录"} · {item.status === "confirmed" ? "作者已确认" : item.status === "withdrawn" ? "已撤回" : "待作者判断"}{!item.sourceCurrent && " · 来源已变化"}{item.note.scope === "task" && " · 仅本次"}</div>
        <p>{item.note.text}</p>
        {item.note.verdict && <p>效果判断：{{ supported: "有证据支持", unmet: "尚未达到", insufficientEvidence: "证据不足" }[item.note.verdict]}</p>}
        {item.note.evidence.map((e, i) => <blockquote key={i}>{e.role === "reference" && <small>对照材料 · </small>}{e.quote}</blockquote>)}
        {item.note.body && <details><summary>查看试写</summary><p className="quality-prose">{item.note.body}</p></details>}
        {item.note.kind === "trial" && item.sourceCurrent && item.status !== "withdrawn" && <label className="quality-select"><input type="checkbox" checked={selected.includes(item.id)} disabled={busy || (selected.length >= 6 && !selected.includes(item.id))} onChange={e => { const checked = e.currentTarget.checked; setSelected(old => checked ? [...old, item.id] : old.filter(id => id !== item.id)); }} />加入匿名比较</label>}
        <div className="quality-actions">{item.status !== "confirmed" && item.sourceCurrent && <button disabled={busy} onClick={() => void act(async () => { await save({ action: "decide", id: item.id, status: "confirmed", reason: "作者在创作回看中明确确认" }); })}>确认这项记录</button>}{item.status !== "withdrawn" && <button disabled={busy} onClick={() => void act(async () => { await save({ action: "decide", id: item.id, status: "withdrawn", reason: "作者撤回；保留历史，不影响正文" }); })}>撤回</button>}</div>
      </article>)}</div>
      {state.nextCursor !== null && <button disabled={busy} onClick={() => void act(async () => { const page = await api.runtime.getQualityNotebook(projectId, state.nextCursor); if (mounted.current) setState(current => current ? { ...page, notes: [...current.notes, ...page.notes.filter(n => !current.notes.some(old => old.id === n.id))], packets: [...current.packets, ...page.packets.filter(n => !current.packets.some(old => old.id === n.id))] } : page); })}>读取更早记录</button>}
      <div className="quality-actions"><button disabled={busy || selected.length < 2} onClick={() => void act(async () => { const result = await save({ action: "packet", kind: "comparison", sources: selected.map(noteId => ({ noteId, revisionId: null, quote: null })) }); setSelected([]); await openPacket(result.result.id); })}>匿名比较所选试写</button></div>
      <details><summary>准备一次读者审阅</summary><p>选择已保存正文。留空引文则读取整章；长章超过阅读包上限时，请明确选择片段。准备材料不消耗模型额度。</p><label>阅读来源<select value={revision} onChange={e => setRevision(e.target.value)}><option value="">选择章节</option>{sources.map(s => <option key={s.id} value={s.id}>{s.title}</option>)}</select></label><label>限定阅读片段（可选）<textarea value={quote} onChange={e => setQuote(e.target.value)} maxLength={6000} /></label><button disabled={busy || !revision} onClick={() => void act(async () => { const result = await save({ action: "packet", kind: "reading", sources: [{ revisionId: revision, quote: quote.trim() ? quote : null, noteId: null }] }); await openPacket(result.result.id); })}>准备阅读包</button></details>
      <h4>阅读与比较</h4>{state.packets.length === 0 && <p>尚无阅读包。</p>}{state.packets.map((packet, i) => <article key={packet.id}><div className="quality-meta">{packet.kind === "comparison" ? "匿名比较" : "读者审阅"} {state.packets.length - i} · {statusNames[packet.status] ?? "状态待核对"}{!packet.sourceCurrent && " · 来源已变化"}{packet.hasReport && " · 报告已保存"} · {packet.feedbackCount} 次作者反馈</div><div className="quality-actions"><button disabled={busy} onClick={() => void act(() => openPacket(packet.id))}>查看材料与反馈</button>{!packet.hasReport && packet.sourceCurrent && ["prepared", "failed", "cancelled"].includes(packet.status) && <button disabled={busy} onClick={() => void act(async () => { await api.runtime.reviewQualityPacket(projectId, packet.id); await reload(); await openPacket(packet.id); })}>启动独立读者任务</button>}</div></article>)}
      {opened && <div className="quality-reader"><h4>匿名阅读材料</h4><p>以下仅隐藏候选标签；你可能已了解原稿。这不等于真人盲评。独立模型任务只接收这些文本，并使用现有任务额度。</p>{opened.packet.items.map(item => <details key={item.label}><summary>文本 {item.label}</summary><p className="quality-prose">{item.text}</p></details>)}
        {!opened.review.sourceCurrent && <p>来源已变化。这份报告仍对应当时的阅读材料，不代表当前稿件。</p>}
        {opened.review.calibration.preferenceDisagreement !== null && <p>{opened.review.calibration.preferenceDisagreement ? "作者与模型的选择不同，保留这处分歧。" : "作者与模型的选择一致，但不能据此认定作品全面改善。"}</p>}
        {opened.review.report && <details><summary>查看模型阅读假设</summary><p>{opened.review.report.report.summary}</p><p>模型选择：{opened.review.report.report.preference === "tie" ? "各有取舍" : opened.review.report.report.preference === "abstain" ? "暂不判断" : `文本 ${opened.review.report.report.preference}`}</p>{opened.review.report.report.findings.map((f, i) => <div key={i}><blockquote>{f.label}：{f.quote}</blockquote><p>{f.effect}</p></div>)}<p>限制：{opened.review.report.report.limitations}</p></details>}
        <form onSubmit={e => { e.preventDefault(); void act(async () => { await save({ action: "feedback", id: opened.id, preference, reason }); await openPacket(opened.id); }); }}><label>你的选择<select value={preference} onChange={e => setPreference(e.target.value)}><option value="abstain">暂不判断</option><option value="tie">各有取舍</option>{opened.packet.items.map(item => <option key={item.label} value={item.label}>文本 {item.label}</option>)}</select></label><label>具体理由<textarea required maxLength={500} value={reason} onChange={e => setReason(e.target.value)} /></label><button disabled={busy}>保存作者反馈</button></form>
        {opened.review.latestAuthorFeedback.map(f => <p key={f.version}>作者反馈 · {f.preference === "tie" ? "各有取舍" : f.preference === "abstain" ? "暂不判断" : `文本 ${f.preference}`}：{f.reason}</p>)}<button onClick={() => setOpened(null)}>收起阅读材料</button>
      </div>}
    </>}{busy && <p role="status">正在处理，已保存的作品不会被改写…</p>}
  </div>;
}
