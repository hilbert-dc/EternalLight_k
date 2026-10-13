import { useState } from "react";
import type { ConversationMessage } from "./types";

export function ConversationMessageQueue({ items, busy, edit, move, remove, reconcile, continueWaiting }: {
  items: ConversationMessage[]; busy: boolean;
  edit: (id: string, text: string, expectedRevision: number) => Promise<unknown>;
  move: (id: string, direction: number) => Promise<unknown>;
  remove: (id: string) => Promise<unknown>;
  reconcile: (id: string) => Promise<unknown>;
  continueWaiting: (id: string) => Promise<unknown>;
}) {
  const [editing, setEditing] = useState<{id: string; revision: number; text: string} | null>(null);
  if (!items.length) return null;
  return <details className="conversation-outbox" aria-label="待发送指令"><summary>待发送 · {items.length}</summary>
    <small>消息已保存。未确认的交付可核对；待发送消息按原条件执行。</small>
    {items.map((item, index) => {
      const editable = item.status === "queued" && !busy;
      const current = editing?.id === item.id ? editing : null;
      const condition = item.trigger.kind === "steer" ? "本轮追加指令" : item.trigger.kind === "interlude" ? "原任务内的临时问题" : item.trigger.kind === "afterTurn" ? "本轮结束后发送" : item.trigger.kind === "afterWork" ? "当前任务结束后发送" : "立即发送";
      const status = item.trigger.kind === "steer" && item.status === "failed" ? item.error ?? "追加要求未发送，请继续原任务" : item.error ? `待核对：${item.error}` : item.status === "sending" ? "已派发，等待交付核对"
        : item.status === "claimed" ? "正在启动" : item.requiresContinue ? "等待确认后发送"
        : item.holdReason === "authorControlChanged" ? "原任务控制已变化" : item.holdReason === "paused" ? "原任务已暂停"
        : item.holdReason === "blocked" ? "原任务受阻" : item.holdReason === "cancelled" ? "原任务已取消" : "等待原条件正常满足";
      return <article key={item.id}>
        {current ? <form onSubmit={async event => { event.preventDefault(); if (await edit(item.id, current.text, current.revision) !== false) setEditing(null); }}>
          <textarea aria-label="编辑待发送消息" value={current.text} onChange={event => setEditing({...current,text:event.target.value})} />
          {current.revision !== item.revision && <small role="alert">消息已变化，编辑草稿已保留；请核对后取消编辑并重新打开。</small>}
          <button disabled={!editable || current.revision !== item.revision || (!current.text.trim() && !item.input.media?.imageAssetRevisionIds.length)} type="submit">保存修改</button>
          <button type="button" onClick={() => setEditing(null)}>取消编辑</button>
        </form> : <p>{item.input.text}</p>}
        <small>{condition}{item.input.media ? ` · 媒体消息 · ${item.input.media.imageAssetRevisionIds.length} 张图片` : ""}{item.input.focus?.excerpts?.length ? ` · 引用 ${item.input.focus.excerpts.length} 处内容` : ""}</small>
        <small>{status}</small>
        <button type="button" onClick={() => void reconcile(item.id)}>核对交付</button>
        {item.trigger.kind !== "steer" && <><button type="button" disabled={!editable} onClick={() => void continueWaiting(item.id)}>继续等待</button>
        <button type="button" disabled={!editable} onClick={() => setEditing({id:item.id,revision:item.revision,text:item.input.text})}>编辑</button>
        <button type="button" disabled={!editable || !index} onClick={() => void move(item.id,-1)}>上移</button>
        <button type="button" disabled={!editable || index === items.length - 1} onClick={() => void move(item.id,1)}>下移</button>
        <button type="button" disabled={!editable} onClick={() => void remove(item.id)}>移除</button></>}
      </article>;
    })}
  </details>;
}
