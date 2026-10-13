import { useEffect, useMemo, useRef, useState } from "react";
import type { NovelCodexApi, ThreadDetail, ThreadSummary } from "./types";

export function useThreadHistoryDetail(api: NovelCodexApi | undefined, projectId: string) {
  const scope = useMemo(() => ({ api, projectId }), [api, projectId]);
  const current = useRef<typeof scope | null>(scope);
  current.current = scope;
  const request = useRef<object | null>(null);
  const launcher = useRef<{ scope: typeof scope; element: HTMLElement } | null>(null);
  const [state, setState] = useState<{ scope: typeof scope; thread: ThreadSummary;
    detail: ThreadDetail | null; loading: boolean; error: string | null } | null>(null);
  useEffect(() => {
    current.current = scope;
    return () => { if (current.current === scope) current.current = null; };
  }, [scope]);
  async function read(thread: ThreadSummary) {
    if (!api || !projectId || current.current !== scope) return;
    const ticket = {};
    request.current = ticket;
    const isCurrent = () => current.current === scope && request.current === ticket;
    setState({ scope, thread, detail: null, loading: true, error: null });
    try {
      const detail = await api.agent.readThread(projectId, thread.id);
      if (!isCurrent()) return;
      if (detail.thread.id !== thread.id) throw new Error("读取结果不属于所选对话。");
      setState({ scope, thread, detail, loading: false, error: null });
    } catch (reason) {
      if (isCurrent()) setState({ scope, thread, detail: null, loading: false,
        error: reason instanceof Error ? reason.message : "读取对话记录失败。" });
    }
  }
  const visible = state?.scope === scope ? state : null;
  return {
    detail: visible?.detail ?? null, loading: visible?.loading ?? false, error: visible?.error ?? null,
    open(thread: ThreadSummary) {
      if (current.current !== scope) return;
      launcher.current = document.activeElement instanceof HTMLElement ? { scope, element: document.activeElement } : null;
      void read(thread);
    },
    retry() { if (visible) void read(visible.thread); },
    close() {
      if (current.current !== scope) return;
      request.current = null;
      setState(null);
      if (launcher.current?.scope === scope && launcher.current.element.isConnected) launcher.current.element.focus({ preventScroll: true });
      launcher.current = null;
    },
  };
}
