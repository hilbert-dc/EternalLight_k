import { useEffect, useRef, useState } from "react";
import type { CanonWorkspace, MediaSourceImpact, MediaSourceImpactPage, MediaSourceRef, MultimediaApi, MultimediaWorkspace } from "../../shared/multimedia-types";
import { CanonEditor } from "./CanonEditor";

const statusLabel: Record<MediaSourceImpact["status"], string> = { current: "引用内容未变", changed: "引用内容已更新", selectionMissing: "原选区已变化", removed: "原内容已移除" };
function label(item: MediaSourceImpact) {
  const s = item.source.selector;
  if (s.kind === "fieldText" || s.kind === "text") {
    const characters = Array.from(s.quote), excerpt = characters.slice(0, 60).join("").replace(/\s+/g, " ").trim();
    const quote = excerpt ? `“${excerpt}${characters.length > 60 ? "…" : ""}”` : "";
    if (s.kind === "text") return quote || "原文选段";
    const field = { balloonText: "对白", panelDescription: "画格说明", shotDescription: "镜头说明", clipCaption: "片段字幕", canonStatement: "共同设定" }[s.field.kind];
    const range = s.startChar < s.endChar ? `第 ${s.startChar + 1}–${s.endChar} 个字符` : s.startChar ? `第 ${s.startChar} 个字符后` : "开头";
    return `${field} · ${range}${quote ? ` · ${quote}` : ""}`;
  }
  return s.kind === "panel" ? "画格来源" : s.kind === "shot" ? "镜头来源" : s.kind === "clip" ? "片段来源" : s.kind === "region" ? "图片选区来源" : s.kind === "time" ? "音视频选段来源" : "作品来源";
}
export function MediaSourcesPanel({ api, authorKey, workspace, busy, source, onAdopt }: {
  api: MultimediaApi; authorKey: string; workspace: MultimediaWorkspace; busy: boolean;
  source: MediaSourceRef | null; onAdopt: (snapshotId: string) => Promise<void>;
}) {
  const [canon, setCanon] = useState<CanonWorkspace | null>(null), [report, setReport] = useState<MediaSourceImpactPage | null>(null);
  const [items, setItems] = useState<MediaSourceImpact[]>([]), [error, setError] = useState<string | null>(null), [loading, setLoading] = useState(false), [editing, setEditing] = useState(false), [refresh, setRefresh] = useState(0);
  const alive = useRef(true), epoch = useRef(0), loadingRef = useRef(false);
  useEffect(() => { alive.current = true; return () => { alive.current = false; epoch.current++; }; }, []);
  useEffect(() => {
    const ticket = ++epoch.current; loadingRef.current = true; setLoading(true); setError(null); setItems([]); setReport(null); setCanon(null);
    void Promise.all([api.readCanon(workspace.project.id, workspace.edition.canonSnapshotId ?? undefined), api.sourceImpacts(workspace.project.id, workspace.edition.id)]).then(([fixed, impacts]) => {
      if (!alive.current || ticket !== epoch.current) return;
      if (fixed.projectId !== workspace.project.id || fixed.snapshotId !== workspace.edition.canonSnapshotId || impacts.projectId !== workspace.project.id || impacts.editionId !== workspace.edition.id || impacts.branchId !== workspace.edition.branchId || impacts.revisionId !== workspace.revisionId) throw Error("来源记录与当前保存版本不一致，请重新打开。");
      setCanon(fixed); setReport(impacts); setItems(impacts.items);
    }).catch(reason => { if (alive.current && ticket === epoch.current) setError(reason instanceof Error ? reason.message : "来源检查未完成。"); }).finally(() => { if (alive.current && ticket === epoch.current) { loadingRef.current = false; setLoading(false); } });
  }, [api, workspace.project.id, workspace.edition.id, workspace.revisionId, workspace.edition.canonSnapshotId, refresh]);
  async function more() {
    if (loadingRef.current || report?.nextOffset === null || !report) return;
    const ticket = epoch.current; loadingRef.current = true; setLoading(true); setError(null);
    try {
      const next = await api.sourceImpacts(workspace.project.id, workspace.edition.id, report.nextOffset);
      if (!alive.current || ticket !== epoch.current) return;
      if (next.projectId !== workspace.project.id || next.branchId !== workspace.edition.branchId || next.revisionId !== report.revisionId || next.editionId !== workspace.edition.id || next.offset !== report.nextOffset) throw Error("来源检查期间内容已变化，请重新检查。");
      setReport(next); setItems(old => [...old, ...next.items]);
    } catch (reason) { if (alive.current && ticket === epoch.current) setError(reason instanceof Error ? reason.message : "来源读取未完成。"); }
    finally { if (alive.current && ticket === epoch.current) { loadingRef.current = false; setLoading(false); } }
  }
  return <section className="mm-sources-panel" aria-label="设定与来源"><header className="mm-inspector-heading"><h3>设定与来源</h3><button type="button" disabled={busy} onClick={() => setEditing(true)}>编辑共同设定</button></header>
    {error && <p className="mm-error" role="alert">{error}<button type="button" disabled={loading} onClick={() => setRefresh(value => value + 1)}>重新检查</button></p>}
    {canon && <details><summary>本作品采用的设定 · {canon.document.entries.length} 条</summary>{canon.document.entries.length ? <ul>{canon.document.entries.map(entry => <li key={entry.id}>{entry.statement}</li>)}</ul> : <p>此作品尚未采用已保存的共同设定。</p>}</details>}
    {report?.canon.changed && <div className="mm-source-change" role="status"><p>共同设定已有新版本。当前作品仍使用原设定。</p><button type="button" disabled={busy || loading} onClick={() => void onAdopt(report.canon.currentSnapshotId).catch(reason => { if (alive.current) setError(reason instanceof Error ? reason.message : "设定未采用，请重试。"); })}>使用最新设定</button><span>会保存新的作品版本。</span></div>}
    {report && <details open={items.some(item => item.status !== "current")}><summary>引用来源 · {report.total} 处</summary><p>检查引用内容的变化，保留原版本和改编内容。</p>{items.length ? <ul className="mm-source-list">{items.map((item, index) => <li key={index}><span>{label(item)}</span><strong>{statusLabel[item.status]}</strong>{item.targets.length > 0 && <small>关联 {item.targets.length} 处作品内容</small>}</li>)}</ul> : <p>没有引用其他作品内容。</p>}{report.nextOffset !== null && <button type="button" disabled={loading} onClick={() => void more()}>查看更多来源</button>}</details>}
    {loading && <p role="status">正在核对保存来源…</p>}
    {editing && <CanonEditor api={api} authorKey={authorKey} projectId={workspace.project.id} busy={busy} source={source} onClose={() => setEditing(false)} onSaved={() => setRefresh(value => value + 1)} />}
  </section>;
}
