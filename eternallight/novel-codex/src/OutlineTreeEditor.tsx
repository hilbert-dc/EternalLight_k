import { useEffect, useRef } from "react";

import type { OutlineNode } from "./types";

interface Props {
  disabled?: boolean;
  error?: { nodeId: string; message: string } | null;
  focusNodeId?: string | null;
  nodes: OutlineNode[];
  onChange: (nodes: OutlineNode[]) => void;
}

function keyOf(node: OutlineNode): string { return node.id ?? node.clientKey ?? ""; }

export function OutlineTreeEditor({ disabled, error, focusNodeId, nodes, onChange }: Props) {
  const titleRefs = useRef(new Map<string, HTMLInputElement>());
  useEffect(() => {
    if (!focusNodeId) return;
    const frame = requestAnimationFrame(() => {
      const input = titleRefs.current.get(focusNodeId);
      input?.scrollIntoView({ block: "center" });
      input?.focus({ preventScroll: true });
    });
    return () => cancelAnimationFrame(frame);
  }, [focusNodeId, nodes]);
  const update = (key: string, change: Partial<OutlineNode>) => onChange(nodes.map((node) => keyOf(node) === key ? { ...node, ...change } : node));
  const add = (kind: "volume" | "unit", parent: OutlineNode) => {
    const clientKey = `outline-${crypto.randomUUID()}`;
    const siblings = nodes.filter((node) => (node.parentId ?? node.parentClientKey) === keyOf(parent));
    onChange([...nodes, { clientKey, parentId: parent.id, parentClientKey: parent.id ? undefined : parent.clientKey, kind, position: siblings.length, title: kind === "volume" ? "新卷" : "新单元", purpose: "", summary: "", dependencyKeys: [], certainty: "open", protection: "free", writerGuidance: "" }]);
    requestAnimationFrame(() => titleRefs.current.get(clientKey)?.focus());
  };
  const move = (node: OutlineNode, delta: number) => {
    const parent = node.parentId ?? node.parentClientKey ?? null;
    const siblings = nodes.filter((item) => (item.parentId ?? item.parentClientKey ?? null) === parent).sort((a, b) => a.position - b.position);
    const index = siblings.findIndex((item) => keyOf(item) === keyOf(node));
    const other = siblings[index + delta];
    if (!other) return;
    onChange(nodes.map((item) => keyOf(item) === keyOf(node) ? { ...item, position: other.position } : keyOf(item) === keyOf(other) ? { ...item, position: node.position } : item));
    requestAnimationFrame(() => titleRefs.current.get(keyOf(node))?.focus());
  };
  const remove = (node: OutlineNode) => {
    const id = keyOf(node);
    if (node.kind === "book" || nodes.some((item) => (item.parentId ?? item.parentClientKey) === id)) return;
    const parent = node.parentId ?? node.parentClientKey ?? null;
    const remaining = nodes.filter((item) => keyOf(item) !== id);
    const siblings = nodes.filter((item) => (item.parentId ?? item.parentClientKey ?? null) === parent).sort((a, b) => a.position - b.position);
    const siblingIndex = siblings.findIndex((item) => keyOf(item) === id);
    const focusAfterDelete = keyOf(siblings[siblingIndex - 1] ?? siblings[siblingIndex + 1] ?? nodes.find((item) => keyOf(item) === parent) ?? node);
    let position = 0;
    onChange(remaining.map((item) => (item.parentId ?? item.parentClientKey ?? null) === parent ? { ...item, position: position++ } : item));
    requestAnimationFrame(() => titleRefs.current.get(focusAfterDelete)?.focus());
  };
  const render = (parent: string | null, depth: number) => nodes
    .filter((node) => (node.parentId ?? node.parentClientKey ?? null) === parent)
    .sort((a, b) => a.position - b.position)
    .map((node) => {
      const id = keyOf(node);
      return <li className={`outline-node depth-${depth}`} key={id}>
        <article>
          <header><span>{node.kind === "book" ? "全书" : node.kind === "volume" ? "卷" : "单元"}</span><input ref={(element) => { if (element) titleRefs.current.set(id, element); else titleRefs.current.delete(id); }} aria-label={`${node.title || "未命名节点"}标题`} aria-invalid={error?.nodeId === id || undefined} aria-describedby={error?.nodeId === id ? `outline-error-${id}` : undefined} value={node.title} onChange={(event) => update(id, { title: event.target.value })} /></header>
          {error?.nodeId === id && <p className="field-error" id={`outline-error-${id}`} role="alert">{error.message}</p>}
          <label><span>这一段要完成什么</span><textarea value={node.purpose} onChange={(event) => update(id, { purpose: event.target.value })} placeholder="这一段在故事里做什么" /></label>
          <details className="outline-node-extra">
            <summary>给正文的指引和保护</summary>
            <label><span>给正文的指引</span><textarea value={node.writerGuidance ?? ""} onChange={(event) => update(id, { writerGuidance: event.target.value })} placeholder="写到这里时需要知道的安排" /></label>
            <div className="outline-node-controls">
              <label><span>成熟度</span><select value={node.certainty} onChange={(event) => update(id, { certainty: event.target.value as OutlineNode["certainty"] })}><option value="open">待探索</option><option value="tentative">暂定</option><option value="settled">已明确</option></select></label>
              <label><span>保护</span><select value={node.protection} onChange={(event) => update(id, { protection: event.target.value as OutlineNode["protection"] })}><option value="free">可调整</option><option value="preserve-intent">保留意图</option><option value="preserve-arrangement">保留层级与顺序</option></select></label>
            </div>
          </details>
          <div className="outline-node-actions">
            {node.kind === "book" && <button type="button" onClick={() => add("volume", node)}>＋ 添加卷</button>}
            {node.kind !== "unit" && <button type="button" onClick={() => add("unit", node)}>＋ 添加单元</button>}
            <button type="button" aria-label={`上移${node.title}`} onClick={() => move(node, -1)}>上移</button>
            <button type="button" aria-label={`下移${node.title}`} onClick={() => move(node, 1)}>下移</button>
            {node.kind !== "book" && <button type="button" aria-label={`删除${node.title || "未命名节点"}`} onClick={() => remove(node)}>删除</button>}
          </div>
        </article>
        <ol>{render(id, depth + 1)}</ol>
      </li>;
    });
  return <fieldset className="outline-tree-editor" disabled={disabled}><legend className="sr-only">全书、卷、单元</legend><ol>{render(null, 0)}</ol></fieldset>;
}
