export type ThreadRestoreState = { projectId: string; threadId: string; status: "loading" | "empty" | "ready" | "error"; error?: string };

export function ThreadRestoreNotice({ state, onRetry }: { state: ThreadRestoreState; onRetry: () => void }) {
  if (state.status === "ready") return null;
  return <section className="discussion-sync-notice" aria-label="恢复对话记录">
    <p role={state.status === "error" ? "alert" : "status"}>{state.status === "loading" ? "正在读取此对话的最近记录…" : state.status === "empty" ? "此对话暂无文本记录，可以开始新的讨论。" : `对话已连接，但记录读取失败：${state.error}。重新读取不会调用模型。`}</p>
    {state.status === "error" && <button type="button" className="secondary-button" onClick={onRetry}>重新读取对话</button>}
  </section>;
}
