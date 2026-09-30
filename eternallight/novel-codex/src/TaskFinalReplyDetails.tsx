import { useEffect, useMemo, useState } from "react";
import type { NovelCodexApi, TaskFinalReply } from "./types";
import { MarkdownContent } from "./MarkdownContent";

export function TaskFinalReplyDetails({ api, projectId, taskId }: { api: NovelCodexApi | undefined; projectId: string; taskId: string }) {
  const scope = useMemo(() => ({ api, projectId, taskId }), [api, projectId, taskId]);
  const [open, setOpen] = useState(false);
  const [retry, setRetry] = useState(0);
  const [state, setState] = useState<{ scope: typeof scope; receipt: TaskFinalReply | null; error: string | null } | null>(null);
  useEffect(() => {
    if (!open) return;
    let active = true;
    setState(null);
    void (async () => {
      try {
        if (!api) throw new Error("最终回复读取接口不可用");
        const receipt = await api.runtime.getTaskFinalReply(projectId, taskId);
        if (receipt && (receipt.taskId !== taskId || receipt.projectId !== projectId)) throw new Error("回复归属不匹配");
        if (active) setState({ scope, receipt, error: null });
      } catch (error) {
        if (active) setState({ scope, receipt: null, error: error instanceof Error ? error.message : "读取最终回复失败" });
      }
    })();
    return () => { active = false; };
  }, [open, retry, scope, api, projectId, taskId]);
  const visible = state?.scope === scope ? state : null;
  return <details onToggle={(event) => setOpen(event.currentTarget.open)}>
    <summary>查看已保存的最终回复</summary>
    {open && (!visible ? <p role="status">读取最终回复中…</p> : visible.error ? <div role="alert">
      <p>{visible.error}</p><button type="button" onClick={() => setRetry((value) => value + 1)}>重新读取回复</button>
    </div> : !visible.receipt ? <p>没有最终回复凭据；任务摘要不能代替最终回复。</p> : <>
      <p>作者要求代次 {visible.receipt.binding.intentRevision} · 回复记录不代表创作目标已达成。</p>
      {visible.receipt.state === "absent" ? <p>引擎回合结束，但没有最终文本回复。</p> : <>
        {visible.receipt.state === "truncated" && <p>回复超过保存上限，以下为部分内容，不是完整回复。</p>}
        <MarkdownContent text={visible.receipt.binding.text} />
      </>}
    </>)}
  </details>;
}
