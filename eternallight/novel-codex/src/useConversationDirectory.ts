import { useEffect, useMemo, useReducer, useRef } from "react";
import type { NovelCodexApi, ThreadSummary } from "./types";

type Directory = { threads: ThreadSummary[]; archived: boolean; loading: boolean; error: string | null };
type Action = { type: "loading"; clearError: boolean } | { type: "loaded"; threads: ThreadSummary[] }
  | { type: "error"; error: string | null } | { type: "settled" } | { type: "reset" }
  | { type: "archiveView" } | { type: "registered"; thread: ThreadSummary }
  | { type: "renamed"; thread: ThreadSummary } | { type: "event"; event: unknown; projectId: string };
const initial: Directory = { threads: [], archived: false, loading: false, error: null };
function eventRecord(value: unknown): Record<string, unknown> | null { return value !== null && typeof value === "object" && !Array.isArray(value) ? value as Record<string, unknown> : null; }
function eventText(value: unknown): string | null { return typeof value === "string" && value.trim() ? value.slice(0, 240) : null; }
function eventThreadId(event: unknown): string | null { const message = eventRecord(event); const params = eventRecord(message?.params); const thread = eventRecord(params?.thread); const id = typeof params?.threadId === "string" ? params.threadId : typeof thread?.id === "string" ? thread.id : null; return id && id.trim() && id.length <= 240 && !/[\r\n]/.test(id) ? id : null; }

function eventThreadStatus(event: unknown): string | null { const message = eventRecord(event); const params = eventRecord(message?.params); const status = eventRecord(params?.status); const type = eventText(status?.type); const flags = Array.isArray(status?.activeFlags) ? status.activeFlags : []; if (flags.some((flag) => flag === "waitingOnApproval" || flag === "waitingOnUserInput")) return "waiting"; if (type) return type; if (message?.method === "turn/started") return "active"; if (message?.method === "turn/completed") { const turn = eventRecord(params?.turn); return turn?.status === "failed" ? "systemError" : turn?.status === "interrupted" ? "interrupted" : "idle"; } return null; }
function applyThreadEvent(threads: ThreadSummary[], event: unknown, projectId: string): ThreadSummary[] {
  const threadId = eventThreadId(event), message = eventRecord(event), params = eventRecord(message?.params);
  if (!threadId) return threads;
  if (message?.method === "thread/name/updated") {
    if ((params?.projectId !== undefined && params.projectId !== projectId)
      || (params?.threadName !== undefined && params.threadName !== null && typeof params.threadName !== "string")) return threads;
    const name = typeof params?.threadName === "string" ? params.threadName : null;
    return threads.map(thread => thread.id === threadId && thread.projectId === projectId ? { ...thread, name } : thread);
  }
  const status = eventThreadStatus(event);
  return status && ["thread/status/changed", "turn/started", "turn/completed"].includes(String(message?.method))
    ? threads.map(thread => thread.id === threadId ? { ...thread, status } : thread) : threads;
}

function reducer(state: Directory, action: Action): Directory {
  switch (action.type) {
    case "reset": return { ...initial, archived: state.archived };
    case "loading": return { ...state, loading: true, error: action.clearError ? null : state.error };
    case "loaded": return { ...state, threads: action.threads, loading: false };
    case "error": return { ...state, error: action.error };
    case "settled": return { ...state, loading: false };
    case "archiveView": return { ...state, archived: !state.archived, error: null };
    case "registered": return { ...state, loading: false, archived: false, threads: [action.thread, ...state.threads.filter(thread => thread.id !== action.thread.id)] };
    case "renamed": return { ...state, threads: state.threads.map(thread => thread.id === action.thread.id ? action.thread : thread) };
    case "event": return { ...state, threads: applyThreadEvent(state.threads, action.event, action.projectId) };
  }
}

export function useConversationDirectory({ api, projectId, authorKey, enabled, configured, changing, isRunning, onArchived }: {
  api?: NovelCodexApi; projectId: string; authorKey: string; enabled: boolean; configured: boolean; changing: boolean;
  isRunning: (threadId: string) => boolean; onArchived: (threadId: string) => void;
}) {
  const [state, dispatch] = useReducer(reducer, initial);
  const owner = useMemo(() => ({}), [api, projectId, authorKey, enabled]);
  const route = useRef(owner); route.current = owner;
  const eventProject = useRef(projectId); eventProject.current = projectId;
  const renderedOwner = useRef(owner);
  if (renderedOwner.current !== owner) { renderedOwner.current = owner; dispatch({ type: "reset" }); }
  const listRequest = useRef<object | null>(null);
  const pending = useRef(new Set<string>());
  const current = () => route.current === owner;
  const report = (error: string | null) => dispatch({ type: "error", error });
  async function refresh(archived = state.archived, clearError = true) {
    if (!api || !enabled) return;
    const request = {}; listRequest.current = request;
    const canApply = () => current() && listRequest.current === request;
    dispatch({ type: "loading", clearError });
    try { const result = await api.agent.listThreads(projectId, archived); if (canApply()) dispatch({ type: "loaded", threads: result.threads.filter(thread => thread.projectId === projectId) }); }
    catch { if (canApply()) report("任务历史暂不可用"); }
    finally { if (canApply()) dispatch({ type: "settled" }); }
  }
  useEffect(() => {
    route.current = owner; dispatch({ type: "reset" }); pending.current = new Set();
    void refresh();
    return () => { listRequest.current = null; if (route.current === owner) route.current = {}; };
  }, [owner, state.archived]);
  async function mutate(thread: ThreadSummary, kind: "pin" | "archive") {
    if (!api || !enabled || !configured || thread.projectId !== projectId || pending.current.has(thread.id)) return;
    if (kind === "archive" && isRunning(thread.id)) { report("此对话正在执行，完成或停止后才能归档。"); return; }
    const operations = pending.current; operations.add(thread.id); report(null);
    try {
      if (kind === "pin") await api.agent.setThreadPinned(projectId, thread.id, !thread.isPinned);
      else if (state.archived) await api.agent.unarchiveThread(projectId, thread.id);
      else { await api.agent.archiveThread(projectId, thread.id); if (current()) onArchived(thread.id); }
      if (!current()) return;
      await refresh(state.archived, false);
    } catch (error) { if (current()) report(`${kind === "pin" ? "更新置顶状态" : state.archived ? "恢复" : "归档"}失败：${error instanceof Error ? error.message : "未知错误"}`); }
    finally { operations.delete(thread.id); }
  }
  async function rename(thread: ThreadSummary, title: string) {
    if (!api || changing || !enabled) throw new Error("正在切换对话，请稍后重命名");
    if (thread.projectId !== projectId) throw new Error("对话不属于当前作品");
    const result = await api.agent.renameThread(projectId, thread.id, title);
    if (!current()) return;
    if (result.thread.id !== thread.id || result.thread.projectId !== projectId) throw new Error("对话回执归属不匹配");
    dispatch({ type: "renamed", thread: result.thread });
  }
  return { ...state, refresh, report, rename,
    pin: (thread: ThreadSummary) => mutate(thread, "pin"), archive: (thread: ThreadSummary) => mutate(thread, "archive"),
    toggleArchived: () => dispatch({ type: "archiveView" }),
    register: (thread: ThreadSummary) => { if (thread.projectId === projectId) { listRequest.current = null; dispatch({ type: "registered", thread }); } },
    applyEvent: (event: unknown) => dispatch({ type: "event", event, projectId: eventProject.current }),
    reset: () => { listRequest.current = null; dispatch({ type: "reset" }); },
  };
}
