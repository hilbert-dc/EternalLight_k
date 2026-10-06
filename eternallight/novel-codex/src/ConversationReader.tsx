import { MarkdownContent } from "./MarkdownContent";
import type { ThreadDetail } from "./types";

export function ConversationReader({ detail, loading, error, onRetry, onFork, busy, showEmpty = true }: {
  detail: ThreadDetail | null; loading: boolean; error: string | null; onRetry: () => void;
  onFork: (turnId: string) => void; busy: boolean; showEmpty?: boolean;
}) {
  return <section className="conversation-reader" aria-label="当前对话消息">
    {loading && <p role="status">正在读取所选对话…</p>}
    {error && <p role="alert">{error}<button type="button" onClick={onRetry}>重新读取对话</button></p>}
    {detail?.turns.map(turn => <article className="conversation-reader-turn" key={turn.id}>
      {turn.items.filter(item => item.type === "userMessage" || item.type === "agentMessage").map(item => <div key={item.id} className={`conversation-message ${item.type === "userMessage" ? "user-message" : "assistant-message"}`}>
        <div className="conversation-message-body">{item.text && <MarkdownContent text={item.text} />}</div>
      </div>)}
      <button type="button" className="text-button" disabled={busy || turn.status !== "completed"} onClick={() => onFork(turn.id)}>从此回合新建分支对话</button>
    </article>)}
    {showEmpty && !loading && !error && !detail?.turns.length && <div className="conversation-empty"><strong>开始一个独立的讨论</strong><p>可选中作品内容加入对话。共享作品资料，不自动继承其他对话的聊天历史。</p></div>}
  </section>;
}
