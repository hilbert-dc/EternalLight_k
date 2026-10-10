import { useEffect, useId, useRef, useState } from "react";
import { ConversationReader } from "./ConversationReader";
import type { NovelCodexApi, ThreadDetail } from "./types";

export function ConversationComparison({ api, projectId, threadId, sourceId, onClose }: {
  api: NovelCodexApi; projectId: string; threadId: string; sourceId: string; onClose: () => void;
}) {
  const dialog = useRef<HTMLDialogElement>(null), close = useRef<HTMLButtonElement>(null);
  const titleId = useId();
  const [attempt, setAttempt] = useState(0);
  const [state, setState] = useState<{ pair: ThreadDetail[] | null; error: string | null }>({ pair: null, error: null });
  useEffect(() => {
    const previous = document.activeElement as HTMLElement | null, element = dialog.current;
    element?.showModal(); close.current?.focus();
    return () => { element?.close(); if (previous?.isConnected) previous.focus({ preventScroll: true }); };
  }, []);
  useEffect(() => {
    let current = true; setState({ pair: null, error: null });
    Promise.all([api.agent.readConversation(projectId, sourceId), api.agent.readConversation(projectId, threadId)]).then(pair => {
      if (pair[0].thread.id !== sourceId || pair[1].thread.id !== threadId || pair.some(item => item.thread.projectId !== projectId)
        || pair[1].thread.forkedFromId !== sourceId) throw Error("分支来源不匹配，请重新打开对照。");
      if (current) setState({ pair, error: null });
    }).catch(() => { if (current) setState({ pair: null, error: "对照暂时无法读取，请重试。" }); });
    return () => { current = false; };
  }, [api, projectId, threadId, sourceId, attempt]);
  return <dialog ref={dialog} className="conversation-comparison" aria-labelledby={titleId}
    onCancel={event => { event.preventDefault(); onClose(); }} onClose={onClose}>
    <header><h2 id={titleId}>对照两种方案</h2><button type="button" ref={close} onClick={onClose}>关闭对照</button></header>
    <p>查看两份对话当前保存的记录，选择适合的方案后可回到相应对话继续。</p>
    {state.error ? <p role="alert">{state.error}<button type="button" onClick={() => setAttempt(value => value + 1)}>重新读取</button></p>
      : !state.pair ? <p role="status">正在读取两份对话…</p> : <div className="conversation-comparison-columns">{state.pair.map((detail, index) => <section key={detail.thread.id}>
        <h3>{index ? "当前分支" : "原对话"} · {detail.thread.name || "未命名对话"}</h3>
        <ConversationReader detail={detail} loading={false} error={null} onRetry={() => {}} onFork={() => {}} busy showActions={false} />
      </section>)}</div>}
  </dialog>;
}
