import { useEffect, useRef, useState } from "react";
import type { NovelCodexApi, SearchResult } from "./types";

export interface WorkspaceSearchState {
  status: "idle" | "loading" | "ready" | "error";
  results: SearchResult[];
  partial: boolean;
  error: string | null;
}
const idle: WorkspaceSearchState = { status: "idle", results: [], partial: false, error: null };

export function useWorkspaceSearch(api: NovelCodexApi | undefined, projectId: string) {
  const [input, setInput] = useState({ projectId, query: "", open: false });
  const [snapshot, setSnapshot] = useState({ projectId, query: "", state: idle });
  const query = input.projectId === projectId ? input.query : "";
  const open = input.projectId === projectId && input.open;
  const context = useRef({ api, projectId, query });
  context.current = { api, projectId, query };
  const request = useRef<{ api: NovelCodexApi | undefined; projectId: string; query: string } | null>(null);
  const mounted = useRef(true);
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; request.current = null; }; }, []);
  const state = snapshot.projectId === projectId && snapshot.query === query ? snapshot.state : idle;
  function setQuery(value: string) {
    context.current = { api, projectId, query: value };
    request.current = null;
    setInput({ projectId, query: value, open: true });
    setSnapshot({ projectId, query: value, state: idle });
  }
  function reset() {
    request.current = null;
    context.current = { api, projectId, query: "" };
    setInput({ projectId, query: "", open: false });
    setSnapshot({ projectId, query: "", state: idle });
  }
  async function submit() {
    if (!query.trim() || !projectId || (request.current && request.current.api === api
      && request.current.projectId === projectId && request.current.query === query)) return;
    const operation = { api, projectId, query };
    request.current = operation;
    const current = () => mounted.current && request.current === operation
      && context.current.api === api && context.current.projectId === projectId && context.current.query === query;
    setSnapshot({ projectId, query, state: { ...idle, status: "loading" } });
    try {
      if (!api) throw new Error("桌面连接未加载，请重新打开应用。");
      const response = await api.runtime.search(projectId, query, 20);
      if (current()) setSnapshot({ projectId, query, state: { status: "ready", results: response.results,
        partial: !response.coverage.complete, error: null } });
    } catch (reason) {
      if (current()) setSnapshot({ projectId, query, state: { ...idle, status: "error",
        error: reason instanceof Error ? reason.message : "搜索失败，请重试。" } });
    } finally { if (request.current === operation) request.current = null; }
  }
  function close() { request.current = null; setInput({ projectId, query, open: false }); setSnapshot({ projectId, query, state: idle }); }
  return { query, open, state, setQuery, submit, reset, close,
    toggle: () => open ? close() : setInput({ projectId, query, open: true }),
    reportError: (error: string) => { request.current = null; setSnapshot({ projectId, query, state: { ...idle, status: "error", error } }); },
  };
}
