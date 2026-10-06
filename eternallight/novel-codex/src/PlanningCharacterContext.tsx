import { createContext, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import type { IdentityCatalog, NovelCodexApi } from "./types";

type Owner = { api: NovelCodexApi | undefined; projectId: string; refreshToken: number; retry: number };
type Read = { owner: Owner; catalog?: IdentityCatalog; loading: boolean; error?: string };
type Characters = { entries: IdentityCatalog["entries"]; current: () => boolean };
const CharacterContext = createContext<Characters | null>(null);

// One read per planning page; selecting a source identity never applies a merge.
export function PlanningCharacterProvider({ api, projectId, refreshToken, children }: {
  api?: NovelCodexApi; projectId: string; refreshToken: number; children: ReactNode;
}) {
  const [retry, setRetry] = useState(0);
  const [read, setRead] = useState<Read | null>(null);
  const owner = useMemo(() => ({ api, projectId, refreshToken, retry }), [api, projectId, refreshToken, retry]);
  const current = useRef<Owner | null>(owner); current.current = owner;
  useEffect(() => {
    current.current = owner;
    if (!api || !projectId) return;
    let active = true;
    setRead(previous => ({ owner, loading: true, catalog: previous?.owner.api === api && previous.owner.projectId === projectId ? previous.catalog : undefined }));
    void api.runtime.getIdentityCatalog(projectId).then(catalog => {
      if (active && current.current === owner) setRead({ owner, catalog, loading: false });
    }).catch(reason => {
      if (active && current.current === owner) setRead(previous => ({ owner, loading: false,
        catalog: previous?.owner === owner ? previous.catalog : undefined,
        error: reason instanceof Error ? reason.message : "人物来源暂时无法读取" }));
    });
    return () => { active = false; if (current.current === owner) current.current = null; };
  }, [owner]);
  const visible = read?.owner === owner ? read : null;
  const entries = visible?.catalog?.entries.filter(entry => entry.kind === "character") ?? [];
  return <CharacterContext.Provider value={{ entries, current: () => current.current === owner }}>
    {children}
    {visible?.error && <p className="dialog-error" role="alert">人物来源读取失败：{visible.error}。已选人物保留。
      <button type="button" className="text-button" onClick={() => { if (current.current === owner) setRetry(value => value + 1); }}>重新读取人物来源</button></p>}
    <details className="plan-character-sources"><summary>人物来源</summary>
      <p role="status">{!api || !projectId ? "打开作品后可读取人物来源。" : !visible || visible.loading ? "正在读取人物来源…" : visible.catalog ? entries.length ? `已读取 ${entries.length} 条人物来源。` : "已读取范围暂无人物来源，已有视角设置仍保留。" : "人物来源暂未读取。"}
        {visible?.catalog?.truncated && " 当前目录仅包含部分来源。"}</p>
      <p>选择原始人物来源作为写作视角，不确认人物设定或合并身份。</p>
    </details>
  </CharacterContext.Provider>;
}

export function PlanningCharacterSelect({ value, onChange, label, disabled = false }: {
  value: string | null; onChange: (value: string | null) => void; label: string; disabled?: boolean;
}) {
  const context = useContext(CharacterContext);
  const blocked = useRef(disabled); blocked.current = disabled;
  if (!context) throw new Error("PlanningCharacterSelect requires PlanningCharacterProvider");
  const selected = context.entries.find(entry => entry.id === value);
  return <div className="plan-field"><label><span>视角人物</span>
    <select aria-label={label} value={value ?? ""} disabled={disabled} onChange={event => {
      if (!context.current() || blocked.current) return;
      const id = event.target.value;
      if (!id || context.entries.some(entry => entry.id === id)) onChange(id || null);
    }}><option value="">未指定</option>
      {value && !selected && <option value={value}>当前人物（名称暂缺）</option>}
      {context.entries.map((entry, index) => <option key={entry.id} value={entry.id}>
        {entry.claims[0]?.displayName || "人物名称暂缺"} · {entry.claims[0]?.evidence.quote.slice(0, 24) || "原文来源暂缺"} · 来源 {index + 1}{entry.resolutionState === "authorMerged" ? " · 身份已合并（原始来源）" : ""}
      </option>)}
    </select></label>
    {value && <details><summary>所选人物来源</summary>
      {selected ? selected.claims.map((claim, index) => <blockquote key={index}>{claim.displayName}：{claim.evidence.quote}</blockquote>) : <p>当前目录未读到这条人物来源，不自动更换视角。</p>}
      <small>原始身份：{value}</small>
    </details>}
  </div>;
}
