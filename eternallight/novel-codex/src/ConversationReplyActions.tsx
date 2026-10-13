import { createContext, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import "./conversation-reply-actions.css";

const NestedActions = createContext(false);
type Props = { identity: string; text: string; label?: string };

/** One action owner for a folded result, including any nested local reply. */
export function ConversationReplyBoundary({ children, ...props }: Props & { children: ReactNode }) {
  return <><NestedActions.Provider value={true}>{children}</NestedActions.Provider><ConversationReplyActions {...props} /></>;
}

export function ConversationReplyActions({ identity, text, label = "复制回复" }: Props) {
  const suppressed = useContext(NestedActions);
  const owner = useMemo(() => ({}), [identity, text]);
  const latest = useRef(owner); latest.current = owner;
  const request = useRef<{ owner: object } | null>(null);
  const [feedback, setFeedback] = useState<{ owner: object; state: "pending" | "copied" | "failed" } | null>(null);
  useEffect(() => () => { request.current = null; }, [owner]);
  const state = feedback?.owner === owner ? feedback.state : null;
  if (suppressed || !text.trim()) return null;
  const copy = async () => {
    if (request.current?.owner === owner) return;
    const ticket = { owner }; request.current = ticket;
    setFeedback({ owner, state: "pending" });
    try {
      if (!navigator.clipboard) throw new Error("Clipboard unavailable");
      await navigator.clipboard.writeText(text);
      if (latest.current === owner && request.current === ticket) setFeedback({ owner, state: "copied" });
    } catch {
      if (latest.current === owner && request.current === ticket) setFeedback({ owner, state: "failed" });
    } finally {
      if (request.current === ticket) request.current = null;
    }
  };
  return <div className="conversation-reply-actions">
    <button type="button" onClick={() => void copy()} aria-disabled={state === "pending"} aria-label={label} title={label}>
      <svg aria-hidden="true" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6"><rect x="8" y="8" width="12" height="12" rx="2" /><path d="M16 8V5a2 2 0 0 0-2-2H5a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h3" /></svg>
      <span>{label}</span>
    </button>
    {state === "copied" && <span role="status">已复制</span>}
    {state === "failed" && <span role="alert">复制失败，请选中文字复制。</span>}
  </div>;
}
