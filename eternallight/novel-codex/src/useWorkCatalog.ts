import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { NovelCodexApi, WorkCatalog, WorkCatalogOperation } from "./types";

export function useWorkCatalog(api: NovelCodexApi | undefined, projectId: string, refreshToken: number, enabled: boolean) {
  const owner = useMemo(() => ({}), [api, projectId, enabled]);
  const current = useRef(owner);
  current.current = owner;
  const [state, setState] = useState<{ owner: object; catalog: WorkCatalog | null; error: string | null }>({ owner, catalog: null, error: null });
  const stateRef = useRef(state);
  stateRef.current = state;
  const sequence = useRef(0);
  const pending = useRef<{ owner: object; request: { expectedRevision: number; idempotencyKey: string; operation: WorkCatalogOperation } } | null>(null);
  const writing = useRef<{ owner: object; ticket: object } | null>(null);
  const [busyOwner, setBusyOwner] = useState<object | null>(null);
  const read = useCallback(async (mode: "background" | "reconcile") => {
    if (!api || !enabled || writing.current?.owner === owner) return;
    const ticket = ++sequence.current;
    try {
      const catalog = await api.runtime.getWorkCatalog(projectId);
      if (current.current === owner && sequence.current === ticket) {
        const unresolved = pending.current?.owner === owner && mode === "background";
        setState({ owner, catalog, error: unresolved ? "上次目录操作未确认，请重新读取目录核对结果。" : null });
        if (!unresolved) pending.current = null;
      }
    } catch (reason) {
      if (current.current === owner && sequence.current === ticket) setState(previous => ({ owner, catalog: previous.owner === owner ? previous.catalog : null, error: reason instanceof Error ? reason.message : "作品目录读取失败" }));
    }
  }, [api, projectId, owner, enabled]);
  const refresh = useCallback(() => read("reconcile"), [read]);
  useEffect(() => { void read("background"); }, [read, refreshToken]);
  async function update(operation: WorkCatalogOperation) {
    const snapshot = stateRef.current;
    if (!api || current.current !== owner || snapshot.owner !== owner || !snapshot.catalog || snapshot.error || writing.current?.owner === owner) throw new Error("请先重新读取作品目录");
    if (pending.current?.owner === owner && JSON.stringify(pending.current.request.operation) !== JSON.stringify(operation)) throw new Error("上次操作结果尚未确认，请重新读取目录后再操作");
    const request = pending.current?.owner === owner ? pending.current.request : { expectedRevision: snapshot.catalog.revision, idempotencyKey: crypto.randomUUID(), operation };
    pending.current = { owner, request };
    const flight = { owner, ticket: {} };
    writing.current = flight;
    ++sequence.current;
    setBusyOwner(owner);
    try {
      const result = await api.runtime.updateWorkCatalog(projectId, request);
      if (current.current !== owner) return;
      pending.current = null;
      setState({ owner, catalog: result.catalog, error: null });
    } catch (reason) {
      if (current.current === owner) setState(previous => ({ ...previous, error: "目录操作未确认，请重新读取目录核对结果；不会自动重复创建。" }));
      throw reason;
    } finally {
      if (writing.current === flight) { writing.current = null; if (current.current === owner) setBusyOwner(null); }
    }
  }
  return { catalog: state.owner === owner ? state.catalog : null, error: state.owner === owner ? state.error : null, busy: busyOwner === owner, refresh, update };
}
