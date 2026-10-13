import "./story-reference-controls.css";

export type ReferenceFilter = "all" | "open" | "fulfilled" | "known" | "unknown";
export function StoryReferenceControls({ view, filter, count, loaded, ready, discussDisabled, onFilter, onDiscuss }: {
  view: "plot" | "timeline"; filter: ReferenceFilter; count: number; loaded: number; ready: boolean; discussDisabled: boolean;
  onFilter: (value: ReferenceFilter) => void; onDiscuss: (prompt: string) => void;
}) {
  const options: { value: ReferenceFilter; label: string }[] = view === "plot"
    ? [{ value: "all", label: "全部" }, { value: "open", label: "未兑现" }, { value: "fulfilled", label: "已兑现" }]
    : [{ value: "all", label: "全部" }, { value: "known", label: "时间已知" }, { value: "unknown", label: "未指定" }];
  return <section className="story-reference-controls" aria-label={view === "plot" ? "伏笔目录操作" : "时间资料操作"}>
    <button type="button" className="primary-button" disabled={discussDisabled} onClick={() => onDiscuss(view === "plot"
      ? "请从已保存正文中提取有原文依据的伏笔与悬念建议，先保存建议供我决定，不修改或发布正文，也不自动确认。"
      : "请分析已保存正文中的事件与故事时间，说明哪些先后有依据、哪些时间尚未指定，先保存分析供我决定，不改写或发布正文。")}>{view === "plot" ? "在对话中梳理伏笔" : "在对话中梳理时间"}</button>
    <p>这里只准备对话，发送后才会执行。{view === "plot" && "未兑现包含部分兑现；已放弃的条目可在全部中查看。"}</p>
    {ready && <><div className="reference-filter" role="group" aria-label="筛选已加载资料">
      {options.map(option => <button key={option.value} type="button" aria-pressed={filter === option.value} onClick={() => onFilter(option.value)}>{option.label}</button>)}
    </div><p role="status">仅筛选已加载范围 · 显示 {count} / {loaded} 条</p>
      {filter !== "all" && count === 0 && <div className="reference-filter-empty"><span>已加载范围内没有匹配条目；不代表作品没有相关资料。</span><button type="button" className="text-button" onClick={() => onFilter("all")}>清除筛选</button></div>}</>}
  </section>;
}
