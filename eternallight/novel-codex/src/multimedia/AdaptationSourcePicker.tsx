import { useEffect, useRef, useState } from "react";
import { mediaSourceIdentity } from "./source-identity";
import type { CreativeEdition, MediaSourceCatalogPage, MediaSourceRef, MultimediaApi } from "../../shared/multimedia-types";

export interface SelectedAdaptationSource { label: string; source: MediaSourceRef }
const sourceKey = mediaSourceIdentity;
const kindLabel: Record<string, string> = { draft: "章节", brief: "创作简报", outline: "全书大纲", plan: "近章规划", comic: "漫画", animation: "动画" };

export function AdaptationSourcePicker({ api, projectId, selected, disabled, onChange }: {
  api: MultimediaApi; projectId: string; selected: SelectedAdaptationSource[]; disabled: boolean;
  onChange: (value: SelectedAdaptationSource[]) => void;
}) {
  const [editions, setEditions] = useState<CreativeEdition[]>([]), [editionId, setEditionId] = useState("");
  const [page, setPage] = useState<MediaSourceCatalogPage | null>(null), [items, setItems] = useState<MediaSourceCatalogPage["items"]>([]);
  const [error, setError] = useState(""), [loading, setLoading] = useState(false), [refresh, setRefresh] = useState(0);
  const epoch = useRef(0), pending = useRef(false);
  useEffect(() => {
    let live = true;
    setLoading(true); setError("");
    void api.listEditions(projectId).then(value => {
      if (!live) return;
      if (value.editions.some(e => e.projectId !== projectId)) throw Error("来源版本不属于当前作品。");
      setEditions(value.editions); setEditionId(id => value.editions.some(e => e.id === id) ? id : value.editions[0]?.id ?? "");
    }).catch(reason => { if (live) setError(String(reason instanceof Error ? reason.message : reason)); })
      .finally(() => { if (live) setLoading(false); });
    return () => { live = false; };
  }, [api, projectId, refresh]);
  useEffect(() => {
    const ticket = ++epoch.current;
    setPage(null); setItems([]); pending.current = false;
    if (!editionId) return;
    setLoading(true); setError("");
    void api.sourceCatalog(projectId, editionId).then(value => {
      if (ticket !== epoch.current) return;
      verify(value, 0); setPage(value); setItems(value.items);
    }).catch(reason => { if (ticket === epoch.current) setError(reason instanceof Error ? reason.message : "来源读取失败。"); })
      .finally(() => { if (ticket === epoch.current) setLoading(false); });
    return () => { epoch.current++; };
  }, [api, projectId, editionId, refresh]);
  function verify(value: MediaSourceCatalogPage, offset: number, snapshotId?: string) {
    const edition = editions.find(e => e.id === editionId);
    if (!edition || value.projectId !== projectId || value.editionId !== editionId || value.branchId !== edition.branchId || !value.snapshotId || value.offset !== offset || snapshotId && value.snapshotId !== snapshotId
      || value.items.some(item => item.source.projectId !== projectId || item.source.scopeId !== edition.scopeId || item.source.branchId !== edition.branchId || item.source.selector.kind !== "whole")) throw Error("来源目录版本不一致，请重新读取。");
  }
  async function more() {
    if (pending.current || !page || page.nextOffset === null) return;
    const ticket = epoch.current; pending.current = true; setLoading(true); setError("");
    try {
      const next = await api.sourceCatalog(projectId, editionId, page.snapshotId, page.nextOffset);
      if (ticket !== epoch.current) return;
      verify(next, page.nextOffset, page.snapshotId); setPage(next); setItems(old => [...old, ...next.items]);
    } catch (reason) { if (ticket === epoch.current) setError(reason instanceof Error ? reason.message : "来源读取失败。"); }
    finally { if (ticket === epoch.current) { pending.current = false; setLoading(false); } }
  }
  return <section className="adaptation-source-picker" aria-label="组合改编来源">
    <label className="dialog-field"><span>添加其他已保存来源</span><select aria-label="来源作品版本" disabled={disabled || !editions.length || loading} value={editionId} onChange={event => setEditionId(event.target.value)}>{editions.map(e => <option key={e.id} value={e.id}>{e.title} · {kindLabel[e.medium] ?? "小说"}</option>)}</select></label>
    <p>可组合小说剧情与漫画视觉等多个来源。选中后固定这份保存版本，最多 100 处。</p>
    {error && <p role="alert">{error}<button type="button" disabled={disabled || loading} onClick={() => setRefresh(v => v + 1)}>重新读取来源</button></p>}
    <ul>{items.map(item => {
      const key = sourceKey(item.source), checked = selected.some(s => sourceKey(s.source) === key);
      const label = `${editions.find(e => e.id === editionId)?.title} · ${kindLabel[item.kind] ?? item.kind}${item.title ? ` · ${item.title}` : ""}`;
      return <li key={key}><label><input type="checkbox" checked={checked} disabled={disabled || !checked && selected.length >= 100} onChange={event => onChange(event.target.checked ? [...selected, { label, source: item.source }] : selected.filter(s => sourceKey(s.source) !== key))} />{label}<small>修订 {item.source.revisionId.slice(-8)}</small></label></li>;
    })}</ul>
    {page && !items.length && <p>这个版本还没有可引用的已保存内容。</p>}
    {page?.nextOffset != null && <button type="button" disabled={disabled || loading} onClick={() => void more()}>更多已保存内容</button>}
    {loading && <p role="status">正在读取来源…</p>}
    {selected.length > 0 && <div aria-label="已选改编来源"><strong>已选 {selected.length} 处</strong><ul>{selected.map(item => <li key={sourceKey(item.source)}>{item.label} · 修订 {item.source.revisionId.slice(-8)}<button type="button" disabled={disabled} aria-label={`移除 ${item.label}`} onClick={() => onChange(selected.filter(s => sourceKey(s.source) !== sourceKey(item.source)))}>移除</button></li>)}</ul></div>}
  </section>;
}
