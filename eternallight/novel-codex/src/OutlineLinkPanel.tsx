import { useRef, useState } from "react";
import type { OutlineNode, WorkCatalog, WorkVolumeSource } from "./types";

type Props = {
  volume: WorkCatalog["volumes"][number]; nodes: OutlineNode[]; revisionId?: string;
  disabled: boolean; onLink?: (volumeId: string, source: WorkVolumeSource) => Promise<void>;
  onDiscuss?: (prompt: string) => void;
};

export function OutlineLinkPanel({ volume, nodes, revisionId, disabled, onLink, onDiscuss }: Props) {
  const [selected, setSelected] = useState("");
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const flight = useRef(false);
  const choices = nodes.filter(node => node.kind === "volume" && (node.id ?? node.clientKey)
    && nodes.filter(other => (other.id ?? other.clientKey) === (node.id ?? node.clientKey)).length === 1);
  const selectedNode = choices.find(node => (node.id ?? node.clientKey) === selected);
  return <section className="outline-link-panel" aria-label="关联卷纲">
    <p>{volume.source ? "原来关联的节点已不在当前大纲中，请选择当前卷纲。" : "本卷尚未关联卷纲，可以先在对话中构思，也可以选择已有卷纲。"}</p>
    {onDiscuss && <button type="button" className="secondary-button" disabled={disabled || pending} onClick={() => onDiscuss(`请为作品目录中的卷《${volume.title}》构思卷纲。目录卷身份 ${volume.id}。请读取当前全书大纲，在同一份大纲中规划本卷，不另建卷简介，不按同名猜关联。先讨论；保存卷纲后提示我选择精确节点关联，不自动采用或改写正文。`)}>讨论本卷构思</button>}
    {choices.length > 0 && onLink && <form onSubmit={event => {
      event.preventDefault();
      if (disabled || flight.current || !selectedNode || !revisionId) return;
      flight.current = true; setPending(true); setError(null);
      void onLink(volume.id, { outlineRevisionId: revisionId, nodeId: selected }).catch(reason => setError(reason instanceof Error ? reason.message : "关联未完成，请核对后重试。")).finally(() => { flight.current = false; setPending(false); });
    }}><label>已有卷纲<select aria-label="选择已有卷纲" value={selected} disabled={disabled || pending} onChange={event => setSelected(event.target.value)}><option value="">选择要关联的卷纲</option>{choices.map((node, index) => <option key={node.id ?? node.clientKey} value={node.id ?? node.clientKey}>{index + 1}. {node.title} · {node.purpose.slice(0, 45)}</option>)}</select></label>
      {selectedNode && <p>{selectedNode.summary || selectedNode.purpose}</p>}
      <button type="submit" className="secondary-button" disabled={disabled || pending || !selectedNode || !revisionId}>{pending ? "正在关联…" : "关联此卷纲"}</button>
      <small>只建立目录关联，不复制内容，也不采用大纲。</small>
    </form>}
    {error && <p role="alert">{error}</p>}
  </section>;
}
