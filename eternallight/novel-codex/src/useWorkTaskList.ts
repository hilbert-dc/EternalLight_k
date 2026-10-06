import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { NovelCodexApi, WorkTaskSummary } from "./types";

type Page = { tasks: WorkTaskSummary[]; nextCursor: string | null; activeCount: number };
const emptyPage: Page = { tasks: [], nextCursor: null, activeCount: 0 };
type Scope = { api: NovelCodexApi | undefined; projectId: string };
type ListState = Page & { scope: Scope; loaded: boolean; reading: boolean; loadingMore: boolean; error: string | null };
type Reader = { scope: Scope; refresh: () => Promise<void>; more: () => Promise<void>; poll: () => void };
export type WorkTaskList = ReturnType<typeof useWorkTaskList>;

// This owns only the lifetime of UI reads, never task execution or domain state.
export function useWorkTaskList(api: NovelCodexApi | undefined, projectId: string, refreshToken: number, agentRunning: boolean) {
  const scope = useMemo(() => ({ api, projectId }), [api, projectId]);
  const [state, setState] = useState<ListState>(() => ({ ...emptyPage, scope, loaded: false, reading: false, loadingMore: false, error: null }));
  const readerRef = useRef<Reader | null>(null);
  useEffect(() => {
    let active = true;
    let page = emptyPage;
    let loaded = false;
    let error: string | null = null;
    let pending: Promise<void> | null = null;
    let queuedRefresh = false;
    let epoch = 0;
    const run = (cursor: string | null): Promise<void> => {
      if (!api || !active) return Promise.resolve();
      if (pending) {
        if (cursor === null) { queuedRefresh = true; epoch += 1; }
        return pending;
      }
      pending = (async () => {
        let nextCursor = cursor;
        do {
          queuedRefresh = false;
          const requestEpoch = ++epoch;
          setState({ ...page, scope, loaded, reading: true, loadingMore: nextCursor !== null, error });
          try {
            const result = await api.runtime.listWorkTasks(projectId, nextCursor);
            if (active && requestEpoch === epoch) {
              const seen = new Set(page.tasks.map((task) => task.id));
              page = nextCursor === null ? result : {
                ...result, tasks: [...page.tasks, ...result.tasks.filter((task) => !seen.has(task.id))],
              };
              loaded = true;
              error = null;
              setState({ ...page, scope, loaded, reading: false, loadingMore: false, error: null });
            }
          } catch (reason) {
            if (active && requestEpoch === epoch) {
              if (nextCursor !== null && reason instanceof Error && reason.message.includes("WORK_TASK_STALE")) {
                queuedRefresh = true;
              } else {
                error = reason instanceof Error ? reason.message : "读取创作任务失败";
                setState({ ...page, scope, loaded, reading: false, loadingMore: false,
                  error });
              }
            }
          }
          nextCursor = null;
        } while (active && queuedRefresh);
      })().finally(() => { pending = null; });
      return pending;
    };
    const reader: Reader = { scope, refresh: () => run(null), more: () => page.nextCursor ? run(page.nextCursor) : Promise.resolve(),
      poll: () => { if (!pending) void run(null); } };
    readerRef.current = reader;
    void reader.refresh();
    return () => { active = false; if (readerRef.current === reader) readerRef.current = null; };
  }, [api, projectId, scope]);

  const refresh = useCallback(() => readerRef.current?.scope === scope ? readerRef.current.refresh() : Promise.resolve(), [scope]);
  const loadMore = useCallback(() => readerRef.current?.scope === scope ? readerRef.current.more() : Promise.resolve(), [scope]);
  const poll = useCallback(() => { if (readerRef.current?.scope === scope) readerRef.current.poll(); }, [scope]);
  // Initial read is owned by the lifecycle effect; subsequent external changes coalesce.
  const signalRef = useRef({ scope, refreshToken, agentRunning });
  useEffect(() => {
    const previous = signalRef.current;
    signalRef.current = { scope, refreshToken, agentRunning };
    if (previous.scope === scope && (previous.refreshToken !== refreshToken || previous.agentRunning !== agentRunning)) void refresh();
  }, [scope, refreshToken, agentRunning, refresh]);
  const visible = !api ? { ...emptyPage, scope, loaded: false, reading: false, loadingMore: false, error: "当前环境未提供任务读取接口" }
    : state.scope === scope ? state : { ...emptyPage, scope, loaded: false, reading: true, loadingMore: false, error: null };
  const running = visible.tasks.some((task) => task.status === "running" || task.deliveryReport.unsettledAttemptIds.length > 0);
  useEffect(() => {
    if (!agentRunning && !running) return;
    const timer = window.setInterval(() => {
      if (readerRef.current?.scope === scope) readerRef.current.poll();
    }, 1500);
    return () => window.clearInterval(timer);
  }, [agentRunning, running, scope]);
  return { ...visible, refresh, loadMore, poll, scope };
}
