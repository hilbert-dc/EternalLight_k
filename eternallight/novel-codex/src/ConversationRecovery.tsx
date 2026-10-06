import { useState } from "react";
import { threadHistory } from "./thread-history";
import { MarkdownContent } from "./MarkdownContent";
import type { RecoveryRun, ThreadDetail } from "./types";
import "./conversation-recovery.css";

const operations: Record<string, string> = {
  advisory: "创作讨论", generateBrief: "生成简报", generateOutline: "生成大纲",
  generatePlan: "生成近章规划", generateChapter: "生成章节", modelKnowledge: "分析章节设定",
  extractCommitments: "提取伏笔候选", reviseCommitment: "修改伏笔候选",
  generatePropertyRules: "生成规则候选", assessRevisionOutcome: "评估修改结果",
  reviewPlanningSeat: "规划检查", assessSceneOutcome: "检查场景结果", reviewQuality: "独立读者审阅",
};
const statuses: Record<string, string> = {
  completed: "执行已结束", failed: "执行失败", cancelled: "执行已取消",
  running: "执行中", paused: "执行已暂停",
};

export function ConversationRecovery({ runs, threadId, busy, onInspect, onReconcile }: {
  runs: RecoveryRun[];
  threadId: string | null;
  busy: boolean;
  onInspect: (run: RecoveryRun) => Promise<ThreadDetail>;
  onReconcile: (run: RecoveryRun) => Promise<boolean>;
}) {
  const [pending, setPending] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [preview, setPreview] = useState<{ runId: string; detail: ThreadDetail } | null>(null);
  if (!runs.length) return null;
  const current = (run: RecoveryRun) => Boolean(threadId) && run.threadId === threadId;
  const ordered = [...runs].sort((a, b) => Number(current(b)) - Number(current(a)));
  return <details className="conversation-recovery">
    <summary>{runs.length} 项保存待核对</summary>
    <p>系统会根据实际保存记录核对这些任务。重新核对不会再次生成、保存或采纳内容。</p>
    {ordered.map(run => <div className="conversation-recovery-row" data-recovery-run={run.runId} key={run.runId}>
      <strong>{operations[run.operation ?? ""] ?? "创作任务"}</strong>
      <small>{current(run) ? "当前对话" : "其他对话"} · {statuses[run.runtimeStatus ?? ""] ?? "执行状态待核对"} · 保存待核对</small>
      <div className="conversation-recovery-actions">
        <button type="button" className="secondary-button" disabled={busy || pending !== null} onClick={async () => {
          if (busy || pending !== null) return;
          setPending(run.runId); setError(null); setPreview(null);
          try {
            const detail = await onInspect(run);
            if (detail.thread.id !== run.threadId) throw new Error("读取结果不属于此对话");
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
    </div>)}
    {error && <p role="alert">{error}</p>}
  </details>;
}
