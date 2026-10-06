import type { DiscussionSource } from "./types";
import "./discussion-source.css";

export function DiscussionSourceChip({ source, pending, disabled, onRemove, onReplace, onKeep }: {
  source: DiscussionSource | null; pending: DiscussionSource | null; disabled: boolean;
  onRemove: () => void; onReplace: () => void; onKeep: () => void;
}) {
  if (!source && !pending) return null;
  return <section className="discussion-source" aria-label="本轮讨论来源">
    {source && <div><span>讨论来源：{source.label}</span><button type="button" disabled={disabled} onClick={onRemove} aria-label="移除讨论来源">移除</button></div>}
    {pending && <div role="status"><span>改为讨论“{pending.label}”？输入的文字会保留。</span><button type="button" disabled={disabled} onClick={onReplace}>替换讨论来源</button><button type="button" disabled={disabled} onClick={onKeep}>保留当前来源</button></div>}
  </section>;
}
