import { ReadFailureNotice } from "./ReadFailureNotice";
import { useLayoutEffect, useRef } from "react";
import { MarkdownContent } from "./MarkdownContent";
import { ConversationPrompt } from "./ConversationPrompt";
import { ConversationReplyActions } from "./ConversationReplyActions";
import type { ThreadDetail } from "./types";
import { ConversationOutput, isUnfinishedConversationTurn } from "./ConversationOutput";
import "./conversation-media.css";

export function ConversationReader({ detail, loading, error, onRetry, onFork, busy, showEmpty = true, showActions = true, emptyDescription = "聊聊故事构想，或选中正文一起修改。" }: {
  detail: ThreadDetail | null; loading: boolean; error: string | null; onRetry: () => void;
  onFork: (turnId: string) => void; busy: boolean; showEmpty?: boolean; showActions?: boolean; emptyDescription?: string;
}) {
  const panel = useRef<HTMLElement>(null);
  useLayoutEffect(() => {
    const scroller = panel.current?.closest<HTMLElement>(".conversation-history-scroll");
    if (!scroller || !detail || loading) return;
    const key = `novel-codex:history-position:${JSON.stringify([detail.thread.projectId, detail.thread.id])}`;
    let top = 0;
    try { const saved = Number(localStorage.getItem(key)); if (Number.isFinite(saved) && saved >= 0) top = saved; } catch { /* Reading still works without local persistence. */ }
    scroller.scrollTop = top;
    const save = () => { try { localStorage.setItem(key, String(scroller.scrollTop)); } catch { /* No content is lost. */ } };
    scroller.addEventListener("scroll", save);
    return () => { scroller.removeEventListener("scroll", save); };
  }, [detail?.thread.id, detail?.thread.projectId, loading]);
  return <section ref={panel} className="conversation-reader" aria-label="当前对话消息">
    {loading && <p role="status">正在读取对话…</p>}
    {error && <ReadFailureNotice message="对话记录暂时无法读取。" error={error} retryLabel="重新读取" busy={loading} onRetry={onRetry} />}
    {detail?.turns.map(turn => {
      const messages = turn.items.filter(item => item.type === "agentMessage");
      const finals = messages.filter(item => item.phase === "final_answer");
      const answer = (finals.length ? finals : messages.filter(item => item.phase === null)).at(-1)?.text ?? "";
      const process = messages.filter(item => item.phase === "commentary" && item.text);
      return <article className="conversation-reader-turn" key={turn.id}>
        {turn.items.filter(item => item.type === "userMessage" && (item.text || item.mediaAttachments?.length)).map(item =>
          <div key={item.id} className="conversation-message user-message"><div className="conversation-message-body">
            {item.text && <ConversationPrompt text={item.text} detailKey={`reader:${detail.thread.id}:${turn.id}:${item.id}`} />}
            {item.mediaAttachments?.length ? <div className="conversation-media-attachments" aria-label="本条消息图片">{item.mediaAttachments.map(asset => <img key={asset.assetRevisionId} src={asset.previewUrl} alt={asset.name} loading="lazy" />)}</div> : null}
          </div></div>)}
        {process.length > 0 && <div className="conversation-reader-process" aria-label="创作进展">
          {process.map(item => <MarkdownContent key={item.id} text={item.text!} />)}
        </div>}
        <div className="conversation-message assistant-message"><div className="conversation-message-body">
          {answer.trim() ? <ConversationOutput state={turn.status} identity={`reader:${detail.thread.id}:${turn.id}:answer`} text={answer}>
            <div className="conversation-answer"><MarkdownContent text={answer} /></div>
            {showActions && <ConversationReplyActions identity={`${detail.thread.id}:${turn.id}`} text={answer}
              label={isUnfinishedConversationTurn(turn.status) ? "复制未完成内容" : "复制回复"} />}
          </ConversationOutput>
            : <p className="conversation-process-meta">{turn.status === "failed" ? "本轮未完成，暂无最终回复。" : turn.status === "interrupted" ? "本轮已停止，暂无最终回复。" : "本轮暂无最终回复。"}</p>}
          {showActions && <div className="conversation-reader-actions">
            <button type="button" className="text-button" disabled={busy || turn.status !== "completed"}
              onClick={() => onFork(turn.id)}>从此回合新建分支对话</button>
          </div>}
        </div></div>
      </article>;
    })}
    {showEmpty && !loading && !error && !detail?.turns.length && <div className="conversation-empty"><strong>开始创作</strong><p>{emptyDescription}</p></div>}
  </section>;
}
