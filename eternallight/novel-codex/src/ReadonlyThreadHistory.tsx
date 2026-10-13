import { ConversationReader } from "./ConversationReader";
import type { useThreadHistoryDetail } from "./useThreadHistoryDetail";

export function ReadonlyThreadHistory({ history }: { history: ReturnType<typeof useThreadHistoryDetail> }) {
  return <div className="conversation-history-scroll"><section className="thread-history-panel" aria-label="只读对话记录">
    <header className="thread-history-toolbar"><strong>只读对话记录</strong><button type="button" className="thread-detail-close" aria-label="关闭只读对话记录" onClick={history.close}>返回当前对话</button></header>
    {history.loading && <div className="thread-detail-loading" role="status">正在读取对话记录…<button type="button" onClick={history.close}>取消读取</button></div>}
    {history.error && <div role="alert"><p>对话记录暂时无法读取。</p><button type="button" onClick={history.retry}>重新读取对话记录</button></div>}
    {history.detail && <div className="thread-detail"><p>查看不会恢复原任务，也不会改变当前对话。</p>
      {history.detail.turns.some(turn => turn.status === "inProgress") && <p>进行中 · 记录尚未完成</p>}
      <ConversationReader detail={history.detail} loading={false} error={null} onRetry={history.retry} onFork={() => {}} busy showActions={false} />
    </div>}
  </section></div>;
}
