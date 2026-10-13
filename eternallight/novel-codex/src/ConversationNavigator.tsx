import { describeAgentError } from "./agent-error-description";
import { useEffect, useMemo, useRef, useState } from "react";
import type { ThreadSummary } from "./types";
import "./conversation-navigator.css";

export type ConversationNavigatorProps = {
  threads: ThreadSummary[];
  selectedId: string | null;
  runningIds: string[];
  busy: boolean;
  loading: boolean;
  error: string | null;
  archived: boolean;
  onSelect: (thread: ThreadSummary) => void;
  onInspect?: (thread: ThreadSummary) => void;
  onCreate: () => void;
  onRename: (thread: ThreadSummary, title: string) => void | Promise<unknown>;
  onPin: (thread: ThreadSummary) => void;
  onArchive: (thread: ThreadSummary) => void;
  onToggleArchived: () => void;
  onRetry: () => void;
};

export function ConversationNavigator({ threads, selectedId, runningIds, busy, loading,
  error, archived, onSelect, onInspect, onCreate, onRename, onPin, onArchive,
  onToggleArchived, onRetry }: ConversationNavigatorProps) {
  const [query, setQuery] = useState("");
  const [actionsId, setActionsId] = useState<string | null>(null);
  const [editing, setEditing] = useState<{ id: string; title: string; error: string | null; pending: boolean } | null>(null);
  const root = useRef<HTMLElement>(null);
  const nameInput = useRef<HTMLInputElement>(null);
  const archiveToggle = useRef<HTMLButtonElement>(null);
  const actionButtons = useRef(new Map<string, HTMLButtonElement>());
  const mounted = useRef(true);
  const renamePending = useRef(false);
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; }; }, []);
  useEffect(() => {
    if (editing) { nameInput.current?.focus(); nameInput.current?.select(); }
  }, [editing?.id]);
  useEffect(() => {
    if (!actionsId) return;
    const close = (event: PointerEvent) => {
      if (event.target instanceof Node && !root.current?.querySelector(`[data-conversation-row="${CSS.escape(actionsId)}"]`)?.contains(event.target)) setActionsId(null);
    };
    document.addEventListener("pointerdown", close);
    return () => document.removeEventListener("pointerdown", close);
  }, [actionsId]);
  useEffect(() => {
    setActionsId(null); setEditing(null);
  }, [archived]);
  const filtered = useMemo(() => {
    const search = query.trim().toLocaleLowerCase();
    return threads.filter(thread => !search || `${thread.name ?? "新对话"}\n${thread.preview ?? ""}`.toLocaleLowerCase().includes(search))
      .sort((a, b) => Number(b.isPinned) - Number(a.isPinned) || (b.updatedAt ?? b.createdAt ?? 0) - (a.updatedAt ?? a.createdAt ?? 0));
  }, [threads, query]);
  function closeEditor(id: string) {
    setEditing(null);
    requestAnimationFrame(() => actionButtons.current.get(id)?.focus({ preventScroll: true }));
  }
  function closeActions(id: string) {
    setActionsId(null);
    requestAnimationFrame(() => (actionButtons.current.get(id) ?? archiveToggle.current)?.focus({ preventScroll: true }));
  }
  async function rename(thread: ThreadSummary) {
    if (!editing || editing.id !== thread.id || busy || renamePending.current) return;
    const title = editing.title.trim();
    if (!title) { setEditing({ ...editing, error: "请填写对话名称。" }); nameInput.current?.focus(); return; }
    renamePending.current = true;
    setEditing({ ...editing, pending: true, error: null });
    try {
      await onRename(thread, title);
      if (mounted.current) closeEditor(thread.id);
    } catch (cause) {
      if (mounted.current) setEditing(current => current?.id === thread.id
        ? { ...current, pending: false, error: cause instanceof Error ? cause.message : "重命名失败，请重试。" } : current);
    } finally { renamePending.current = false; }
  }
  return <section ref={root} className="conversation-navigator" aria-label="作品对话">
    <header><strong>对话 <small>{threads.length}</small></strong><button type="button" className="conversation-new-button" disabled={busy} onClick={onCreate} aria-label="新建对话" title="新建独立对话">＋<span>新建</span></button></header>
    <div className="conversation-navigator-tools">
      <input type="search" value={query} onChange={event => setQuery(event.target.value)} aria-label="搜索对话" placeholder="搜索对话" />
      <button ref={archiveToggle} type="button" aria-pressed={archived} onClick={onToggleArchived} title={archived ? "查看最近对话" : "查看已归档对话"}>{archived ? "最近" : "已归档"}</button>
    </div>
    {archived && <p className="conversation-archive-heading">已归档对话 · 恢复后可继续</p>}
    {loading && <p className="conversation-navigator-notice" role="status">正在读取对话…</p>}
    {error && <div className="conversation-navigator-error" role="alert"><p>{describeAgentError(error) !== error ? describeAgentError(error) : error.length > 80 ? "对话操作未完成，请重试。" : error}</p>{(error.length > 80 || describeAgentError(error) !== error) && <details><summary>查看错误详情</summary><p>{error}</p></details>}<button type="button" disabled={loading} onClick={onRetry}>重试</button></div>}
    <div className="conversation-navigator-list">
      {filtered.map(thread => {
        const title = thread.name?.trim() || "新对话";
        const current = thread.id === selectedId;
        const running = runningIds.includes(thread.id);
        const waiting = running && thread.status === "waiting";
        const expanded = actionsId === thread.id;
        const editor = editing?.id === thread.id ? editing : null;
        return <article className={`conversation-navigator-row${current ? " is-current" : ""}${running ? " is-running" : ""}`} key={thread.id} data-conversation-row={thread.id}>
          <div className="conversation-navigator-row-main">
            <button type="button" className="conversation-select-button" aria-current={current ? "true" : undefined} aria-label={`切换对话：${title}`} title={title} onClick={() => { setActionsId(null); onSelect(thread); }}>
              <span className="conversation-navigator-marker" aria-hidden="true">{running ? "◌" : thread.isPinned ? "★" : "○"}</span>
              <span className="conversation-navigator-title">{title}</span>
              {(running || current) && <small>{waiting ? "待回答" : running ? "执行中" : "当前"}</small>}
            </button>
            <button type="button" className="conversation-more-button" ref={node => { if (node) actionButtons.current.set(thread.id, node); else actionButtons.current.delete(thread.id); }} aria-label={`管理对话：${title}`} aria-expanded={expanded || Boolean(editor)} title="管理对话" onClick={() => { if (editor) { if (!editor.pending) closeEditor(thread.id); } else setActionsId(expanded ? null : thread.id); }} onKeyDown={event => { if (event.key === "Escape") { setActionsId(null); event.currentTarget.focus(); } }}>⋯</button>
          </div>
          {expanded && <div className="conversation-navigator-row-actions" onKeyDown={event => { if (event.key === "Escape") { event.preventDefault(); setActionsId(null); actionButtons.current.get(thread.id)?.focus(); } }}>
            {onInspect && <button type="button" onClick={() => { actionButtons.current.get(thread.id)?.focus({ preventScroll: true }); onInspect(thread); setActionsId(null); }}>只读查看</button>}
            <button type="button" disabled={busy} onClick={() => { setActionsId(null); setEditing({ id: thread.id, title, error: null, pending: false }); }}>重命名</button>
            <button type="button" disabled={busy} aria-pressed={thread.isPinned} onClick={() => { closeActions(thread.id); onPin(thread); }}>{thread.isPinned ? "取消置顶" : "置顶"}</button>
            <button type="button" disabled={busy || running} title={running ? "执行结束后可归档" : undefined} onClick={() => { closeActions(thread.id); onArchive(thread); }}>{archived ? "恢复" : "归档"}</button>
          </div>}
          {editor && <form className="conversation-rename-form" aria-label={`重命名对话：${title}`} onSubmit={event => { event.preventDefault(); void rename(thread); }} onKeyDown={event => { if (event.key === "Escape" && !editor.pending) { event.preventDefault(); closeEditor(thread.id); } }}>
            <input ref={nameInput} value={editor.title} maxLength={120} disabled={editor.pending} aria-label="对话名称" aria-invalid={Boolean(editor.error)} onChange={event => setEditing({ ...editor, title: event.target.value, error: null })} />
            {editor.error && <p role="alert">{editor.error}</p>}
            <div><button type="button" disabled={editor.pending} onClick={() => closeEditor(thread.id)}>取消</button><button type="submit" disabled={busy || editor.pending}>{editor.pending ? "保存中…" : "保存"}</button></div>
          </form>}
        </article>;
      })}
      {!loading && filtered.length === 0 && <p className="conversation-navigator-empty">{query.trim() ? "没有匹配的对话，请换个关键词。" : archived ? "暂无已归档对话。" : "从新对话开始，消息与引用会独立保存。"}</p>}
    </div>
  </section>;
}
