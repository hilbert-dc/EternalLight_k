import { ConversationReplyActions, ConversationReplyBoundary } from "./ConversationReplyActions";
import { useEffect, useMemo, useState, type ReactNode } from "react";
import type { NovelCodexApi, TaskFinalReply, WorkTaskSummary } from "./types";
import { MarkdownContent } from "./MarkdownContent";

export function GeneratedReplyDescription({ projectId, taskId, text, children }: {
  projectId: string; taskId: string; text: string; children: ReactNode;
}) {
  return <details className="generated-reply-description" data-conversation-detail={`task:${taskId}:description`}>
    <summary>查看结果说明</summary>
    <ConversationReplyBoundary identity={`${projectId}:${taskId}`} text={text} label="复制结果说明">{children}</ConversationReplyBoundary>
  </details>;
}

export function SavedTaskReply({ api, projectId, attempt, foldDescription = false }: {
  api: NovelCodexApi | undefined; projectId: string; attempt: WorkTaskSummary["steps"][number]["attempts"][number];
  foldDescription?: boolean;
}) {
  const scope = useMemo(() => ({ api, projectId, taskId: attempt.id, threadId: attempt.threadId, turnId: attempt.turnId }),
    [api, projectId, attempt.id, attempt.threadId, attempt.turnId]);
  const [retry, setRetry] = useState(0);
  const [state, setState] = useState<{ scope: typeof scope; receipt: TaskFinalReply | null; error: string | null } | null>(null);
  useEffect(() => {
    let active = true;
    setState(null);
    void (async () => {
      try {
        if (!api) throw new Error("最终回复读取接口不可用");
        const receipt = await api.runtime.getTaskFinalReply(projectId, scope.taskId);
        if (receipt && (receipt.taskId !== scope.taskId || receipt.projectId !== projectId
          || receipt.binding.threadId !== scope.threadId || (scope.turnId && receipt.binding.turnId !== scope.turnId))) {
          throw new Error("回复归属不匹配，请重新读取");
        }
        if (active) setState({ scope, receipt, error: null });
      } catch (reason) {
        if (active) setState({ scope, receipt: null, error: reason instanceof Error ? reason.message : "读取最终回复失败" });
      }
    })();
    return () => { active = false; };
  }, [scope, retry, api, projectId]);
  const visible = state?.scope === scope ? state : null;
  return <article className="conversation-message assistant-message saved-task-reply">
    <span className="conversation-avatar assistant" aria-hidden="true">✦</span>
    <div className="conversation-message-body">
      <div className="conversation-message-meta"><strong>Novel Engine</strong><span>回复</span></div>
      {!visible ? <p role="status">正在读取保存的回复…</p> : visible.error ? <div role="alert"><p>{visible.error}</p>
        <button type="button" onClick={() => setRetry(value => value + 1)}>重新读取回复</button></div>
        : !visible.receipt ? <p>没有保存的最终回复，执行摘要不能代替模型答复。</p>
          : visible.receipt.state === "absent" ? <p>本轮执行已结束，没有最终文本回复。</p> : <>
            {(visible.receipt.state === "truncated" || visible.receipt.binding.truncated) && <p role="status">保存的回复超过上限，以下为部分内容。</p>}
            {foldDescription && visible.receipt.state === "recorded" && !visible.receipt.binding.truncated && visible.receipt.binding.text.trim()
              ? <GeneratedReplyDescription projectId={projectId} taskId={attempt.id} text={visible.receipt.binding.text}>
                <div className="conversation-answer"><MarkdownContent text={visible.receipt.binding.text} /></div>
              </GeneratedReplyDescription>
              : <><div className="conversation-answer"><MarkdownContent text={visible.receipt.binding.text} /></div>
                <ConversationReplyActions identity={`${projectId}:${attempt.id}`} text={visible.receipt.binding.text}
                  label={visible.receipt.state === "truncated" || visible.receipt.binding.truncated ? "复制已显示内容" : "复制回复"} /></>}
          </>}
    </div>
  </article>;
}
