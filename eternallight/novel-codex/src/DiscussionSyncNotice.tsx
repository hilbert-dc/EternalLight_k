export function DiscussionSyncNotice({ message, busy, blocked, onRefresh }: {
  message: string; busy: boolean; blocked: boolean; onRefresh: () => void;
}) {
  return <section className="discussion-sync-notice" aria-label="对话后工作区同步">
    <p role="status">{message}</p>
    <button type="button" className="secondary-button" disabled={busy || blocked} onClick={onRefresh}>
      {busy ? "正在读取…" : "重新读取工作区"}
    </button>
    {blocked && <small>请先保存当前修改，并等待正在执行的任务结束。</small>}
  </section>;
}
