import { useEffect, useRef, useState } from "react";
import type { OutlineNode } from "./types";
import { outlineLayerCopy } from "./outline-layer-copy";
import "./outline-document-editor.css";

interface Props {
  disabled?: boolean;
  error?: { nodeId: string; message: string } | null;
  focusNodeId?: string | null;
  rootNodeId?: string | null;
  nodes: OutlineNode[];
  onChange: (nodes: OutlineNode[]) => void;
}
function keyOf(node: OutlineNode): string { return node.id ?? node.clientKey ?? ""; }
function parentOf(node: OutlineNode): string | null { return node.parentId ?? node.parentClientKey ?? null; }

export function OutlineTreeEditor({ disabled, error, focusNodeId, rootNodeId, nodes, onChange }: Props) {
  const books = nodes.filter(node => node.kind === "book");
  const matches = rootNodeId ? nodes.filter(node => keyOf(node) === rootNodeId) : books;
  const root = matches.length === 1 ? matches[0] : null;
  const rootKey = root ? keyOf(root) : null;
  const [selectedKey, setSelectedKey] = useState<string | null>(rootKey);
  const [correctingOutside, setCorrectingOutside] = useState(false);
  const titleRef = useRef<HTMLInputElement>(null);
  const structureRef = useRef<HTMLDetailsElement>(null);
  const lastFocus = useRef<{ key: string; error: Props["error"] } | null>(null);
  const rows: { node: OutlineNode; depth: number }[] = [];
  const visited = new Set<OutlineNode>();
  const append = (node: OutlineNode, depth: number) => {
    if (visited.has(node)) return;
    visited.add(node); rows.push({ node, depth });
    nodes.filter(child => parentOf(child) === keyOf(node)).sort((a, b) => a.position - b.position).forEach(child => append(child, depth + 1));
  };
  const depthOf = (node: OutlineNode) => {
    const seen = new Set<string>([keyOf(node)]);
    let parent = parentOf(node), depth = 0;
    while (parent && !seen.has(parent)) {
      seen.add(parent);
      const ancestor = nodes.find(item => keyOf(item) === parent);
      if (!ancestor) break;
      depth++; parent = parentOf(ancestor);
    }
    return depth;
  };
  if (root) append(root, depthOf(root));
  if (!rootNodeId || correctingOutside) nodes.filter(node => !visited.has(node)).sort((a, b) => a.position - b.position).forEach(node => append(node, 0));
  const outsideError = error && !rows.some(row => keyOf(row.node) === error.nodeId);
  const identities = nodes.map(keyOf);
  const invalidIdentity = identities.some((id, index) => !id || identities.indexOf(id) !== index);
  const byKey = new Map(nodes.map(node => [keyOf(node), node]));
  const cyclic = nodes.some(node => {
    const path = new Set<string>();
    let current: OutlineNode | undefined = node;
    while (current) {
      const key = keyOf(current);
      if (path.has(key)) return true;
      path.add(key); current = byKey.get(parentOf(current) ?? "");
    }
    return false;
  });
  const selected = rows.find(row => keyOf(row.node) === selectedKey) ?? rows.find(row => keyOf(row.node) === rootKey);
  useEffect(() => { setSelectedKey(rootKey); setCorrectingOutside(false); lastFocus.current = null; }, [rootKey]);
  const target = error && !outsideError ? error.nodeId : focusNodeId;
  useEffect(() => {
    if (!target) { lastFocus.current = null; return; }
    if (!rows.some(row => keyOf(row.node) === target) || lastFocus.current?.key === target && (!error || lastFocus.current.error === error)) return;
    setSelectedKey(target);
    const frame = requestAnimationFrame(() => {
      if (titleRef.current?.closest<HTMLElement>("[data-outline-node-id]")?.dataset.outlineNodeId !== target) return;
      lastFocus.current = { key: target, error };
      const editor = titleRef.current.closest<HTMLDetailsElement>(".planning-manual-editor");
      if (editor) editor.open = true;
      titleRef.current.scrollIntoView({ block: "center" });
      titleRef.current.focus({ preventScroll: true });
    });
    return () => cancelAnimationFrame(frame);
  }, [target, selectedKey, nodes, error, outsideError]);
  const select = (id: string) => {
    setSelectedKey(id);
    requestAnimationFrame(() => { titleRef.current?.scrollIntoView({ block: "center" }); titleRef.current?.focus({ preventScroll: true }); });
  };
  const update = (key: string, change: Partial<OutlineNode>) => onChange(nodes.map(node => keyOf(node) === key ? { ...node, ...change } : node));
  const add = (kind: "volume" | "unit", parent: OutlineNode) => {
    const clientKey = `outline-${crypto.randomUUID()}`;
    const siblings = nodes.filter(node => parentOf(node) === keyOf(parent));
    onChange([...nodes, { clientKey, parentId: parent.id, parentClientKey: parent.id ? undefined : parent.clientKey, kind, position: siblings.length, title: kind === "volume" ? "新卷" : "新单元", purpose: "", summary: "", dependencyKeys: [], certainty: "open", protection: "free", writerGuidance: "" }]);
    if (structureRef.current) structureRef.current.open = true;
    select(clientKey);
  };
  const move = (node: OutlineNode, delta: number) => {
    const siblings = nodes.filter(item => parentOf(item) === parentOf(node)).sort((a, b) => a.position - b.position);
    const other = siblings[siblings.indexOf(node) + delta];
    if (!other || rootNodeId === keyOf(node)) return;
    onChange(nodes.map(item => keyOf(item) === keyOf(node) ? { ...item, position: other.position } : keyOf(item) === keyOf(other) ? { ...item, position: node.position } : item));
  };
  const remove = (node: OutlineNode) => {
    const id = keyOf(node);
    if (node.kind === "book" || rootNodeId === id || nodes.some(item => parentOf(item) === id)) return;
    const siblings = nodes.filter(item => parentOf(item) === parentOf(node)).sort((a, b) => a.position - b.position);
    const index = siblings.indexOf(node);
    const positions = new Map(siblings.filter(item => keyOf(item) !== id).map((item, position) => [keyOf(item), position]));
    onChange(nodes.filter(item => keyOf(item) !== id).map(item => positions.has(keyOf(item)) ? { ...item, position: positions.get(keyOf(item))! } : item));
    const next = siblings[index - 1] ?? siblings[index + 1] ?? byKey.get(parentOf(node) ?? "") ?? root;
    if (next) select(keyOf(next));
  };
  const node = selected?.node;
  const id = node ? keyOf(node) : "";
  const siblings = node ? nodes.filter(item => parentOf(item) === parentOf(node)).sort((a, b) => a.position - b.position) : [];
  const index = node ? siblings.indexOf(node) : -1;
  const hasChildren = nodes.some(item => parentOf(item) === id);
  const scopeRoot = Boolean(rootNodeId && id === rootNodeId);
  return <fieldset className="outline-tree-editor outline-document-editor" disabled={disabled || invalidIdentity}>
    <legend className="sr-only">当前故事层级</legend>
    {invalidIdentity && <p role="alert">大纲节点身份缺失或重复，请在对话中修正节点身份后再编辑。原内容保留。</p>}
    {cyclic && <p role="alert">大纲层级存在循环，已停止重复展开。原内容保留，请在对话中调整上级关系后再保存。</p>}
    {!root && <p role="alert">{rootNodeId ? "找不到唯一的当前节点" : "大纲需要唯一的全书节点"}，请在对话中修正结构。原内容保留。</p>}
    {outsideError && <p className="field-error" role="alert">{byKey.get(error.nodeId)?.title || "其他安排"}：{error.message} <button type="button" className="text-button" onClick={() => { setCorrectingOutside(true); select(error.nodeId); }}>定位并修正</button></p>}
    {correctingOutside && <p role="status">正在修正其他层级的保存问题，原编辑内容仍保留。<button type="button" className="text-button" onClick={() => { setCorrectingOutside(false); select(rootKey!); }}>回到{root?.kind === "volume" ? "本卷" : "全书"}</button></p>}
    {node && <ol><li className={`outline-node depth-${selected.depth}`} data-outline-node-id={id} data-discussion-label={node.title} key={id}><article>
      <header><span>{node.kind === "book" ? "全书" : node.kind === "volume" ? "卷" : "单元"}</span><input ref={titleRef} aria-label={`${node.title || "未命名节点"}标题`} aria-invalid={error?.nodeId === id || undefined} aria-describedby={error?.nodeId === id ? `outline-error-${id}` : undefined} value={node.title} onChange={event => update(id, { title: event.target.value })} /></header>
      {error?.nodeId === id && <p className="field-error" id={`outline-error-${id}`} role="alert">{error.message}</p>}
      <label><span>{outlineLayerCopy[node.kind].purpose}</span><textarea value={node.purpose} onChange={event => update(id, { purpose: event.target.value })} placeholder={outlineLayerCopy[node.kind].purposeHint} /></label>
      <label className="outline-story-field"><span>{outlineLayerCopy[node.kind].summary}</span><textarea value={node.summary} onChange={event => update(id, { summary: event.target.value })} placeholder={outlineLayerCopy[node.kind].summaryHint} /></label>
      <details className="outline-node-extra"><summary>备注、保护与结构操作</summary>
        <label><span>写作备注</span><textarea value={node.writerGuidance ?? ""} onChange={event => update(id, { writerGuidance: event.target.value })} placeholder="写正文时需要留意的安排，可留空" /></label>
        <div className="outline-node-controls"><label><span>修改时保留什么</span><select value={node.protection} onChange={event => update(id, { protection: event.target.value as OutlineNode["protection"] })}><option value="free">不特别保护</option><option value="preserve-intent">保留目标与意图</option><option value="preserve-arrangement">保留层级与顺序</option></select></label></div>
        <div className="outline-node-actions">
          {node.kind === "book" && <button type="button" onClick={() => add("volume", node)}>＋ 添加卷</button>}
          {node.kind !== "unit" && <button type="button" onClick={() => add("unit", node)}>＋ 添加单元</button>}
          <button type="button" disabled={index === 0 || scopeRoot} aria-label={`上移${node.title}`} onClick={() => move(node, -1)}>上移一位</button>
          <button type="button" disabled={index === siblings.length - 1 || scopeRoot} aria-label={`下移${node.title}`} onClick={() => move(node, 1)}>下移一位</button>
          {node.kind !== "book" && <button type="button" disabled={hasChildren || scopeRoot} aria-label={`删除${node.title || "未命名节点"}`} onClick={() => remove(node)}>删除这个{node.kind === "volume" ? "卷" : "单元"}</button>}
        </div>
        <p>{scopeRoot ? "调整卷的顺序或删除卷，请返回全书结构。" : hasChildren ? "这段还有下级安排，先处理下级后才能删除。" : "移动只调整同级顺序，删除只移除这一段。"}</p>
      </details>
    </article></li></ol>}
    <details ref={structureRef} className="outline-structure-tools"><summary>{rootNodeId && !correctingOutside ? "本卷结构与单元" : "全书结构与分卷"}</summary>
      <p>选择一段单独编辑。保存时会保留其他层级的安排。</p>
      <ol>{rows.map(({ node: item, depth }, position) => <li key={`${keyOf(item)}:${position}`} data-outline-structure-node={keyOf(item)}><button type="button" className="outline-structure-node" style={{ paddingInlineStart: `${12 + depth * 16}px` }} aria-label={`编辑${item.title || "未命名节点"}`} aria-current={keyOf(item) === id ? "true" : undefined} onClick={() => select(keyOf(item))}><small>{item.kind === "book" ? "全书" : item.kind === "volume" ? "卷" : "单元"}</small><span>{item.title || "未命名节点"}</span></button></li>)}</ol>
    </details>
  </fieldset>;
}
