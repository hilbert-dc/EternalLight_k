import { ConversationReplyActions, ConversationReplyBoundary } from "./ConversationReplyActions";
import { useEffect, useMemo, useState, type ReactNode } from "react";
import type { NovelCodexApi, TaskFinalReply, WorkTaskSummary } from "./types";
import { MarkdownContent } from "./MarkdownContent";
import type { FinalReplyDisplayState } from "./ConversationOutput";

type ReplyAttempt = Pick<WorkTaskSummary["steps"][number]["attempts"][number], "id" | "threadId" | "turnId">;

export function useTaskFinalReply(api: NovelCodexApi | undefined, projectId: string, attempt: ReplyAttempt) {
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
  const finalReplyState: FinalReplyDisplayState = !visible || visible.error ? "unknown" : visible.receipt?.state ?? "absent";
  return { loading: !visible, receipt: visible?.receipt ?? null, error: visible?.error ?? null,
    finalReplyState, retry: () => setRetry(value => value + 1) };
}

type ReplyRead = ReturnType<typeof useTaskFinalReply>;

/** An unfinished local turn needs this extra read. Its original stream remains
 * available separately, while a proven saved reply takes the normal answer slot. */
export function FailedTaskReply({ api, projectId, attempt, render }: {
  api: NovelCodexApi | undefined; projectId: string; attempt: ReplyAttempt;
  render: (state: FinalReplyDisplayState) => ReactNode;
}) {
  const read = useTaskFinalReply(api, projectId, attempt);
  return <>
    {read.loading ? <p className="conversation-process-meta" role="status">正在核对保存的回复…</p>
      : read.error ? <div className="conversation-final-reply-error" role="alert"><p>{read.error}</p>
        <button type="button" className="text-button" onClick={read.retry}>重新核对回复</button></div>
        : read.receipt && read.receipt.state !== "absent"
          ? <SavedTaskReplyView projectId={projectId} taskId={attempt.id} read={read} /> : null}
    {render(read.finalReplyState)}
  </>;
}

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
  const read = useTaskFinalReply(api, projectId, attempt);
  return <SavedTaskReplyView projectId={projectId} taskId={attempt.id} read={read} foldDescription={foldDescription} />;
}

function SavedTaskReplyView({ projectId, taskId, read, foldDescription = false }: {
  projectId: string; taskId: string; read: ReplyRead; foldDescription?: boolean;
}) {
  return <article className="conversation-message assistant-message saved-task-reply">
    <span className="conversation-avatar assistant" aria-hidden="true">✦</span>
    <div className="conversation-message-body">
      <div className="conversation-message-meta"><strong>Novel Engine</strong><span>回复</span></div>
      {read.loading ? <p role="status">正在读取保存的回复…</p> : read.error ? <div role="alert"><p>{read.error}</p>
        <button type="button" onClick={read.retry}>重新读取回复</button></div>
        : !read.receipt ? <p>本轮尚无回复。</p>
          : read.receipt.state === "absent" ? <p>本轮执行已结束，没有最终文本回复。</p> : <>
            {(read.receipt.state === "truncated" || read.receipt.binding.truncated) && <p role="status">保存的回复超过上限，以下为部分内容。</p>}
            {foldDescription && read.receipt.state === "recorded" && !read.receipt.binding.truncated && read.receipt.binding.text.trim()
              ? <GeneratedReplyDescription projectId={projectId} taskId={taskId} text={read.receipt.binding.text}>
                <div className="conversation-answer"><MarkdownContent text={read.receipt.binding.text} /></div>
              </GeneratedReplyDescription>
              : <><div className="conversation-answer"><MarkdownContent text={read.receipt.binding.text} /></div>
                <ConversationReplyActions identity={`${projectId}:${taskId}`} text={read.receipt.binding.text}
                  label={read.receipt.state === "truncated" || read.receipt.binding.truncated ? "复制已显示内容" : "复制回复"} /></>}
          </>}
    </div>
  </article>;
}
