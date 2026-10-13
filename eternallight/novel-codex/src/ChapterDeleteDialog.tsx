import { useEffect, useRef } from "react";
import { createPortal } from "react-dom";
import type { useChapterDeletion } from "./useChapterDeletion";
import "./chapter-deletion.css";

export function ChapterDeleteDialog({ deletion }: { deletion: ReturnType<typeof useChapterDeletion> }) {
  const dialog = useRef<HTMLDialogElement>(null);
  const cancel = useRef<HTMLButtonElement>(null);
  const state = deletion.state;
  useEffect(() => {
    dialog.current?.showModal(); cancel.current?.focus({ preventScroll: true });
    return () => { dialog.current?.close(); };
  }, []);
  useEffect(() => {
    if (state && !state.busy && state.phase !== "check") cancel.current?.focus({ preventScroll: true });
  }, [state?.phase, state?.busy]);
  if (!state) return null;
  const checking = state.phase === "check";
  return createPortal(<dialog ref={dialog} className="chapter-delete-dialog work-directory-dialog" aria-labelledby="chapter-delete-title" aria-describedby="chapter-delete-description" onCancel={event => { event.preventDefault(); deletion.close(); }}>
    <h2 id="chapter-delete-title">删除章节</h2>
    <p className="chapter-delete-target">{state.number ? `第 ${state.number} 章 · ${state.title || "未命名章节"}` : "正在读取本章…"}</p>
    <p id="chapter-delete-description">该章将从作品目录和当前正文中移除。其他章节不会删除或重新编号。</p>
    <p>删除后会自动检查故事衔接，结果显示在对话中。是否调整其他章节，由你决定。</p>
    <small>已确认的故事设定和历史记录会保留。</small>
    {state.error && <p role="alert">{state.error}</p>}
    {checking && <p role="status">{state.busy ? "正在核对删除结果…" : "请先核对结果，避免重复删除。"}</p>}
    <footer>
      <button ref={cancel} type="button" disabled={state.busy || checking} onClick={deletion.close}>取消</button>
      {state.phase === "confirm" ? <button type="button" className="chapter-delete-confirm" disabled={state.busy || deletion.blocked} onClick={() => void deletion.confirm()}>删除本章</button>
        : <button type="button" disabled={state.busy} onClick={() => void deletion.reconcile()}>{checking ? "核对删除结果" : "重新读取章节"}</button>}
    </footer>
  </dialog>, document.body);
}
