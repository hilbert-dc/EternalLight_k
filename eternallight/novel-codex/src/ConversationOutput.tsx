import type { ReactNode } from "react";
import "./conversation-presentation.css";

export type FinalReplyDisplayState = "unknown" | "absent" | "recorded" | "truncated";

export function isUnfinishedConversationTurn(state: string | null) {
  return state === "error" || state === "failed" || state === "cancelled" || state === "interrupted";
}

/** Reply streams only. Public commentary is always shown in the timeline.
 * Length controls reading density; delivery is determined by receipt metadata. */
export function ConversationOutput({ state, identity, text, finalReplyState = "unknown", children }: {
  state: string | null; identity: string; text: string; finalReplyState?: FinalReplyDisplayState;
  children: ReactNode;
}) {
  if (!isUnfinishedConversationTurn(state)) return <>{children}</>;
  const lead = state === "cancelled" || state === "interrupted" ? "本轮已停止" : "本轮未完成";
  const saved = finalReplyState === "recorded" || finalReplyState === "truncated";
  const fold = saved || Array.from(text).length > 600;
  const notice = finalReplyState === "absent" ? `${lead}，以下内容未作为最终回复交付。`
    : saved ? "已保存的回复显示在上方，其余未完成内容可按需查看。"
      : `${lead}，保存状态尚未核实。`;
  return <div className="conversation-unfinished-output">
    <p className="conversation-process-meta" role="status">{notice}</p>
    {fold ? <details key={identity} className="conversation-unfinished-content" data-conversation-detail={`${identity}:unfinished`}>
      <summary>查看未完成的回复内容</summary>
      {children}
    </details> : children}
  </div>;
}
