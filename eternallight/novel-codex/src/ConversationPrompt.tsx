import { useLayoutEffect, useRef, useState } from "react";

/** Display folding only; the original author text remains available in full. */
export function ConversationPrompt({ text, detailKey, proposal = false }: {
  text: string; detailKey: string; proposal?: boolean;
}) {
  const detail = useRef<HTMLDetailsElement>(null);
  const preview = useRef<HTMLSpanElement>(null);
  const [overflow, setOverflow] = useState(false);
  const [open, setOpen] = useState(false);
  useLayoutEffect(() => {
    const node = preview.current;
    if (!node) return;
    const measure = () => {
      if (!detail.current?.open) setOverflow(node.scrollHeight > node.clientHeight + 1);
    };
    measure();
    const observer = new ResizeObserver(measure); observer.observe(node);
    return () => observer.disconnect();
  }, [text]);
  return <details ref={detail} className="conversation-request" data-overflow={overflow}
    data-conversation-detail={detailKey} onToggle={event => setOpen(event.currentTarget.open)}>
    <summary tabIndex={overflow || open ? 0 : -1} onClick={event => { if (!overflow && !detail.current?.open) event.preventDefault(); }}>
      <span ref={preview} className="conversation-request-preview">{text}</span>
      {(overflow || open) && <span className="conversation-request-toggle"><span className="when-closed">{proposal ? "展开完整建议" : "展开完整指令"}</span><span className="when-open">收起全文</span></span>}
    </summary>
    <p>{text}</p>
  </details>;
}
