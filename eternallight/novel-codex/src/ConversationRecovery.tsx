import { useEffect, useId, useRef, useState } from "react";
import { threadHistory } from "./thread-history";
import { MarkdownContent } from "./MarkdownContent";
import type { RecoveryRun, ThreadDetail, WorkTaskSummary } from "./types";
import { recoveryRunName } from "./work-step-presentation";
import "./conversation-recovery.css";

const statuses: Record<string, string> = {
  completed: "执行已结束", failed: "执行失败", cancelled: "执行已取消",
  running: "执行中", paused: "执行已暂停",
};

export function ConversationRecovery({ runs, tasks, threadId, busy, onInspect, onReconcile, onClose }: {
  runs: RecoveryRun[];
  tasks: WorkTaskSummary[];
  threadId: string | null;
  busy: boolean;
  onInspect: (run: RecoveryRun) => Promise<ThreadDetail>;
  onReconcile: (run: RecoveryRun) => Promise<boolean>;
  onClose: () => void;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const closeButton = useRef<HTMLButtonElement>(null);
  const titleId = useId();
  const [pending, setPending] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [preview, setPreview] = useState<{ runId: string; detail: ThreadDetail } | null>(null);
  useEffect(() => {
    const previous = document.activeElement as HTMLElement | null, element = dialog.current;
    element?.showModal(); closeButton.current?.focus();
    return () => { element?.close(); if (previous?.isConnected) previous.focus({ preventScroll: true }); };
  }, []);
  const current = (run: RecoveryRun) => Boolean(threadId) && run.threadId === threadId;
  const ordered = [...runs].sort((a, b) => Number(current(b)) - Number(current(a)));
  return <dialog ref={dialog} className="conversation-recovery" aria-labelledby={titleId} onCancel={event => { event.preventDefault(); onClose(); }}>
    <header><h2 id={titleId}>保存记录</h2><button ref={closeButton} type="button" className="text-button" onClick={onClose}>关闭保存记录</button></header>
    <p>{runs.length ? `${runs.length} 项操作的保存结果尚未确认。重新核对只读取已有记录，不会重复执行。` : "保存结果已核对。"}</p>
    {ordered.map(run => {
      const name = recoveryRunName(run, tasks);
      return <div className="conversation-recovery-row" data-recovery-run={run.runId} key={run.runId}>
      <strong title={name.fullLabel} aria-label={name.fullLabel}>{name.label}</strong>
      <small>{current(run) ? "当前对话" : "其他对话"} · {statuses[run.runtimeStatus ?? ""] ?? "执行状态待核对"} · 保存待核对</small>
      <div className="conversation-recovery-actions">
        <button type="button" className="secondary-button" disabled={busy || pending !== null} onClick={async () => {
          if (busy || pending !== null) return;
          setPending(run.runId); setError(null); setPreview(null);
          try {
            const detail = await onInspect(run);
            if (detail.thread.id !== run.threadId || detail.thread.projectId !== run.projectId) throw new Error("读取结果不属于此作品和对话");
            setPreview({ runId: run.runId, detail });
          } catch (cause) { setError(`对话读取失败：${cause instanceof Error ? cause.message : "请重试"}`); }
          finally { setPending(null); }
        }}>查看对应对话</button>
        <button type="button" className="secondary-button" disabled={busy || pending !== null} onClick={async () => {
          if (busy || pending !== null) return;
          setPending(run.runId); setError(null);
          try {
            if (!await onReconcile(run)) setError("暂时还不能确认保存结果，记录已保留。可以查看对应对话；系统不会重复执行写入。");
          }
          catch (cause) { setError(`保存状态核对失败：${cause instanceof Error ? cause.message : "请重试"}`); }
          finally { setPending(null); }
        }}>{pending === run.runId ? "正在核对…" : "重新核对保存状态"}</button>
      </div>
      {preview?.runId === run.runId && <section className="conversation-recovery-preview" aria-label="待核对任务的对话记录">
        <p>只读对话记录</p>
        <button type="button" className="secondary-button" onClick={() => setPreview(null)}>收起记录</button>
        {threadHistory(preview.detail).length === 0 ? <p>此任务暂无可展示的对话记录。</p> : threadHistory(preview.detail).map(turn => <article key={turn.id}>
          {turn.prompt && <p>{turn.prompt}</p>}
          <MarkdownContent text={turn.answer || turn.summary || "暂无最终回复"} />
          {turn.commentary && <details><summary>过程记录</summary><MarkdownContent text={turn.commentary} /></details>}
        </article>)}
      </section>}
    </div>; })}
    {error && <p role="alert">{error}</p>}
  </dialog>;
}
