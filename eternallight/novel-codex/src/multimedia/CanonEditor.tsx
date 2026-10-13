import { useEffect, useRef, useState } from "react";
import type { CanonDocument, CanonEntry, CanonWorkspace, MediaSourceRef, MultimediaApi } from "../../shared/multimedia-types";
import { newMediaId } from "./editor-helpers";

const categories: Record<CanonEntry["category"], string> = { character: "人物", world: "世界", rule: "规则", timeline: "时间线", style: "风格" };
export function CanonEditor({ api, authorKey, projectId, busy, source, onClose, onSaved }: {
  api: MultimediaApi; authorKey: string; projectId: string; busy: boolean; source: MediaSourceRef | null;
  onClose: () => void; onSaved: () => void;
}) {
  const [workspace, setWorkspace] = useState<CanonWorkspace | null>(null), [documentValue, setDocument] = useState<CanonDocument | null>(null);
  const [expected, setExpected] = useState<string | null>(null), [error, setError] = useState<string | null>(null), [notice, setNotice] = useState<string | null>(null);
  const [saving, setSaving] = useState(false); const alive = useRef(true), pending = useRef(false), form = useRef<HTMLFormElement>(null);
  const attempt = useRef<{ fingerprint: string; key: string } | null>(null);
  const key = `novel-codex:media-editor-draft:${JSON.stringify([authorKey, projectId, "canon"])}`;
  const dirty = Boolean(workspace && documentValue && (JSON.stringify(workspace.document) !== JSON.stringify(documentValue) || expected !== workspace.revisionId));
  useEffect(() => {
    alive.current = true; const previous = document.activeElement as HTMLElement | null;
    void api.readCanon(projectId).then(value => {
      if (!alive.current) return;
      if (value.projectId !== projectId) throw Error("共同设定不属于当前作品。");
      setWorkspace(value); let draft: { document: CanonDocument; baseRevisionId: string | null; attempt?: { fingerprint: string; key: string } } | null = null;
      try { draft = JSON.parse(localStorage.getItem(key) ?? "null"); } catch { /* Malformed local recovery is never used. */ }
      if (draft && Array.isArray(draft.document?.entries) && (draft.baseRevisionId === null || typeof draft.baseRevisionId === "string")) {
        setDocument(draft.document); setExpected(draft.baseRevisionId); attempt.current = draft.attempt ?? null;
        setNotice(draft.baseRevisionId === value.revisionId ? "已恢复未保存的设定。" : "共同设定已有新版本，已保留本地修改，请核对后保存。");
      } else { setDocument(value.document); setExpected(value.revisionId); }
    }).catch(reason => { if (alive.current) setError(reason instanceof Error ? reason.message : "共同设定读取未完成。"); });
    return () => { alive.current = false; if (previous?.isConnected) previous.focus({ preventScroll: true }); };
  }, [api, projectId, key]);
  useEffect(() => { if (documentValue) form.current?.querySelector<HTMLInputElement>("input")?.focus(); }, [Boolean(documentValue)]);
  useEffect(() => {
    if (!workspace || !documentValue) return;
    try { if (dirty) localStorage.setItem(key, JSON.stringify({ document: documentValue, baseRevisionId: expected, attempt: attempt.current })); else localStorage.removeItem(key); }
    catch { setNotice("本地恢复空间不足，请保存当前设定。"); }
  }, [workspace, documentValue, expected, dirty, key]);
  function changeEntry(id: string, change: Partial<CanonEntry>) { setDocument(value => value ? { ...value, entries: value.entries.map(entry => entry.id === id ? { ...entry, ...change } : entry) } : value); }
  async function save() {
    if (!documentValue || !workspace || pending.current || busy) return;
    if (documentValue.entries.some(entry => !entry.statement.trim())) { setError("请补全每条设定，或删除空白条目。"); return; }
    pending.current = true; setSaving(true); setError(null);
    const fingerprint = JSON.stringify({ expectedRevisionId: expected, document: documentValue });
    if (attempt.current?.fingerprint !== fingerprint) attempt.current = { fingerprint, key: newMediaId("canon-save") };
    try {
      try { localStorage.setItem(key, JSON.stringify({ document: documentValue, baseRevisionId: expected, attempt: attempt.current })); } catch { /* Authoritative saving remains available when local recovery is full. */ }
      const value = await api.saveCanon(projectId, { expectedRevisionId: expected, document: documentValue, idempotencyKey: attempt.current.key });
      if (!alive.current) return;
      if (value.projectId !== projectId || !value.revisionId || JSON.stringify(value.document) !== JSON.stringify(documentValue)) throw Error("未收到当前设定的保存回执。");
      setWorkspace(value); setExpected(value.revisionId); setDocument(value.document); attempt.current = null; try { localStorage.removeItem(key); } catch { /* The durable receipt is authoritative. */ } setNotice("共同设定已保存。作品可明确选择使用此版本。"); onSaved();
    } catch (reason) { if (alive.current) setError(reason instanceof Error ? reason.message : "保存未完成，修改已保留。"); }
    finally { pending.current = false; if (alive.current) setSaving(false); }
  }
  return <div className="modal-backdrop" role="presentation"><form ref={form} className="project-dialog mm-canon-dialog" role="dialog" aria-modal="true" aria-labelledby="canon-title" onSubmit={event => { event.preventDefault(); void save(); }} onKeyDown={event => {
    if (event.key === "Escape" && !pending.current) { event.preventDefault(); onClose(); }
    if (event.key === "Tab") {
      const items = [...event.currentTarget.querySelectorAll<HTMLElement>("button:not(:disabled),input:not(:disabled),select:not(:disabled),textarea:not(:disabled)")];
      if (event.shiftKey && document.activeElement === items[0]) { event.preventDefault(); items.at(-1)?.focus(); }
      else if (!event.shiftKey && document.activeElement === items.at(-1)) { event.preventDefault(); items[0]?.focus(); }
    }
  }}><header className="mm-inspector-heading"><h2 id="canon-title">共同设定</h2><button type="button" disabled={saving} onClick={onClose}>关闭</button></header>
    <p>你明确保存的设定可用于小说、漫画和动画。作品保留各自采用的设定版本。</p>
    {error && <p role="alert" className="mm-error">{error}</p>}{notice && <p role="status">{notice}</p>}
    {!documentValue ? <p role="status">正在读取共同设定…</p> : <fieldset disabled={saving || busy} className="mm-fields">
      <label className="mm-field"><span>名称</span><input value={documentValue.title} maxLength={160} onChange={event => setDocument({ ...documentValue, title: event.target.value })} /></label>
      <div className="mm-canon-entries">{documentValue.entries.map((entry, index) => <article className="mm-canon-entry" key={entry.id}>
        <label className="mm-field"><span>第 {index + 1} 条 · 类别</span><select value={entry.category} onChange={event => changeEntry(entry.id, { category: event.target.value as CanonEntry["category"] })}>{Object.entries(categories).map(([id, label]) => <option key={id} value={id}>{label}</option>)}</select></label>
        <label className="mm-field"><span>设定内容</span><textarea value={entry.statement} rows={3} maxLength={4000} onChange={event => changeEntry(entry.id, { statement: event.target.value })} /></label>
        <div className="mm-inline-actions">{entry.sources.length > 0 && <span>已关联 {entry.sources.length} 处保存内容</span>}{source && <button type="button" disabled={entry.sources.length >= 10 || entry.sources.some(ref => JSON.stringify(ref) === JSON.stringify(source))} onClick={() => changeEntry(entry.id, { sources: [...entry.sources, source] })}>关联当前保存内容</button>}<button type="button" onClick={() => setDocument({ ...documentValue, entries: documentValue.entries.filter(value => value.id !== entry.id) })} aria-label={`删除第 ${index + 1} 条设定`}>删除</button></div>
      </article>)}</div>
      <button type="button" disabled={documentValue.entries.length >= 500} onClick={() => setDocument({ ...documentValue, entries: [...documentValue.entries, { id: newMediaId("canon-entry"), category: "world", statement: "", sources: [] }] })}>＋ 添加设定</button>
      <div className="mm-inline-actions"><button className="primary-button" type="submit" disabled={!dirty}>{saving ? "保存中…" : "保存共同设定"}</button>{dirty && <><button type="button" onClick={() => { setDocument(workspace!.document); setExpected(workspace!.revisionId); attempt.current = null; setNotice(null); setError(null); }}>恢复已保存设定</button><span role="status">未保存修改会保留在本地。</span></>}</div>
    </fieldset>}
  </form></div>;
}
