import type { Chapter, SearchResult } from "./types";
import type { WorkspaceSearchState } from "./useWorkspaceSearch";
import "./SearchResults.css";

const labels = { draft: "章节", brief: "作品简报", outline: "大纲", plan: "近章规划" };

export function searchNavigation(result: SearchResult, chapters: Chapter[]):
  { kind: "chapter"; chapterId: string } | { kind: "view"; view: "brief" | "outline" | "plans" } | null {
  switch (result.kind) {
    case "draft": {
      const chapter = chapters.find(item => `draft-${item.id}` === result.id);
      return chapter ? { kind: "chapter", chapterId: chapter.id } : null;
    }
    case "brief": return { kind: "view", view: "brief" };
    case "outline": return { kind: "view", view: "outline" };
    case "plan": return { kind: "view", view: "plans" };
  }
}

export function SearchResults({ state, onSelect, onRetry }: {
  state: WorkspaceSearchState; onSelect: (result: SearchResult) => void; onRetry: () => void;
}) {
  if (state.status === "idle") return null;
  return <section className="search-results workspace-search-results" aria-label="作品搜索结果">
    {state.status === "loading" && <p role="status">正在搜索当前作品…</p>}
    {state.status === "error" && <div role="alert"><p>搜索失败：{state.error}</p>
      <button type="button" className="secondary-button" onClick={onRetry}>重试搜索</button></div>}
    {state.status === "ready" && <>
      {state.partial && <p role="status">结果未覆盖全部匹配，请缩小查询范围。</p>}
      {state.results.length === 0 && <p role="status">{state.partial ? "当前范围未返回匹配内容。" : "未找到匹配内容，请尝试其他关键词。"}</p>}
      {state.results.map(result => <button type="button"
        key={`${result.kind}:${result.id}:${result.sourceRevisionId}:${result.sourcePath}`}
        onClick={() => onSelect(result)}><strong>{labels[result.kind]} · {result.title}</strong>
        <small>{result.snippet}</small></button>)}
    </>}
  </section>;
}
