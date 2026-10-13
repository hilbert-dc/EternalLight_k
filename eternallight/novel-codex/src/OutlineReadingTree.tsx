import type { OutlineNode } from "./types";
import "./outline-reading.css";
import { outlineLayerCopy } from "./outline-layer-copy";

type Props = {
  nodes: OutlineNode[];
  focusNodeId?: string | null;
  onOpenNode?: (id: string) => void;
  revisionId: string;
  blocked: boolean;
  dirty: boolean;
  onDiscuss?: (prompt: string) => void;
};

const kinds = { book: "全书", volume: "卷", unit: "单元" };
const protections = { free: "", "preserve-intent": "调整时保留这一段的故事目标", "preserve-arrangement": "调整时保留这一段的层级与顺序" };
const identity = (node: OutlineNode) => node.id ?? node.clientKey;

export function OutlineReadingTree({ nodes, focusNodeId = null, onOpenNode, revisionId, blocked, dirty, onDiscuss }: Props) {
  const ordered = [...nodes].sort((left, right) => left.position - right.position);
  const byId = new Map<string, OutlineNode[]>();
  for (const node of ordered) {
    const id = identity(node);
    if (id) byId.set(id, [...(byId.get(id) ?? []), node]);
  }
  const uniqueIdentity = (node: OutlineNode) => Boolean(identity(node)) && byId.get(identity(node)!)?.length === 1;
  const parentOf = (node: OutlineNode) => {
    const parentId = node.parentId ?? node.parentClientKey;
    const matches = parentId ? byId.get(parentId) : undefined;
    return matches?.length === 1 ? matches[0] : null;
  };
  const visited = new Set<OutlineNode>();
  const pathOf = (node: OutlineNode) => {
    const titles = [node.title || "未命名安排"];
    const seen = new Set([node]);
    let parent = parentOf(node);
    while (parent && !seen.has(parent)) {
      seen.add(parent); titles.unshift(parent.title || "未命名安排"); parent = parentOf(parent);
    }
    return titles.join(" / ");
  };
  const body = (node: OutlineNode) => <>
    <dl><div><dt>{outlineLayerCopy[node.kind].purpose}</dt><dd>{node.purpose || "尚未说明"}</dd></div>
      {node.summary ? <div><dt>{outlineLayerCopy[node.kind].summary}</dt><dd>{node.summary}</dd></div> : node.kind === "book" && <div><dt>故事脉络</dt><dd className="planning-reading-hint">尚未梳理完整故事脉络，可在对话中讨论起因、转折与结局；已有卷纲仍保留。</dd></div>}
    </dl>
    {(node.writerGuidance || protections[node.protection]) && <details className="outline-reading-notes"><summary>写作备注</summary>
      {node.writerGuidance && <p>{node.writerGuidance}</p>}
      {protections[node.protection] && <p className="planning-protection-note">{protections[node.protection]}</p>}
    </details>}
    {onDiscuss && <div className="planning-reading-actions"><button type="button" className="text-button" aria-label={`讨论${node.title || "未命名安排"}`} disabled={blocked || dirty || !identity(node) || byId.get(identity(node)!)?.length !== 1} onClick={() => onDiscuss(`请和我讨论这段大纲：${pathOf(node)}。定位线索：已保存修订 ${revisionId}，节点${node.id ? " ID" : " clientKey"} ${identity(node)}。请先核对当前来源；如果节点或版本已经变化，请先说明，不要猜测对应段落。${outlineLayerCopy[node.kind].discussion}可以讨论写作要求、哪些安排要保留。先讨论，不要修改或采用。`)}>讨论这一段</button></div>}
  </>;
  const item = (node: OutlineNode, children: React.ReactNode) => <li key={uniqueIdentity(node) ? `node:${identity(node)}` : `unlinked:${nodes.indexOf(node)}`} data-outline-reading-node={identity(node)} data-discussion-label={node.title}>
    <details className="outline-reading-node" open={node.kind === "book" || identity(node) === focusNodeId}>
      <summary><span className="outline-reading-kind">{kinds[node.kind]}</span><span className="outline-reading-title">{node.title || "未命名安排"}</span>{node.protection !== "free" && <span className="outline-reading-protected">有保留要求</span>}</summary>
      <article className="planning-reading-card">{body(node)}{children}</article>
    </details>
  </li>;
  const branch = (node: OutlineNode): React.ReactNode => {
    if (visited.has(node)) return null;
    visited.add(node);
    const id = identity(node);
    const children = id && byId.get(id)?.length === 1 ? ordered.filter(child => uniqueIdentity(child) && parentOf(child) === node && !visited.has(child)) : [];
    const rendered = children.map(branch);
    if (!focusNodeId && node.kind === "volume" && uniqueIdentity(node) && onOpenNode) return <li key={`node:${id}`} data-outline-reading-node={id} className="outline-volume-overview"><h3>{node.title || "未命名卷"}</h3><p>{node.purpose.length > 180 ? `${node.purpose.slice(0, 180)}…` : node.purpose || "尚未说明本卷目标"}</p><button type="button" className="text-button" disabled={blocked || dirty} onClick={() => onOpenNode(id!)}>查看卷纲</button></li>;
    return item(node, rendered.length > 0 && <ol className="outline-reading-children">{rendered}</ol>);
  };
  const roots = ordered.filter(node => uniqueIdentity(node) && (focusNodeId ? identity(node) === focusNodeId : !(node.parentId ?? node.parentClientKey))).map(branch);
  const unlinked = focusNodeId ? [] : ordered.filter(node => !visited.has(node));
  return <div className="outline-reading-tree">
    {roots.length > 0 && <ol className="planning-reading-list">{roots}</ol>}
    {unlinked.length > 0 && <section className="outline-reading-unlinked" aria-label="未归入层级的安排"><h3>未归入层级的安排</h3><p className="planning-reading-hint">这些安排的上级缺失、身份重复或层级存在循环；内容仍保留，请核对结构。</p><ol className="planning-reading-list">{unlinked.map(node => item(node, null))}</ol></section>}
  </div>;
}
